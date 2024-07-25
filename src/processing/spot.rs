extern crate diesel;
extern crate dotenv;

use crate::config::get_config;
use crate::config::get_long_tail_threshold;
use crate::config::DataType;
use crate::config::NetworkName;
use crate::constants::NUM_SOURCES;
use crate::constants::ON_OFF_PRICE_DEVIATION;
use crate::constants::PAIR_PRICE;
use crate::constants::PRICE_DEVIATION;
use crate::constants::PRICE_DEVIATION_SOURCE;
use crate::constants::TIME_SINCE_LAST_UPDATE_PAIR_ID;
use crate::constants::TIME_SINCE_LAST_UPDATE_PUBLISHER;
use crate::constants::{LONG_TAIL_ASSET_DEVIATING_SOURCES, LONG_TAIL_ASSET_TOTAL_SOURCES};
use crate::diesel::QueryDsl;
use crate::error::MonitoringError;
use crate::models::SpotEntry;
use crate::monitoring::{
    on_off_price_deviation, price_deviation, source_deviation, time_since_last_update,
};

use crate::schema::mainnet_spot_entry::dsl as mainnet_dsl;
use crate::schema::spot_entry::dsl as testnet_dsl;

use bigdecimal::ToPrimitive;
use diesel::ExpressionMethods;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;
use diesel_async::RunQueryDsl;

pub async fn process_data_by_pair(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    pair: String,
) -> Result<u64, MonitoringError> {
    let mut conn = pool.get().await.map_err(MonitoringError::Connection)?;

    let config = get_config(None).await;

    let result: Result<SpotEntry, _> = match config.network().name {
        NetworkName::Testnet => {
            testnet_dsl::spot_entry
                .filter(testnet_dsl::pair_id.eq(pair.clone()))
                .order(testnet_dsl::block_timestamp.desc())
                .first(&mut conn)
                .await
        }
        NetworkName::Mainnet => {
            mainnet_dsl::mainnet_spot_entry
                .filter(mainnet_dsl::pair_id.eq(pair.clone()))
                .order(mainnet_dsl::block_timestamp.desc())
                .first(&mut conn)
                .await
        }
    };

    log::info!("Processing data for pair: {}", pair);

    match result {
        Ok(data) => {
            let network_env = &config.network_str();
            let data_type = "spot";

            let seconds_since_last_publish = time_since_last_update(&data);
            let time_labels =
                TIME_SINCE_LAST_UPDATE_PAIR_ID.with_label_values(&[network_env, &pair, data_type]);
            let num_sources_labels =
                NUM_SOURCES.with_label_values(&[network_env, &pair, data_type]);

            let (on_off_deviation, num_sources_aggregated) = on_off_price_deviation(
                pair.clone(),
                data.timestamp.timestamp() as u64,
                DataType::Spot,
            )
            .await?;

            ON_OFF_PRICE_DEVIATION
                .with_label_values(&[network_env, &pair.clone(), data_type])
                .set(on_off_deviation);
            time_labels.set(seconds_since_last_publish as f64);
            num_sources_labels.set(num_sources_aggregated as i64);

            Ok(seconds_since_last_publish)
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn process_data_by_pair_and_sources(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    pair: String,
    sources: Vec<String>,
) -> Result<u64, MonitoringError> {
    let mut timestamps = Vec::new();
    let config = get_config(None).await;

    let decimals = *config.decimals(DataType::Spot).get(&pair.clone()).unwrap();

    for src in sources {
        log::info!("Processing data for pair: {} and source: {}", pair, src);
        let res = process_data_by_pair_and_source(pool.clone(), &pair, &src, decimals).await?;
        timestamps.push(res);
    }

    Ok(*timestamps.last().unwrap())
}

pub async fn process_data_by_pair_and_source(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    pair: &str,
    src: &str,
    decimals: u32,
) -> Result<u64, MonitoringError> {
    let mut conn = pool.get().await.map_err(MonitoringError::Connection)?;

    let config = get_config(None).await;

    let filtered_by_source_result: Result<SpotEntry, _> = match config.network().name {
        NetworkName::Testnet => {
            testnet_dsl::spot_entry
                .filter(testnet_dsl::pair_id.eq(pair))
                .filter(testnet_dsl::source.eq(src))
                .order(testnet_dsl::block_timestamp.desc())
                .first(&mut conn)
                .await
        }
        NetworkName::Mainnet => {
            mainnet_dsl::mainnet_spot_entry
                .filter(mainnet_dsl::pair_id.eq(pair))
                .filter(mainnet_dsl::source.eq(src))
                .order(mainnet_dsl::block_timestamp.desc())
                .first(&mut conn)
                .await
        }
    };

    match filtered_by_source_result {
        Ok(data) => {
            let network_env = &config.network_str();
            let data_type = "spot";

            // Get the labels
            let price_labels = PAIR_PRICE.with_label_values(&[network_env, pair, src, data_type]);
            let deviation_labels =
                PRICE_DEVIATION.with_label_values(&[network_env, pair, src, data_type]);
            let source_deviation_labels =
                PRICE_DEVIATION_SOURCE.with_label_values(&[network_env, pair, src, data_type]);

            // Compute metrics
            let time = time_since_last_update(&data);
            let price_as_f64 = data.price.to_f64().ok_or(MonitoringError::Price(
                "Failed to convert price to f64".to_string(),
            ))?;
            let normalized_price = price_as_f64 / (10_u64.pow(decimals)) as f64;

            let deviation = price_deviation(&data, normalized_price).await?;
            let (source_deviation, _) = source_deviation(&data, normalized_price).await?;

            // Set the metrics
            price_labels.set(normalized_price);
            deviation_labels.set(deviation);
            source_deviation_labels.set(source_deviation);

            Ok(time)
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn process_data_by_publisher(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    publisher: String,
) -> Result<(), MonitoringError> {
    let mut conn = pool.get().await.map_err(MonitoringError::Connection)?;

    let config = get_config(None).await;

    let result: Result<SpotEntry, _> = match config.network().name {
        NetworkName::Testnet => {
            testnet_dsl::spot_entry
                .filter(testnet_dsl::publisher.eq(publisher.clone()))
                .order(testnet_dsl::block_timestamp.desc())
                .first(&mut conn)
                .await
        }
        NetworkName::Mainnet => {
            mainnet_dsl::mainnet_spot_entry
                .filter(mainnet_dsl::publisher.eq(publisher.clone()))
                .order(mainnet_dsl::block_timestamp.desc())
                .first(&mut conn)
                .await
        }
    };

    log::info!("Processing data for publisher: {}", publisher);

    match result {
        Ok(data) => {
            let network_env = &config.network_str();

            let seconds_since_last_publish = time_since_last_update(&data);
            let time_labels = TIME_SINCE_LAST_UPDATE_PUBLISHER.with_label_values(&[
                network_env,
                &publisher,
                "spot",
            ]);

            time_labels.set(seconds_since_last_publish as f64);

            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn process_long_tail_asset(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    pair: String,
    sources: Vec<String>,
) -> Result<u64, MonitoringError> {
    let config = get_config(None).await;
    let network_env = &config.network_str();
    let decimals = *config.decimals(DataType::Spot).get(&pair).unwrap();

    let mut timestamps = Vec::with_capacity(sources.len());
    let mut deviations = Vec::with_capacity(sources.len());

    for source in sources.iter() {
        log::info!("Processing data for pair: {} and source: {}", pair, source);
        let (timestamp, price_deviation) =
            get_price_deviation_for_source_from_chain(pool.clone(), &pair, source, decimals)
                .await?;
        timestamps.push(timestamp);
        deviations.push(price_deviation);
    }

    let threshold = get_long_tail_threshold(&pair, sources.len()).unwrap();

    // TODO: Maybe we should only consider recent sources, i.e in the last hour?
    // Count deviating sources
    let deviating_sources = deviations
        .iter()
        .filter(|&&deviation| deviation >= threshold)
        .count();

    // Set the metric for the number of deviating sources
    LONG_TAIL_ASSET_DEVIATING_SOURCES
        .with_label_values(&[network_env, &pair, "spot"])
        .set(deviating_sources as f64);

    // Set the metric for the total number of sources
    LONG_TAIL_ASSET_TOTAL_SOURCES
        .with_label_values(&[network_env, &pair, "spot"])
        .set(sources.len() as f64);

    Ok(timestamps.last().copied().unwrap())
}

pub async fn get_price_deviation_for_source_from_chain(
    pool: deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    pair: &str,
    source: &str,
    decimals: u32,
) -> Result<(u64, f64), MonitoringError> {
    let mut conn = pool.get().await.map_err(MonitoringError::Connection)?;

    let config = get_config(None).await;

    let filtered_by_source_result: Result<SpotEntry, _> = match config.network().name {
        NetworkName::Testnet => {
            testnet_dsl::spot_entry
                .filter(testnet_dsl::pair_id.eq(pair))
                .filter(testnet_dsl::source.eq(source))
                .order(testnet_dsl::block_timestamp.desc())
                .first(&mut conn)
                .await
        }
        NetworkName::Mainnet => {
            mainnet_dsl::mainnet_spot_entry
                .filter(mainnet_dsl::pair_id.eq(pair))
                .filter(mainnet_dsl::source.eq(source))
                .order(mainnet_dsl::block_timestamp.desc())
                .first(&mut conn)
                .await
        }
    };

    match filtered_by_source_result {
        Ok(data) => {
            let time = time_since_last_update(&data);
            let price_as_f64 = data.price.to_f64().ok_or(MonitoringError::Price(
                "Failed to convert price to f64".to_string(),
            ))?;
            let normalized_price = price_as_f64 / (10_u64.pow(decimals)) as f64;
            let (source_deviation, _) = source_deviation(&data, normalized_price).await?;
            Ok((time, source_deviation))
        }
        Err(e) => Err(e.into()),
    }
}
