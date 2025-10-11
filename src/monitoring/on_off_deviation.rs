use bigdecimal::ToPrimitive;
use moka::future::Cache;
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

use crate::processing::common::query_defillama_api;
use crate::{
    config::{DataType, get_config},
    constants::COINGECKO_IDS,
    error::MonitoringError,
    utils::try_felt_to_u32,
};

use super::price_deviation::CoinPricesDTO;

/// On-chain price deviation from the reference price.
/// Returns the deviation and the number of sources aggregated.
///
/// # Arguments
///
/// * `pair_id` - The pair id.
/// * `timestamp` - The timestamp for which to get the price.
/// * `data_type` - The type of data to get.
///
/// # Returns
///
/// * `Ok((deviation, num_sources_aggregated))` - The deviation and the number of sources aggregated.
/// * `Err(MonitoringError)` - The error.
pub async fn on_off_price_deviation(
    pair_id: String,
    timestamp: u64,
    data_type: DataType,
    cache: Cache<(String, u64), CoinPricesDTO>,
) -> Result<(f64, u32), MonitoringError> {
    let ids = &COINGECKO_IDS;
    let config = get_config(None).await;
    let client = &config.network().provider;
    let field_pair = cairo_short_string_to_felt(&pair_id).expect("failed to convert pair id");

    let calldata = match data_type {
        DataType::Spot => vec![Felt::ZERO, field_pair],
        DataType::Future => vec![Felt::ONE, field_pair, Felt::ZERO],
    };

    // Add timeout to RPC call (10 seconds)
    let rpc_call = client.call(
        FunctionCall {
            contract_address: config.network().oracle_address,
            entry_point_selector: selector!("get_data_median"),
            calldata,
        },
        BlockId::Tag(BlockTag::Latest),
    );

    let data = match timeout(Duration::from_secs(10), rpc_call).await {
        Ok(Ok(data)) => data,
        Ok(Err(e)) => {
            tracing::warn!("Failed to get data median for pair {}: {:?}", pair_id, e);
            return Err(MonitoringError::OnChain(e.to_string()));
        }
        Err(_) => {
            tracing::warn!("RPC call timeout for pair {}: exceeded 10 seconds", pair_id);
            return Err(MonitoringError::OnChain(format!(
                "RPC call timeout for pair {}",
                pair_id
            )));
        }
    };

    let decimals =
        config
            .decimals(data_type.clone())
            .get(&pair_id)
            .ok_or(MonitoringError::OnChain(format!(
                "Failed to get decimals for pair {:?}",
                pair_id
            )))?;

    let on_chain_price = data
        .first()
        .ok_or(MonitoringError::OnChain("No data".to_string()))?
        .to_bigint()
        .to_f64()
        .ok_or(MonitoringError::Conversion(
            "Failed to convert to f64".to_string(),
        ))?
        / 10u64.pow(*decimals as u32) as f64;

    let (deviation, num_sources_aggregated) = match data_type {
        DataType::Spot => {
            // Check if coingecko_id exists, skip gracefully if not
            let coingecko_id = match ids.get(&pair_id) {
                Some(id) => id.to_string(),
                None => {
                    tracing::warn!(
                        "No coingecko_id mapping found for pair: {}. Skipping price deviation calculation.",
                        pair_id
                    );
                    return Err(MonitoringError::Api(format!(
                        "No coingecko_id mapping for pair: {}",
                        pair_id
                    )));
                }
            };

            let coins_prices =
                query_defillama_api(timestamp, coingecko_id.to_owned(), cache.clone()).await?;

            let api_id = format!("coingecko:{}", coingecko_id);

            let reference_price = match coins_prices.get_coins().get(&api_id) {
                Some(coin_data) => coin_data.get_price(),
                None => {
                    tracing::warn!(
                        "No price data found in DefiLlama response for coingecko id: {}. Skipping deviation.",
                        coingecko_id
                    );
                    return Err(MonitoringError::Api(format!(
                        "No price data for coingecko_id: {}",
                        coingecko_id
                    )));
                }
            };

            let deviation = (reference_price - on_chain_price) / on_chain_price;
            let num_sources = data.get(3).unwrap();
            let num_sources_aggregated = try_felt_to_u32(num_sources).map_err(|e| {
                MonitoringError::Conversion(format!("Failed to convert num sources {:?}", e))
            })?;
            (deviation, num_sources_aggregated)
        }

        DataType::Future => {
            // TODO: work on a different API for futures

            (0.0, 5)
        }
    };

    Ok((deviation, num_sources_aggregated))
}
