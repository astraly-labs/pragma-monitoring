use coingecko::CoinGeckoClient;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const CACHE_DURATION: Duration = Duration::from_secs(300); // Cache for 5 minutes

#[derive(Debug, Clone)]
struct CacheEntry {
    data: Arc<HashMap<String, String>>,
    timestamp: Instant,
}

lazy_static! {
    static ref CACHE: Arc<RwLock<Option<CacheEntry>>> = Arc::new(RwLock::new(None));
    static ref API_KEY: String = std::env::var("COINGECKO_API_KEY").unwrap_or_default();
}

#[derive(Debug, thiserror::Error)]
pub enum CoinGeckoError {
    #[error("Empty response from CoinGecko API")]
    EmptyResponse,
    #[error("Coingecko API error: {0}")]
    ApiError(String),
}

pub async fn get_coingecko_mappings() -> Result<HashMap<String, String>, CoinGeckoError> {
    // Check cache first
    if let Some(cache_entry) = CACHE.read().await.as_ref() {
        if cache_entry.timestamp.elapsed() < CACHE_DURATION {
            tracing::debug!("Returning cached CoinGecko mappings");
            return Ok((*cache_entry.data).clone());
        }
    }

    let client = CoinGeckoClient::new(&API_KEY);
    let mut mappings = HashMap::new();

    // Get all coins list
    let coins = client
        .coins_list(false)
        .await
        .map_err(|e| CoinGeckoError::ApiError(e.to_string()))?;

    if coins.is_empty() {
        return Err(CoinGeckoError::EmptyResponse);
    }

    for coin in coins {
        mappings.insert(
            format!("{}/USD", coin.symbol.to_uppercase()),
            coin.id.to_string(),
        );
    }

    // Update cache
    let cache_entry = CacheEntry {
        data: Arc::new(mappings.clone()),
        timestamp: Instant::now(),
    };
    *CACHE.write().await = Some(cache_entry);

    Ok(mappings)
}
