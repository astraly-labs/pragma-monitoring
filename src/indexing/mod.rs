pub mod database_handler;
pub mod status;

use anyhow::Result;
use deadpool::managed::Pool;
use diesel::prelude::*;
use diesel_async::AsyncPgConnection;
use diesel_async::RunQueryDsl;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use evian::oracles::starknet::pragma::data::indexer::PragmaDataIndexer;
use evian::oracles::starknet::pragma::data::indexer::events::{PairId, PragmaEvent};
use evian::utils::indexer::handler::OutputEvent;
use pragma_common::starknet::network::StarknetNetwork;
use std::collections::HashSet;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::config::{Config, NetworkName, get_config};
use crate::schema::{future_entry, mainnet_future_entry, mainnet_spot_entry, spot_entry};
use starknet::providers::Provider;

/// Starts the Pragma data indexer and returns a receiver for indexed events
pub async fn start_pragma_indexer() -> Result<(
    mpsc::UnboundedReceiver<OutputEvent<PragmaEvent>>,
    JoinHandle<Result<()>>,
)> {
    let config = get_config(None).await;

    // Get the network from config
    let network = match config.network().name {
        crate::config::NetworkName::Mainnet => StarknetNetwork::Mainnet,
        crate::config::NetworkName::Testnet => StarknetNetwork::Sepolia, // Assuming testnet maps to Sepolia
    };

    // Get API key from config
    let apibara_api_key = config.apibara_api_key().to_string();

    // Get target assets from config
    let target_assets: HashSet<PairId> = config
        .sources(crate::config::DataType::Spot)
        .keys()
        .map(|pair| PairId::new(pair.clone()))
        .collect();

    // Log target assets before creating indexer
    tracing::info!(
        "🎯 [INDEXER] Target pairs: {:?}",
        target_assets.iter().map(|p| &p.0).collect::<Vec<_>>()
    );
    tracing::info!("📊 [INDEXER] Monitoring {} pairs", target_assets.len());

    // Determine starting block - use latest block from blockchain if no indexed data
    let starting_block = match get_last_indexed_block(&config).await {
        Some(block) => {
            tracing::info!("📍 [INDEXER] Resuming from block {}", block);
            block
        }
        None => {
            tracing::info!("🆕 [INDEXER] No previous state, fetching latest block...");
            // Get the latest block from the blockchain, but start a few blocks back to catch recent events
            match get_latest_block_from_blockchain(&config).await {
                Some(latest) => {
                    let start_block = if latest > 10 { latest - 10 } else { latest };
                    tracing::info!(
                        "📍 [INDEXER] Starting from block {} (latest: {})",
                        start_block,
                        latest
                    );
                    start_block
                }
                None => {
                    tracing::warn!("⚠️  [INDEXER] Could not fetch latest block, using default");
                    2769044
                }
            }
        }
    };

    tracing::info!("🚀 [INDEXER] Starting from block {}", starting_block);

    // Create and start the indexer
    let indexer = PragmaDataIndexer::new(
        network,
        apibara_api_key,
        target_assets,
        Some(starting_block),
    )?;

    let (event_rx, handle) = indexer.start().await?;

    Ok((event_rx, handle))
}

/// Gets the last indexed block number from the database
/// Returns None if no blocks have been indexed yet
async fn get_last_indexed_block(config: &Config) -> Option<u64> {
    // Get database URL from environment
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            tracing::warn!("DATABASE_URL not set, using default block 2769044");
            return None;
        }
    };

    // Create database connection
    let config_manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url);
    let pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>> =
        Pool::builder(config_manager).build().unwrap();

    let mut conn = match pool.get().await {
        Ok(conn) => conn,
        Err(e) => {
            tracing::error!("Failed to get database connection: {:?}", e);
            return None;
        }
    };

    // Determine which tables to query based on network
    // IMPORTANT: Must match the casing used in database_handler.rs when inserting
    let network_str = match config.network.name {
        NetworkName::Mainnet => "Mainnet",
        NetworkName::Testnet => "Testnet",
    };

    // Query all relevant tables to find the maximum block number
    let mut max_block: Option<i64> = None;

    // Query spot_entry table
    match spot_entry::table
        .filter(spot_entry::network.eq(network_str))
        .select(diesel::dsl::max(spot_entry::block_number))
        .first::<Option<i64>>(&mut conn)
        .await
    {
        Ok(Some(block_num)) => {
            max_block = Some(max_block.map_or(block_num, |m| m.max(block_num)));
        }
        Ok(None) => {
            // No results, continue
        }
        Err(e) => {
            tracing::warn!("Failed to query spot_entry table: {:?}", e);
        }
    }

    // Query mainnet_spot_entry table
    match mainnet_spot_entry::table
        .filter(mainnet_spot_entry::network.eq(network_str))
        .select(diesel::dsl::max(mainnet_spot_entry::block_number))
        .first::<Option<i64>>(&mut conn)
        .await
    {
        Ok(Some(block_num)) => {
            max_block = Some(max_block.map_or(block_num, |m| m.max(block_num)));
        }
        Ok(None) => {
            // No results, continue
        }
        Err(e) => {
            tracing::warn!("Failed to query mainnet_spot_entry table: {:?}", e);
        }
    }

    // Query future_entry table
    match future_entry::table
        .filter(future_entry::network.eq(network_str))
        .select(diesel::dsl::max(future_entry::block_number))
        .first::<Option<i64>>(&mut conn)
        .await
    {
        Ok(Some(block_num)) => {
            max_block = Some(max_block.map_or(block_num, |m| m.max(block_num)));
        }
        Ok(None) => {
            // No results, continue
        }
        Err(e) => {
            tracing::warn!("Failed to query future_entry table: {:?}", e);
        }
    }

    // Query mainnet_future_entry table
    match mainnet_future_entry::table
        .filter(mainnet_future_entry::network.eq(network_str))
        .select(diesel::dsl::max(mainnet_future_entry::block_number))
        .first::<Option<i64>>(&mut conn)
        .await
    {
        Ok(Some(block_num)) => {
            max_block = Some(max_block.map_or(block_num, |m| m.max(block_num)));
        }
        Ok(None) => {
            // No results, continue
        }
        Err(e) => {
            tracing::warn!("Failed to query mainnet_future_entry table: {:?}", e);
        }
    }

    match max_block {
        Some(block_num) => {
            let block_u64 = block_num as u64;
            tracing::info!(
                "Found last indexed block: {} for network: {}",
                block_u64,
                network_str
            );
            Some(block_u64)
        }
        None => {
            tracing::info!(
                "No indexed blocks found for network: {}, using default 2769044",
                network_str
            );
            None
        }
    }
}

/// Gets the latest block number from the blockchain
async fn get_latest_block_from_blockchain(config: &Config) -> Option<u64> {
    tracing::info!("Fetching latest block from blockchain...");
    match config.network().provider.block_number().await {
        Ok(block_number) => {
            tracing::info!("Latest blockchain block: {}", block_number);
            Some(block_number)
        }
        Err(e) => {
            tracing::error!("Failed to get latest block from blockchain: {:?}", e);
            None
        }
    }
}
