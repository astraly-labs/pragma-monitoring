extern crate diesel;
extern crate dotenv;

use crate::config::Config;
use crate::diesel::QueryDsl;
use crate::error::MonitoringError;
use crate::models::SpotEntry;
use crate::monitoring::price_deviation::price_deviation;
use crate::monitoring::source_deviation::source_deviation;
use crate::monitoring::time_since_last_update::time_since_last_update;
use crate::schema::spot_entry::dsl::*;

use bigdecimal::ToPrimitive;
use diesel::ExpressionMethods;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;
use diesel_async::RunQueryDsl;
use lazy_static::lazy_static;
use prometheus::register_int_gauge_vec;
use prometheus::IntGaugeVec;
use prometheus::{opts, register_gauge_vec, GaugeVec};

lazy_static! {
    static ref TIME_SINCE_LAST_UPDATE_SOURCE: GaugeVec = register_gauge_vec!(
        opts!(
            "time_since_last_update_seconds",
            "Time since the last update in seconds."
        ),
        &["source"]
    )
    .unwrap();
}

lazy_static! {
    static ref PAIR_PRICE: GaugeVec = register_gauge_vec!(
        opts!("pair_price", "Price of the pair from the source."),
        &["pair", "source"]
    )
    .unwrap();
}

lazy_static! {
    static ref TIME_SINCE_LAST_UPDATE_PAIR_ID: GaugeVec = register_gauge_vec!(
        opts!(
            "time_since_last_update_pair_id",
            "Time since the last update in seconds."
        ),
        &["pair"]
    )
    .unwrap();
}

lazy_static! {
    static ref PRICE_DEVIATION: GaugeVec = register_gauge_vec!(
        opts!(
            "price_deviation",
            "Price deviation from the reference price."
        ),
        &["pair", "source"]
    )
    .unwrap();
}

lazy_static! {
    static ref PRICE_DEVIATION_SOURCE: GaugeVec = register_gauge_vec!(
        opts!(
            "price_deviation_source",
            "Price deviation from the reference price."
        ),
        &["source"]
    )
    .unwrap();
}

lazy_static! {
    static ref NUM_SOURCES: IntGaugeVec = register_int_gauge_vec!(
        opts!(
            "num_sources",
            "Number of sources that have published data for a pair."
        ),
        &["pair"]
    )
    .unwrap();
}

pub async fn process_data_by_pair(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    pair: String,
) -> Result<u64, MonitoringError> {
    let mut conn = pool
        .get()
        .await
        .map_err(|_| MonitoringError::Connection("Failed to get connection".to_string()))?;

    let result: Result<SpotEntry, _> = spot_entry
        .filter(pair_id.eq(pair.clone()))
        .order(block_timestamp.desc())
        .first(&mut conn)
        .await;

    log::info!("Processing data for pair: {}", pair);

    match result {
        Ok(data) => {
            let seconds_since_last_publish = time_since_last_update(&data);
            let time_labels = TIME_SINCE_LAST_UPDATE_PAIR_ID.with_label_values(&[&pair]);

            time_labels.set(seconds_since_last_publish as f64);

            Ok(seconds_since_last_publish)
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn process_data_by_pair_and_sources(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    pair: String,
    sources: Vec<String>,
    config: Config,
) -> Result<u64, MonitoringError> {
    let mut timestamps = Vec::new();

    let decimals = *config.decimals.get(&pair.clone()).unwrap();

    for src in sources {
        log::info!("Processing data for pair: {} and source: {}", pair, src);
        let res =
            process_data_by_pair_and_source(pool.clone(), &pair, &src, decimals, &config).await?;
        timestamps.push(res);
    }

    Ok(*timestamps.last().unwrap())
}

pub async fn process_data_by_pair_and_source(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    pair: &str,
    src: &str,
    decimals: u32,
    config: &Config,
) -> Result<u64, MonitoringError> {
    let mut conn = pool
        .get()
        .await
        .map_err(|_| MonitoringError::Connection("Failed to get connection".to_string()))?;

    let filtered_by_source_result: Result<SpotEntry, _> = spot_entry
        .filter(pair_id.eq(pair.clone()))
        .filter(source.eq(src))
        .order(block_timestamp.desc())
        .first(&mut conn)
        .await;

    match filtered_by_source_result {
        Ok(data) => {
            let time = time_since_last_update(&data);

            let time_labels = TIME_SINCE_LAST_UPDATE_SOURCE.with_label_values(&[src]);
            let price_labels = PAIR_PRICE.with_label_values(&[pair, src]);
            let deviation_labels = PRICE_DEVIATION.with_label_values(&[pair, src]);
            let source_deviation_labels = PRICE_DEVIATION_SOURCE.with_label_values(&[src]);
            let num_sources_labels = NUM_SOURCES.with_label_values(&[pair]);

            let price_as_f64 = data.price.to_f64().ok_or(MonitoringError::Price(
                "Failed to convert price to f64".to_string(),
            ))?;
            let normalized_price = price_as_f64 / (10_u64.pow(decimals)) as f64;

            let deviation = price_deviation(&data).await?;
            let (source_deviation, num_sources_aggregated) =
                source_deviation(&data, config.clone()).await?;

            price_labels.set(normalized_price);
            time_labels.set(time as f64);
            deviation_labels.set(deviation);
            source_deviation_labels.set(source_deviation);
            num_sources_labels.set(num_sources_aggregated as i64);

            Ok(time)
        }
        Err(e) => Err(e.into()),
    }
}
