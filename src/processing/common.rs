use serde::{Deserialize, Serialize};
use starknet::providers::{jsonrpc::HttpTransport, JsonRpcClient, Provider};

use crate::{config::get_config, constants::INDEXER_BLOCKS_LEFT, error::MonitoringError};

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct IndexerServerStatus {
    pub status: i32,
    pub starting_block: Option<u64>,
    pub current_block: Option<u64>,
    pub head_block: Option<u64>,
    #[serde(rename = "reason")]
    pub reason_: Option<String>,
}

/// Checks if all the indexers are synced
/// Returns true if any of the indexers is still syncing
pub async fn is_syncing() -> Result<bool, MonitoringError> {
    let config = get_config(None).await;

    // TODO: Add this to the config
    let table_names = vec![
        "spot_entry",
        "mainnet_spot_entry",
        "future_entry",
        "mainnet_future_entry",
    ];

    let statuses = futures::future::join_all(
        table_names
            .iter()
            .map(|table_name| get_sink_status(table_name, config.indexer_url())),
    )
    .await;

    let provider = &config.network().provider;

    // Process only the successful statuses
    // Initialize a vector to store the blocks left for each status
    let mut blocks_left_vec = Vec::new();

    // Iterate over statuses and process each
    for status in statuses {
        match status {
            Ok(status) => {
                let blocks_left = blocks_left(&status, provider).await?;
                blocks_left_vec.push(blocks_left);

                // Update the prometheus metric
                INDEXER_BLOCKS_LEFT
                    .with_label_values(&[(&config.network().name).into(), "indexer"])
                    .set(blocks_left.unwrap_or(0) as i64);
            }
            Err(e) => return Err(e), // If any error, return immediately
        }
    }

    // Check if any indexer is still syncing
    Ok(blocks_left_vec
        .iter()
        .any(|blocks_left| blocks_left.is_some()))
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
    let block_n = sink_status.head_block.unwrap();
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
