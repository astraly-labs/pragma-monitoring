extern crate diesel;
extern crate dotenv;

use diesel_async::pooled_connection::deadpool::*;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use dotenv::dotenv;
use std::env;

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

#[tokio::main]
async fn main() {
    // Load environment variables from .env file
    dotenv().ok();

    // Define the pairs to monitor
    let pairs = vec![
        ("BTC/USD", "CEX", 8),
        ("ETH/USD", "CEX", 8),
        ("BTC/USD", "COINBASE", 8),
        ("ETH/USD", "COINBASE", 8),
        ("BTC/USD", "BITSTAMP", 8),
        ("ETH/USD", "BITSTAMP", 8),
        ("BTC/USD", "OKX", 8),
        ("ETH/USD", "OKX", 8),
        ("BTC/USD", "GECKOTERMINAL", 8),
        ("ETH/USD", "GECKOTERMINAL", 8),
        ("BTC/USD", "KAIKO", 8),
        ("ETH/USD", "KAIKO", 8),
    ];

    tokio::spawn(server::run_metrics_server());

    let database_url: String = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(database_url);
    let pool = Pool::builder(config).build().unwrap();

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

    loop {
        interval.tick().await; // Wait for the next tick

        let tasks: Vec<_> = pairs
            .clone()
            .into_iter()
            .flat_map(|(pair, srce, decimals)| {
                vec![
                    tokio::spawn(Box::pin(process_data::process_data_by_pair(
                        pool.clone(),
                        pair,
                        decimals,
                    ))),
                    tokio::spawn(Box::pin(process_data::process_data_by_pair_and_source(
                        pool.clone(),
                        pair,
                        srce,
                        decimals,
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
                Ok(data) => println!("Task succeeded with data: {:?}", data),
                Err(e) => eprintln!("Task failed with error: {:?}", e),
            }
        }
    }
}
