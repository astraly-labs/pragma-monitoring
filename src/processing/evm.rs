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

pub async fn get_all_feed_ids(config: &Guard<Arc<Config>>, client : &Arc<JsonRpcClient<HttpTransport>>) -> Vec<Felt> {
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
    feed_list
}

pub async fn check_feed_update_state() -> Result<(), MonitoringError> {
    let config = get_config(None).await;
    let evm_config = config.evm_configs();
    
    let client = &config.network().provider;
    let feed_list = get_all_feed_ids(&config, client).await;

    for chain in evm_config.iter() {
        let fl = feed_list.clone();
        for feed in fl.into_iter() {
            let main_type = ((feed.to_bigint() & BigInt::from(65280)) / BigInt::from(256))
                .to_u64()
                .expect("invalid main type");
            let variant = (feed.to_bigint() & BigInt::from(255))
                .to_u64()
                .expect("invalid variant");
            let feed_id_as_calldata: FixedBytes<32> =
                alloy::sol_types::private::FixedBytes::from_hex(feed.to_hex_string())
                    .expect("failed to parse felt id");
            match main_type {
                0 => match variant {
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
