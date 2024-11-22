use bigdecimal::ToPrimitive;
use starknet::{
    core::{
        types::{BlockId, BlockTag, Felt, FunctionCall},
        utils::cairo_short_string_to_felt,
    },
    macros::selector,
    providers::Provider,
};

use crate::monitoring::price_deviation::CoinPricesDTO;
use crate::{
    config::{get_config, DataType},
    constants::COINGECKO_IDS,
    error::MonitoringError,
    utils::try_felt_to_u32,
};

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
) -> Result<(f64, u32), MonitoringError> {
    let ids = &COINGECKO_IDS;
    let config = get_config(None).await;
    let client = &config.network().provider;
    let field_pair = cairo_short_string_to_felt(&pair_id).expect("failed to convert pair id");

    let calldata = match data_type {
        DataType::Spot => vec![Felt::ZERO, field_pair],
        DataType::Future => vec![Felt::ONE, field_pair, Felt::ZERO],
    };

    let data = client
        .call(
            FunctionCall {
                contract_address: config.network().oracle_address,
                entry_point_selector: selector!("get_data_median"),
                calldata,
            },
            BlockId::Tag(BlockTag::Latest),
        )
        .await
        .map_err(|e| MonitoringError::OnChain(e.to_string()))?;

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
            let coingecko_id = *ids.get(&pair_id).expect("Failed to get coingecko id");

            let api_key = std::env::var("DEFILLAMA_API_KEY");

            let request_url = if let Ok(api_key) = api_key {
                format!(
                    "https://pro-api.llama.fi/{apikey}/coins/prices/historical/{timestamp}/coingecko:{id}",
                    timestamp = timestamp,
                    id = coingecko_id,
                    apikey = api_key
                )
            } else {
                format!(
                    "https://coins.llama.fi/prices/historical/{timestamp}/coingecko:{id}",
                    timestamp = timestamp,
                    id = coingecko_id,
                )
            };

            let response = reqwest::get(&request_url)
                .await
                .map_err(|e| MonitoringError::Api(e.to_string()))?;

            let response_text = response
                .text()
                .await
                .map_err(|e| MonitoringError::Api(format!("Failed to get response text: {}", e)))?;

            println!("Response JSON: {}", response_text);

            let coins_prices: CoinPricesDTO =
                serde_json::from_str(&response_text).map_err(|e| {
                    MonitoringError::Api(format!(
                        "Failed to parse JSON: {}. Response: {}",
                        e, response_text
                    ))
                })?;

            let api_id = format!("coingecko:{}", coingecko_id);

            let reference_price = coins_prices
                .get_coins()
                .get(&api_id)
                .ok_or(MonitoringError::Api(format!(
                    "Failed to get coingecko price for id {:?}",
                    coingecko_id
                )))?
                .get_price();

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
