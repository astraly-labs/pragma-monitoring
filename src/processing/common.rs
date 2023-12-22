use serde::{Deserialize, Serialize};
use starknet::providers::{jsonrpc::HttpTransport, JsonRpcClient, Provider};

use crate::{
    config::{get_config, DataType},
    constants::INDEXER_BLOCKS_LEFT,
    error::MonitoringError,
};

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

    let status = get_sink_status(table_name, config.indexer_url()).await?;

    let provider = &config.network().provider;

    let blocks_left = blocks_left(&status, provider).await?;

    // Update the prometheus metric
    INDEXER_BLOCKS_LEFT
        .with_label_values(&[(&config.network().name).into(), &data_type.to_string()])
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
