use bigdecimal::ToPrimitive;
use num_bigint::ToBigInt;
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
        .expect("failed to get median price");

    let decimals = config.decimals.get(&query.pair_id).unwrap();
    let on_chain_price = data
        .first()
        .unwrap()
        .to_big_decimal(*decimals)
        .to_bigint()
        .unwrap();

    let published_price = query.price.to_bigint().unwrap();

    let deviation = ((published_price - on_chain_price.clone()) / on_chain_price.clone())
        .to_f64()
        .unwrap();
    let num_sources_aggregated = (*data.get(3).unwrap()).try_into().unwrap();

    Ok((deviation, num_sources_aggregated))
}
