use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct Coin {
    id: String,
    symbol: String,
}

fn to_usd_pair(symbol: String) -> String {
    format!("{}/USD", symbol)
}

#[derive(Debug, thiserror::Error)]
pub enum CoinGeckoError {
    #[error("Empty response from CoinGecko API")]
    EmptyResponse,
    #[error("Coingecko Coins API request failed with status: {0}")]
    RequestFailed(String),
    #[error("Coingecko Coins Failed to parse response: {0}")]
    ParseError(#[from] reqwest::Error),
}

pub async fn get_coingecko_mappings() -> Result<HashMap<String, String>, CoinGeckoError> {
    let client = reqwest::Client::new();
    let mut mappings = HashMap::new();
    let mut page = 1;

    loop {
        let url = format!(
            "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=100&page={}",
            page
        );

        let response = client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .send()
            .await
            .map_err(CoinGeckoError::ParseError)?;

        if !response.status().is_success() {
            return Err(CoinGeckoError::RequestFailed(response.status().to_string()));
        }

        let coins: Vec<Coin> = response.json().await?;
        if coins.is_empty() {
            if page == 1 {
                return Err(CoinGeckoError::EmptyResponse);
            }
            break;
        }

        coins.into_iter().for_each(|coin| {
            mappings.insert(to_usd_pair(coin.symbol.to_uppercase()), coin.id);
        });

        page += 1;
    }

    Ok(mappings)
}
