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

async fn get_pair_decimals(pair: &str) -> Result<u32, MonitoringError> {
    let config = get_config(None).await;
    config
        .decimals(DataType::Spot)
        .get(pair)
        .copied()
        .ok_or_else(|| MonitoringError::Api("Pair not found".to_string()))
}

pub async fn process_lst_data_by_pair(pair: String) -> Result<(), MonitoringError> {
    let config = get_config(None).await;
    let client = &config.network().provider;
    let network = config.network_str();
    let field_pair = cairo_short_string_to_felt(&pair).expect("failed to convert pair id");
    let decimals = get_pair_decimals(&pair).await?;

    // Call get_data with AggregationMode::ConversionRate
    let data = client
        .call(
            FunctionCall {
                contract_address: config.network().oracle_address,
                entry_point_selector: selector!("get_data"),
                calldata: vec![Felt::ZERO, field_pair, Felt::from(2)], // 2 represents ConversionRate
            },
            BlockId::Tag(BlockTag::Latest),
        )
        .await
        .map_err(|e| MonitoringError::OnChain(e.to_string()))?;

    let conversion_rate = data
        .first()
        .ok_or(MonitoringError::OnChain("No data".to_string()))?
        .to_bigint()
        .to_f64()
        .ok_or(MonitoringError::Conversion(
            "Failed to convert to f64".to_string(),
        ))?
        / 10u64.pow(decimals) as f64;

    // Update metric
    LST_CONVERSION_RATE
        .with_label_values(&[network, &pair])
        .set(conversion_rate);

    Ok(())
}
