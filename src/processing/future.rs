extern crate diesel;
extern crate dotenv;

use crate::config::DataType;
use crate::config::NetworkName;
use crate::config::get_config;
use crate::error::MonitoringError;
use crate::models::FutureEntry;
use crate::monitoring::metrics::MONITORING_METRICS;
use crate::monitoring::price_deviation::CoinPricesDTO;
use crate::monitoring::{
    on_off_price_deviation, price_deviation, source_deviation, time_since_last_update,
};
use crate::utils::get_db_connection_with_retry;
use diesel::QueryDsl;

use crate::schema::future_entry::dsl as testnet_dsl;
use crate::schema::mainnet_future_entry::dsl as mainnet_dsl;

use bigdecimal::ToPrimitive;
use diesel::ExpressionMethods;
use diesel_async::AsyncPgConnection;
use diesel_async::RunQueryDsl;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use moka::future::Cache;

pub async fn process_data_by_pair(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    pair: String,
    cache: Cache<(String, u64), CoinPricesDTO>,
) -> Result<u64, MonitoringError> {
    let mut conn =
        get_db_connection_with_retry(&pool, &format!("process_data_by_pair({})", pair)).await?;

    let config = get_config(None).await;

    let data: FutureEntry = match config.network().name {
        NetworkName::Testnet => {
            testnet_dsl::future_entry
                .filter(testnet_dsl::pair_id.eq(pair.clone()))
                .order(testnet_dsl::block_timestamp.desc())
                .first(&mut conn)
                .await?
        }
        NetworkName::Mainnet => {
            mainnet_dsl::mainnet_future_entry
                .filter(mainnet_dsl::pair_id.eq(pair.clone()))
                .order(mainnet_dsl::block_timestamp.desc())
                .first(&mut conn)
                .await?
        }
    };

    tracing::info!("Processing data for pair: {}", pair);

    let network_env = &config.network_str();
    let data_type = "future";

    let seconds_since_last_publish = time_since_last_update(&data);

    // Set time since last update metric
    MONITORING_METRICS
        .monitoring_metrics
        .set_time_since_last_update_pair_id(
            seconds_since_last_publish as f64,
            network_env,
            &pair,
            data_type,
        );

    let (on_off_deviation, num_sources_aggregated) = on_off_price_deviation(
        pair.clone(),
        data.timestamp.and_utc().timestamp() as u64,
        DataType::Future,
        cache,
    )
    .await?;

    // Set on/off deviation and num sources metrics
    MONITORING_METRICS
        .monitoring_metrics
        .set_on_off_price_deviation(on_off_deviation, network_env, &pair, data_type);
    MONITORING_METRICS.monitoring_metrics.set_num_sources(
        num_sources_aggregated as i64,
        network_env,
        &pair,
        data_type,
    );

    Ok(seconds_since_last_publish)
}

pub async fn process_data_by_pair_and_sources(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    pair: String,
    sources: Vec<String>,
    cache: Cache<(String, u64), CoinPricesDTO>,
) -> Result<u64, MonitoringError> {
    let mut timestamps = Vec::new();

    let config = get_config(None).await;

    let decimals = match config.decimals(DataType::Future).get(&pair.clone()) {
        Some(decimals) => *decimals,
        None => {
            tracing::error!("No decimals found for pair: {}", pair);
            return Err(MonitoringError::Conversion(format!(
                "No decimals found for pair: {}",
                pair
            )));
        }
    };

    for src in sources {
        tracing::info!("Processing data for pair: {} and source: {}", pair, src);
        let res =
            process_data_by_pair_and_source(pool.clone(), &pair, &src, decimals, cache.clone())
                .await?;
        timestamps.push(res);
    }

    match timestamps.last() {
        Some(timestamp) => Ok(*timestamp),
        None => {
            tracing::error!("No timestamps collected for pair: {}", pair);
            Err(MonitoringError::Conversion(format!(
                "No timestamps collected for pair: {}",
                pair
            )))
        }
    }
}

pub async fn process_data_by_pair_and_source(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    pair: &str,
    src: &str,
    decimals: u32,
    cache: Cache<(String, u64), CoinPricesDTO>,
) -> Result<u64, MonitoringError> {
    let mut conn = get_db_connection_with_retry(
        &pool,
        &format!("process_data_by_pair_and_source({}, {})", pair, src),
    )
    .await?;

    let config = get_config(None).await;

    let data: FutureEntry = match config.network().name {
        NetworkName::Testnet => {
            testnet_dsl::future_entry
                .filter(testnet_dsl::pair_id.eq(pair))
                .filter(testnet_dsl::source.eq(src))
                .order(testnet_dsl::block_timestamp.desc())
                .first(&mut conn)
                .await?
        }
        NetworkName::Mainnet => {
            mainnet_dsl::mainnet_future_entry
                .filter(mainnet_dsl::pair_id.eq(pair))
                .filter(mainnet_dsl::source.eq(src))
                .order(mainnet_dsl::block_timestamp.desc())
                .first(&mut conn)
                .await?
        }
    };

    let network_env = &config.network_str();
    let data_type = "future";

    // Compute metrics
    let time = time_since_last_update(&data);
    let price_as_f64 = data.price.to_f64().ok_or(MonitoringError::Price(
        "Failed to convert price to f64".to_string(),
    ))?;
    let normalized_price = price_as_f64 / (10_u64.pow(decimals)) as f64;

    let deviation = price_deviation(&data, normalized_price, cache.clone()).await?;
    let (source_deviation, _) = source_deviation(&data, normalized_price).await?;

    // Set all metrics using OTEL
    MONITORING_METRICS.monitoring_metrics.set_pair_price(
        normalized_price,
        network_env,
        pair,
        src,
        data_type,
    );
    MONITORING_METRICS.monitoring_metrics.set_price_deviation(
        deviation,
        network_env,
        pair,
        src,
        data_type,
    );
    MONITORING_METRICS
        .monitoring_metrics
        .set_price_deviation_source(source_deviation, network_env, pair, src, data_type);

    Ok(time)
}

pub async fn process_data_by_publisher(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    publisher: String,
) -> Result<(), MonitoringError> {
    let mut conn =
        get_db_connection_with_retry(&pool, &format!("process_data_by_publisher({})", publisher))
            .await?;

    let config = get_config(None).await;

    let data: FutureEntry = match config.network().name {
        NetworkName::Testnet => {
            testnet_dsl::future_entry
                .filter(testnet_dsl::publisher.eq(publisher.clone()))
                .order(testnet_dsl::block_timestamp.desc())
                .first(&mut conn)
                .await?
        }
        NetworkName::Mainnet => {
            mainnet_dsl::mainnet_future_entry
                .filter(mainnet_dsl::publisher.eq(publisher.clone()))
                .order(mainnet_dsl::block_timestamp.desc())
                .first(&mut conn)
                .await?
        }
    };

    tracing::info!("Processing data for publisher: {}", publisher);

    let network_env = &config.network_str();
    let seconds_since_last_publish = time_since_last_update(&data);

    MONITORING_METRICS
        .monitoring_metrics
        .set_time_since_last_update_publisher(
            seconds_since_last_publish as f64,
            network_env,
            &publisher,
            "future",
        );

    Ok(())
}
