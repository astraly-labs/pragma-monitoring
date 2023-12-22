use std::{collections::HashMap, str::FromStr, sync::Arc};

use arc_swap::{ArcSwap, Guard};
use starknet::{
    core::{
        types::{BlockId, BlockTag, FieldElement, FunctionCall},
        utils::{cairo_short_string_to_felt, parse_cairo_short_string},
    },
    macros::selector,
    providers::{jsonrpc::HttpTransport, JsonRpcClient, Provider},
};
use strum::{EnumString, IntoStaticStr};
use tokio::sync::OnceCell;
use url::Url;

#[derive(Debug, Clone, EnumString, IntoStaticStr)]
pub enum NetworkName {
    #[strum(ascii_case_insensitive)]
    Mainnet,
    #[strum(ascii_case_insensitive)]
    Testnet,
}

#[derive(Debug, EnumString, IntoStaticStr)]
pub enum DataType {
    #[strum(ascii_case_insensitive)]
    Spot,
    #[strum(ascii_case_insensitive)]
    Future,
}

#[derive(Debug, Clone)]
pub struct Network {
    pub name: NetworkName,
    pub provider: Arc<JsonRpcClient<HttpTransport>>,
    pub oracle_address: FieldElement,
    pub publisher_registry_address: FieldElement,
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Config {
    spot_pairs: Vec<String>,
    future_pairs: Vec<String>,
    sources: HashMap<String, Vec<String>>, // Mapping from pair to sources
    decimals: HashMap<String, u32>,        // Mapping from pair to decimals
    publishers: Vec<String>,
    network: Network,
    indexer_url: String,
    table_names: Vec<String>,
}

/// We are using `ArcSwap` as it allow us to replace the new `Config` with
/// a new one which is required when running test cases. This approach was
/// inspired from here - https://github.com/matklad/once_cell/issues/127
pub static CONFIG: OnceCell<ArcSwap<Config>> = OnceCell::const_new();

impl Config {
    pub async fn new(config_input: ConfigInput) -> Self {
        let indexer_url =
            std::env::var("INDEXER_SERVICE_URL").expect("INDEXER_SERVICE_URL must be set");

        // Create RPC Client
        let rpc_url = std::env::var("RPC_URL").expect("RPC_URL must be set");
        let rpc_client = JsonRpcClient::new(HttpTransport::new(Url::parse(&rpc_url).unwrap()));

        let (decimals, sources, publishers, publisher_registry_address) = init_oracle_config(
            &rpc_client,
            config_input.oracle_address,
            config_input.spot_pairs.clone(),
            config_input.future_pairs.clone(),
        )
        .await;

        let table_names = match config_input.network {
            NetworkName::Mainnet => vec![
                "mainnet_spot_entry".to_string(),
                "mainnet_future_entry".to_string(),
            ],
            NetworkName::Testnet => vec!["spot_entry".to_string(), "future_entry".to_string()],
        };

        Self {
            indexer_url,
            spot_pairs: config_input.spot_pairs,
            future_pairs: config_input.future_pairs,
            sources,
            publishers,
            decimals,
            network: Network {
                name: config_input.network,
                provider: Arc::new(rpc_client),
                oracle_address: config_input.oracle_address,
                publisher_registry_address,
            },
            table_names,
        }
    }

    pub fn sources(&self) -> &HashMap<String, Vec<String>> {
        &self.sources
    }

    pub fn decimals(&self) -> &HashMap<String, u32> {
        &self.decimals
    }

    pub fn network(&self) -> &Network {
        &self.network
    }

    pub fn network_str(&self) -> &str {
        self.network.name.clone().into()
    }

    pub fn indexer_url(&self) -> &str {
        &self.indexer_url
    }

    pub fn table_names(&self) -> &Vec<String> {
        &self.table_names
    }
}

#[derive(Debug)]
pub struct ConfigInput {
    pub network: NetworkName,
    pub oracle_address: FieldElement,
    pub spot_pairs: Vec<String>,
    pub future_pairs: Vec<String>,
}

pub async fn get_config(config_input: Option<ConfigInput>) -> Guard<Arc<Config>> {
    let cfg = CONFIG
        .get_or_init(|| async {
            match config_input {
                Some(config_input) => ArcSwap::from_pointee(Config::new(config_input).await),
                None => {
                    let network = std::env::var("NETWORK").expect("NETWORK must be set");
                    let oracle_address =
                        std::env::var("ORACLE_ADDRESS").expect("ORACLE_ADDRESS must be set");
                    let spot_pairs = std::env::var("SPOT_PAIRS").expect("SPOT_PAIRS must be set");
                    let future_pairs =
                        std::env::var("FUTURE_PAIRS").expect("FUTURE_PAIRS must be set");

                    ArcSwap::from_pointee(
                        Config::new(ConfigInput {
                            network: NetworkName::from_str(&network).expect("Invalid network name"),
                            oracle_address: FieldElement::from_hex_be(&oracle_address)
                                .expect("Invalid oracle address"),
                            spot_pairs: parse_pairs(&spot_pairs),
                            future_pairs: parse_pairs(&future_pairs),
                        })
                        .await,
                    )
                }
            }
        })
        .await;
    cfg.load()
}

/// OnceCell only allows us to initialize the config once and that's how it should be on production.
/// However, when running tests, we often want to reinitialize because we want to clear the DB and
/// set it up again for reuse in new tests. By calling `config_force_init` we replace the already
/// stored config inside `ArcSwap` with the new configuration and pool settings.
#[cfg(test)]
pub async fn config_force_init(config_input: ConfigInput) {
    match CONFIG.get() {
        Some(arc) => arc.store(Arc::new(Config::new(config_input).await)),
        None => {
            get_config(Some(config_input)).await;
        }
    };
}

async fn init_oracle_config(
    rpc_client: &JsonRpcClient<HttpTransport>,
    oracle_address: FieldElement,
    spot_pairs: Vec<String>,
    future_pairs: Vec<String>,
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
    let publishers: Vec<String> = rpc_client
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
        .map(|publisher| parse_cairo_short_string(&publisher).unwrap())
        .collect();

    let publishers = publishers[1..].to_vec();

    // Exclude publishers that are not supported by the monitoring service
    let excluded_publishers = std::env::var("IGNORE_PUBLISHERS")
        .unwrap_or("".to_string())
        .split(',')
        .map(|publisher| publisher.to_string())
        .collect::<Vec<String>>();

    let publishers = publishers
        .into_iter()
        .filter(|publisher| !excluded_publishers.contains(publisher))
        .collect::<Vec<String>>();

    let mut sources: HashMap<String, Vec<String>> = HashMap::new();
    let mut decimals: HashMap<String, u32> = HashMap::new();

    let excluded_sources = std::env::var("IGNORE_SOURCES")
        .unwrap_or("".to_string())
        .split(',')
        .map(|source| source.to_string())
        .collect::<Vec<String>>();

    for pair in &[spot_pairs, future_pairs].concat() {
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

        let _future_pair_sources = rpc_client
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

        // Remove first elements of sources' arrays
        let spot_pair_sources = spot_pair_sources[1..].to_vec();
        // let future_pair_sources = future_pair_sources[1..].to_vec();

        for source in spot_pair_sources {
            let source = parse_cairo_short_string(&source).unwrap();
            if !pair_sources.contains(&source) && !excluded_sources.contains(&source) {
                pair_sources.push(source);
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
