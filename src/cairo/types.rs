use alloy::{hex::FromHex, primitives::FixedBytes};
use bigdecimal::ToPrimitive;
use num_bigint::BigInt;
use starknet::core::types::Felt;

use crate::{config::EvmConfig, error::MonitoringError};

#[derive(Clone)]
pub struct Feed {
    feed_id: FeedId,
    feed_type: FeedType,
}

impl TryFrom<FeedId> for Feed {
    type Error = MonitoringError;

    fn try_from(feed_id: FeedId) -> Result<Self, Self::Error> {
        let feed_type = FeedType::try_from(feed_id.0)?;
        Ok(Self { feed_id, feed_type })
    }
}

impl Feed {
    pub async fn get_latest_update_timestamp(
        self,
        chain: &EvmConfig,
    ) -> Result<u64, MonitoringError> {
        match self.feed_type {
            FeedType::Unique(unique_variant) => {
                unique_variant
                    .get_latest_update_timestamp(chain, self.feed_id)
                    .await
            }
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum FeedType {
    Unique(UniqueVariant),
}

#[derive(Clone, PartialEq, Debug)]
pub enum UniqueVariant {
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
    async fn get_latest_update_timestamp(
        &self,
        chain: &EvmConfig,
        feed_id: FeedId,
    ) -> Result<u64, MonitoringError>;
}

impl DataFetcher for UniqueVariant {
    async fn get_latest_update_timestamp(
        &self,
        chain: &EvmConfig,
        feed_id: FeedId,
    ) -> Result<u64, MonitoringError> {
        match self {
            UniqueVariant::SpotMedian => {
                let result = chain
                    .pragma
                    .spotMedianFeeds(feed_id.to_calldata()?)
                    .call()
                    .await
                    .expect("failed to retrieve spot median feed");
                Ok(result.metadata.timestamp)
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
