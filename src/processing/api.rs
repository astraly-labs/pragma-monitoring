use std::time::Duration;

use bigdecimal::{Num, ToPrimitive};
use moka::future::Cache;
use num_bigint::BigInt;
use starknet::{
    core::types::{BlockId, BlockTag},
    providers::SequencerGatewayProvider,
};

use crate::{
    config::get_config,
    error::MonitoringError,
    monitoring::metrics::MONITORING_METRICS,
    monitoring::{
        price_deviation::{raw_price_deviation, CoinPricesDTO},
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

    let result = query_pragma_api(&pair, network_env, "median", "1min").await?;

    // sleep for rate limiting
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Parse the hex string price
    let parsed_price = BigInt::from_str_radix(&result.price[2..], 16)
        .unwrap()
        .to_string();
    let normalized_price =
        parsed_price.to_string().parse::<f64>().unwrap() / 10_f64.powi(result.decimals as i32);

    let price_deviation = raw_price_deviation(&pair, normalized_price, cache).await?;
    let time_since_last_update = raw_time_since_last_update(result.timestamp)?;

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

    let result = query_pragma_api(&pair, network_env, "twap", "2h").await?;

    // Parse the hex string price
    let parsed_price = BigInt::from_str_radix(&result.price[2..], 16)
        .unwrap()
        .to_string();
    let normalized_price =
        parsed_price.to_string().parse::<f64>().unwrap() / 10_f64.powi(result.decimals as i32);

    let provider = match network_env {
        "Testnet" => SequencerGatewayProvider::starknet_alpha_sepolia(),
        "Mainnet" => SequencerGatewayProvider::starknet_alpha_mainnet(),
        _ => panic!("Invalid network env"),
    };

    #[allow(deprecated)]
    let block = provider
        .get_block(BlockId::Tag(BlockTag::Pending).into())
        .await
        .map_err(MonitoringError::Provider)?;

    let eth = block.l1_gas_price.price_in_wei.to_bigint();
    let strk = block.l1_gas_price.price_in_fri.to_bigint();

    let expected_price = (strk / eth).to_f64().ok_or(MonitoringError::Conversion(
        "Failed to convert expected price to f64".to_string(),
    ))?;

    let price_deviation = (normalized_price - expected_price) / expected_price;
    MONITORING_METRICS
        .monitoring_metrics
        .set_api_sequencer_deviation(price_deviation, network_env);

    Ok(())
}
