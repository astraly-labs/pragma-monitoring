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
// Indexing functions
mod indexing;
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

use std::collections::HashMap;
use std::time::Duration;
use std::{env, vec};

use axum::{Router, extract::State, response::Json, routing::get};
use deadpool::managed::Pool;
use diesel_async::AsyncPgConnection;
use diesel_async::RunQueryDsl;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use dotenv::dotenv;
use moka::future::Cache;
use monitoring::price_deviation::CoinPricesDTO;
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use tokio::time::interval;

use config::{DataType, get_config, periodic_config_update};
use error::MonitoringError;
use evian::utils::indexer::handler::OutputEvent;
use indexing::{
    database_handler::DatabaseHandler, start_pragma_indexer, status::INTERNAL_INDEXER_TRACKER,
};
use processing::common::{check_publisher_balance, data_indexers_are_synced};
use tokio::time::sleep;
use tracing::instrument;
use utils::{log_monitoring_results, log_tasks_results};

/// Test database connectivity before starting the application
async fn test_database_connection(
    pool: &Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
) -> Result<(), MonitoringError> {
    tracing::info!("Testing database connectivity...");

    let mut conn = pool.get().await.map_err(MonitoringError::Connection)?;

    // Simple query to test connectivity
    match diesel::sql_query("SELECT 1 as test")
        .execute(&mut conn)
        .await
    {
        Ok(_) => {
            tracing::info!("Database connectivity test successful");
            Ok(())
        }
        Err(e) => {
            tracing::error!("Database connectivity test failed: {:?}", e);
            Err(MonitoringError::Database(e))
        }
    }
}

#[derive(Debug)]
struct MonitoringTask {
    name: String,
    handle: JoinHandle<()>,
}

#[tokio::main]
#[tracing::instrument]
async fn main() {
    // Load environment variables from .env file
    dotenv().ok();

    if env::var("RUST_LOG").is_err() {
        unsafe {
            env::set_var("RUST_LOG", "evian=error,pragma_monitoring=info");
        }
    }

    let otel_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());
    pragma_common::telemetry::init_telemetry("pragma-monitoring", Some(otel_endpoint))
        .expect("Failed to initialize telemetry");

    // Define the pairs to monitor
    let monitoring_config = get_config(None).await;
    tracing::info!("Successfully fetched config: {:?}", monitoring_config);

    // Setup write connection pool (primary database)
    let write_database_url: String = match env::var("DATABASE_URL") {
        Ok(url) => {
            tracing::info!("Write database URL configured successfully");
            url
        }
        Err(e) => {
            tracing::error!(
                "DATABASE_URL environment variable is required but not set: {:?}",
                e
            );
            std::process::exit(1);
        }
    };

    let write_config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(
        write_database_url.clone(),
    );
    let write_pool = match Pool::builder(write_config).max_size(20).build() {
        Ok(pool) => {
            tracing::info!("Write database connection pool created successfully");
            pool
        }
        Err(e) => {
            tracing::error!("Failed to create write database connection pool: {:?}", e);
            std::process::exit(1);
        }
    };

    // Setup read connection pool (replica database or same as write if no replica)
    let read_database_url: String = env::var("DATABASE_READ_URL").unwrap_or_else(|_| {
        tracing::info!("DATABASE_READ_URL not set, using DATABASE_URL for reads");
        write_database_url.clone()
    });

    let read_config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(
        read_database_url.clone(),
    );
    let read_pool = match Pool::builder(read_config).max_size(20).build() {
        Ok(pool) => {
            tracing::info!("Read database connection pool created successfully");
            pool
        }
        Err(e) => {
            tracing::error!("Failed to create read database connection pool: {:?}", e);
            std::process::exit(1);
        }
    };

    // Get replication delay setting (in milliseconds)
    let replication_delay_ms: u64 = env::var("REPLICATION_DELAY_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if replication_delay_ms > 0 {
        tracing::info!("Replication delay configured: {}ms", replication_delay_ms);
    }

    // Test database connectivity before starting monitoring
    match test_database_connection(&write_pool).await {
        Ok(_) => {
            tracing::info!("Write database connectivity test passed");
        }
        Err(e) => {
            tracing::error!("Write database connectivity test failed: {:?}", e);
            tracing::error!("Please check your DATABASE_URL and ensure the database is accessible");
            std::process::exit(1);
        }
    }

    match test_database_connection(&read_pool).await {
        Ok(_) => {
            tracing::info!("Read database connectivity test passed");
        }
        Err(e) => {
            tracing::error!("Read database connectivity test failed: {:?}", e);
            tracing::error!(
                "Please check your DATABASE_READ_URL and ensure the database is accessible"
            );
            std::process::exit(1);
        }
    }

    // Start health check server
    let health_pool = read_pool.clone();
    tokio::spawn(async move {
        let app = Router::new()
            .route("/health", get(health_check))
            .route("/health/detailed", get(detailed_health_check))
            .with_state(health_pool);

        let health_port = env::var("HEALTH_PORT").unwrap_or_else(|_| "8080".to_string());
        let health_bind_addr = format!("0.0.0.0:{}", health_port);
        tracing::info!("Health check server started on {}", health_bind_addr);
        axum::Server::bind(&health_bind_addr.parse().unwrap())
            .serve(app.into_make_service())
            .await
            .unwrap();
    });

    // Monitor spot/future in parallel
    let monitoring_tasks =
        spawn_monitoring_tasks(write_pool.clone(), read_pool.clone(), replication_delay_ms).await;
    handle_task_results(monitoring_tasks).await;
}

#[instrument(skip_all)]
async fn spawn_monitoring_tasks(
    write_pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    read_pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    replication_delay_ms: u64,
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
                read_pool.clone(),
                true,
                &DataType::Spot,
                cache.clone(),
            )),
        },
        // MonitoringTask {
        //     name: "Future Monitoring".to_string(),
        //     handle: tokio::spawn(onchain_monitor(
        //         read_pool.clone(),
        //         true,
        //         &DataType::Future,
        //         cache.clone(),
        //     )),
        // },
        MonitoringTask {
            name: "Publisher Monitoring".to_string(),
            handle: tokio::spawn(publisher_monitor(read_pool.clone(), false)),
        },
        // MonitoringTask {
        //     name: "API Monitoring".to_string(),
        //     handle: tokio::spawn(api_monitor(cache.clone())),
        // },
        MonitoringTask {
            name: "Pragma Indexing".to_string(),
            handle: tokio::spawn(pragma_indexing_monitor(
                write_pool.clone(),
                replication_delay_ms,
            )),
        },
    ];

    tasks
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
#[allow(dead_code)]
pub(crate) async fn api_monitor(cache: Cache<(String, u64), CoinPricesDTO>) {
    let monitoring_config = get_config(None).await;
    tracing::info!("[API] Monitoring API..");

    let mut interval = interval(Duration::from_secs(30));

    loop {
        interval.tick().await; // Wait for the next tick

        let mut tasks: Vec<_> = monitoring_config
            .sources(DataType::Spot)
            .iter()
            .flat_map(|(pair, _)| {
                let my_cache = cache.clone();
                vec![tokio::spawn(Box::pin(
                    processing::api::process_data_by_pair(pair.clone(), my_cache),
                ))]
            })
            .collect();

        tasks.push(tokio::spawn(Box::pin(
            processing::api::process_sequencer_data(),
        )));

        let results: Vec<_> = futures::future::join_all(tasks).await;
        log_tasks_results("API", results);
    }
}

#[instrument(skip_all)]
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
            tracing::warn!("[{data_type}] Indexers are still syncing");
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
                match data_type {
                    DataType::Spot => {
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
                }
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
pub(crate) async fn pragma_indexing_monitor(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    replication_delay_ms: u64,
) {
    tracing::info!("[INDEXING] Starting Pragma data indexer..");

    if replication_delay_ms > 0 {
        tracing::info!(
            "[INDEXING] Replication delay enabled: {}ms",
            replication_delay_ms
        );
    }

    // Set indexer as running
    INTERNAL_INDEXER_TRACKER.set_running(true).await;

    let mut restart_count = 0;
    const MAX_RESTARTS: u32 = 10; // Increased from 5 to 10
    const RESTART_DELAY: Duration = Duration::from_secs(30);
    const EXPONENTIAL_BACKOFF_MAX: Duration = Duration::from_secs(300); // 5 minutes max

    loop {
        // Start the indexer with retry logic
        let (mut event_rx, mut indexer_handle) = match start_pragma_indexer().await {
            Ok((rx, handle)) => {
                tracing::info!("[INDEXING] Successfully started Pragma indexer");
                restart_count = 0; // Reset restart count on successful start
                (rx, handle)
            }
            Err(e) => {
                restart_count += 1;
                let error_msg = format!(
                    "Failed to start Pragma indexer (attempt {}): {:?}",
                    restart_count, e
                );
                tracing::error!("{}", error_msg);

                // Record error in status tracker
                INTERNAL_INDEXER_TRACKER.record_error(error_msg).await;

                if restart_count >= MAX_RESTARTS {
                    tracing::error!("Max restart attempts reached. Stopping indexer.");
                    INTERNAL_INDEXER_TRACKER.set_running(false).await;
                    break;
                }

                // Exponential backoff with jitter
                let backoff_delay = std::cmp::min(
                    RESTART_DELAY * (2_u32.pow(restart_count.min(10))), // Cap exponential growth
                    EXPONENTIAL_BACKOFF_MAX,
                );
                let jitter = Duration::from_millis(fastrand::u64(0..1000));
                let total_delay = backoff_delay + jitter;

                tracing::info!(
                    "Restarting indexer in {:?} (backoff: {:?}, jitter: {:?})...",
                    total_delay,
                    backoff_delay,
                    jitter
                );
                sleep(total_delay).await;
                continue;
            }
        };

        // Create database handler
        let db_handler = DatabaseHandler::new(pool.clone());

        // Process events in batches
        let mut event_batch = Vec::new();
        let batch_size = 100;
        let mut batch_timeout = interval(Duration::from_secs(5));
        let mut last_processed_block = 0u64;
        let mut events_processed = 0u64;

        loop {
            tokio::select! {
                // Receive events from indexer
                event = event_rx.recv() => {
                    match event {
                        Some(event) => {
                            event_batch.push(event);
                            events_processed += 1;
                        }
                        None => {
                            tracing::warn!("Indexer event channel closed");
                            break;
                        }
                    }
                }

                // Process batch on timeout
                _ = batch_timeout.tick() => {
                    // Process batch if it has events
                    if !event_batch.is_empty() {
                        // Update last processed block before processing
                        if let Some(OutputEvent::Event { event_metadata, .. }) = event_batch.last() {
                            last_processed_block = event_metadata.block_number;
                        }

                        if let Err(e) = db_handler.process_indexed_events(std::mem::take(&mut event_batch)).await {
                            tracing::error!("Failed to process indexed events on timeout: {:?}", e);
                        } else if replication_delay_ms > 0 {
                            // Wait for replication to catch up after successful write
                            tracing::debug!("Waiting {}ms for replication after batch write", replication_delay_ms);
                            sleep(Duration::from_millis(replication_delay_ms)).await;
                        }
                    }
                }

                // Check if indexer task has finished
                result = &mut indexer_handle => {
                    match result {
                        Ok(Ok(())) => {
                            tracing::info!("Indexer task completed successfully");
                            break;
                        }
                        Ok(Err(e)) => {
                            tracing::error!("Indexer task failed: {:?}", e);
                            break;
                        }
                        Err(e) => {
                            tracing::error!("Indexer task panicked: {:?}", e);
                            break;
                        }
                    }
                }
            }

            // After select, process batch if needed
            // Process batch if it reached the desired size
            if event_batch.len() >= batch_size {
                // Update last processed block before processing
                if let Some(OutputEvent::Event { event_metadata, .. }) = event_batch.last() {
                    last_processed_block = event_metadata.block_number;
                }

                if let Err(e) = db_handler
                    .process_indexed_events(std::mem::take(&mut event_batch))
                    .await
                {
                    tracing::error!("Failed to process indexed events: {:?}", e);
                    // Continue processing other events even if one batch fails
                } else if replication_delay_ms > 0 {
                    // Wait for replication to catch up after successful write
                    tracing::debug!(
                        "Waiting {}ms for replication after batch write",
                        replication_delay_ms
                    );
                    sleep(Duration::from_millis(replication_delay_ms)).await;
                }
            }
        }

        // Log final statistics
        tracing::info!(
            "[INDEXING] Indexer session ended. Events processed: {}, Last block: {}",
            events_processed,
            last_processed_block
        );

        // Handle indexer failure - restart logic
        restart_count += 1;
        tracing::error!("Indexer failed (attempt {}): restarting...", restart_count);

        if restart_count >= MAX_RESTARTS {
            tracing::error!("Max restart attempts reached. Stopping indexer.");
            break;
        }

        tracing::info!("Restarting indexer in {:?}...", RESTART_DELAY);
        sleep(RESTART_DELAY).await;
    }
}

/// Simple health check endpoint
async fn health_check() -> Json<Value> {
    let status = if INTERNAL_INDEXER_TRACKER.is_healthy().await {
        "healthy"
    } else {
        "unhealthy"
    };

    Json(json!({
        "status": status,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "service": "pragma-monitoring"
    }))
}

/// Detailed health check endpoint
async fn detailed_health_check(
    State(pool): State<Pool<AsyncDieselConnectionManager<AsyncPgConnection>>>,
) -> Json<Value> {
    let indexer_status = INTERNAL_INDEXER_TRACKER.get_status().await;
    let is_healthy = INTERNAL_INDEXER_TRACKER.is_healthy().await;

    // Test database connectivity
    let db_healthy = test_database_connection(&pool).await.is_ok();

    Json(json!({
        "status": if is_healthy && db_healthy { "healthy" } else { "unhealthy" },
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "service": "pragma-monitoring",
        "components": {
            "indexer": {
                "running": indexer_status.is_running,
                "last_processed_block": indexer_status.last_processed_block,
                "events_processed": indexer_status.events_processed,
                "error_count": indexer_status.error_count,
                "last_error": indexer_status.last_error,
                "healthy": is_healthy
            },
            "database": {
                "healthy": db_healthy
            }
        }
    }))
}
