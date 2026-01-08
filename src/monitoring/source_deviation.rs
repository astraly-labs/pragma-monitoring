use bigdecimal::ToPrimitive;
use starknet::{
    core::{
        types::{BlockId, BlockTag, Felt, FunctionCall},
        utils::cairo_short_string_to_felt,
    },
    macros::selector,
    providers::Provider,
};
use std::time::Duration;
use tokio::time::timeout;

use crate::{config::get_config, error::MonitoringError, types::Entry, utils::try_felt_to_u32};

/// Calculates the deviation of a price from the on-chain price
/// Returns the deviation and the number of sources aggregated
pub async fn source_deviation<T: Entry>(
    query: &T,
    normalized_price: f64,
) -> Result<(f64, u32), MonitoringError> {
    let config = get_config(None).await;

    let client = &config.network().provider;
    let field_pair =
        cairo_short_string_to_felt(query.pair_id()).expect("failed to convert pair id");

    // Add timeout to RPC call (10 seconds)
    let rpc_call = client.call(
        FunctionCall {
            contract_address: config.network().oracle_address,
            entry_point_selector: selector!("get_data_median"),
            calldata: vec![Felt::ZERO, field_pair],
        },
        BlockId::Tag(BlockTag::Latest),
    );

    let data = match timeout(Duration::from_secs(10), rpc_call).await {
        Ok(Ok(data)) => data,
        Ok(Err(e)) => {
            tracing::debug!("⚠️  [SOURCE_DEV] RPC error for {}: {:?}", query.pair_id(), e);
            return Err(MonitoringError::OnChain(e.to_string()));
        }
        Err(_) => {
            tracing::debug!("⏱️  [SOURCE_DEV] RPC timeout (10s) for {}", query.pair_id());
            return Err(MonitoringError::OnChain(format!(
                "RPC call timeout for pair {}",
                query.pair_id()
            )));
        }
    };

    let decimals = config
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
        ))?
        / 10u32.pow(*decimals as u32) as f64;

    let deviation = (normalized_price - on_chain_price) / on_chain_price;
    let num_sources_aggregated = try_felt_to_u32(data.get(3).unwrap()).map_err(|e| {
        MonitoringError::Conversion(format!("Failed to convert num sources {:?}", e))
    })?;

    Ok((deviation, num_sources_aggregated))
}
