extern crate diesel;
extern crate dotenv;

use config::parse_pairs;
use config::Config;
use config::NetworkName;
use diesel_async::pooled_connection::deadpool::*;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use dotenv::dotenv;
use starknet::core::types::FieldElement;
use std::env;
use std::str::FromStr;

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

#[tokio::main]
async fn main() {
    env_logger::init();

    // Load environment variables from .env file
    dotenv().ok();

    // Define the pairs to monitor
    let network = std::env::var("NETWORK").expect("NETWORK must be set");
    let oracle_address = std::env::var("ORACLE_ADDRESS").expect("ORACLE_ADDRESS must be set");
    let pairs = std::env::var("PAIRS").expect("PAIRS must be set");

    let monitoring_config = Config::new(
        NetworkName::from_str(&network).expect("Invalid network name"),
        FieldElement::from_hex_be(&oracle_address).expect("Invalid oracle address"),
        parse_pairs(&pairs),
    )
    .await;

    tokio::spawn(server::run_metrics_server());

    let database_url: String = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(database_url);
    let pool = Pool::builder(config).build().unwrap();

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

    loop {
        interval.tick().await; // Wait for the next tick

        let tasks: Vec<_> = monitoring_config
            .clone()
            .sources
            .into_iter()
            .flat_map(|(pair, sources)| {
                vec![
                    tokio::spawn(Box::pin(process_data::process_data_by_pair(
                        pool.clone(),
                        pair.clone(),
                    ))),
                    tokio::spawn(Box::pin(process_data::process_data_by_pair_and_sources(
                        pool.clone(),
                        pair.clone(),
                        sources,
                        monitoring_config.clone(),
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
