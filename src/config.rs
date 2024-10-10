use std::{
    collections::HashMap,
    fs::File,
    path::PathBuf,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use alloy::{hex::FromHex, primitives::Address, providers::ProviderBuilder};
use arc_swap::{ArcSwap, Guard};
use serde::Deserialize;
use starknet::{
    core::{
        types::{BlockId, BlockTag, Felt, FunctionCall},
        utils::{cairo_short_string_to_felt, parse_cairo_short_string},
    },
    macros::selector,
    providers::{jsonrpc::HttpTransport, JsonRpcClient, Provider},
};
use strum::{Display, EnumString, IntoStaticStr};
use tokio::sync::OnceCell;
use url::Url;

use crate::{
    constants::{
        CONFIG_UPDATE_INTERVAL, LONG_TAIL_ASSETS, LONG_TAIL_ASSET_THRESHOLD, LOW_SOURCES_THRESHOLD,
    },
    evm::pragma::{Pragma, PragmaContract},
    utils::{is_long_tail_asset, try_felt_to_u32},
};

#[derive(Debug, Clone, EnumString, IntoStaticStr)]
pub enum NetworkName {
    #[strum(ascii_case_insensitive)]
    Mainnet,
    #[strum(ascii_case_insensitive)]
    Testnet,
    #[strum(ascii_case_insensitive, serialize = "pragma-devnet")]
    PragmaDevnet,
}

#[derive(Debug, EnumString, IntoStaticStr, PartialEq, Eq, Hash, Clone, Display)]
pub enum DataType {
    #[strum(ascii_case_insensitive)]
    Spot,
    #[strum(ascii_case_insensitive)]
    Future,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Network {
    pub name: NetworkName,
    pub provider: Arc<JsonRpcClient<HttpTransport>>,
    pub oracle_address: Felt,
    pub vrf_address: Felt,
    pub publisher_registry_address: Felt,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DataInfo {
    pub pairs: Vec<String>,
    pub sources: HashMap<String, Vec<String>>,
    pub decimals: HashMap<String, u32>,
    pub table_name: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct EvmConfig {
    pub name: String,
    pub pragma: PragmaContract,
}

impl EvmConfig {
    pub fn new(network_name: String, mut contract_adress: String, rpc_url: Url) -> Self {
        let provider = ProviderBuilder::new()
            .with_recommended_fillers()
            .on_http(rpc_url);
        if contract_adress.starts_with("0x") {
            contract_adress = contract_adress.replace("0x", "");
        }
        let address = Address::from_hex(contract_adress)
            .expect("Invalid Pragma Address specified. Make sure it is an hexadecimal address.");
        let pragma = Pragma::new(address, provider);
        Self {
            name: network_name,
            pragma,
        }
    }
}

#[derive(Debug, Deserialize)]
struct EvmChainConfig {
    rpc_url: String,
    contract_address: String,
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Config {
    data_info: HashMap<DataType, DataInfo>,
    publishers: HashMap<String, Felt>,
    network: Network,
    indexer_url: String,
    evm_config: Vec<EvmConfig>,
    feed_registry_address: Option<Felt>,
}

/// We are using `ArcSwap` as it allow us to replace the new `Config` with
/// a new one which is required when running test cases. This approach was
/// inspired from here - https://github.com/matklad/once_cell/issues/127
#[allow(unused)]
pub static CONFIG: OnceCell<ArcSwap<Config>> = OnceCell::const_new();

#[allow(unused)]
impl Config {
    pub async fn new(config_input: ConfigInput) -> Self {
        let indexer_url =
            std::env::var("INDEXER_SERVICE_URL").expect("INDEXER_SERVICE_URL must be set");

        // Create RPC Client
        let rpc_url = std::env::var("RPC_URL").expect("RPC_URL must be set");
        let rpc_client = JsonRpcClient::new(HttpTransport::new(Url::parse(&rpc_url).unwrap()));

        let (publishers, publisher_registry_address) =
            init_publishers(&rpc_client, config_input.oracle_address).await;

        let spot_info = init_spot_config(
            &rpc_client,
            config_input.oracle_address,
            config_input.spot_pairs.clone(),
        )
        .await;

        let future_info = init_future_config(
            &rpc_client,
            config_input.oracle_address,
            config_input.future_pairs.clone(),
        )
        .await;

        let data_info = vec![(DataType::Spot, spot_info), (DataType::Future, future_info)]
            .into_iter()
            .collect::<HashMap<DataType, DataInfo>>();

        let file = File::open(config_input.config_path).expect("cannot open config file");
        let config: HashMap<String, EvmChainConfig> =
            serde_yaml::from_reader(file).expect("failed to parse config");

        let evm_config = config
            .into_iter()
            .filter_map(|(network_name, chain_config)| {
                if !chain_config.contract_address.is_empty() {
                    match Url::parse(&chain_config.rpc_url) {
                        Ok(rpc_url) => Some(Ok(EvmConfig::new(
                            network_name,
                            chain_config.contract_address,
                            rpc_url,
                        ))),
                        Err(e) => Some(Err(
                            format!("Invalid URL for {}: {}", network_name, e).into()
                        )),
                    }
                } else {
                    None
                }
            })
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()
            .expect("failed to retrieve configs");

        Self {
            indexer_url,
            publishers,
            data_info,
            network: Network {
                name: config_input.network,
                provider: Arc::new(rpc_client),
                oracle_address: config_input.oracle_address,
                vrf_address: config_input.vrf_address,
                publisher_registry_address,
            },
            evm_config,
            feed_registry_address: config_input.feed_registry_address,
        }
    }

    pub async fn create_from_env() -> Config {
        let network = std::env::var("NETWORK").expect("NETWORK must be set");
        let oracle_address = std::env::var("ORACLE_ADDRESS").expect("ORACLE_ADDRESS must be set");
        let vrf_address = std::env::var("VRF_ADDRESS").expect("VRF_ADDRESS must be set");
        let spot_pairs = std::env::var("SPOT_PAIRS").expect("SPOT_PAIRS must be set");
        let future_pairs = std::env::var("FUTURE_PAIRS").expect("FUTURE_PAIRS must be set");
        let evm_config = std::env::var("EVM_CONFIG_PATH").expect("EVM_CONFIG_PATH must be set");

        let feed_registry_address = match network.starts_with("pragma") {
            true => {
                let env_var = std::env::var("FEED_REGISTRY_ADDRESS")
                    .expect("FEED_REGISTRY_ADDRESS must be set for pragma chains");
                Some(
                    Felt::from_hex(env_var.as_str())
                        .expect("failed to parse feed registry address"),
                )
            }
            false => None,
        };

        Config::new(ConfigInput {
            network: NetworkName::from_str(&network).expect("Invalid network name"),
            oracle_address: Felt::from_hex_unchecked(&oracle_address),
            vrf_address: Felt::from_hex_unchecked(&vrf_address),
            spot_pairs: parse_pairs(&spot_pairs),
            future_pairs: parse_pairs(&future_pairs),
            config_path: PathBuf::from_str(&evm_config).expect("invalid evm config path"),
            feed_registry_address,
        })
        .await
    }

    pub fn sources(&self, data_type: DataType) -> &HashMap<String, Vec<String>> {
        &self.data_info.get(&data_type).unwrap().sources
    }

    pub fn decimals(&self, data_type: DataType) -> &HashMap<String, u32> {
        &self.data_info.get(&data_type).unwrap().decimals
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

    pub fn table_name(&self, data_type: DataType) -> String {
        let table_name = &self.data_info.get(&data_type).unwrap().table_name;
        match self.network.name {
            NetworkName::Mainnet => format!("mainnet_{}", table_name),
            NetworkName::Testnet => table_name.to_string(),
            NetworkName::PragmaDevnet => format!("pragma_devnet_{}", table_name),
        }
    }

    pub fn all_publishers(&self) -> &HashMap<String, Felt> {
        &self.publishers
    }

    pub fn evm_configs(&self) -> &[EvmConfig] {
        &self.evm_config
    }

    pub fn feed_registry_address(&self) -> &Option<Felt> {
        &self.feed_registry_address
    }

    /// Check if the configuration is set for a Pragma Chain
    pub fn is_pragma_chain(&self) -> bool {
        matches!(self.network.name, NetworkName::PragmaDevnet)
    }
}

#[derive(Debug, Clone)]
pub struct ConfigInput {
    pub network: NetworkName,
    pub oracle_address: Felt,
    pub vrf_address: Felt,
    pub spot_pairs: Vec<String>,
    pub future_pairs: Vec<String>,
    pub config_path: PathBuf,
    pub feed_registry_address: Option<Felt>,
}

#[allow(unused)]
pub async fn get_config(config_input: Option<ConfigInput>) -> Guard<Arc<Config>> {
    let cfg = CONFIG
        .get_or_init(|| async {
            match config_input {
                Some(config_input) => ArcSwap::from_pointee(Config::new(config_input).await),
                None => ArcSwap::from_pointee(Config::create_from_env().await),
            }
        })
        .await;
    cfg.load()
}

/// This function is used to periodically update the configuration settings
/// from the environment variables. This is useful when we want to update the
/// configuration settings without restarting the service.
#[allow(unused)]
pub async fn periodic_config_update() {
    let interval = Duration::from_secs(CONFIG_UPDATE_INTERVAL); // Set the update interval as needed (3 hours in this example)

    let mut next_update = Instant::now() + interval;

    loop {
        log::info!("[CONFIG] Updating config...");

        let new_config = Config::create_from_env().await;
        let updated_config = ArcSwap::from_pointee(new_config.clone());

        let current_config_cell = CONFIG.get_or_init(|| async { updated_config }).await;

        // Store the updated config in the ArcSwap
        current_config_cell.store(new_config.into());

        tokio::time::sleep_until(next_update.into()).await;

        next_update += interval;
    }
}

/// OnceCell only allows us to initialize the config once and that's how it should be on production.
/// However, when running tests, we often want to reinitialize because we want to clear the DB and
/// set it up again for reuse in new tests. By calling `config_force_init` we replace the already
/// stored config inside `ArcSwap` with the new configuration and pool settings.
#[allow(unused)]
#[cfg(test)]
pub async fn config_force_init(config_input: ConfigInput) {
    match CONFIG.get() {
        Some(arc) => arc.store(Arc::new(Config::new(config_input).await)),
        None => {
            get_config(Some(config_input)).await;
        }
    };
}

async fn init_publishers(
    rpc_client: &JsonRpcClient<HttpTransport>,
    oracle_address: Felt,
) -> (HashMap<String, Felt>, Felt) {
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

    let mut publishers_map: HashMap<String, Felt> = HashMap::new();
    for publisher in publishers {
        let field_publisher =
            cairo_short_string_to_felt(&publisher).expect("Failed to parse publisher");
        let publisher_address = *rpc_client
            .call(
                FunctionCall {
                    contract_address: publisher_registry_address,
                    entry_point_selector: selector!("get_publisher_address"), // Replace with actual function name
                    calldata: vec![field_publisher],
                },
                BlockId::Tag(BlockTag::Latest),
            )
            .await
            .expect("failed to get publisher address")
            .first()
            .unwrap();

        publishers_map.insert(publisher, publisher_address);
    }
    (publishers_map, publisher_registry_address)
}

async fn init_spot_config(
    rpc_client: &JsonRpcClient<HttpTransport>,
    oracle_address: Felt,
    pairs: Vec<String>,
) -> DataInfo {
    let mut sources: HashMap<String, Vec<String>> = HashMap::new();
    let mut decimals: HashMap<String, u32> = HashMap::new();

    let excluded_sources = std::env::var("IGNORE_SOURCES")
        .unwrap_or("".to_string())
        .split(',')
        .map(|source| source.to_string())
        .collect::<Vec<String>>();

    for pair in pairs.clone() {
        let field_pair = cairo_short_string_to_felt(&pair).unwrap();

        // Fetch decimals
        let spot_decimals = *rpc_client
            .call(
                FunctionCall {
                    contract_address: oracle_address,
                    entry_point_selector: selector!("get_decimals"),
                    calldata: vec![Felt::ZERO, field_pair],
                },
                BlockId::Tag(BlockTag::Latest),
            )
            .await
            .expect("failed to get decimals")
            .first()
            .unwrap();

        decimals.insert(pair.to_string(), try_felt_to_u32(&spot_decimals).unwrap());

        // Fetch sources
        let spot_pair_sources = rpc_client
            .call(
                FunctionCall {
                    contract_address: oracle_address,
                    entry_point_selector: selector!("get_all_sources"),
                    calldata: vec![Felt::ZERO, field_pair],
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

    DataInfo {
        decimals,
        pairs,
        sources,
        table_name: "spot_entry".to_string(),
    }
}

async fn init_future_config(
    rpc_client: &JsonRpcClient<HttpTransport>,
    oracle_address: Felt,
    pairs: Vec<String>,
) -> DataInfo {
    let mut sources: HashMap<String, Vec<String>> = HashMap::new();
    let mut decimals: HashMap<String, u32> = HashMap::new();

    let excluded_sources = std::env::var("IGNORE_SOURCES")
        .unwrap_or("".to_string())
        .split(',')
        .map(|source| source.to_string())
        .collect::<Vec<String>>();

    for pair in pairs.clone() {
        let field_pair = cairo_short_string_to_felt(&pair).unwrap();

        // Fetch decimals
        let future_decimals = *rpc_client
            .call(
                FunctionCall {
                    contract_address: oracle_address,
                    entry_point_selector: selector!("get_decimals"),
                    calldata: vec![Felt::ONE, field_pair, Felt::ZERO],
                },
                BlockId::Tag(BlockTag::Latest),
            )
            .await
            .expect("failed to get decimals")
            .first()
            .unwrap();

        decimals.insert(pair.to_string(), try_felt_to_u32(&future_decimals).unwrap());

        // Fetch sources
        let future_pair_sources = rpc_client
            .call(
                FunctionCall {
                    contract_address: oracle_address,
                    entry_point_selector: selector!("get_all_sources"),
                    calldata: vec![Felt::ONE, field_pair, Felt::ZERO],
                },
                BlockId::Tag(BlockTag::Latest),
            )
            .await
            .expect("failed to get pair sources");

        // Store all sources for the given pair
        let mut pair_sources = Vec::new();

        // Remove first elements of sources' arrays
        let future_pair_sources = future_pair_sources[1..].to_vec();

        for source in future_pair_sources {
            let source = parse_cairo_short_string(&source).unwrap();
            if !pair_sources.contains(&source) && !excluded_sources.contains(&source) {
                pair_sources.push(source);
            }
        }

        sources.insert(pair.to_string(), pair_sources);
    }

    DataInfo {
        decimals,
        pairs,
        sources,
        table_name: "future_entry".to_string(),
    }
}

#[allow(dead_code)]
/// Fill the LONG_TAIL_ASSET_THRESHOLD metrics with every long tail assets configuration
/// fetched from LONG_TAIL_ASSETS.
/// TODO: LONG_TAIL_ASSETS should be an independent (db, yaml...) configuration?
pub fn init_long_tail_asset_configuration() {
    for (pair, (threshold_low, threshold_high)) in LONG_TAIL_ASSETS.iter() {
        LONG_TAIL_ASSET_THRESHOLD
            .with_label_values(&[pair, "low"])
            .set(*threshold_low);
        LONG_TAIL_ASSET_THRESHOLD
            .with_label_values(&[pair, "high"])
            .set(*threshold_high);
    }
}

#[allow(dead_code)]
/// Retrieves the long tail asset threshold configuration depending on the number of sources.
pub fn get_long_tail_threshold(pair: &str, number_of_sources: usize) -> Option<f64> {
    if !is_long_tail_asset(pair) {
        return None;
    };
    let (threshold_low, threshold_high) = LONG_TAIL_ASSETS.get(pair).unwrap();

    if number_of_sources <= LOW_SOURCES_THRESHOLD {
        Some(*threshold_low)
    } else {
        Some(*threshold_high)
    }
}

/// Parse pairs from a comma separated string.
/// e.g BTC/USD,ETH/USD
pub fn parse_pairs(pairs: &str) -> Vec<String> {
    pairs
        .split(',')
        .map(|pair| pair.to_string())
        .collect::<Vec<String>>()
}
