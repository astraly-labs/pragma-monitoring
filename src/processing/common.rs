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
use std::time::Duration;

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct IndexerServerStatus {
    pub status: Option<i32>,
    pub starting_block: Option<u64>,
    pub current_block: Option<u64>,
    pub head_block: Option<u64>,
    #[serde(rename = "reason")]
    pub reason_: Option<String>,
}

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct PragmaDataDTO {
    pub price: String,
    pub decimals: u32,
    pub timestamp: i64,
    pub num_sources_aggregated: u32,
}

/// Checks if indexers of the given data type are still syncing
/// Now checks the internal indexer status
pub async fn data_is_syncing(data_type: &DataType) -> Result<bool, MonitoringError> {
    use crate::indexing::status::INTERNAL_INDEXER_TRACKER;

    // Check if internal indexer is healthy
    let is_healthy = INTERNAL_INDEXER_TRACKER.is_healthy().await;

    if !is_healthy {
        tracing::warn!("[{data_type}] Internal indexer is not healthy");
        return Ok(true); // Still syncing if not healthy
    }

    // Check if we have recent activity
    let status = INTERNAL_INDEXER_TRACKER.get_status().await;
    if let Some(last_activity) = status.last_activity
        && last_activity.elapsed() > Duration::from_secs(300)
    {
        // 5 minutes
        tracing::warn!("[{data_type}] Internal indexer has no recent activity");
        return Ok(true); // Still syncing if no recent activity
    }

    Ok(false) // Synced
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
/// Since we now handle indexing internally, this always returns false (synced)
#[allow(unused)]
pub async fn is_syncing(table_name: &str) -> Result<bool, MonitoringError> {
    // With integrated indexing, we don't need to check external indexer status
    // The indexing is handled internally by the monitoring service
    Ok(false)
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
        _ => {
            tracing::error!("Invalid network environment: {}", network_env);
            return Err(MonitoringError::Api(format!(
                "Invalid network environment: {}",
                network_env
            )));
        }
    };
    // Set headers
    let mut headers = HeaderMap::new();
    let api_key = match std::env::var("PRAGMA_API_KEY") {
        Ok(key) => key,
        Err(e) => {
            tracing::error!(
                "PRAGMA_API_KEY environment variable is required but not set: {:?}",
                e
            );
            return Err(MonitoringError::Api(
                "PRAGMA_API_KEY must be set".to_string(),
            ));
        }
    };
    headers.insert(
        HeaderName::from_static("x-api-key"),
        match HeaderValue::from_str(&api_key) {
            Ok(value) => value,
            Err(e) => {
                tracing::error!("Failed to parse PRAGMA_API_KEY as header value: {:?}", e);
                return Err(MonitoringError::Api(format!(
                    "Failed to parse API key: {}",
                    e
                )));
            }
        },
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| MonitoringError::Api(format!("Failed to build HTTP client: {}", e)))?;

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

    // Create client with timeout
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| MonitoringError::Api(format!("Failed to build HTTP client: {}", e)))?;

    let response = client
        .get(&request_url)
        .send()
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

/// Check the balance of a publisher
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
