use crate::monitoring::data_providers_balance::data_provider_balance;
use crate::{
    config::{get_config, DataType},
    constants::{DATA_PROVIDER_BALANCE_DEF, INDEXER_BLOCKS_LEFT},
    error::MonitoringError,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use starknet::providers::{jsonrpc::HttpTransport, JsonRpcClient, Provider};
#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct IndexerServerStatus {
    pub status: i32,
    pub starting_block: Option<u64>,
    pub current_block: Option<u64>,
    pub head_block: Option<u64>,
    #[serde(rename = "reason")]
    pub reason_: Option<String>,
}

/// Checks if indexers of the given data type are still syncing
/// Returns true if any of the indexers is still syncing
pub async fn is_syncing(data_type: &DataType) -> Result<bool, MonitoringError> {
    let config = get_config(None).await;

    let table_name = config.table_name(data_type.clone());

    let status = get_sink_status(&table_name, config.indexer_url()).await?;

    let provider = &config.network().provider;

    let blocks_left = blocks_left(&status, provider).await?;

    // Update the prometheus metric
    INDEXER_BLOCKS_LEFT
        .with_label_values(&[
            (&config.network().name).into(),
            &data_type.to_string().to_ascii_lowercase(),
        ])
        .set(blocks_left.unwrap_or(0) as i64);

    // Check if any indexer is still syncing
    Ok(blocks_left.is_some())
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
    let request_url = format!(
        "{base_url}/status/table/{table_name}",
        base_url = base_url,
        table_name = table_name
    );

    let response = reqwest::get(&request_url)
        .await
        .map_err(|e| MonitoringError::Api(e.to_string()))?;

    let status = response
        .json::<IndexerServerStatus>()
        .await
        .map_err(|e| MonitoringError::Api(e.to_string()))?;

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
    // Safe to unwrap as defined by apibara spec
    let block_n = sink_status.current_block.unwrap();
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
) -> Result<PragmaDataDTO, MonitoringError> {
    let request_url = match network_env {
        "Testnet" => format!(
            "https://api.dev.pragma.build/node/v1/data/{pair}?routing=true",
            pair = pair,
        ),
        "Mainnet" => format!(
            "https://api.prod.pragma.build/node/v1/data/{pair}?routing=true",
            pair = pair,
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
        .get(request_url)
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
        other => Err(MonitoringError::Api(format!(
            "Unexpected response status: {}",
            other
        ))),
    }
}

pub async fn check_data_provider_balance(
    dp_name: String,
    dp_address: FieldElement,
) -> Result<bool, MonitoringError> {
    let config = get_config(None).await;
    let balance = data_provider_balance(dp_address).await;

    // Update the prometheus metric
    DATA_PROVIDER_BALANCE_DEF
        .with_label_values(&[
            (&config.network().name).into(),
            &dp_name.to_ascii_lowercase(),
        ])
        .set(balance.unwrap_or(0.0) as f64);
    Ok(true)
}
