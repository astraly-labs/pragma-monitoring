use anyhow::Result;
use bigdecimal::BigDecimal;
use chrono::{DateTime, NaiveDateTime};
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
use crate::schema::future_entry::dsl as future_dsl;
use crate::schema::mainnet_future_entry::dsl as mainnet_future_dsl;
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
                    // This is crucial because a reorg can invalidate multiple blocks:
                    // e.g., if we receive Invalidate(42), we might have indexed blocks 42->50,
                    // so we need to delete all events from block 42 onwards (42, 43, 44, ..., 50)
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
            .map(|h| h.to_fixed_hex_string())
            .unwrap_or_else(|| "0x0".to_string());

        let transaction_hash = event_metadata.transaction_hash.to_fixed_hex_string();

        // Convert timestamp to NaiveDateTime with proper error handling
        let block_timestamp = match DateTime::from_timestamp(event_metadata.timestamp, 0) {
            Some(dt) => dt.naive_utc(),
            None => {
                tracing::error!(
                    "Invalid block timestamp {} for block {}, skipping event",
                    event_metadata.timestamp,
                    event_metadata.block_number
                );
                return Err(MonitoringError::InvalidTimestamp(
                    event_metadata.timestamp as u64,
                ));
            }
        };

        let entry_timestamp = match DateTime::from_timestamp(spot_event.base.timestamp as i64, 0) {
            Some(dt) => dt.naive_utc(),
            None => {
                tracing::error!(
                    "Invalid entry timestamp {} for pair {}, skipping event",
                    spot_event.base.timestamp,
                    spot_event.pair_id
                );
                return Err(MonitoringError::InvalidTimestamp(
                    spot_event.base.timestamp as u64,
                ));
            }
        };

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
            .set_latest_indexed_block(event_metadata.block_number as u64, network_str);

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

    /// Deletes events to handle reorgs
    /// This handles both spot and future entries
    /// If invalidated_block is 0, deletes ALL events (nuclear option)
    /// Otherwise, deletes all events from the invalidated block onwards
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

        // Determine cleanup strategy
        let is_nuclear_option = invalidated_block == 0;

        if is_nuclear_option {
            tracing::warn!(
                "Starting NUCLEAR reorg cleanup: Deleting ALL events for {}",
                network_str
            );
        } else {
            tracing::warn!(
                "Starting reorg cleanup: Deleting all events from block {} onwards for {}",
                invalidated_block,
                network_str
            );
        }

        let total_deleted = match network_name {
            NetworkName::Mainnet => {
                let deleted_spot_entries = if is_nuclear_option {
                    // Delete ALL spot entries for this network
                    diesel::delete(
                        mainnet_spot_dsl::mainnet_spot_entry
                            .filter(mainnet_spot_dsl::network.eq(network_str)),
                    )
                    .execute(&mut conn)
                    .await
                    .map_err(MonitoringError::Database)?
                } else {
                    // Delete from specific block onwards
                    diesel::delete(
                        mainnet_spot_dsl::mainnet_spot_entry
                            .filter(mainnet_spot_dsl::block_number.ge(invalidated_block as i64))
                            .filter(mainnet_spot_dsl::network.eq(network_str)),
                    )
                    .execute(&mut conn)
                    .await
                    .map_err(MonitoringError::Database)?
                };

                let deleted_future_entries = if is_nuclear_option {
                    // Delete ALL future entries for this network
                    diesel::delete(
                        mainnet_future_dsl::mainnet_future_entry
                            .filter(mainnet_future_dsl::network.eq(network_str)),
                    )
                    .execute(&mut conn)
                    .await
                    .map_err(MonitoringError::Database)?
                } else {
                    // Delete from specific block onwards
                    diesel::delete(
                        mainnet_future_dsl::mainnet_future_entry
                            .filter(mainnet_future_dsl::block_number.ge(invalidated_block as i64))
                            .filter(mainnet_future_dsl::network.eq(network_str)),
                    )
                    .execute(&mut conn)
                    .await
                    .map_err(MonitoringError::Database)?
                };

                let total = deleted_spot_entries + deleted_future_entries;

                if is_nuclear_option {
                    tracing::info!(
                        "NUCLEAR cleanup completed for mainnet: Deleted ALL {} spot entries and {} future entries (total: {})",
                        deleted_spot_entries,
                        deleted_future_entries,
                        total
                    );
                } else {
                    tracing::info!(
                        "Reorg cleanup completed for mainnet: Deleted {} spot entries and {} future entries (total: {}) from blocks >= {}",
                        deleted_spot_entries,
                        deleted_future_entries,
                        total,
                        invalidated_block
                    );
                }

                total
            }
            NetworkName::Testnet => {
                let deleted_spot_entries = if is_nuclear_option {
                    // Delete ALL spot entries for this network
                    diesel::delete(spot_dsl::spot_entry.filter(spot_dsl::network.eq(network_str)))
                        .execute(&mut conn)
                        .await
                        .map_err(MonitoringError::Database)?
                } else {
                    // Delete from specific block onwards
                    diesel::delete(
                        spot_dsl::spot_entry
                            .filter(spot_dsl::block_number.ge(invalidated_block as i64))
                            .filter(spot_dsl::network.eq(network_str)),
                    )
                    .execute(&mut conn)
                    .await
                    .map_err(MonitoringError::Database)?
                };

                let deleted_future_entries = if is_nuclear_option {
                    // Delete ALL future entries for this network
                    diesel::delete(
                        future_dsl::future_entry.filter(future_dsl::network.eq(network_str)),
                    )
                    .execute(&mut conn)
                    .await
                    .map_err(MonitoringError::Database)?
                } else {
                    // Delete from specific block onwards
                    diesel::delete(
                        future_dsl::future_entry
                            .filter(future_dsl::block_number.ge(invalidated_block as i64))
                            .filter(future_dsl::network.eq(network_str)),
                    )
                    .execute(&mut conn)
                    .await
                    .map_err(MonitoringError::Database)?
                };

                let total = deleted_spot_entries + deleted_future_entries;

                if is_nuclear_option {
                    tracing::info!(
                        "NUCLEAR cleanup completed for testnet: Deleted ALL {} spot entries and {} future entries (total: {})",
                        deleted_spot_entries,
                        deleted_future_entries,
                        total
                    );
                } else {
                    tracing::info!(
                        "Reorg cleanup completed for testnet: Deleted {} spot entries and {} future entries (total: {}) from blocks >= {}",
                        deleted_spot_entries,
                        deleted_future_entries,
                        total,
                        invalidated_block
                    );
                }

                total
            }
        };

        // Update monitoring metrics to reflect the deletion
        let new_latest_block = if is_nuclear_option {
            // Nuclear option: reset to 0
            0
        } else if invalidated_block > 0 {
            // Normal reorg: set to one less than the invalidated block
            (invalidated_block - 1) as i64
        } else {
            0
        };

        MONITORING_METRICS
            .monitoring_metrics
            .set_latest_indexed_block(new_latest_block as u64, network_str);

        if is_nuclear_option {
            tracing::info!(
                "NUCLEAR cleanup completed: Deleted {} total entries, reset latest indexed block to {}",
                total_deleted,
                new_latest_block
            );
        } else {
            tracing::info!(
                "Reorg cleanup completed: Deleted {} total entries, updated latest indexed block to {}",
                total_deleted,
                new_latest_block
            );
        }

        Ok(())
    }
}
