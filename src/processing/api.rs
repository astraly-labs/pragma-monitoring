use bigdecimal::ToPrimitive;
use starknet::{
    core::types::{BlockId, BlockTag},
    providers::SequencerGatewayProvider,
};

use crate::{
    config::get_config,
    constants::{
        API_NUM_SOURCES, API_PRICE_DEVIATION, API_SEQUENCER_DEVIATION, API_TIME_SINCE_LAST_UPDATE,API_ON_OFF_PRICE_DEVIATION
    },
    error::MonitoringError,
    monitoring::{
        price_deviation::raw_price_deviation, time_since_last_update::raw_time_since_last_update, on_off_deviation::raw_on_off_price_deviation,
    },
    processing::common::query_pragma_api,
};

pub async fn process_data_by_pair(pair: String) -> Result<f64, MonitoringError> {
    // Query the Pragma API
    let config = get_config(None).await;
    let network_env = &config.network_str();

    let result = query_pragma_api(&pair, network_env).await?;

    log::info!("Processing data for pair: {}", pair);

    let normalized_price =
        result.price.parse::<f64>().unwrap() / 10_f64.powi(result.decimals as i32);

    let price_deviation = raw_price_deviation(&pair, normalized_price).await?;
    let time_since_last_update = raw_time_since_last_update(result.timestamp)?;
    let (on_off_price_deviation, _) = raw_on_off_price_deviation(&pair, normalized_price).await?;

    API_PRICE_DEVIATION
        .with_label_values(&[network_env, &pair])
        .set(price_deviation);
    API_TIME_SINCE_LAST_UPDATE
        .with_label_values(&[network_env, &pair])
        .set(time_since_last_update as f64);
    API_NUM_SOURCES
        .with_label_values(&[network_env, &pair])
        .set(result.num_sources_aggregated as i64);
    API_ON_OFF_PRICE_DEVIATION
        .with_label_values(&[network_env, &pair])
        .set(on_off_price_deviation);
    if pair == "ETH/STRK" {
        // Query the feeder gateway gas price
        let provider = SequencerGatewayProvider::starknet_alpha_goerli();
        #[allow(deprecated)]
        let block = provider
            .get_block(BlockId::Tag(BlockTag::Pending).into())
            .await
            .map_err(MonitoringError::Provider)?;

        let eth = block.eth_l1_gas_price.to_big_decimal(18);
        let strk = block.strk_l1_gas_price.to_big_decimal(18);

        let expected_price = (eth / strk).to_f64().ok_or(MonitoringError::Conversion(
            "Failed to convert expected price to f64".to_string(),
        ))?;

        let price_deviation = (normalized_price - expected_price) / expected_price;
        API_SEQUENCER_DEVIATION
            .with_label_values(&[network_env])
            .set(price_deviation);
    }

    Ok(price_deviation)
}
