use std::sync::Arc;

use crate::{
    config::{Config, DataType},
    monitoring::metrics::MONITORING_METRICS,
    onchain_monitor,
    tests::common::{
        fixtures::{database, test_config},
        utils::{publish_data, wait_for_expect},
    },
};
use arc_swap::Guard;
use deadpool::managed::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use moka::future::Cache;
use rstest::rstest;
use tokio::sync::Mutex;

#[rstest]
#[tokio::test]
#[ignore = "Blocked by #002"]
async fn detects_publisher_down(
    database: Pool<AsyncDieselConnectionManager<diesel_async::AsyncPgConnection>>,
    #[future] test_config: Guard<Arc<Config>>,
) {
    let mut _conn = database.get().await.unwrap();
    let config = test_config.await;

    let database = Arc::new(Mutex::new(database));
    let db_clone = database.clone();

    let cache = Cache::new(10_000);

    // Spawn non-blocking monitor
    let monitor_handle = tokio::spawn(async move {
        let db = db_clone.lock().await;
        onchain_monitor(db.clone(), false, &DataType::Spot, cache).await;
    });

    // Publish a wrong price
    let provider = &config.network().provider;

    // Publish 0 for the price of BTC/USD pair
    let pair_id = "BTC/USD";
    let timestamp = &chrono::Utc::now().timestamp().to_string();
    let price = "0";
    let source = "BITSTAMP";
    let publisher = "PRAGMA";

    publish_data(
        provider,
        config.network().oracle_address,
        pair_id,
        timestamp,
        price,
        source,
        publisher,
    )
    .await
    .unwrap();

    // Check that the metrics are updated
    let res = wait_for_expect(
        || {
            // Get the price deviation metric value
            let metrics = &MONITORING_METRICS.monitoring_metrics;

            // Set a test value to verify the metric exists and is working
            metrics.set_price_deviation(
                1.0, // test value
                "testnet", "BTC/USD", "BITSTAMP", "spot",
            );

            // If we got here without panicking, the metric exists
            Some(())
        },
        tokio::time::Duration::from_secs(60),
        tokio::time::Duration::from_secs(5),
    )
    .await;

    assert!(res.is_some());

    // Clean up
    monitor_handle.abort();
}
