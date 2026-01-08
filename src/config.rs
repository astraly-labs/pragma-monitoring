use std::{
    collections::HashMap,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use arc_swap::{ArcSwap, Guard};
use starknet::{
    core::{
        types::{BlockId, BlockTag, Felt, FunctionCall},
        utils::{cairo_short_string_to_felt, parse_cairo_short_string},
    },
    macros::selector,
    providers::{JsonRpcClient, Provider, jsonrpc::HttpTransport},
};
use strum::{Display, EnumString, IntoStaticStr};
use tokio::sync::OnceCell;
use url::Url;

use crate::{constants::CONFIG_UPDATE_INTERVAL, utils::try_felt_to_u32};

#[derive(Debug, Clone, EnumString, IntoStaticStr)]
pub enum NetworkName {
    #[strum(ascii_case_insensitive)]
    Mainnet,
    #[strum(ascii_case_insensitive)]
    Testnet,
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
#[allow(unused)]
pub struct Config {
    pub(crate) data_info: HashMap<DataType, DataInfo>,
    pub(crate) publishers: HashMap<String, Felt>,
    pub(crate) network: Network,
    pub(crate) apibara_api_key: String,
}

/// We are using `ArcSwap` as it allow us to replace the new `Config` with
/// a new one which is required when running test cases. This approach was
/// inspired from here - https://github.com/matklad/once_cell/issues/127
#[allow(unused)]
pub static CONFIG: OnceCell<ArcSwap<Config>> = OnceCell::const_new();

#[allow(unused)]
impl Config {
    pub async fn new(config_input: ConfigInput) -> Self {
        // Create RPC Client
        let rpc_url = match std::env::var("RPC_URL") {
            Ok(url) => {
                tracing::info!("RPC URL configured");
                url
            }
            Err(e) => {
                tracing::error!(
                    "RPC_URL environment variable is required but not set: {:?}",
                    e
                );
                panic!("RPC_URL must be set");
            }
        };

        let parsed_url = match Url::parse(&rpc_url) {
            Ok(url) => url,
            Err(e) => {
                tracing::error!("Invalid RPC_URL format: {:?}", e);
                panic!("Invalid RPC_URL format: {}", e);
            }
        };

        let rpc_client = JsonRpcClient::new(HttpTransport::new(parsed_url));

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

        // Get APIBARA_API_KEY from environment
        let apibara_api_key = match std::env::var("APIBARA_API_KEY") {
            Ok(key) => {
                tracing::info!("APIBARA_API_KEY configured (length: {})", key.len());
                key
            }
            Err(e) => {
                tracing::error!(
                    "APIBARA_API_KEY environment variable is required but not set: {:?}",
                    e
                );
                panic!("APIBARA_API_KEY must be set");
            }
        };

        Self {
            publishers,
            data_info,
            network: Network {
                name: config_input.network,
                provider: Arc::new(rpc_client),
                oracle_address: config_input.oracle_address,
                publisher_registry_address,
            },
            apibara_api_key,
        }
    }

    pub async fn create_from_env() -> Config {
        let network = match std::env::var("NETWORK") {
            Ok(network) => {
                tracing::info!("Network configured: {}", network);
                network
            }
            Err(e) => {
                tracing::error!(
                    "NETWORK environment variable is required but not set: {:?}",
                    e
                );
                panic!("NETWORK must be set");
            }
        };

        let oracle_address = match std::env::var("ORACLE_ADDRESS") {
            Ok(address) => {
                tracing::info!("Oracle address configured: {}", address);
                address
            }
            Err(e) => {
                tracing::error!(
                    "ORACLE_ADDRESS environment variable is required but not set: {:?}",
                    e
                );
                panic!("ORACLE_ADDRESS must be set");
            }
        };

        let spot_pairs = match std::env::var("SPOT_PAIRS") {
            Ok(pairs) => {
                tracing::info!("Spot pairs configured: {}", pairs);
                pairs
            }
            Err(e) => {
                tracing::error!(
                    "SPOT_PAIRS environment variable is required but not set: {:?}",
                    e
                );
                panic!("SPOT_PAIRS must be set");
            }
        };

        let future_pairs = match std::env::var("FUTURE_PAIRS") {
            Ok(pairs) => {
                tracing::info!("Future pairs configured: {}", pairs);
                pairs
            }
            Err(e) => {
                tracing::error!(
                    "FUTURE_PAIRS environment variable is required but not set: {:?}",
                    e
                );
                panic!("FUTURE_PAIRS must be set");
            }
        };

        Config::new(ConfigInput {
            network: NetworkName::from_str(&network).expect("Invalid network name"),
            oracle_address: Felt::from_hex_unchecked(&oracle_address),
            spot_pairs: parse_pairs(&spot_pairs),
            future_pairs: parse_pairs(&future_pairs),
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

    pub fn table_name(&self, data_type: DataType) -> String {
        let table_name = &self.data_info.get(&data_type).unwrap().table_name;
        match self.network.name {
            NetworkName::Mainnet => format!("mainnet_{}", table_name),
            NetworkName::Testnet => table_name.to_string(),
        }
    }

    pub fn all_publishers(&self) -> &HashMap<String, Felt> {
        &self.publishers
    }

    pub fn apibara_api_key(&self) -> &str {
        &self.apibara_api_key
    }
}

#[derive(Debug, Clone)]
pub struct ConfigInput {
    pub network: NetworkName,
    pub oracle_address: Felt,
    pub spot_pairs: Vec<String>,
    pub future_pairs: Vec<String>,
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
    let interval = Duration::from_secs(CONFIG_UPDATE_INTERVAL);
    tracing::info!(
        "🔄 [CONFIG] Config auto-refresh enabled (every {}h)",
        CONFIG_UPDATE_INTERVAL / 3600
    );

    let mut next_update = Instant::now() + interval;

    loop {
        tracing::info!("🔄 [CONFIG] Refreshing configuration...");

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

/// Parse pairs from a comma separated string.
/// e.g BTC/USD,ETH/USD
pub fn parse_pairs(pairs: &str) -> Vec<String> {
    if pairs.is_empty() {
        vec![]
    } else {
        pairs
            .split(',')
            .map(|pair| pair.to_string())
            .collect::<Vec<String>>()
    }
}
