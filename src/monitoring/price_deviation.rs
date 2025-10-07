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
        .unwrap_or_else(|| {
            tracing::warn!("No coingecko id found for pair: {}", pair_id);
            &"bitcoin" // Default fallback
        })
        .to_string();

    let timestamp = match query.timestamp().and_utc().timestamp().try_into() {
        Ok(timestamp) => timestamp,
        Err(e) => {
            tracing::error!("Failed to convert timestamp to u64: {:?}", e);
            return Err(MonitoringError::Conversion(format!(
                "Invalid timestamp: {:?}",
                e
            )));
        }
    };

    let coins_prices =
        query_defillama_api(timestamp, coingecko_id.to_owned(), cache.clone()).await?;

    let api_id = format!("coingecko:{}", coingecko_id);

    let reference_price = match coins_prices.coins.get(&api_id) {
        Some(coin_data) => coin_data.price,
        None => {
            tracing::warn!(
                "No price data found for coingecko id: {}, using fallback",
                coingecko_id
            );
            // Return 0 deviation for unknown coins to avoid breaking the monitoring
            return Ok(0.0);
        }
    };

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
        .unwrap_or_else(|| {
            tracing::warn!("No coingecko id found for pair: {}", pair_id);
            &"bitcoin" // Default fallback
        })
        .to_string();

    let coins_prices = query_defillama_api(
        match chrono::Utc::now().timestamp().try_into() {
            Ok(timestamp) => timestamp,
            Err(e) => {
                tracing::error!("Failed to convert current timestamp to u64: {:?}", e);
                return Err(MonitoringError::Conversion(format!(
                    "Invalid timestamp: {:?}",
                    e
                )));
            }
        },
        coingecko_id.to_owned(),
        cache.clone(),
    )
    .await?;

    let api_id = format!("coingecko:{}", coingecko_id);

    let reference_price = match coins_prices.coins.get(&api_id) {
        Some(coin_data) => coin_data.price,
        None => {
            tracing::warn!(
                "No price data found for coingecko id: {}, using fallback",
                coingecko_id
            );
            // Return 0 deviation for unknown coins to avoid breaking the monitoring
            return Ok(0.0);
        }
    };

    Ok((price - reference_price) / reference_price)
}
