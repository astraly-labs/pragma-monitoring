use anyhow::Result;
use bigdecimal::BigDecimal;
use chrono::{DateTime, NaiveDateTime, Utc};
use deadpool::managed::Pool;
use diesel::prelude::*;
use diesel_async::AsyncPgConnection;
use diesel_async::RunQueryDsl;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use evian::oracles::starknet::pragma::data::indexer::events::{PragmaEvent, SpotEntryEvent};
use evian::utils::indexer::handler::{OutputEvent, StarknetEventMetadata};
use std::time::Duration;
use tokio::time::sleep;

use crate::config::{NetworkName, get_config};
use crate::error::MonitoringError;
use crate::indexing::status::INTERNAL_INDEXER_TRACKER;
use crate::monitoring::metrics::MONITORING_METRICS;
use crate::schema::mainnet_spot_entry::dsl as mainnet_spot_dsl;
use crate::schema::spot_entry::dsl as spot_dsl;

// Insertable structs for database operations
#[derive(Insertable)]
#[diesel(table_name = crate::schema::spot_entry)]
struct NewSpotEntry {
    network: String,
    pair_id: String,
    data_id: String,
    block_hash: String,
    block_number: i64,
    block_timestamp: NaiveDateTime,
    transaction_hash: String,
    price: BigDecimal,
    timestamp: NaiveDateTime,
    publisher: String,
    source: String,
    volume: BigDecimal,
    _cursor: i64,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::mainnet_spot_entry)]
struct NewMainnetSpotEntry {
    network: String,
    pair_id: String,
    data_id: String,
    block_hash: String,
    block_number: i64,
    block_timestamp: NaiveDateTime,
    transaction_hash: String,
    price: BigDecimal,
    timestamp: NaiveDateTime,
    publisher: String,
    source: String,
    volume: BigDecimal,
    _cursor: i64,
}

/// Handles database operations for indexed Pragma events
pub struct DatabaseHandler {
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    max_retries: u32,
    retry_delay: Duration,
}

impl DatabaseHandler {
    pub fn new(pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>) -> Self {
        Self {
            pool,
            max_retries: 3,
            retry_delay: Duration::from_secs(1),
        }
    }

    /// Processes indexed events and stores them in the database
    pub async fn process_indexed_events(
        &self,
        events: Vec<OutputEvent<PragmaEvent>>,
    ) -> Result<()> {
        let config = get_config(None).await;
        let network_name = &config.network().name;
        let mut events_processed = 0u64;

        for event in events {
            match event {
                OutputEvent::Event {
                    event,
                    event_metadata,
                } => {
                    match event {
                        PragmaEvent::Spot(spot_event) => {
                            let block_number = event_metadata.block_number;
                            self.insert_spot_entry_with_retry(
                                spot_event,
                                event_metadata,
                                network_name,
                            )
                            .await?;
                            events_processed += 1;

                            // Update status tracker
                            INTERNAL_INDEXER_TRACKER
                                .update_processed_block(block_number)
                                .await;
                        }
                    }
                }
                OutputEvent::Synced => {
                    tracing::info!("Indexer is now synced with the blockchain");
                }
                OutputEvent::Finalized(block_number) => {
                    tracing::debug!("Block {} has been finalized", block_number);
                }
                OutputEvent::Invalidated(block_number) => {
                    tracing::warn!(
                        "Block {} has been invalidated - handling reorg",
                        block_number
                    );

                    // Handle reorg by deleting all events from the invalidated block onwards
                    self.delete_invalidated_events(block_number, network_name)
                        .await?;

                    // Update status tracker to reflect the reorg
                    INTERNAL_INDEXER_TRACKER.handle_reorg(block_number).await;
                }
            }
        }

        // Update events processed count
        if events_processed > 0 {
            INTERNAL_INDEXER_TRACKER
                .increment_events_processed(events_processed)
                .await;
        }

        Ok(())
    }

    /// Inserts a spot entry with retry logic
    async fn insert_spot_entry_with_retry(
        &self,
        spot_event: SpotEntryEvent,
        event_metadata: StarknetEventMetadata,
        network_name: &NetworkName,
    ) -> Result<()> {
        let mut last_error = None;

        for attempt in 1..=self.max_retries {
            match self
                .insert_spot_entry(spot_event.clone(), &event_metadata, network_name)
                .await
            {
                Ok(()) => {
                    if attempt > 1 {
                        tracing::info!(
                            "Successfully inserted spot entry after {} attempts",
                            attempt
                        );
                    }
                    return Ok(());
                }
                Err(e) => {
                    last_error = Some(e);
                    if attempt < self.max_retries {
                        tracing::warn!(
                            "Failed to insert spot entry (attempt {}/{}): {:?}. Retrying in {:?}...",
                            attempt,
                            self.max_retries,
                            last_error.as_ref().unwrap(),
                            self.retry_delay
                        );
                        sleep(self.retry_delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap().into())
    }

    /// Inserts a spot entry into the appropriate database table
    async fn insert_spot_entry(
        &self,
        spot_event: SpotEntryEvent,
        event_metadata: &StarknetEventMetadata,
        network_name: &NetworkName,
    ) -> Result<(), MonitoringError> {
        let mut conn = self.pool.get().await.map_err(MonitoringError::Connection)?;

        let data_id = format!(
            "{}_{}_{}_{}",
            event_metadata.block_number,
            spot_event.base.timestamp,
            spot_event.pair_id,
            spot_event.base.publisher
        );

        // Convert Felt to hex string for block hash and transaction hash
        let block_hash = event_metadata
            .block_hash
            .map(|h| format!("0x{:x}", h))
            .unwrap_or_else(|| "0x0".to_string());

        let transaction_hash = format!("0x{:x}", event_metadata.transaction_hash);

        // Convert timestamp to NaiveDateTime
        let block_timestamp = DateTime::from_timestamp(event_metadata.timestamp, 0)
            .unwrap_or_else(Utc::now)
            .naive_utc();

        let entry_timestamp = DateTime::from_timestamp(spot_event.base.timestamp as i64, 0)
            .unwrap_or_else(Utc::now)
            .naive_utc();

        // Create the network string
        let network_str = match network_name {
            NetworkName::Mainnet => "Mainnet",
            NetworkName::Testnet => "Testnet",
        };

        // Clone values for logging
        let pair_id = spot_event.pair_id.clone();
        let publisher = spot_event.base.publisher.clone();
        let price = spot_event.price;
        let volume = spot_event.volume;

        match network_name {
            NetworkName::Mainnet => {
                let spot_entry = NewMainnetSpotEntry {
                    network: network_str.to_string(),
                    pair_id: spot_event.pair_id,
                    data_id,
                    block_hash,
                    block_number: event_metadata.block_number as i64,
                    block_timestamp,
                    transaction_hash,
                    price: BigDecimal::from(spot_event.price),
                    timestamp: entry_timestamp,
                    publisher: spot_event.base.publisher,
                    source: spot_event.base.source,
                    volume: BigDecimal::from(spot_event.volume),
                    _cursor: event_metadata.block_number as i64, // Use block number as cursor for now
                };

                diesel::insert_into(mainnet_spot_dsl::mainnet_spot_entry)
                    .values(spot_entry)
                    .execute(&mut conn)
                    .await
                    .map_err(MonitoringError::Database)?;
            }
            NetworkName::Testnet => {
                let spot_entry = NewSpotEntry {
                    network: network_str.to_string(),
                    pair_id: spot_event.pair_id,
                    data_id,
                    block_hash,
                    block_number: event_metadata.block_number as i64,
                    block_timestamp,
                    transaction_hash,
                    price: BigDecimal::from(spot_event.price),
                    timestamp: entry_timestamp,
                    publisher: spot_event.base.publisher,
                    source: spot_event.base.source,
                    volume: BigDecimal::from(spot_event.volume),
                    _cursor: event_metadata.block_number as i64, // Use block number as cursor for now
                };

                diesel::insert_into(spot_dsl::spot_entry)
                    .values(spot_entry)
                    .execute(&mut conn)
                    .await
                    .map_err(MonitoringError::Database)?;
            }
        }

        // Update monitoring metrics
        let network_str = match network_name {
            NetworkName::Mainnet => "Mainnet",
            NetworkName::Testnet => "Testnet",
        };

        // Track indexed events
        MONITORING_METRICS
            .monitoring_metrics
            .set_indexed_events_count(1, network_str, &pair_id, "spot");

        // Track latest indexed block
        MONITORING_METRICS
            .monitoring_metrics
            .set_latest_indexed_block(event_metadata.block_number as i64, network_str);

        tracing::info!(
            "Successfully inserted spot entry: pair={}, publisher={}, price={}, volume={}, block={}",
            pair_id,
            publisher,
            price,
            volume,
            event_metadata.block_number
        );

        Ok(())
    }

    /// Deletes all events from the invalidated block onwards to handle reorgs
    async fn delete_invalidated_events(
        &self,
        invalidated_block: u64,
        network_name: &NetworkName,
    ) -> Result<()> {
        let mut conn = self.pool.get().await.map_err(MonitoringError::Connection)?;

        let network_str = match network_name {
            NetworkName::Mainnet => "Mainnet",
            NetworkName::Testnet => "Testnet",
        };

        match network_name {
            NetworkName::Mainnet => {
                // Delete from mainnet_spot_entry table
                let deleted_spot_entries = diesel::delete(
                    mainnet_spot_dsl::mainnet_spot_entry
                        .filter(mainnet_spot_dsl::block_number.ge(invalidated_block as i64))
                        .filter(mainnet_spot_dsl::network.eq(network_str)),
                )
                .execute(&mut conn)
                .await
                .map_err(MonitoringError::Database)?;

                tracing::info!(
                    "Reorg cleanup: Deleted {} spot entries from mainnet for blocks >= {}",
                    deleted_spot_entries,
                    invalidated_block
                );
            }
            NetworkName::Testnet => {
                // Delete from spot_entry table
                let deleted_spot_entries = diesel::delete(
                    spot_dsl::spot_entry
                        .filter(spot_dsl::block_number.ge(invalidated_block as i64))
                        .filter(spot_dsl::network.eq(network_str)),
                )
                .execute(&mut conn)
                .await
                .map_err(MonitoringError::Database)?;

                tracing::info!(
                    "Reorg cleanup: Deleted {} spot entries from testnet for blocks >= {}",
                    deleted_spot_entries,
                    invalidated_block
                );
            }
        }

        // Update monitoring metrics to reflect the deletion
        MONITORING_METRICS
            .monitoring_metrics
            .set_latest_indexed_block((invalidated_block - 1) as i64, network_str);

        Ok(())
    }
}
