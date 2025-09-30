pub mod database_handler;
pub mod status;

use anyhow::Result;
use evian::oracles::starknet::pragma::data::indexer::PragmaDataIndexer;
use evian::oracles::starknet::pragma::data::indexer::events::{PairId, PragmaEvent};
use evian::utils::indexer::handler::OutputEvent;
use pragma_common::starknet::network::StarknetNetwork;
use std::collections::HashSet;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::config::get_config;

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

    // Get API key from environment
    let apibara_api_key = std::env::var("APIBARA_API_KEY")
        .map_err(|e| anyhow::anyhow!("APIBARA_API_KEY not set: {}", e))?;

    // Get target assets from config
    let target_assets: HashSet<PairId> = config
        .sources(crate::config::DataType::Spot)
        .keys()
        .map(|pair| PairId::new(pair.clone()))
        .collect();

    // Log target assets before creating indexer
    tracing::info!(
        "Configured target assets for indexing: {:?}",
        target_assets.iter().map(|p| &p.0).collect::<Vec<_>>()
    );
    tracing::info!(
        "Starting Pragma data indexer with {} target assets",
        target_assets.len()
    );
    tracing::info!(
        "Note: Warnings about 'Ignoring event for non-target asset' are expected and normal behavior"
    );

    // Create and start the indexer
    let indexer = PragmaDataIndexer::new(
        network,
        apibara_api_key,
        target_assets,
        None, // Starting block - will start from 0
    )?;

    let (event_rx, handle) = indexer.start().await?;

    Ok((event_rx, handle))
}
