use bigdecimal::ToPrimitive;
use starknet::{
    core::{
        types::{BlockId, BlockTag, FieldElement, FunctionCall},
        utils::cairo_short_string_to_felt,
    },
    macros::selector,
    providers::Provider,
};

use crate::{config::Config, error::MonitoringError, models::SpotEntry};

/// Calculates the deviation from the on-chain price
/// Returns the deviation and the number of sources aggregated
pub async fn source_deviation(
    query: &SpotEntry,
    normalized_price: f64,
    config: Config,
) -> Result<(f64, u32), MonitoringError> {
    let client = config.network.provider;
    let field_pair = cairo_short_string_to_felt(&query.pair_id).expect("failed to convert pair id");

    let data = client
        .call(
            FunctionCall {
                contract_address: config.network.oracle_address,
                entry_point_selector: selector!("get_data_median"),
                calldata: vec![FieldElement::ZERO, field_pair],
            },
            BlockId::Tag(BlockTag::Latest),
        )
        .await
        .map_err(|e| MonitoringError::OnChain(e.to_string()))?;

    let decimals = config.decimals.get(&query.pair_id).unwrap();
    let on_chain_price = data
        .first()
        .unwrap()
        .to_big_decimal(*decimals)
        .to_f64()
        .unwrap();

    let deviation = (normalized_price - on_chain_price) / on_chain_price;
    let num_sources_aggregated = (*data.get(3).unwrap()).try_into().map_err(|e| {
        MonitoringError::Conversion(format!("Failed to convert num sources {:?}", e))
    })?;

    Ok((deviation, num_sources_aggregated))
}
