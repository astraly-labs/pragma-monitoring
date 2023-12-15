use crate::{constants::COINGECKO_IDS, error::MonitoringError, models::SpotEntry};
use bigdecimal::ToPrimitive;
use coingecko::CoinGeckoClient;

/// Calculates the deviation of the price from a trusted API (Coingecko)
pub async fn price_deviation(query: &SpotEntry) -> Result<f64, MonitoringError> {
    let coingecko_client = CoinGeckoClient::default();

    let ids = &COINGECKO_IDS;

    let pair_id = query.pair_id.to_string();
    let coingecko_id = *ids.get(&pair_id).expect("Failed to get coingecko id");

    let coingecko_price = coingecko_client
        .price(&[coingecko_id], &["USD"], false, false, false, true)
        .await
        .map_err(|e| MonitoringError::Api(e.to_string()))?;

    // TODO: Check returned timestamp

    let published_price = query.price.to_f64().ok_or(MonitoringError::Conversion(
        "Failed to convert price to f64".to_string(),
    ))?;

    let reference_price = coingecko_price
        .get(coingecko_id)
        .expect("Failed to get coingecko price")
        .usd
        .ok_or(MonitoringError::Conversion(
            "Failed get usd price".to_string(),
        ))?;

    Ok((published_price - reference_price) / reference_price)
}
