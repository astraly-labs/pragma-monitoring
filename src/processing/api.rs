use crate::{
    constants::API_PRICE_DEVIATION, error::MonitoringError,
    monitoring::price_deviation::raw_price_deviation, processing::common::query_pragma_api,
};

pub async fn process_data_by_pair(pair: String) -> Result<f64, MonitoringError> {
    // Query the Pragma API
    let result = query_pragma_api(&pair).await?;

    log::info!("Processing data for pair: {}", pair);

    let normalized_price =
        result.price.parse::<f64>().unwrap() / 10_f64.powi(result.decimals as i32);

    let price_deviation = raw_price_deviation(&pair, normalized_price).await?;
    API_PRICE_DEVIATION
        .with_label_values(&[&pair])
        .set(price_deviation);

    Ok(price_deviation)
}
