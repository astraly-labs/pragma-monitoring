use std::sync::Arc;

use alloy::hex::FromHex;
use alloy::primitives::FixedBytes;
use arc_swap::Guard;
use bigdecimal::ToPrimitive;
use num_bigint::BigInt;
use starknet::core::types::{BlockId, BlockTag, Felt, FunctionCall};
use starknet::macros::selector;
use starknet::providers::jsonrpc::HttpTransport;
use starknet::providers::{JsonRpcClient, Provider};

use crate::config::Config;
use crate::constants::EVM_TIME_SINCE_LAST_FEED_UPDATE;
use crate::{config::get_config, error::MonitoringError};

#[derive(Clone)]
struct FeedId{
    pub feed_id : Felt,
    pub main_type: u64,
    pub variant: u64,
}

impl FeedId{
    pub fn new(feed_id: Felt) -> Self {
        Self { feed_id: feed_id, main_type: 9999, variant: 999 }
    }

    pub fn parse_feed_id(&mut self) -> Result<(), MonitoringError> {
        self.main_type = ((self.feed_id.to_bigint() & BigInt::from(65280)) / BigInt::from(256))
            .to_u64()
            .ok_or(MonitoringError::Evm("invalid main type".to_string()))?;
        self.variant = (self.feed_id.to_bigint() & BigInt::from(255))
            .to_u64()
            .ok_or(MonitoringError::Evm(format!("invalid variant for main type {}", self.main_type)))?;
        Ok(())
    }

    pub fn to_calldata(&self) -> Result<FixedBytes<32>,MonitoringError>{
        alloy::sol_types::private::FixedBytes::from_hex(self.feed_id.to_hex_string())
                    .map_err(|e| MonitoringError::Evm(e.to_string()))
    }
}

pub async fn get_all_feed_ids(config: &Guard<Arc<Config>>, client : &Arc<JsonRpcClient<HttpTransport>>) -> Result<Vec<FeedId>, MonitoringError> {
    let feed_registry_address = config
        .feed_registry_address()
        .ok_or(MonitoringError::Evm("Failed to parse feed registry address".to_string()))?;
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

    let mut feeds: Vec<FeedId> = feed_list.iter().map(|&feed| {FeedId::new(feed)}).collect();
    for feed in feeds.iter_mut() {
        feed.parse_feed_id()?;
    }

    Ok(feeds)
}



pub fn monitor_evm_chain() -> Result<(), MonitoringError>{

    Ok(())
}

pub async fn check_feed_update_state() -> Result<(), MonitoringError> {
    let config = get_config(None).await;
    let evm_config = config.evm_configs();
    
    let client = &config.network().provider;
    let feed_list = get_all_feed_ids(&config, client).await?;

    for chain in evm_config.iter() {
        let fl = feed_list.clone();
        for feed in fl.into_iter() {
            let feed_id_as_calldata=  feed.to_calldata()?;
            match feed.main_type {
                0 => match feed.variant {
                    0 => {
                        let result = chain
                            .pragma
                            .spotMedianFeeds(feed_id_as_calldata)
                            .call()
                            .await
                            .expect("failed to retrieve spot median feed");
                        EVM_TIME_SINCE_LAST_FEED_UPDATE
                            .with_label_values(&[
                                chain.name.as_str(),
                                feed.to_hex_string().as_str(),
                            ])
                            .set(result.metadata.timestamp as f64);
                    }
                    1 => {
                        let result = chain
                            .pragma
                            .perpFeeds(feed_id_as_calldata)
                            .call()
                            .await
                            .expect("failed to retrieve spot perp feed");
                        EVM_TIME_SINCE_LAST_FEED_UPDATE
                            .with_label_values(&[
                                chain.name.as_str(),
                                feed.to_hex_string().as_str(),
                            ])
                            .set(result.metadata.timestamp as f64);
                    }
                    va => {
                        return Err(MonitoringError::Evm(format!(
                            "unknown variant {va} for main type Unique"
                        )))
                    }
                },
                1 => match variant {
                    0 => {
                        let result = chain
                            .pragma
                            .twapFeeds(feed_id_as_calldata)
                            .call()
                            .await
                            .expect("failed to retrieve twap feed");
                        EVM_TIME_SINCE_LAST_FEED_UPDATE
                            .with_label_values(&[
                                chain.name.as_str(),
                                feed.to_hex_string().as_str(),
                            ])
                            .set(result.metadata.timestamp as f64);
                    }
                    va => {
                        return Err(MonitoringError::Evm(format!(
                            "unknown variant {va} for main type Twap"
                        )))
                    }
                },
                2 => match variant {
                    0 => {
                        let result = chain
                            .pragma
                            .twapFeeds(feed_id_as_calldata)
                            .call()
                            .await
                            .expect("failed to retrieve twap feed");
                        EVM_TIME_SINCE_LAST_FEED_UPDATE
                            .with_label_values(&[
                                chain.name.as_str(),
                                feed.to_hex_string().as_str(),
                            ])
                            .set(result.metadata.timestamp as f64);
                    }
                    va => {
                        return Err(MonitoringError::Evm(format!(
                            "unknown variant {va} for main type Realized Volatility"
                        )))
                    }
                },
                va => {
                    return Err(MonitoringError::Evm(format!(
                        "unknown variant {va} for main type Unique"
                    )))
                }
            };
        }
    }

    Ok(())
}
