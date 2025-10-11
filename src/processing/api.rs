use std::time::Duration;

use bigdecimal::{Num, ToPrimitive};
use moka::future::Cache;
use num_bigint::BigInt;
use starknet::core::types::{BlockId, BlockTag};
use starknet::providers::{JsonRpcClient, Provider, jsonrpc::HttpTransport};
use tokio::time::timeout;

use crate::{
    config::get_config,
    error::MonitoringError,
    monitoring::metrics::MONITORING_METRICS,
    monitoring::{
        price_deviation::{CoinPricesDTO, raw_price_deviation},
        time_since_last_update::raw_time_since_last_update,
    },
    processing::common::query_pragma_api,
};

pub async fn process_data_by_pair(
    pair: String,
    cache: Cache<(String, u64), CoinPricesDTO>,
) -> Result<(), MonitoringError> {
    // Query the Pragma API
    let config = get_config(None).await;
    let network_env = &config.network_str();
    tracing::info!("Processing data for pair: {}", pair);

    let result = match query_pragma_api(&pair, network_env, "median", "1min").await {
        Ok(result) => result,
        Err(e) => {
            tracing::warn!("Failed to query Pragma API for pair {}: {:?}", pair, e);
            return Ok(()); // Skip this pair instead of failing
        }
    };

    // sleep for rate limiting
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Parse the hex string price with better error handling
    let parsed_price = match BigInt::from_str_radix(&result.price[2..], 16) {
        Ok(price) => price,
        Err(e) => {
            tracing::error!("Failed to parse price hex string for pair {}: {}", pair, e);
            return Ok(()); // Skip this pair
        }
    };

    let normalized_price = match parsed_price.to_string().parse::<f64>() {
        Ok(price) => price / 10_f64.powi(result.decimals as i32),
        Err(e) => {
            tracing::error!("Failed to convert price to f64 for pair {}: {}", pair, e);
            return Ok(()); // Skip this pair
        }
    };

    // Handle price deviation calculation gracefully
    let price_deviation = match raw_price_deviation(&pair, normalized_price, cache).await {
        Ok(deviation) => deviation,
        Err(e) => {
            tracing::warn!(
                "Failed to calculate price deviation for pair {}: {:?}",
                pair,
                e
            );
            0.0 // Use 0 as default deviation
        }
    };

    let time_since_last_update = match raw_time_since_last_update(result.timestamp as u64) {
        Ok(time) => time,
        Err(e) => {
            tracing::warn!(
                "Failed to calculate time since last update for pair {}: {:?}",
                pair,
                e
            );
            0 // Use 0 as default
        }
    };

    MONITORING_METRICS
        .monitoring_metrics
        .set_api_price_deviation(price_deviation, network_env, &pair);
    MONITORING_METRICS
        .monitoring_metrics
        .set_api_time_since_last_update(time_since_last_update as f64, network_env, &pair);
    MONITORING_METRICS.monitoring_metrics.set_api_num_sources(
        result.num_sources_aggregated as i64,
        network_env,
        &pair,
    );

    Ok(())
}

pub async fn process_sequencer_data() -> Result<(), MonitoringError> {
    let pair = "ETH/STRK".to_string();
    tracing::info!("Processing sequencer data");

    // Query the Pragma API
    let config = get_config(None).await;
    let network_env = config.network_str();

    let result = match query_pragma_api(&pair, network_env, "twap", "2h").await {
        Ok(result) => result,
        Err(e) => {
            tracing::warn!("Failed to query Pragma API for sequencer data: {:?}", e);
            return Ok(()); // Skip sequencer processing
        }
    };

    // Parse the hex string price with better error handling
    let parsed_price = match BigInt::from_str_radix(&result.price[2..], 16) {
        Ok(price) => price,
        Err(e) => {
            tracing::error!("Failed to parse sequencer price hex string: {}", e);
            return Ok(());
        }
    };

    let normalized_price = match parsed_price.to_string().parse::<f64>() {
        Ok(price) => price / 10_f64.powi(result.decimals as i32),
        Err(e) => {
            tracing::error!("Failed to convert sequencer price to f64: {}", e);
            return Ok(());
        }
    };

    // Use Cartridge RPC endpoint for sequencer data
    let rpc_url = match network_env {
        "Testnet" => "https://api.cartridge.gg/x/starknet/sepolia",
        "Mainnet" => "https://api.cartridge.gg/x/starknet/mainnet",
        _ => {
            tracing::error!("Invalid network env: {}", network_env);
            return Ok(());
        }
    };

    let provider = JsonRpcClient::new(HttpTransport::new(url::Url::parse(rpc_url).map_err(
        |e| {
            tracing::error!("Failed to parse RPC URL: {}", e);
            MonitoringError::Api(format!("Invalid RPC URL: {}", e))
        },
    )?));

    // Add timeout to RPC call (10 seconds)
    let rpc_call = provider.get_block_with_tx_hashes(BlockId::Tag(BlockTag::Latest));

    let block = match timeout(Duration::from_secs(10), rpc_call).await {
        Ok(Ok(block)) => block,
        Ok(Err(e)) => {
            tracing::warn!("Failed to get block from RPC provider: {:?}", e);
            // Set a default deviation of 0 to avoid missing metrics
            MONITORING_METRICS
                .monitoring_metrics
                .set_api_sequencer_deviation(0.0, network_env);
            return Ok(());
        }
        Err(_) => {
            tracing::warn!("RPC call timeout for get_block: exceeded 10 seconds");
            // Set a default deviation of 0 to avoid missing metrics
            MONITORING_METRICS
                .monitoring_metrics
                .set_api_sequencer_deviation(0.0, network_env);
            return Ok(());
        }
    };

    let gas_price = block.l2_gas_price();
    let eth = gas_price.price_in_wei.to_bigint();
    let strk = gas_price.price_in_fri.to_bigint();

    let expected_price = match (strk / eth).to_f64() {
        Some(price) => price,
        None => {
            tracing::warn!("Failed to convert expected price to f64");
            // Set a default deviation of 0
            MONITORING_METRICS
                .monitoring_metrics
                .set_api_sequencer_deviation(0.0, network_env);
            return Ok(());
        }
    };

    let price_deviation = (normalized_price - expected_price) / expected_price;
    MONITORING_METRICS
        .monitoring_metrics
        .set_api_sequencer_deviation(price_deviation, network_env);

    Ok(())
}
