use arc_swap::ArcSwap;
use lazy_static::lazy_static;
use prometheus::{opts, register_gauge_vec, register_int_gauge_vec, GaugeVec, IntGaugeVec};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::coingecko::get_coingecko_mappings;
pub(crate) static LOW_SOURCES_THRESHOLD: usize = 6;

lazy_static! {
    pub static ref COINGECKO_IDS: ArcSwap<HashMap<String, String>> =
        ArcSwap::new(Arc::new(HashMap::new()));
}

#[allow(dead_code)]
pub async fn initialize_coingecko_mappings() {
    match get_coingecko_mappings().await {
        Ok(mappings) => {
            COINGECKO_IDS.store(Arc::new(mappings));
            tracing::info!("Successfully initialized CoinGecko mappings");
        }
        Err(e) => {
            tracing::error!("Failed to initialize CoinGecko mappings: {}", e);
            // Don't panic, just log the error
            tracing::warn!("Continuing without CoinGecko mappings");
        }
    }
}

lazy_static! {
    /// TODO: Current storage of long tail assets here is not really good.
    /// We should probably store them either in a yaml config file or a
    /// database (cons of a database => update the threshold/pairs without restarting
    /// the monitoring service).
    ///
    /// Stores the threshold for when:
    ///     - `low`: the pair has 6 sources or less
    ///     - `high`: the pair has more than 6 sources.
    pub static ref LONG_TAIL_ASSETS: HashMap<String, (f64, f64)> = {
        let mut map = HashMap::new();
        map.insert("ZEND/USD".to_string(), (0.05, 0.03));
        map.insert("NSTR/USD".to_string(), (0.05, 0.03));
        map.insert("LUSD/USD".to_string(), (0.05, 0.03));
        map.insert("LORDS/USD".to_string(), (0.05, 0.03));
        map
    };

    /// Stores the pairs that are long tail assets and have a conversion rate
    /// to USD.
    /// The conversion rate is used to convert the price of the pair to USD
    /// and then compare it to the reference price.
    /// The conversion rate is stored in the `LST_CONVERSION_RATE` metric.
    /// The conversion rate is used to convert the price of the pair to USD
    /// and then compare it to the reference price.
    pub static ref LST_PAIRS: HashSet<&'static str> = {
        let mut set = HashSet::new();
        set.insert("XSTRK/STRK");
        set.insert("SSTRK/STRK");
        set.insert("KSTRK/STRK");
        set
    };

    /// We have a list of assets that are defined as long tail assets.
    /// They have lower liquidity and higher volatilty - thus, it is trickier
    /// to track their prices and have good alerting.
    /// Our way of dealing with those assets is:
    /// - we don't use the usual metrics "price_deviation" below
    /// - instead, we compare all the sources one to one and if the deviation
    /// between the two is greater than a certain threshold, we send an alert.
    ///
    /// "LONG_TAIL_ASSET_THRESHOLD" will contain the long tail assets pairs
    /// and the threshold.
    /// "LONG_TAIL_ASSET_DEVIATION" will contain the deviation between two sources.
    ///
    /// We define all the long tail assets in the config::init_long_tail_asset_configuration
    /// function.
    ///
    pub static ref LONG_TAIL_ASSET_THRESHOLD: GaugeVec = register_gauge_vec!(
        opts!(
            "long_tail_asset_threshold",
            "Deviation threshold configuration for long tail assets"
        ),
        &["pair", "type"]
    )
    .unwrap();
    pub static ref LONG_TAIL_ASSET_SOURCE_DEVIATION: GaugeVec = register_gauge_vec!(
        opts!(
            "long_tail_asset_source_deviation",
            "Deviation of each source from our onchain aggregated price for long tail assets"
        ),
        &["network", "pair", "type", "source"]
    ).unwrap();
    pub static ref LONG_TAIL_ASSET_TOTAL_SOURCES: GaugeVec = register_gauge_vec!(
        opts!(
            "long_tail_asset_total_sources",
            "Total number of sources for long tail assets"
        ),
        &["network", "pair", "type"]
    ).unwrap();

    // Regular metrics below

    pub static ref TIME_SINCE_LAST_UPDATE_PUBLISHER: GaugeVec = register_gauge_vec!(
        opts!(
            "time_since_last_update_seconds",
            "Time since the last update in seconds."
        ),
        &["network", "publisher", "type"]
    )
    .unwrap();
    pub static ref PAIR_PRICE: GaugeVec = register_gauge_vec!(
        opts!("pair_price", "Price of the pair from the source."),
        &["network", "pair", "source", "type"]
    )
    .unwrap();
    pub static ref TIME_SINCE_LAST_UPDATE_PAIR_ID: GaugeVec = register_gauge_vec!(
        opts!(
            "time_since_last_update_pair_id",
            "Time since the last update in seconds."
        ),
        &["network", "pair", "type"]
    )
    .unwrap();
    pub static ref PRICE_DEVIATION: GaugeVec = register_gauge_vec!(
        opts!(
            "price_deviation",
            "Price deviation for a source compared to a reference price (DefiLlama)."
        ),
        &["network", "pair", "source", "type"]
    )
    .unwrap();
    pub static ref PRICE_DEVIATION_SOURCE: GaugeVec = register_gauge_vec!(
        opts!(
            "price_deviation_source",
            "Price deviation for a source compared to our oracle price."
        ),
        &["network", "pair", "source", "type"]
    )
    .unwrap();
    pub static ref NUM_SOURCES: IntGaugeVec = register_int_gauge_vec!(
        opts!(
            "num_sources",
            "Number of sources that have published data for a pair."
        ),
        &["network", "pair", "type"]
    )
    .unwrap();
    pub static ref INDEXER_BLOCKS_LEFT: IntGaugeVec = register_int_gauge_vec!(
        opts!(
            "indexer_blocks_left",
            "Number of blocks left to index for a given indexer."
        ),
        &["network", "type"]
    )
    .unwrap();
    pub static ref PUBLISHER_BALANCE: GaugeVec = register_gauge_vec!(
        opts!("publisher_balance", "Balance of the publisher in ETH"),
        &["network", "publisher"]
    )
    .unwrap();
    pub static ref API_PRICE_DEVIATION: GaugeVec = register_gauge_vec!(
        opts!(
            "api_price_deviation",
            "Price deviation for our API compared to a reference price (DefiLlama)."
        ),
        &["network", "pair"]
    )
    .unwrap();
    pub static ref ON_OFF_PRICE_DEVIATION: GaugeVec = register_gauge_vec!(
        opts!(
            "on_off_price_deviation",
            "Median on chain price deviation compared to a reference price (Defillama)."
        ),
        &["network", "pair", "type"]
    )
    .unwrap();
    pub static ref API_TIME_SINCE_LAST_UPDATE: GaugeVec = register_gauge_vec!(
        opts!(
            "api_time_since_last_update",
            "Time since the last update in seconds."
        ),
        &["network", "pair"]
    )
    .unwrap();
    pub static ref API_NUM_SOURCES: IntGaugeVec = register_int_gauge_vec!(
        opts!(
            "api_num_sources",
            "Number of sources aggregated for a pair."
        ),
        &["network", "pair"]
    )
    .unwrap();
    pub static ref API_SEQUENCER_DEVIATION: GaugeVec = register_gauge_vec!(
        opts!(
            "api_sequencer_deviation",
            "Price deviation from starknet gateway price."
        ),
        &["network"]
    )
    .unwrap();
    pub static ref VRF_BALANCE: GaugeVec = register_gauge_vec!(
        opts!("vrf_balance", "Balance of the VRF contract in ETH"),
        &["network"]
    )
    .unwrap();
    pub static ref VRF_REQUESTS_COUNT: GaugeVec = register_gauge_vec!(
        opts!(
            "vrf_requests_count",
            "Number of requests for a given network and a status."
        ),
        &["network", "status"]
    )
    .unwrap();
    pub static ref VRF_TIME_SINCE_LAST_HANDLE_REQUEST: GaugeVec = register_gauge_vec!(
        opts!(
            "vrf_time_since_last_handle_request",
            "Time since the latest request was handled for a given network."
        ),
        &["network"]
    )
    .unwrap();
    pub static ref VRF_TIME_SINCE_OLDEST_REQUEST_IN_PENDING_STATUS: GaugeVec = register_gauge_vec!(
        opts!(
            "vrf_time_since_oldest_request_in_pending_status",
            "Time in seconds that the oldest pending VRF request has been in the initial status for a given network."
        ),
        &["network"]
    )
    .unwrap();
    // LST conversion rate metrics
    pub static ref LST_CONVERSION_RATE: GaugeVec = register_gauge_vec!(
        opts!(
            "lst_conversion_rate",
            "Conversion rate for LST pairs"
        ),
        &["network", "pair"]
    ).unwrap();
}

#[allow(unused)]
pub const FEE_TOKEN_DECIMALS: u32 = 18;
#[allow(unused)]
pub const FEE_TOKEN_ADDRESS: &str =
    "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7";

pub const CONFIG_UPDATE_INTERVAL: u64 = 3 * 3600;
