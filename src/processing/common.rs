use crate::monitoring::balance::get_on_chain_balance;
use crate::monitoring::metrics::MONITORING_METRICS;
use crate::monitoring::price_deviation::CoinPricesDTO;
use crate::{
    config::{DataType, get_config},
    error::MonitoringError,
};
use moka::future::Cache;
use starknet::core::types::Felt;
use std::time::Duration;

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

    if !status.is_synced {
        tracing::warn!("[{data_type}] Internal indexer not yet synced");
        return Ok(true);
    }

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
            tracing::debug!("🔄 [{data_type}] Still syncing...");
            false
        }
        Ok(false) => {
            tracing::debug!("✅ [{data_type}] Synced");
            true
        }
        Err(e) => {
            tracing::error!("❌ [{data_type}] Failed to check sync status: {:?}", e);
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

/// Queries Defillama API
/// See docs [here](https://defillama.com/pro-api/docs)
/// NOTE: it will use the PRO api if we find an api key in the `DEFILLAMA_API_KEY` env var
/// else it will use the regular api endpoint.
pub async fn query_defillama_api(
    timestamp: u64,
    coingecko_id: String,
    cache: Cache<(String, u64), CoinPricesDTO>,
) -> Result<CoinPricesDTO, MonitoringError> {
    // Check for non-empty API key
    let api_key = std::env::var("DEFILLAMA_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty());

    if let Some(cached_value) = cache.get(&(coingecko_id.clone(), timestamp)).await {
        return Ok(cached_value);
    }

    let request_url = if let Some(api_key) = api_key {
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
