use bigdecimal::{Num, ToPrimitive};
use num_bigint::BigInt;
use starknet::{
    core::types::{BlockId, BlockTag},
    providers::SequencerGatewayProvider,
};

use crate::{
    config::get_config,
    constants::{
        API_NUM_SOURCES, API_PRICE_DEVIATION, API_SEQUENCER_DEVIATION, API_TIME_SINCE_LAST_UPDATE,
    },
    error::MonitoringError,
    monitoring::{
        price_deviation::raw_price_deviation, time_since_last_update::raw_time_since_last_update,
    },
    processing::common::query_pragma_api,
};

pub async fn process_data_by_pair(pair: String) -> Result<(), MonitoringError> {
    // Query the Pragma API
    let config = get_config(None).await;
    let network_env = &config.network_str();

    let result = query_pragma_api(&pair, network_env, "median", "1min").await?;

    log::info!("Processing data for pair: {}", pair);

    // Parse the hex string price
    let parsed_price = BigInt::from_str_radix(&result.price[2..], 16)
        .unwrap()
        .to_string();
    let normalized_price =
        parsed_price.to_string().parse::<f64>().unwrap() / 10_f64.powi(result.decimals as i32);

    let price_deviation = raw_price_deviation(&pair, normalized_price).await?;
    let time_since_last_update = raw_time_since_last_update(result.timestamp)?;

    API_PRICE_DEVIATION
        .with_label_values(&[network_env, &pair])
        .set(price_deviation);
    API_TIME_SINCE_LAST_UPDATE
        .with_label_values(&[network_env, &pair])
        .set(time_since_last_update as f64);
    API_NUM_SOURCES
        .with_label_values(&[network_env, &pair])
        .set(result.num_sources_aggregated as i64);

    Ok(())
}

pub async fn process_sequencer_data() -> Result<(), MonitoringError> {
    let pair = "ETH/STRK".to_string();

    // Query the Pragma API
    let config = get_config(None).await;
    let network_env = config.network_str();

    let result = query_pragma_api(&pair, network_env, "twap", "15min").await?;

    log::info!("Processing sequencer data");

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
    API_SEQUENCER_DEVIATION
        .with_label_values(&[network_env])
        .set(price_deviation);

    Ok(())
}
