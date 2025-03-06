extern crate diesel;
extern crate dotenv;

// Configuration
mod config;
// Error handling
mod error;
// Database models
mod models;
// Monitoring functions
mod monitoring;
// Processing functions
mod processing;
// Server
mod server;
// Database schema
mod schema;
// Constants
mod constants;
// Types
mod types;
// Utils
mod utils;
// coingecko
mod coingecko;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::time::Duration;
use std::{env, vec};

use deadpool::managed::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;
use dotenv::dotenv;
use moka::future::Cache;
use monitoring::price_deviation::CoinPricesDTO;
use tokio::task::JoinHandle;
use tokio::time::interval;

use config::{get_config, init_long_tail_asset_configuration, periodic_config_update, DataType};
use constants::{initialize_coingecko_mappings, LST_PAIRS};
use processing::common::{check_publisher_balance, data_indexers_are_synced};
use tracing::instrument;
use utils::{is_long_tail_asset, log_monitoring_results, log_tasks_results};

#[derive(Debug)]
struct MonitoringTask {
    name: String,
    handle: JoinHandle<()>,
}

#[tokio::main]
async fn main() {
    // Initialize CoinGecko mappings first
    initialize_coingecko_mappings().await;

    // Start configuring a `fmt` subscriber
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();

    // Load environment variables from .env file
    dotenv().ok();

    // Define the pairs to monitor
    let monitoring_config = get_config(None).await;
    tracing::info!("Successfully fetched config: {:?}", monitoring_config);
    tokio::spawn(server::run_metrics_server());

    let database_url: String = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(database_url);
    let pool = Pool::builder(config).build().unwrap();

    // Set the long tail asset list
    init_long_tail_asset_configuration();

    // Monitor spot/future in parallel
    let monitoring_tasks = spawn_monitoring_tasks(pool.clone()).await;
    handle_task_results(monitoring_tasks).await;
}

#[instrument(skip_all)]
async fn spawn_monitoring_tasks(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
) -> Vec<MonitoringTask> {
    let cache = Cache::new(10_000);

    let tasks = vec![
        MonitoringTask {
            name: "Config Update".to_string(),
            handle: tokio::spawn(periodic_config_update()),
        },
        MonitoringTask {
            name: "Spot Monitoring".to_string(),
            handle: tokio::spawn(onchain_monitor(
                pool.clone(),
                true,
                &DataType::Spot,
                cache.clone(),
            )),
        },
        MonitoringTask {
            name: "Future Monitoring".to_string(),
            handle: tokio::spawn(onchain_monitor(
                pool.clone(),
                true,
                &DataType::Future,
                cache.clone(),
            )),
        },
        MonitoringTask {
            name: "Publisher Monitoring".to_string(),
            handle: tokio::spawn(publisher_monitor(pool.clone(), false)),
        },
        MonitoringTask {
            name: "API Monitoring".to_string(),
            handle: tokio::spawn(api_monitor(cache.clone())),
        },
        MonitoringTask {
            name: "VRF Monitoring".to_string(),
            handle: tokio::spawn(vrf_monitor(pool.clone())),
        },
        MonitoringTask {
            name: "CoinGecko Mappings Update".to_string(),
            handle: tokio::spawn(periodic_coingecko_update()),
        },
    ];

    tasks
}

async fn periodic_coingecko_update() {
    let mut interval = interval(Duration::from_secs(86400)); // 1 day
    loop {
        interval.tick().await;
        initialize_coingecko_mappings().await;
    }
}

#[instrument]
async fn handle_task_results(tasks: Vec<MonitoringTask>) {
    let mut results = HashMap::new();
    for task in tasks {
        let result = task.handle.await;
        results.insert(task.name, result);
    }
    log_monitoring_results(results);
}

#[instrument(skip(cache))]
pub(crate) async fn api_monitor(cache: Cache<(String, u64), CoinPricesDTO>) {
    let monitoring_config = get_config(None).await;
    tracing::info!("[API] Monitoring API..");

    let mut interval = interval(Duration::from_secs(30));

    loop {
        interval.tick().await; // Wait for the next tick

        let mut tasks: Vec<_> = monitoring_config
            .sources(DataType::Spot)
            .iter()
            .flat_map(|(pair, sources)| {
                let my_cache = cache.clone();
                if is_long_tail_asset(pair) {
                    vec![tokio::spawn(Box::pin(
                        processing::api::process_long_tail_assets(pair.clone(), sources.clone()),
                    ))]
                } else {
                    vec![tokio::spawn(Box::pin(
                        processing::api::process_data_by_pair(pair.clone(), my_cache),
                    ))]
                }
            })
            .collect();

        tasks.push(tokio::spawn(Box::pin(
            processing::api::process_sequencer_data(),
        )));

        let results: Vec<_> = futures::future::join_all(tasks).await;
        log_tasks_results("API", results);
    }
}

pub(crate) async fn onchain_monitor(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    wait_for_syncing: bool,
    data_type: &DataType,
    cache: Cache<(String, u64), CoinPricesDTO>,
) {
    let mut interval = interval(Duration::from_secs(30));

    loop {
        interval.tick().await; // Wait for the next tick

        // Skip if indexer is still syncing
        if wait_for_syncing && !data_indexers_are_synced(data_type).await {
            continue;
        }

        // Get fresh config for each iteration
        let monitoring_config = get_config(None).await;

        // Clone the sources map before moving into tasks
        let sources_map = monitoring_config.sources(data_type.clone());

        let tasks: Vec<_> = sources_map
            .iter()
            .flat_map(|(pair, sources)| {
                let pair = pair.clone();
                let sources = sources.clone();
                let mut pair_tasks = match data_type {
                    DataType::Spot => {
                        if is_long_tail_asset(&pair) {
                            vec![tokio::spawn(Box::pin(
                                processing::spot::process_long_tail_asset(
                                    pool.clone(),
                                    pair.clone(),
                                    sources.to_vec(),
                                ),
                            ))]
                        } else {
                            vec![
                                tokio::spawn(Box::pin(processing::spot::process_data_by_pair(
                                    pool.clone(),
                                    pair.clone(),
                                    cache.clone(),
                                ))),
                                tokio::spawn(Box::pin(
                                    processing::spot::process_data_by_pair_and_sources(
                                        pool.clone(),
                                        pair.clone(),
                                        sources.to_vec(),
                                        cache.clone(),
                                    ),
                                )),
                            ]
                        }
                    }
                    // TODO: Long tail assets aren't treated as such for Future data
                    DataType::Future => {
                        vec![
                            tokio::spawn(Box::pin(processing::future::process_data_by_pair(
                                pool.clone(),
                                pair.clone(),
                                cache.clone(),
                            ))),
                            tokio::spawn(Box::pin(
                                processing::future::process_data_by_pair_and_sources(
                                    pool.clone(),
                                    pair.clone(),
                                    sources.to_vec(),
                                    cache.clone(),
                                ),
                            )),
                        ]
                    }
                };

                // Add LST monitoring task if applicable
                if LST_PAIRS.contains(pair.as_str()) {
                    pair_tasks.push(tokio::spawn(Box::pin(async move {
                        // Map the Result<(), MonitoringError> to Result<u64, MonitoringError>
                        monitoring::process_lst_data_by_pair(pair)
                            .await
                            .map(|_| 0u64)
                    })));
                }

                pair_tasks
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(tasks).await;
        log_tasks_results(data_type.into(), results);
    }
}

#[instrument(skip(pool))]
pub(crate) async fn publisher_monitor(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    wait_for_syncing: bool,
) {
    tracing::info!("[PUBLISHERS] Monitoring Publishers..");

    let mut interval = interval(Duration::from_secs(30));
    let monitoring_config: arc_swap::Guard<std::sync::Arc<config::Config>> = get_config(None).await;

    loop {
        interval.tick().await; // Wait for the next tick

        // Skip if indexer is still syncing
        if wait_for_syncing && !data_indexers_are_synced(&DataType::Spot).await {
            continue;
        }

        let tasks: Vec<_> = monitoring_config
            .all_publishers()
            .iter()
            .flat_map(|(publisher, address)| {
                vec![
                    tokio::spawn(Box::pin(check_publisher_balance(
                        publisher.clone(),
                        *address,
                    ))),
                    tokio::spawn(Box::pin(processing::spot::process_data_by_publisher(
                        pool.clone(),
                        publisher.clone(),
                    ))),
                    tokio::spawn(Box::pin(processing::future::process_data_by_publisher(
                        pool.clone(),
                        publisher.clone(),
                    ))),
                ]
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(tasks).await;
        log_tasks_results("PUBLISHERS", results);
    }
}

#[instrument(skip(pool))]
pub(crate) async fn vrf_monitor(pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>) {
    tracing::info!("[VRF] Monitoring VRF requests..");

    let monitoring_config = get_config(None).await;
    let mut interval = interval(Duration::from_secs(30));
    loop {
        interval.tick().await; // Wait for the next tick

        let tasks: Vec<_> = vec![
            tokio::spawn(Box::pin(processing::vrf::check_vrf_balance(
                monitoring_config.network().vrf_address,
            ))),
            tokio::spawn(Box::pin(processing::vrf::check_vrf_request_count(
                pool.clone(),
            ))),
            tokio::spawn(Box::pin(processing::vrf::check_vrf_time_since_last_handle(
                pool.clone(),
            ))),
            tokio::spawn(Box::pin(
                processing::vrf::check_vrf_oldest_request_pending_status_duration(pool.clone()),
            )),
        ];

        let results: Vec<_> = futures::future::join_all(tasks).await;
        log_tasks_results("VRF", results);
    }
}
