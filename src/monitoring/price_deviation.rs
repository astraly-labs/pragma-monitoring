use std::collections::HashMap;

use crate::{constants::COINGECKO_IDS, error::MonitoringError, types::Entry};

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
#[derive(serde::Deserialize, Debug)]
pub struct CoinPricesDTO {
    coins: HashMap<String, CoinPriceDTO>,
}

#[allow(unused)]
#[derive(serde::Deserialize, Debug)]
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
) -> Result<f64, MonitoringError> {
    let ids = &COINGECKO_IDS;

    let pair_id = query.pair_id().to_string();
    let coingecko_id = *ids.get(&pair_id).expect("Failed to get coingecko id");

    let api_key = std::env::var("DEFILLAMA_API_KEY");

    let request_url = if let Ok(api_key) = api_key {
        format!(
            "https://coins.llama.fi/prices/historical/{timestamp}/coingecko:{id}?apikey={apikey}",
            timestamp = query.timestamp().timestamp(),
            id = coingecko_id,
            apikey = api_key
        )
    } else {
        format!(
            "https://coins.llama.fi/prices/historical/{timestamp}/coingecko:{id}",
            timestamp = query.timestamp().timestamp(),
            id = coingecko_id,
        )
    };

    let response = reqwest::get(&request_url)
        .await
        .map_err(|e| MonitoringError::Api(e.to_string()))?;

    let coins_prices: CoinPricesDTO = response.json().await.map_err(|e| {
        MonitoringError::Api(format!(
            "Failed to convert to DTO object, got error {:?}",
            e.to_string()
        ))
    })?;

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
pub async fn raw_price_deviation(pair_id: &String, price: f64) -> Result<f64, MonitoringError> {
    let ids = &COINGECKO_IDS;

    let coingecko_id = *ids.get(pair_id).expect("Failed to get coingecko id");

    let api_key = std::env::var("DEFILLAMA_API_KEY");

    let request_url = if let Ok(api_key) = api_key {
        format!(
            "https://coins.llama.fi/prices/historical/{timestamp}/coingecko:{id}?apikey={apikey}",
            timestamp = chrono::Utc::now().timestamp(),
            id = coingecko_id,
            apikey = api_key
        )
    } else {
        format!(
            "https://coins.llama.fi/prices/historical/{timestamp}/coingecko:{id}",
            timestamp = chrono::Utc::now().timestamp(),
            id = coingecko_id,
        )
    };

    let response = reqwest::get(&request_url)
        .await
        .map_err(|e| MonitoringError::Api(e.to_string()))?;

    let coins_prices: CoinPricesDTO = response.json().await.map_err(|e| {
        MonitoringError::Api(format!(
            "Failed to convert to DTO object, got error {:?}",
            e.to_string()
        ))
    })?;

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
