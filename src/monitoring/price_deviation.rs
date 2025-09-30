use std::collections::HashMap;

use moka::future::Cache;

use crate::{
    constants::COINGECKO_IDS, error::MonitoringError, processing::common::query_defillama_api,
    types::Entry,
};

/// Data Transfer Object for Defillama API
/// e.g
///{
//   "coins": {
//     "coingecko:bitcoin": {
//       "price": 42220,
//       "symbol": "BTC",
//       "timestamp": 1702677632,
//       "confidence": 0.99
//     }
//   }
// }
#[derive(serde::Deserialize, Debug, Clone)]
pub struct CoinPricesDTO {
    coins: HashMap<String, CoinPriceDTO>,
}

#[allow(unused)]
#[derive(serde::Deserialize, Debug, Clone)]
pub struct CoinPriceDTO {
    price: f64,
    symbol: String,
    timestamp: u64,
    confidence: f64,
}

impl CoinPricesDTO {
    pub fn get_coins(&self) -> &HashMap<String, CoinPriceDTO> {
        &self.coins
    }
}
impl CoinPriceDTO {
    pub fn get_price(&self) -> f64 {
        self.price
    }
}

/// Calculates the deviation of the price from a trusted API (DefiLLama)
pub async fn price_deviation<T: Entry>(
    query: &T,
    normalized_price: f64,
    cache: Cache<(String, u64), CoinPricesDTO>,
) -> Result<f64, MonitoringError> {
    let ids = &COINGECKO_IDS;

    let pair_id = query.pair_id().to_string();
    let coingecko_id = ids
        .get(&pair_id)
        .expect("Failed to get coingecko id")
        .to_string();

    let coins_prices = query_defillama_api(
        query.timestamp().and_utc().timestamp().try_into().unwrap(),
        coingecko_id.to_owned(),
        cache.clone(),
    )
    .await?;

    let api_id = format!("coingecko:{}", coingecko_id);

    let reference_price = coins_prices
        .coins
        .get(&api_id)
        .ok_or(MonitoringError::Api(format!(
            "Failed to get coingecko price for id {:?}",
            coingecko_id
        )))?
        .price;

    Ok((normalized_price - reference_price) / reference_price)
}

/// Calculates the raw deviation of the price from a trusted API (DefiLLama)
pub async fn raw_price_deviation(
    pair_id: &str,
    price: f64,
    cache: Cache<(String, u64), CoinPricesDTO>,
) -> Result<f64, MonitoringError> {
    let ids = &COINGECKO_IDS;

    let coingecko_id = ids
        .get(pair_id)
        .expect("Failed to get coingecko id")
        .to_string();

    let coins_prices = query_defillama_api(
        chrono::Utc::now().timestamp().try_into().unwrap(),
        coingecko_id.to_owned(),
        cache.clone(),
    )
    .await?;

    let api_id = format!("coingecko:{}", coingecko_id);

    let reference_price = coins_prices
        .coins
        .get(&api_id)
        .ok_or(MonitoringError::Api(format!(
            "Failed to get coingecko price for id {:?}",
            coingecko_id
        )))?
        .price;

    Ok((price - reference_price) / reference_price)
}
