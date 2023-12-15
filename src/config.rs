use std::{collections::HashMap, sync::Arc};

use starknet::{
    core::{
        types::{BlockId, BlockTag, FieldElement, FunctionCall},
        utils::cairo_short_string_to_felt,
    },
    macros::selector,
    providers::{jsonrpc::HttpTransport, JsonRpcClient, Provider},
};
use strum::EnumString;
use url::Url;

// https://blastapi.io/public-api/starknet
const DEFAULT_MAINNET_RPC_URL: &str = "https://starknet-mainnet.public.blastapi.io";
const DEFAULT_TESTNET_RPC_URL: &str = "https://starknet-sepolia.public.blastapi.io";

#[derive(Debug, EnumString)]
pub enum NetworkName {
    #[strum(ascii_case_insensitive)]
    Mainnet,
    #[strum(ascii_case_insensitive)]
    Testnet,
}

#[derive(Debug, Clone)]
pub struct Network {
    pub name: String,
    pub provider: Arc<JsonRpcClient<HttpTransport>>,
    pub oracle_address: FieldElement,
    pub publisher_registry_address: FieldElement,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub pairs: Vec<String>,
    pub sources: HashMap<String, Vec<String>>, // Mapping from pair to sources
    pub decimals: HashMap<String, u32>,        // Mapping from pair to decimals
    pub publishers: Vec<String>,
    pub network: Network,
}

impl Config {
    pub async fn new(
        network: NetworkName,
        oracle_address: FieldElement,
        pairs: Vec<String>,
    ) -> Self {
        match network {
            NetworkName::Mainnet => {
                // Create RPC Client
                let rpc_url =
                    std::env::var("MAINNET_RPC_URL").unwrap_or(DEFAULT_MAINNET_RPC_URL.to_string());
                let rpc_client =
                    JsonRpcClient::new(HttpTransport::new(Url::parse(&rpc_url).unwrap()));

                let (decimals, sources, publishers, publisher_registry_address) =
                    init_oracle_config(&rpc_client, oracle_address, pairs.clone()).await;

                Self {
                    pairs,
                    sources,
                    publishers,
                    decimals,
                    network: Network {
                        name: "mainnet".to_string(),
                        provider: Arc::new(rpc_client),
                        oracle_address,
                        publisher_registry_address,
                    },
                }
            }
            NetworkName::Testnet => {
                // Create RPC Client
                let rpc_url =
                    std::env::var("TESTNET_RPC_URL").unwrap_or(DEFAULT_TESTNET_RPC_URL.to_string());
                let rpc_client =
                    JsonRpcClient::new(HttpTransport::new(Url::parse(&rpc_url).unwrap()));

                let (decimals, sources, publishers, publisher_registry_address) =
                    init_oracle_config(&rpc_client, oracle_address, pairs.clone()).await;

                Self {
                    pairs,
                    sources,
                    publishers,
                    decimals,
                    network: Network {
                        name: "testnet".to_string(),
                        provider: Arc::new(rpc_client),
                        oracle_address,
                        publisher_registry_address,
                    },
                }
            }
        }
    }
}

async fn init_oracle_config(
    rpc_client: &JsonRpcClient<HttpTransport>,
    oracle_address: FieldElement,
    pairs: Vec<String>,
) -> (
    HashMap<String, u32>,
    HashMap<String, Vec<String>>,
    Vec<String>,
    FieldElement,
) {
    // Fetch publisher registry address
    let publisher_registry_address = *rpc_client
        .call(
            FunctionCall {
                contract_address: oracle_address,
                entry_point_selector: selector!("get_publisher_registry_address"),
                calldata: vec![],
            },
            BlockId::Tag(BlockTag::Latest),
        )
        .await
        .expect("failed to call contract")
        .first()
        .unwrap();

    // Fetch publishers
    let publishers = rpc_client
        .call(
            FunctionCall {
                contract_address: publisher_registry_address,
                entry_point_selector: selector!("get_all_publishers"),
                calldata: vec![],
            },
            BlockId::Tag(BlockTag::Latest),
        )
        .await
        .expect("failed to get publishers")
        .into_iter()
        .map(|publisher| publisher.to_string())
        .collect();

    let mut sources: HashMap<String, Vec<String>> = HashMap::new();
    let mut decimals: HashMap<String, u32> = HashMap::new();

    for pair in &pairs {
        let field_pair = cairo_short_string_to_felt(pair).unwrap();

        // Fetch decimals
        let spot_decimals = *rpc_client
            .call(
                FunctionCall {
                    contract_address: oracle_address,
                    entry_point_selector: selector!("get_decimals"),
                    calldata: vec![FieldElement::ZERO, field_pair],
                },
                BlockId::Tag(BlockTag::Latest),
            )
            .await
            .expect("failed to get decimals")
            .first()
            .unwrap();

        // TODO: support future pairs
        let _future_decimals = *rpc_client
            .call(
                FunctionCall {
                    contract_address: oracle_address,
                    entry_point_selector: selector!("get_decimals"),
                    calldata: vec![FieldElement::ONE, field_pair, FieldElement::ZERO],
                },
                BlockId::Tag(BlockTag::Latest),
            )
            .await
            .expect("failed to get decimals")
            .first()
            .unwrap();

        decimals.insert(pair.to_string(), spot_decimals.try_into().unwrap());

        // Fetch sources
        let spot_pair_sources = rpc_client
            .call(
                FunctionCall {
                    contract_address: oracle_address,
                    entry_point_selector: selector!("get_all_sources"),
                    calldata: vec![FieldElement::ZERO, field_pair],
                },
                BlockId::Tag(BlockTag::Latest),
            )
            .await
            .expect("failed to get pair sources");

        let future_pair_sources = rpc_client
            .call(
                FunctionCall {
                    contract_address: oracle_address,
                    entry_point_selector: selector!("get_all_sources"),
                    calldata: vec![FieldElement::ONE, field_pair, FieldElement::ZERO],
                },
                BlockId::Tag(BlockTag::Latest),
            )
            .await
            .expect("failed to get pair sources");

        // Store all sources for the given pair
        let mut pair_sources = Vec::new();
        for source in [spot_pair_sources, future_pair_sources].concat() {
            if !pair_sources.contains(&source.to_string()) {
                pair_sources.push(source.to_string());
            }
        }

        sources.insert(pair.to_string(), pair_sources);
    }

    (decimals, sources, publishers, publisher_registry_address)
}

/// Parse pairs from a comma separated string.
/// e.g BTC/USD,ETH/USD
pub fn parse_pairs(pairs: &str) -> Vec<String> {
    pairs
        .split(',')
        .map(|pair| pair.to_string())
        .collect::<Vec<String>>()
}
