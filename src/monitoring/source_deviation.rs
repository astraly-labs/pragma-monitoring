use bigdecimal::ToPrimitive;
use starknet::{
    core::{
        types::{BlockId, BlockTag, Felt, FunctionCall},
        utils::cairo_short_string_to_felt,
    },
    macros::selector,
    providers::Provider,
};

use crate::{config::get_config, error::MonitoringError, types::Entry, utils::try_felt_to_u32};

/// Calculates the deviation from the on-chain price
/// Returns the deviation and the number of sources aggregated
pub async fn source_deviation<T: Entry>(
    query: &T,
    normalized_price: f64,
) -> Result<(f64, u32), MonitoringError> {
    let config = get_config(None).await;

    let client = &config.network().provider;
    let field_pair =
        cairo_short_string_to_felt(query.pair_id()).expect("failed to convert pair id");

    let data = client
        .call(
            FunctionCall {
                contract_address: config.network().oracle_address,
                entry_point_selector: selector!("get_data_median"),
                calldata: vec![Felt::ZERO, field_pair],
            },
            BlockId::Tag(BlockTag::Latest),
        )
        .await
        .map_err(|e| MonitoringError::OnChain(e.to_string()))?;

    let _decimals = config
        .decimals(query.data_type())
        .get(query.pair_id())
        .ok_or(MonitoringError::OnChain(format!(
            "Failed to get decimals for pair {:?}",
            query.pair_id()
        )))?;

    let on_chain_price = data
        .first()
        .ok_or(MonitoringError::OnChain("No data".to_string()))?
        .to_bigint()
        .to_f64()
        .ok_or(MonitoringError::Conversion(
            "Failed to convert to f64".to_string(),
        ))?;

    let deviation = (normalized_price - on_chain_price) / on_chain_price;
    let num_sources_aggregated = try_felt_to_u32(data.get(3).unwrap()).map_err(|e| {
        MonitoringError::Conversion(format!("Failed to convert num sources {:?}", e))
    })?;

    Ok((deviation, num_sources_aggregated))
}
