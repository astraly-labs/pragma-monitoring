use arc_swap::ArcSwap;
use lazy_static::lazy_static;
use prometheus::{opts, register_gauge_vec, register_int_gauge_vec, GaugeVec, IntGaugeVec};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

use crate::coingecko::get_coingecko_mappings;
pub(crate) static LOW_SOURCES_THRESHOLD: usize = 6;

lazy_static! {
    pub static ref COINGECKO_IDS: ArcSwap<HashMap<String, String>> =
        ArcSwap::new(Arc::new(HashMap::new()));
}

const MAX_RETRIES: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_secs(2);

#[allow(dead_code)]
pub async fn initialize_coingecko_mappings() {
    let mappings = retry_with_backoff(get_coingecko_mappings)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(
                "Failed to initialize CoinGecko mappings after {} retries: {}",
                MAX_RETRIES,
                e
            );
            panic!("Cannot start monitoring without CoinGecko mappings: {}", e);
        });

    COINGECKO_IDS.store(Arc::new(mappings));
    tracing::info!("Successfully initialized CoinGecko mappings");
}

async fn retry_with_backoff<F, Fut, T, E>(f: F) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempts = 0;
    let mut last_error = None;

    while attempts < MAX_RETRIES {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                attempts += 1;
                last_error = Some(e);
                if attempts < MAX_RETRIES {
                    tracing::warn!(
                        "Attempt {} failed, retrying after {} seconds",
                        attempts,
                        RETRY_DELAY.as_secs()
                    );
                    sleep(RETRY_DELAY).await;
                }
            }
        }
    }

    Err(last_error.unwrap())
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
    pub static ref DISPATCH_EVENT_LATEST_BLOCK: GaugeVec = register_gauge_vec!(
        opts!(
            "dispatch_event_latest_block",
            "The latest block that triggered a Dispatch event from Hyperlane"
        ),
        &["network"]
    ).unwrap();
    pub static ref DISPATCH_EVENT_FEED_LATEST_BLOCK_UPDATE: GaugeVec = register_gauge_vec!(
        opts!(
            "dispatch_event_feed_latest_block_update",
            "The latest block that triggered a Dispatch event from Hyperlane for a specific Feed ID"
        ),
        &["network", "feed_id"]
    ).unwrap();
    pub static ref DISPATCH_EVENT_NB_FEEDS_UPDATED: GaugeVec = register_gauge_vec!(
        opts!(
            "dispatch_event_nb_feeds_updated",
            "The number of feeds updated per Dispatch event at a given block"
        ),
        &["network", "block"]
    ).unwrap();
}

#[allow(unused)]
pub const FEE_TOKEN_DECIMALS: u32 = 18;
#[allow(unused)]
pub const FEE_TOKEN_ADDRESS: &str =
    "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7";

pub const CONFIG_UPDATE_INTERVAL: u64 = 3 * 3600;
