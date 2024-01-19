use crate::{
    config::get_config,
    constants::{API_PRICE_DEVIATION, API_TIME_SINCE_LAST_UPDATE},
    error::MonitoringError,
    monitoring::{
        price_deviation::raw_price_deviation, time_since_last_update::raw_time_since_last_update,
    },
    processing::common::query_pragma_api,
};

pub async fn process_data_by_pair(pair: String) -> Result<f64, MonitoringError> {
    // Query the Pragma API
    let config = get_config(None).await;
    let network_env = &config.network_str();

    let result = query_pragma_api(&pair, network_env).await?;

    log::info!("Processing data for pair: {}", pair);

    let normalized_price =
        result.price.parse::<f64>().unwrap() / 10_f64.powi(result.decimals as i32);

    let price_deviation = raw_price_deviation(&pair, normalized_price).await?;
    let time_since_last_update = raw_time_since_last_update(result.timestamp)?;

    API_PRICE_DEVIATION
        .with_label_values(&[network_env, &pair])
        .set(price_deviation);
    API_TIME_SINCE_LAST_UPDATE
        .with_label_values(&[network_env, &pair])
        .set(time_since_last_update as f64);

    Ok(price_deviation)
}
