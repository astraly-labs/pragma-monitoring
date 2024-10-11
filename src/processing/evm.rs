use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use alloy::hex::FromHex;
use alloy::primitives::FixedBytes;
use arc_swap::Guard;
use bigdecimal::ToPrimitive;
use num_bigint::BigInt;
use starknet::core::types::{BlockId, BlockTag, Felt, FunctionCall};
use starknet::macros::selector;
use starknet::providers::jsonrpc::HttpTransport;
use starknet::providers::{JsonRpcClient, Provider};

use crate::config::{Config, EvmConfig};
use crate::constants::EVM_TIME_SINCE_LAST_FEED_UPDATE;
use crate::{config::get_config, error::MonitoringError};

#[derive(Clone)]
struct Feed {
    feed_id: FeedId,
    feed_type: FeedType,
}

impl TryFrom<FeedId> for Feed {
    type Error = MonitoringError;

    fn try_from(feed_id: FeedId) -> Result<Self, Self::Error>{
        let feed_type =  FeedType::try_from(feed_id.0)?;
        Ok(Self {
            feed_id,
            feed_type: feed_type,
        })
    }
}

impl Feed {
    async fn get_latest_data(self, chain: &EvmConfig) -> Result<(), MonitoringError> {
        match self.feed_type {
            FeedType::Unique(unique_variant) => unique_variant.get_latest_data(chain, self.feed_id).await,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
enum FeedType {
    Unique(UniqueVariant),
}

#[derive(Clone, PartialEq, Debug)]
enum UniqueVariant {
    SpotMedian,
}

// let main_type = (id & 0xFF00) / 0x100;
// let variant = id & 0x00FF;

impl TryFrom<Felt> for FeedType {
    type Error = MonitoringError;

    fn try_from(feed_id: Felt) -> Result<Self, Self::Error> {
        // Retrieving FeedIdType by dividing as bitshifting on bigint is impossible
        let feed_id_type = feed_id.to_bigint() / BigInt::from(2).pow(216);
        let feed_type = ((feed_id_type.clone() & BigInt::from(65280)) / BigInt::from(256))
            .to_u64()
            .ok_or(MonitoringError::Evm("invalid main type".to_string()))?;
        let variant = (feed_id_type & BigInt::from(255))
            .to_u64()
            .ok_or(MonitoringError::Evm(format!(
                "invalid variant for main type {}",
                feed_type
            )))?;
        match feed_type {
            0 => match variant {
                0 => Ok(FeedType::Unique(UniqueVariant::SpotMedian)),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
}

pub trait DataFetcher: Clone + Send + Sync {
    async fn get_latest_data(&self, chain: &EvmConfig, feed_id: FeedId) -> Result<(), MonitoringError>;
}

impl DataFetcher for UniqueVariant {
    async fn get_latest_data(&self, chain: &EvmConfig, feed_id: FeedId) -> Result<(), MonitoringError> {
        match self {
            UniqueVariant::SpotMedian => {
                let result = chain
                    .pragma
                    .spotMedianFeeds(feed_id.to_calldata()?)
                    .call()
                    .await
                    .expect("failed to retrieve spot median feed");
                EVM_TIME_SINCE_LAST_FEED_UPDATE
                    .with_label_values(&[chain.name.as_str(), feed_id.to_hex_string().as_str()])
                    .set(get_time_diff(result.metadata.timestamp));
                Ok(())
            }
        }
    }
}


#[derive(Clone)]
pub struct FeedId(Felt);

impl FeedId {
    pub fn new(feed_id: Felt) -> Self {
        Self(feed_id)
    }

    pub fn to_calldata(&self) -> Result<FixedBytes<32>, MonitoringError> {
        alloy::sol_types::private::FixedBytes::from_hex(self.to_hex_string())
            .map_err(|e| MonitoringError::Evm(e.to_string()))
    }

    pub fn to_hex_string(&self) -> String {
        self.0.to_hex_string()
    }
}

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

fn get_time_diff(timestamp: u64) -> f64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs_f64();

    now - timestamp as f64
}

pub async fn check_feed_update_state() -> Result<(), MonitoringError> {
    let config = get_config(None).await;
    let evm_config = config.evm_configs();

    let client = &config.network().provider;
    let feed_list = get_all_feed_ids(&config, client).await?;

    for chain in evm_config.iter() {
        let fl = feed_list.clone();
        for feedid in fl.into_iter() {
            let feed = Feed::try_from(feedid)?;
            feed.get_latest_data(chain).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use starknet::core::types::Felt;

    use crate::processing::evm::UniqueVariant;

    use super::FeedType;

    #[test]
    fn feed_id_parse_test() {
        let unique_spot_median_feedid = Felt::from_hex("0x4254432f555344").unwrap();
        assert_eq!(
            FeedType::try_from(unique_spot_median_feedid).unwrap(),
            FeedType::Unique(UniqueVariant::SpotMedian)
        );
    }
}
