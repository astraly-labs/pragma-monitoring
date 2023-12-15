extern crate diesel;
extern crate dotenv;

use crate::diesel::QueryDsl;
use crate::error::MonitoringError;
use crate::models::SpotEntry;
use crate::monitoring::time_last_update_pair_id::time_last_update_pair_id;
use crate::monitoring::time_last_update_source::time_since_last_update;
use crate::schema::spot_entry::dsl::*;

use bigdecimal::ToPrimitive;
use diesel::ExpressionMethods;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;
use diesel_async::RunQueryDsl;
use prometheus::{opts, register_gauge_vec, GaugeVec};

lazy_static::lazy_static! {
    static ref TIME_SINCE_LAST_UPDATE_SOURCE: GaugeVec = register_gauge_vec!(
        opts!("time_since_last_update_seconds", "Time since the last update in seconds."),
        &["pair", "source"]
    ).unwrap();
}

lazy_static::lazy_static! {
    static ref PAIR_PRICE: GaugeVec = register_gauge_vec!(
        opts!("pair_price", "Price of the pair from the source."),
        &["pair", "source"]
    ).unwrap();
}

lazy_static::lazy_static! {
    static ref TIME_SINCE_LAST_UPDATE_PAIR_ID: GaugeVec = register_gauge_vec!(
        opts!("time_since_last_update_pair_id", "Time since the last update in seconds."),
        &["pair"]
    ).unwrap();
}

pub async fn process_data_by_pair(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    pair: &str,
    decimals: u32,
) -> Result<u64, MonitoringError> {
    let mut conn = pool
        .get()
        .await
        .map_err(|_| MonitoringError::Connection("Failed to get connection".to_string()))?;

    let result: Result<SpotEntry, _> = spot_entry
        .filter(pair_id.eq(pair))
        .order(block_timestamp.desc())
        .first(&mut conn)
        .await;

    match result {
        Ok(data) => {
            let minute_since_last_publish = time_last_update_pair_id(&data).await;
            let time_labels = TIME_SINCE_LAST_UPDATE_PAIR_ID.with_label_values(&[pair]);
            time_labels.set(minute_since_last_publish as f64);
            Ok(minute_since_last_publish)
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn process_data_by_pair_and_source(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    pair: &str,
    src: &str,
    decimals: u32,
) -> Result<u64, MonitoringError> {
    let mut conn = pool
        .get()
        .await
        .map_err(|_| MonitoringError::Connection("Failed to get connection".to_string()))?;

    let filtered_by_source_result: Result<SpotEntry, _> = spot_entry
        .filter(pair_id.eq(pair))
        .filter(source.eq(src))
        .order(block_timestamp.desc())
        .first(&mut conn)
        .await;

    match filtered_by_source_result {
        Ok(data) => {
            let time = time_since_last_update(&data).await;
            let time_labels = TIME_SINCE_LAST_UPDATE_SOURCE.with_label_values(&[pair, src]);
            let price_labels = PAIR_PRICE.with_label_values(&[pair, src]);
            let price_as_f64 = data.price.to_f64().ok_or(MonitoringError::Price(
                "Failed to convert BigDecimal to f64".to_string(),
            ))?;
            let dec: i32 = 10;
            price_labels.set(price_as_f64 / (dec.pow(decimals)) as f64);
            time_labels.set(time as f64);
            Ok(time)
        }
        Err(e) => Err(e.into()),
    }
}
