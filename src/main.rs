extern crate diesel;
extern crate dotenv;

use config::{get_config, DataType};
use diesel_async::pooled_connection::deadpool::*;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;

use dotenv::dotenv;
use processing::common::check_data_provider_balance;
use std::env;
use std::time::Duration;
use tokio::time::interval;

use crate::processing::common::is_syncing;

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

#[cfg(test)]
mod tests;

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

    // Monitor spot/future in parallel
    let spot_monitoring = tokio::spawn(monitor(pool.clone(), true, &DataType::Spot));
    let future_monitoring = tokio::spawn(monitor(pool.clone(), true, &DataType::Future));

    let balance_monitoring = tokio::spawn(balance_monitor());
    let api_monitoring = tokio::spawn(monitor_api());

    // Wait for the monitoring to finish
    let results = futures::future::join_all(vec![
        spot_monitoring,
        future_monitoring,
        api_monitoring,
        balance_monitoring,
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
        log::error!("[BALANCE] Monitoring failed: {:?}", e);
    }
}

pub(crate) async fn monitor_api() {
    let monitoring_config = get_config(None).await;
    log::info!("[API] Monitoring API..");

    let mut interval = interval(Duration::from_secs(30));

    loop {
        interval.tick().await; // Wait for the next tick

        let mut tasks: Vec<_> = monitoring_config
            .sources(DataType::Spot)
            .iter()
            .flat_map(|(pair, _sources)| {
                vec![tokio::spawn(Box::pin(
                    processing::api::process_data_by_pair(pair.clone()),
                ))]
            })
            .collect();
        tasks.push(tokio::spawn(Box::pin(
            processing::api::process_sequencer_data(),
        )));

        let results: Vec<_> = futures::future::join_all(tasks).await;

        // Process or output the results
        for result in &results {
            match result {
                Ok(data) => match data {
                    Ok(_) => log::info!("[API] Task finished successfully",),
                    Err(e) => log::error!("[API] Task failed with error: {e}"),
                },
                Err(e) => log::error!("[API] Task failed with error: {:?}", e),
            }
        }
    }
}

pub(crate) async fn monitor(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    wait_for_syncing: bool,
    data_type: &DataType,
) {
    let monitoring_config: arc_swap::Guard<std::sync::Arc<config::Config>> = get_config(None).await;

    let mut interval = interval(Duration::from_secs(30));

    loop {
        interval.tick().await; // Wait for the next tick

        // Skip if indexer is still syncing
        if wait_for_syncing {
            match is_syncing(data_type).await {
                Ok(true) => {
                    log::info!("[{data_type}] Indexers are still syncing ♻️");
                    continue;
                }
                Ok(false) => {
                    log::info!("[{data_type}] Indexers are synced ✅");
                }
                Err(e) => {
                    log::error!(
                        "[{data_type}] Failed to check if indexers are syncing: {:?}",
                        e
                    );
                    continue;
                }
            }
        }

        let tasks: Vec<_> = monitoring_config
            .sources(data_type.clone())
            .iter()
            .flat_map(|(pair, sources)| match data_type {
                DataType::Spot => {
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

        // Process or output the results
        for result in &results {
            match result {
                Ok(data) => match data {
                    Ok(_) => log::info!("[{data_type}] Task finished successfully",),
                    Err(e) => log::error!("[{data_type}] Task failed with error: {e}"),
                },
                Err(e) => log::error!("[{data_type}] Task failed with error: {:?}", e),
            }
        }
    }
}

pub(crate) async fn balance_monitor() {
    log::info!("[Balance] Monitoring Balance..");
    let mut interval = interval(Duration::from_secs(30));
    let monitoring_config: arc_swap::Guard<std::sync::Arc<config::Config>> = get_config(None).await;

    loop {
        interval.tick().await; // Wait for the next tick

        let tasks: Vec<_> = monitoring_config
            .publishers
            .clone()
            .iter()
            .map(|(name, address)| {
                tokio::spawn(Box::pin(check_data_provider_balance(
                    name.clone(),
                    *address,
                )))
            })
            .collect();
        let results: Vec<_> = futures::future::join_all(tasks).await;
        // Process or output the results
        for result in &results {
            match result {
                Ok(data) => match data {
                    Ok(_) => log::info!("Balance monitoring: Task finished successfully",),
                    Err(e) => log::error!("Balance monitoring: Task failed with error: {e}"),
                },
                Err(e) => log::error!("Balance monitoring: Task failed with error: {:?}", e),
            }
        }
    }
}
