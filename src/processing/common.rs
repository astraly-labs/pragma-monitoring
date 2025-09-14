use crate::monitoring::balance::get_on_chain_balance;
use crate::monitoring::metrics::MONITORING_METRICS;
use crate::monitoring::price_deviation::CoinPricesDTO;
use crate::{
    config::{DataType, get_config},
    error::MonitoringError,
};
use moka::future::Cache;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use starknet::core::types::Felt;
use starknet::providers::{JsonRpcClient, Provider, jsonrpc::HttpTransport};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct IndexerServerStatus {
    pub status: Option<i32>,
    pub starting_block: Option<u64>,
    pub current_block: Option<u64>,
    pub head_block: Option<u64>,
    #[serde(rename = "reason")]
    pub reason_: Option<String>,
}

// Simple circuit breaker for indexer health tracking
#[derive(Default)]
struct IndexerHealthTracker {
    last_failure: Option<Instant>,
    failure_count: u32,
}

use std::sync::OnceLock;

static INDEXER_HEALTH: OnceLock<Arc<RwLock<IndexerHealthTracker>>> = OnceLock::new();

fn get_indexer_health() -> &'static RwLock<IndexerHealthTracker> {
    INDEXER_HEALTH.get_or_init(|| Arc::new(RwLock::new(IndexerHealthTracker::default())))
}

/// Checks if indexers of the given data type are still syncing
/// Returns true if any of the indexers is still syncing
pub async fn data_is_syncing(data_type: &DataType) -> Result<bool, MonitoringError> {
    let config = get_config(None).await;

    let table_name = config.table_name(data_type.clone());

    let status = get_sink_status(&table_name, config.indexer_url()).await?;

    let provider = &config.network().provider;

    let blocks_left = blocks_left(&status, provider).await?;

    // Update the metric
    MONITORING_METRICS
        .monitoring_metrics
        .set_indexer_blocks_left(
            blocks_left.unwrap_or(0) as i64,
            (&config.network().name).into(),
            &table_name.to_string().to_ascii_lowercase(),
        );

    // Check if any indexer is still syncing
    Ok(blocks_left.is_some())
}

/// Check if the indexers are still syncing
pub async fn data_indexers_are_synced(data_type: &DataType) -> bool {
    match data_is_syncing(data_type).await {
        Ok(true) => {
            tracing::info!("[{data_type}] Indexers are still syncing ♻️");
            false
        }
        Ok(false) => {
            tracing::info!("[{data_type}] Indexers are synced ✅");
            true
        }
        Err(e) => {
            tracing::error!(
                "[{data_type}] Failed to check if indexers are syncing: {:?}",
                e
            );
            false
        }
    }
}

/// Checks if indexers of the given data type are still syncing
/// Returns true if any of the indexers is still syncing
#[allow(unused)]
pub async fn is_syncing(table_name: &str) -> Result<bool, MonitoringError> {
    let config = get_config(None).await;

    let status = get_sink_status(table_name, config.indexer_url()).await?;

    let provider = &config.network().provider;

    let blocks_left = blocks_left(&status, provider).await?;

    // Update the metric
    MONITORING_METRICS
        .monitoring_metrics
        .set_indexer_blocks_left(
            blocks_left.unwrap_or(0) as i64,
            (&config.network().name).into(),
            &table_name.to_string().to_ascii_lowercase(),
        );

    // Check if any indexer is still syncing
    Ok(blocks_left.is_some())
}

/// Check if the indexers are still syncing
#[allow(unused)]
pub async fn indexers_are_synced(table_name: &str) -> bool {
    match is_syncing(table_name).await {
        Ok(true) => {
            tracing::info!("[{table_name}] Indexers are still syncing ♻️");
            false
        }
        Ok(false) => {
            tracing::info!("[{table_name}] Indexers are synced ✅");
            true
        }
        Err(e) => {
            tracing::error!(
                "[{table_name}] Failed to check if indexers are syncing: {:?}",
                e
            );
            false
        }
    }
}

/// Returns the status of the indexer
///
/// # Arguments
///
/// * `table_name` - The name of the table to check
/// * `base_url` - The base url of the indexer server
async fn get_sink_status(
    table_name: &str,
    base_url: &str,
) -> Result<IndexerServerStatus, MonitoringError> {
    // Check circuit breaker - if we've had recent failures, skip the check
    {
        let health = get_indexer_health().read().await;
        if let Some(last_failure) = health.last_failure
            && last_failure.elapsed() < Duration::from_secs(60)
            && health.failure_count > 3
        {
            tracing::debug!("Skipping indexer status check due to recent failures");
            return Ok(IndexerServerStatus {
                status: Some(0), // Assume synced
                current_block: None,
                starting_block: None,
                head_block: None,
                reason_: Some("Indexer service experiencing issues, skipping check".to_string()),
            });
        }
    }
    let request_url = format!(
        "{base_url}/status/table/{table_name}",
        base_url = base_url,
        table_name = table_name
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| MonitoringError::Api(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .get(&request_url)
        .send()
        .await
        .map_err(|e| MonitoringError::Api(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        match status {
            reqwest::StatusCode::INTERNAL_SERVER_ERROR => {
                tracing::warn!(
                    "Indexer service returned 500 Internal Server Error - service may be down or experiencing issues"
                );

                // Update circuit breaker
                {
                    let mut health = get_indexer_health().write().await;
                    health.last_failure = Some(Instant::now());
                    health.failure_count += 1;
                }

                // Return a default status indicating we should assume it's not syncing
                return Ok(IndexerServerStatus {
                    status: Some(0), // Assume synced to avoid blocking monitoring
                    current_block: None,
                    starting_block: None,
                    head_block: None,
                    reason_: Some("Indexer service returned 500 error".to_string()),
                });
            }
            reqwest::StatusCode::SERVICE_UNAVAILABLE => {
                tracing::warn!("Indexer service is unavailable (503) - service may be restarting");
                return Ok(IndexerServerStatus {
                    status: Some(0), // Assume synced
                    current_block: None,
                    starting_block: None,
                    head_block: None,
                    reason_: Some("Indexer service unavailable".to_string()),
                });
            }
            reqwest::StatusCode::NOT_FOUND => {
                tracing::warn!(
                    "Indexer status endpoint not found (404) - table {} may not exist",
                    table_name
                );
                return Ok(IndexerServerStatus {
                    status: Some(0), // Assume synced
                    current_block: None,
                    starting_block: None,
                    head_block: None,
                    reason_: Some(format!("Table {} not found", table_name)),
                });
            }
            _ => {
                tracing::error!("Indexer status check failed with status: {}", status);
                return Err(MonitoringError::Api(format!(
                    "Indexer status check failed with status: {}",
                    status
                )));
            }
        }
    }

    let response_text = response
        .text()
        .await
        .map_err(|e| MonitoringError::Api(format!("Failed to get response text: {}", e)))?;

    // Try to parse as JSON, but handle cases where the response might be malformed
    let status = match serde_json::from_str::<IndexerServerStatus>(&response_text) {
        Ok(status) => {
            // Reset circuit breaker on successful response
            {
                let mut health = get_indexer_health().write().await;
                health.failure_count = 0;
                health.last_failure = None;
            }
            status
        }
        Err(e) => {
            tracing::warn!(
                "Failed to parse indexer status response: {}. Response: {}",
                e,
                response_text
            );
            // Update circuit breaker for parsing failure
            {
                let mut health = get_indexer_health().write().await;
                health.last_failure = Some(Instant::now());
                health.failure_count += 1;
            }
            // Return a default status indicating we should assume it's not syncing
            IndexerServerStatus {
                status: Some(0), // Assume synced
                current_block: None,
                starting_block: None,
                head_block: None,
                reason_: Some("Failed to parse status response".to_string()),
            }
        }
    };

    Ok(status)
}

/// Returns the number of blocks left to sync
/// Returns None if the indexer is synced
///
/// # Arguments
///
/// * `sink_status` - The status of the indexer
/// * `provider` - The provider to check the current block number
async fn blocks_left(
    sink_status: &IndexerServerStatus,
    provider: &JsonRpcClient<HttpTransport>,
) -> Result<Option<u64>, MonitoringError> {
    // Handle case where current_block might be None
    let block_n = match sink_status.current_block {
        Some(block) => block,
        None => {
            tracing::warn!("Indexer status has no current_block information");
            return Ok(None); // Assume synced if we can't determine
        }
    };

    let current_block = provider
        .block_number()
        .await
        .map_err(MonitoringError::Provider)?;

    if block_n < current_block {
        Ok(Some(current_block - block_n))
    } else {
        Ok(None)
    }
}

/// Data Transfer Object for Pragma API
/// e.g
/// {
//     "num_sources_aggregated": 2,
//     "pair_id": "ETH/STRK",
//     "price": "0xd136e79f57d9198",
//     "timestamp": 1705669200000,
//     "decimals": 18
// }
#[derive(serde::Deserialize, Debug)]
#[allow(dead_code)]
pub struct PragmaDataDTO {
    pub num_sources_aggregated: u32,
    pub pair_id: String,
    pub price: String,
    pub timestamp: u64,
    pub decimals: u32,
}

/// Queries Pragma API
pub async fn query_pragma_api(
    pair: &str,
    network_env: &str,
    aggregation: &str,
    interval: &str,
) -> Result<PragmaDataDTO, MonitoringError> {
    let request_url = match network_env {
        "Testnet" => format!(
            "https://api.devnet.pragma.build/node/v1/data/{pair}?aggregation={aggregation}&interval={interval}&routing=true",
            pair = pair,
        ),
        "Mainnet" => format!(
            "https://api.production.pragma.build/node/v1/data/{pair}?aggregation={aggregation}&interval={interval}&routing=true",
            pair = pair,
            aggregation = aggregation,
            interval = interval,
        ),
        _ => panic!("Invalid network env"),
    };
    // Set headers
    let mut headers = HeaderMap::new();
    let api_key = std::env::var("PRAGMA_API_KEY").expect("PRAGMA_API_KEY must be set");
    headers.insert(
        HeaderName::from_static("x-api-key"),
        HeaderValue::from_str(&api_key).expect("Failed to parse api key"),
    );

    let client = reqwest::Client::new();
    let response = client
        .get(request_url.clone())
        .headers(headers)
        .send()
        .await
        .map_err(|e| MonitoringError::Api(e.to_string()))?;

    match response.status() {
        reqwest::StatusCode::OK => {
            // on success, parse our JSON to an APIResponse
            match response.json::<PragmaDataDTO>().await {
                Ok(parsed) => Ok(parsed),
                Err(e) => Err(MonitoringError::Api(e.to_string())),
            }
        }
        reqwest::StatusCode::UNAUTHORIZED => Err(MonitoringError::Api("Unauthorized".to_string())),
        reqwest::StatusCode::NOT_FOUND => {
            tracing::warn!("Pair {} not found in Pragma API", pair);
            Err(MonitoringError::Api(format!("Pair {} not found", pair)))
        }
        other => Err(MonitoringError::Api(format!(
            "Unexpected response status: {}",
            other
        ))),
    }
}

/// Queries Defillama API
/// See docs [here](https://defillama.com/pro-api/docs)
/// NOTE: it will use the PRO api if we find an api key in the `DEFILLAMA_API_KEY` env var
/// else it will use the regular api endpoint.
pub async fn query_defillama_api(
    timestamp: u64,
    coingecko_id: String,
    cache: Cache<(String, u64), CoinPricesDTO>,
) -> Result<CoinPricesDTO, MonitoringError> {
    let api_key = std::env::var("DEFILLAMA_API_KEY");

    if let Some(cached_value) = cache.get(&(coingecko_id.clone(), timestamp)).await {
        tracing::info!("Using cached defillama value..");
        return Ok(cached_value);
    }

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

    let coin_prices: CoinPricesDTO = serde_json::from_str(&response_text).map_err(|e| {
        MonitoringError::Api(format!(
            "Failed to parse JSON: {}. Response: {}",
            e, response_text
        ))
    })?;

    cache
        .insert((coingecko_id, timestamp), coin_prices.clone())
        .await;

    Ok(coin_prices)
}

pub async fn check_publisher_balance(
    publisher: String,
    publisher_address: Felt,
) -> Result<(), MonitoringError> {
    let config = get_config(None).await;
    let balance = get_on_chain_balance(publisher_address).await?;

    let network_env = &config.network_str();

    MONITORING_METRICS
        .monitoring_metrics
        .set_publisher_balance(balance, network_env, &publisher);
    Ok(())
}
