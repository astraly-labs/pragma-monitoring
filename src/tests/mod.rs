use crate::config::{Config, ConfigInput, DataInfo, DataType, NetworkName};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use mockall::automock;
use starknet::{
    core::types::{BlockId, Felt, FunctionCall},
    providers::{JsonRpcClient, Provider, ProviderError, jsonrpc::HttpTransport},
};
use std::collections::HashMap;
use std::sync::Arc;
use std::{env, sync::Once};
use tokio::sync::OnceCell;

#[allow(dead_code)]
static INIT: Once = Once::new();

/// Initialize test environment with mock values
#[allow(dead_code)]
pub fn init_test_env() {
    INIT.call_once(|| unsafe {
        env::set_var("NETWORK", "testnet");
        env::set_var("ORACLE_ADDRESS", "0x1234567890");
        env::set_var("SPOT_PAIRS", "XSTRK/STRK,BTC/USD");
        env::set_var("FUTURE_PAIRS", "BTC-PERP/USD");
        // INDEXER_SERVICE_URL no longer needed - indexing is handled internally
        env::set_var("RPC_URL", "http://localhost:5050");
    });
}

#[automock]
#[async_trait]
pub trait TestProvider: Send + Sync {
    #[allow(dead_code)]
    async fn call(
        &self,
        request: FunctionCall,
        block_id: BlockId,
    ) -> Result<Vec<Felt>, ProviderError>;
}

// Wrapper type for providers that can be either real or mock
#[derive(Clone)]
#[allow(dead_code)]
pub enum ProviderWrapper {
    Real(Arc<JsonRpcClient<HttpTransport>>),
    Mock(Arc<MockTestProvider>),
}

impl ProviderWrapper {
    #[allow(dead_code)]
    pub async fn call(
        &self,
        request: FunctionCall,
        block_id: BlockId,
    ) -> Result<Vec<Felt>, ProviderError> {
        match self {
            ProviderWrapper::Real(provider) => provider.call(request, block_id).await,
            ProviderWrapper::Mock(provider) => provider.call(request, block_id).await,
        }
    }
}

// Modified Network struct to use ProviderWrapper
#[derive(Clone)]
#[allow(dead_code)]
pub struct TestNetwork {
    pub name: NetworkName,
    pub provider: ProviderWrapper,
    pub oracle_address: Felt,
    pub publisher_registry_address: Felt,
}

impl Clone for MockTestProvider {
    fn clone(&self) -> Self {
        MockTestProvider::new()
    }
}

#[allow(dead_code)]
static TEST_CONFIG: OnceCell<ArcSwap<Config>> = OnceCell::const_new();

#[allow(dead_code)]
pub async fn init_test_config() -> MockTestProvider {
    init_test_env();
    let mock_provider = MockTestProvider::new();
    let config = create_mock_config(mock_provider.clone());

    TEST_CONFIG
        .get_or_init(|| async { ArcSwap::new(Arc::new(config)) })
        .await;

    mock_provider
}

// Create a mock Config directly without using JsonRpcClient
#[allow(unused)]
pub fn create_mock_config(provider: MockTestProvider) -> Config {
    let mut spot_decimals = HashMap::new();
    spot_decimals.insert("XSTRK/STRK".to_string(), 8);
    spot_decimals.insert("BTC/USD".to_string(), 8);

    let mut future_decimals = HashMap::new();
    future_decimals.insert("BTC-PERP/USD".to_string(), 8);

    let mut spot_sources = HashMap::new();
    spot_sources.insert(
        "XSTRK/STRK".to_string(),
        vec!["source1".to_string(), "source2".to_string()],
    );
    spot_sources.insert(
        "BTC/USD".to_string(),
        vec!["source1".to_string(), "source2".to_string()],
    );

    let mut future_sources = HashMap::new();
    future_sources.insert("BTC-PERP/USD".to_string(), vec!["source1".to_string()]);

    let mut data_info = HashMap::new();
    data_info.insert(
        DataType::Spot,
        DataInfo {
            pairs: vec!["XSTRK/STRK".to_string(), "BTC/USD".to_string()],
            sources: spot_sources,
            decimals: spot_decimals,
            table_name: "spot_entry".to_string(),
        },
    );

    data_info.insert(
        DataType::Future,
        DataInfo {
            pairs: vec!["BTC-PERP/USD".to_string()],
            sources: future_sources,
            decimals: future_decimals,
            table_name: "future_entry".to_string(),
        },
    );

    let mut publishers = HashMap::new();
    publishers.insert("publisher1".to_string(), Felt::from_hex_unchecked("0x123"));

    Config {
        data_info,
        publishers,
        network: crate::config::Network {
            name: NetworkName::Testnet,
            provider: Arc::new(JsonRpcClient::new(HttpTransport::new(
                url::Url::parse("http://localhost:5050").unwrap(),
            ))),
            oracle_address: Felt::from_hex_unchecked("0x1234567890"),
            publisher_registry_address: Felt::from_hex_unchecked("0x5555"),
        },
        apibara_api_key: "test_api_key".to_string(),
    }
}

/// Helper function to force initialize config for tests
#[allow(dead_code)]
pub async fn set_test_config(mock_provider: &MockTestProvider) {
    let config = create_mock_config(mock_provider.clone());
    crate::config::config_force_init(ConfigInput {
        network: config.network.name.clone(),
        oracle_address: config.network.oracle_address,
        spot_pairs: config.data_info[&DataType::Spot].pairs.clone(),
        future_pairs: config.data_info[&DataType::Future].pairs.clone(),
    })
    .await;
}

mod common;

#[cfg(test)]
mod monitoring;
