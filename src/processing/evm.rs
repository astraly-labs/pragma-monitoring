use arc_swap::Guard;
use starknet::core::types::{BlockId, BlockTag, FunctionCall};
use starknet::macros::selector;
use starknet::providers::jsonrpc::HttpTransport;
use starknet::providers::{JsonRpcClient, Provider};
use std::sync::Arc;

use crate::cairo::types::{Feed, FeedId};
use crate::config::Config;
use crate::constants::EVM_TIME_SINCE_LAST_FEED_UPDATE;
use crate::utils::get_time_diff;
use crate::{config::get_config, error::MonitoringError};

pub async fn get_all_feed_ids(
    config: &Guard<Arc<Config>>,
    client: &Arc<JsonRpcClient<HttpTransport>>,
) -> Result<Vec<FeedId>, MonitoringError> {
    let feed_registry_address = config.feed_registry_address().ok_or(MonitoringError::Evm(
        "Failed to parse feed registry address".to_string(),
    ))?;
    let mut feed_list = client
        .call(
            FunctionCall {
                contract_address: feed_registry_address,
                entry_point_selector: selector!("get_all_feeds"),
                calldata: vec![],
            },
            BlockId::Tag(BlockTag::Pending),
        )
        .await
        .map_err(|e| MonitoringError::Evm(e.to_string()))?;
    feed_list.remove(0);

    let feedids = feed_list.iter().map(|elem| FeedId::new(*elem)).collect();

    Ok(feedids)
}

pub async fn check_feed_update_state() -> Result<(), MonitoringError> {
    let config = get_config(None).await;
    let evm_config = config.evm_configs();

    let client = &config.network().provider;
    let feed_list = get_all_feed_ids(&config, client).await?;

    for chain in evm_config.iter() {
        let fl = feed_list.clone();
        for feedid in fl.into_iter() {
            let feed = Feed::try_from(feedid.clone())?;
            let latest_data = feed.get_latest_data(chain).await?;
            EVM_TIME_SINCE_LAST_FEED_UPDATE
                    .with_label_values(&[chain.name.as_str(), feedid.to_hex_string().as_str()])
                    .set(get_time_diff(latest_data));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use starknet::core::types::Felt;

    use crate::cairo::types::{FeedType, UniqueVariant};

    #[test]
    fn feed_id_parse_test() {
        let unique_spot_median_feedid = Felt::from_hex("0x4254432f555344").unwrap();
        assert_eq!(
            FeedType::try_from(unique_spot_median_feedid).unwrap(),
            FeedType::Unique(UniqueVariant::SpotMedian)
        );
    }
}
