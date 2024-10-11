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
enum FeedType {
    Unique(UniqueVariant),
    Twap(TwapVariant),
}

impl FeedType {
    async fn update_data(&self, chain: &EvmConfig, feed_id: FeedId) -> Result<(), MonitoringError> {
        match self {
            FeedType::Unique(unique_variant) => unique_variant.update_data(chain, feed_id).await,
            FeedType::Twap(twap_variant) => twap_variant.update_data(chain, feed_id).await,
        }
    }
}

#[derive(Clone)]
enum UniqueVariant {
    SpotMedian,
    PerpMedian,
}

#[derive(Clone)]
enum TwapVariant {
    SpotTwap = 0,
}

impl TryFrom<Felt> for FeedType {
    type Error = MonitoringError;

    fn try_from(feed_id: Felt) -> Result<Self, Self::Error> {
        let feed_type = ((feed_id.to_bigint() & BigInt::from(65280)) / BigInt::from(256))
            .to_u64()
            .ok_or(MonitoringError::Evm("invalid main type".to_string()))?;
        let variant =
            (feed_id.to_bigint() & BigInt::from(255))
                .to_u64()
                .ok_or(MonitoringError::Evm(format!(
                    "invalid variant for main type {}",
                    feed_type
                )))?;
        match feed_type {
            0 => match variant {
                0 => Ok(FeedType::Unique(UniqueVariant::SpotMedian)),
                1 => Ok(FeedType::Unique(UniqueVariant::PerpMedian)),
                _ => unreachable!(),
            },
            1 => match variant {
                0 => Ok(FeedType::Twap(TwapVariant::SpotTwap)),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
}

pub trait Variant: Clone + Send + Sync {
    async fn update_data(&self, chain: &EvmConfig, feed_id: FeedId) -> Result<(), MonitoringError>;
}

impl Variant for UniqueVariant {
    async fn update_data(&self, chain: &EvmConfig, feed_id: FeedId) -> Result<(), MonitoringError> {
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
            UniqueVariant::PerpMedian => {
                //TODO: must be changed with real fn
                let result = chain
                    .pragma
                    .perpFeeds(feed_id.to_calldata()?)
                    .call()
                    .await
                    .expect("failed to retrieve spot perp feed");
                EVM_TIME_SINCE_LAST_FEED_UPDATE
                    .with_label_values(&[chain.name.as_str(), feed_id.to_hex_string().as_str()])
                    .set(get_time_diff(result.metadata.timestamp));
                Ok(())
            }
        }
    }
}

impl Variant for TwapVariant {
    async fn update_data(&self, chain: &EvmConfig, feed_id: FeedId) -> Result<(), MonitoringError> {
        match self {
            TwapVariant::SpotTwap => {
                let result = chain
                    .pragma
                    .twapFeeds(feed_id.to_calldata()?)
                    .call()
                    .await
                    .expect("failed to retrieve twap feed");
                EVM_TIME_SINCE_LAST_FEED_UPDATE
                    .with_label_values(&[chain.name.as_str(), feed_id.to_hex_string().as_str()])
                    .set(get_time_diff(result.metadata.timestamp));
                Ok(())
            }
        }
    }
}

#[derive(Clone)]
pub struct FeedId {
    pub feed_id: Felt,
}

impl FeedId {
    pub fn new(feed_id: Felt) -> Self {
        Self { feed_id }
    }

    pub fn to_calldata(&self) -> Result<FixedBytes<32>, MonitoringError> {
        alloy::sol_types::private::FixedBytes::from_hex(self.feed_id.to_hex_string())
            .map_err(|e| MonitoringError::Evm(e.to_string()))
    }

    pub fn to_hex_string(&self) -> String {
        self.feed_id.to_hex_string()
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
        for feed in fl.into_iter() {
            let feed_handler = FeedType::try_from(feed.feed_id)?;
            feed_handler.update_data(chain, feed).await?;
        }
    }

    Ok(())
}
