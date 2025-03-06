use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;

const RATE_LIMIT_PER_MINUTE: u32 = 30; // CoinGecko's free tier limit
const CACHE_DURATION: Duration = Duration::from_secs(300); // Cache for 5 minutes
const BACKOFF_BASE: Duration = Duration::from_secs(2);
const MAX_RETRIES: u32 = 3;

#[derive(Debug, Clone)]
struct RateLimiter {
    last_reset: Instant,
    requests_made: u32,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            last_reset: Instant::now(),
            requests_made: 0,
        }
    }

    async fn wait_if_needed(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_reset);

        if elapsed >= Duration::from_secs(60) {
            // Reset counter if a minute has passed
            self.last_reset = now;
            self.requests_made = 0;
        } else if self.requests_made >= RATE_LIMIT_PER_MINUTE {
            // Wait for the remainder of the minute if we've hit the limit
            let wait_time = Duration::from_secs(60) - elapsed;
            tracing::info!(
                "Rate limit reached, waiting for {} seconds",
                wait_time.as_secs()
            );
            sleep(wait_time).await;
            self.last_reset = Instant::now();
            self.requests_made = 0;
        }
    }

    fn increment(&mut self) {
        self.requests_made += 1;
    }
}

#[derive(Debug, Clone)]
struct CacheEntry {
    data: Arc<HashMap<String, String>>,
    timestamp: Instant,
}

lazy_static! {
    static ref RATE_LIMITER: Arc<RwLock<RateLimiter>> = Arc::new(RwLock::new(RateLimiter::new()));
    static ref CACHE: Arc<RwLock<Option<CacheEntry>>> = Arc::new(RwLock::new(None));
}

#[derive(Debug, thiserror::Error)]
pub enum CoinGeckoError {
    #[error("Empty response from CoinGecko API")]
    EmptyResponse,
    #[error("Coingecko Coins API request failed with status: {0}")]
    RequestFailed(String),
    #[error("Coingecko Coins Failed to parse response: {0}")]
    ParseError(#[from] reqwest::Error),
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Coin {
    id: String,
    symbol: String,
}

pub async fn get_coingecko_mappings() -> Result<HashMap<String, String>, CoinGeckoError> {
    // Check cache first
    if let Some(cache_entry) = CACHE.read().await.as_ref() {
        if cache_entry.timestamp.elapsed() < CACHE_DURATION {
            tracing::debug!("Returning cached CoinGecko mappings");
            return Ok((*cache_entry.data).clone());
        }
    }

    // If not in cache, fetch with exponential backoff
    let mut retry_count = 0;
    let mut last_error = None;

    while retry_count < MAX_RETRIES {
        match fetch_mappings().await {
            Ok(mappings) => {
                // Update cache
                let cache_entry = CacheEntry {
                    data: Arc::new(mappings.clone()),
                    timestamp: Instant::now(),
                };
                *CACHE.write().await = Some(cache_entry);
                return Ok(mappings);
            }
            Err(e) => {
                retry_count += 1;
                last_error = Some(e);

                if retry_count < MAX_RETRIES {
                    let backoff = BACKOFF_BASE * 2u32.pow(retry_count - 1);
                    tracing::warn!(
                        "Attempt {} failed, retrying after {} seconds",
                        retry_count,
                        backoff.as_secs()
                    );
                    sleep(backoff).await;
                }
            }
        }
    }

    Err(last_error.unwrap_or(CoinGeckoError::EmptyResponse))
}

async fn fetch_mappings() -> Result<HashMap<String, String>, CoinGeckoError> {
    let api_key = std::env::var("COINGECKO_API_KEY")
        .expect("COINGECKO_API_KEY must be set in environment");
    let client = reqwest::Client::new();
    let mut mappings = HashMap::new();
    let mut page = 1;

    loop {
        // Wait if we need to respect rate limits
        RATE_LIMITER.write().await.wait_if_needed().await;

        let url = format!(
            "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=100&page={}",
            page
        );

        let response = client
            .get(&url)
            .header("x-cg-pro-api-key", &api_key)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
            )
            .send()
            .await
            .map_err(CoinGeckoError::ParseError)?;

        // Increment the rate limiter counter
        RATE_LIMITER.write().await.increment();

        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(CoinGeckoError::RateLimitExceeded);
        }

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

        for coin in coins {
            mappings.insert(format!("{}/USD", coin.symbol.to_uppercase()), coin.id);
        }

        page += 1;
    }

    Ok(mappings)
}
