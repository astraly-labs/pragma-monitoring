extern crate diesel;
extern crate dotenv;

use config::get_config;
use diesel_async::pooled_connection::deadpool::*;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;

use dotenv::dotenv;
use std::env;

use crate::process_data::is_syncing;

// Configuration
mod config;
// Error handling
mod error;
// Database models
mod models;
// Monitoring functions
mod monitoring;
// Processing functions
mod process_data;
// Server
mod server;
// Database schema
mod schema;
// Constants
mod constants;

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

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

    monitor(&pool, &mut interval, true).await;
}

pub(crate) async fn monitor(
    pool: &deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    interval: &mut tokio::time::Interval,
    wait_for_syncing: bool,
) {
    let monitoring_config = get_config(None).await;

    loop {
        interval.tick().await; // Wait for the next tick

        // Skip if indexer is still syncing
        if wait_for_syncing {
            if let Some(blocks_left) =
                is_syncing(pool.clone(), monitoring_config.network().provider.clone())
                    .await
                    .unwrap()
            {
                log::info!("Indexer is still syncing ♻️ blocks left: {}", blocks_left);
                continue;
            }
        }

        let tasks: Vec<_> = monitoring_config
            .sources()
            .iter()
            .flat_map(|(pair, sources)| {
                vec![
                    tokio::spawn(Box::pin(process_data::process_data_by_pair(
                        pool.clone(),
                        pair.clone(),
                    ))),
                    tokio::spawn(Box::pin(process_data::process_data_by_pair_and_sources(
                        pool.clone(),
                        pair.clone(),
                        sources.to_vec(),
                    ))),
                ]
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(tasks)
            .await
            .into_iter()
            .map(|task| task.unwrap()) // task.unwrap() is used to get the Result returned by process_data
            .collect();

        // Process or output the results
        for result in &results {
            match result {
                Ok(data) => log::info!("Task succeeded with data: {:?}", data),
                Err(e) => log::error!("Task failed with error: {:?}", e),
            }
        }
    }
}
