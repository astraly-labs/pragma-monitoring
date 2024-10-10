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
// Evm Config utils
mod evm;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::time::Duration;
use std::{env, vec};

use alloy::primitives::address;
use deadpool::managed::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;
use dotenv::dotenv;
use tokio::task::JoinHandle;
use tokio::time::interval;

use config::{get_config, init_long_tail_asset_configuration, periodic_config_update, DataType};
use processing::common::{check_publisher_balance, data_indexers_are_synced, indexers_are_synced};
use utils::{is_long_tail_asset, log_monitoring_results, log_tasks_results};

struct MonitoringTask {
    name: String,
    handle: JoinHandle<()>,
}


#[tokio::main]
async fn main() {
    env_logger::init();

    // Load environment variables from .env file
    dotenv().ok();

    // Define the pairs to monitor
    let monitoring_config = get_config(None).await;
    log::info!("Successfully fetched config: {:?}", monitoring_config);
    tokio::spawn(server::run_metrics_server());

    let database_url: String = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(database_url);
    let pool = Pool::builder(config).build().unwrap();

    // Set the long tail asset list
    init_long_tail_asset_configuration();

    // Monitor spot/future in parallel
    let monitoring_tasks = spawn_monitoring_tasks(pool.clone(), &monitoring_config).await;
    handle_task_results(monitoring_tasks).await;
}

async fn spawn_monitoring_tasks(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    monitoring_config: &config::Config,
) -> Vec<MonitoringTask> {
    let mut tasks = vec![
        MonitoringTask {
            name: "Config Update".to_string(),
            handle: tokio::spawn(periodic_config_update()),
        },
        MonitoringTask {
            name: "Spot Monitoring".to_string(),
            handle: tokio::spawn(onchain_monitor(pool.clone(), true, &DataType::Spot)),
        },
        MonitoringTask {
            name: "Future Monitoring".to_string(),
            handle: tokio::spawn(onchain_monitor(pool.clone(), true, &DataType::Future)),
        },
        MonitoringTask {
            name: "Publisher Monitoring".to_string(),
            handle: tokio::spawn(publisher_monitor(pool.clone(), false)),
        },
    ];

    if monitoring_config.is_pragma_chain() {
        tasks.push(MonitoringTask {
            name: "Hyperlane Dispatches Monitoring".to_string(),
            handle: tokio::spawn(hyperlane_dispatch_monitor(pool.clone(), true)),
        });
    } else {
        tasks.push(MonitoringTask {
            name: "API Monitoring".to_string(),
            handle: tokio::spawn(api_monitor()),
        });
        tasks.push(MonitoringTask {
            name: "VRF Monitoring".to_string(),
            handle: tokio::spawn(vrf_monitor(pool.clone())),
        });
    }

    tasks
}

async fn handle_task_results(tasks: Vec<MonitoringTask>) {
    let mut results = HashMap::new();
    for task in tasks {
        let result = task.handle.await;
        results.insert(task.name, result);
    }
    log_monitoring_results(results);
}

pub(crate) async fn api_monitor() {
    let monitoring_config = get_config(None).await;
    log::info!("[API] Monitoring API..");

    let mut interval = interval(Duration::from_secs(30));

    loop {
        interval.tick().await; // Wait for the next tick

        let mut tasks: Vec<_> = monitoring_config
            .sources(DataType::Spot)
            .iter()
            .flat_map(|(pair, sources)| {
                if is_long_tail_asset(pair) {
                    vec![tokio::spawn(Box::pin(
                        processing::api::process_long_tail_assets(pair.clone(), sources.clone()),
                    ))]
                } else {
                    vec![tokio::spawn(Box::pin(
                        processing::api::process_data_by_pair(pair.clone()),
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
) {
    let monitoring_config = get_config(None).await;

    let mut interval = interval(Duration::from_secs(30));

    loop {
        interval.tick().await; // Wait for the next tick

        // Skip if indexer is still syncing
        if wait_for_syncing && !data_indexers_are_synced(data_type).await {
            continue;
        }

        let tasks: Vec<_> = monitoring_config
            .sources(data_type.clone())
            .iter()
            .flat_map(|(pair, sources)| match data_type {
                DataType::Spot => {
                    if is_long_tail_asset(pair) {
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
                            ))),
                            tokio::spawn(Box::pin(
                                processing::spot::process_data_by_pair_and_sources(
                                    pool.clone(),
                                    pair.clone(),
                                    sources.to_vec(),
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
                        ))),
                        tokio::spawn(Box::pin(
                            processing::future::process_data_by_pair_and_sources(
                                pool.clone(),
                                pair.clone(),
                                sources.to_vec(),
                            ),
                        )),
                    ]
                }
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(tasks).await;
        log_tasks_results(data_type.into(), results);
    }
}

pub(crate) async fn publisher_monitor(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    wait_for_syncing: bool,
) {
    log::info!("[PUBLISHERS] Monitoring Publishers..");

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

pub(crate) async fn vrf_monitor(pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>) {
    log::info!("[VRF] Monitoring VRF requests..");

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



pub(crate) async fn hyperlane_dispatch_monitor(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    wait_for_syncing: bool,
) {
    let mut interval = interval(Duration::from_secs(5));

    loop {
        interval.tick().await; // Wait for the next tick
               // Skip if indexer is still syncing
        if wait_for_syncing && !indexers_are_synced("pragma_devnet_dispatch_event").await {
                continue;
            }
    
            let tasks: Vec<_> = vec![tokio::spawn(Box::pin(
                processing::dispatch::process_dispatch_events(pool.clone()),
            ))];
            let results: Vec<_> = futures::future::join_all(tasks).await;
            log_tasks_results("Dispatch", results);
        }
    }

    pub(crate) async fn evm_monitor() {
        log::info!("[EVM] Monitoring EVM..");
        let mut interval = interval(Duration::from_secs(30));
        loop {
            interval.tick().await; // Wait for the next tick
        let tasks: Vec<_> = vec![tokio::spawn(Box::pin(processing::evm::check_feed_update_state()))];

        let results: Vec<_> = futures::future::join_all(tasks).await;
        log_tasks_results("EVM", results);
    }
}
 
