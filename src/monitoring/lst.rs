use crate::{
    config::{get_config, DataType},
    constants::LST_CONVERSION_RATE,
    error::MonitoringError,
};
use num_traits::ToPrimitive;
use starknet::{
    core::{
        types::{BlockId, BlockTag, Felt, FunctionCall},
        utils::cairo_short_string_to_felt,
    },
    macros::selector,
    providers::Provider,
};

/// Get the decimals for a specific pair from the configuration
async fn get_pair_decimals(pair: &str) -> Result<u32, MonitoringError> {
    let config = get_config(None).await;
    config
        .decimals(DataType::Spot)
        .get(pair)
        .copied()
        .ok_or_else(|| MonitoringError::Api(format!("Pair {} not found", pair)))
}

/// Process LST data for a specific pair and update the conversion rate metric
pub async fn process_lst_data_by_pair(pair: String) -> Result<(), MonitoringError> {
    let config = get_config(None).await;
    let client = &config.network().provider;
    let network = config.network_str();
    let field_pair = cairo_short_string_to_felt(&pair).expect("failed to convert pair id");
    let decimals = get_pair_decimals(&pair).await?;

    // Call get_data with AggregationMode::ConversionRate (2)
    let data = client
        .call(
            FunctionCall {
                contract_address: config.network().oracle_address,
                entry_point_selector: selector!("get_data"),
                calldata: vec![Felt::ZERO, field_pair, Felt::from(2)],
            },
            BlockId::Tag(BlockTag::Latest),
        )
        .await
        .map_err(|e| MonitoringError::OnChain(e.to_string()))?;

    let conversion_rate = data
        .first()
        .ok_or(MonitoringError::OnChain(
            "No data returned from contract".to_string(),
        ))?
        .to_bigint()
        .to_f64()
        .ok_or(MonitoringError::Conversion(
            "Failed to convert conversion rate to f64".to_string(),
        ))?
        / 10u64.pow(decimals) as f64;

    // Update metric before validation
    LST_CONVERSION_RATE
        .with_label_values(&[network, &pair])
        .set(conversion_rate);

    Ok(())
}
