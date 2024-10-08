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

#[cfg(test)]
mod tests;

use std::time::Duration;
use std::{env, vec};

use deadpool::managed::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;
use dotenv::dotenv;
use tokio::time::interval;

use config::{get_config, init_long_tail_asset_configuration, periodic_config_update, DataType};
use processing::common::{check_publisher_balance, indexers_are_synced};
use utils::{is_long_tail_asset, log_tasks_results};

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
    let spot_monitoring = tokio::spawn(onchain_monitor(pool.clone(), true, &DataType::Spot));
    let future_monitoring = tokio::spawn(onchain_monitor(pool.clone(), true, &DataType::Future));
    let pragmagix_monitoring = tokio::spawn(pragmagix_monitor(pool.clone(), true, &DataType::Spot));
    let publisher_monitoring = tokio::spawn(publisher_monitor(pool.clone(), false));
    let api_monitoring = tokio::spawn(api_monitor());
    let vrf_monitoring = tokio::spawn(vrf_monitor(pool.clone()));

    let config_update = tokio::spawn(periodic_config_update());

    // Wait for the monitoring to finish
    let results = futures::future::join_all(vec![
        spot_monitoring,
        future_monitoring,
        api_monitoring,
        publisher_monitoring,
        vrf_monitoring,
        config_update,
    ])
    .await;

    // Check if any of the monitoring tasks failed
    if let Err(e) = &results[0] {
        log::error!("[SPOT] Monitoring failed: {:?}", e);
    }
    if let Err(e) = &results[1] {
        log::error!("[FUTURE] Monitoring failed: {:?}", e);
    }
    if let Err(e) = &results[2] {
        log::error!("[API] Monitoring failed: {:?}", e);
    }
    if let Err(e) = &results[3] {
        log::error!("[PUBLISHERS] Monitoring failed: {:?}", e);
    }

    if let Err(e) = &results[4] {
        log::error!("[VRF] Monitoring failed: {:?}", e);
    }

    if let Err(e) = &results[5] {
        log::error!("[CONFIG] Config Update failed: {:?}", e);
    }
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
        if wait_for_syncing && !indexers_are_synced(data_type).await {
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
        if wait_for_syncing && !indexers_are_synced(&DataType::Spot).await {
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


pub(crate) async fn pragmagix_monitor(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    wait_for_syncing: bool,
    data_type: &DataType,
) {
    let monitoring_config = get_config(None).await;

    let mut interval = interval(Duration::from_secs(30));

    loop {
        interval.tick().await; // Wait for the next tick

        // Skip if indexer is still syncing
        if wait_for_syncing && !indexers_are_synced(data_type).await {
            continue;
        }

        let tasks: Vec<_> = monitoring_config
            .sources(data_type.clone())
            .iter()
            .flat_map(|(pair, sources)| {
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
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(tasks).await;
        log_tasks_results(data_type.into(), results);
    }
}