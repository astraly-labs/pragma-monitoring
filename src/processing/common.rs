use serde::{Deserialize, Serialize};
use starknet::providers::{jsonrpc::HttpTransport, JsonRpcClient, Provider};

use crate::{config::get_config, error::MonitoringError};

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

    // TODO: add real values
    let sink_ids = vec!["0", "1", "2", "3"];

    let statuses = futures::future::join_all(
        sink_ids
            .iter()
            .map(|sink_id| get_sink_status(sink_id, config.indexer_url())),
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
            }
            Err(e) => return Err(e), // If any error, return immediately
        }
    }

    // Check if any indexer is still syncing
    Ok(blocks_left_vec
        .iter()
        .any(|blocks_left| blocks_left.is_some()))
}

async fn get_sink_status(
    sink_id: &str,
    base_url: &str,
) -> Result<IndexerServerStatus, MonitoringError> {
    let request_url = format!(
        "{base_url}/status/{sink_id}",
        base_url = base_url,
        sink_id = sink_id
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
