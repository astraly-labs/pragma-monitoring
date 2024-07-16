extern crate diesel;
extern crate dotenv;

use bigdecimal::BigDecimal;
use chrono::Utc;
use deadpool::managed::Pool;
use diesel::dsl::max;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;
use diesel_async::RunQueryDsl;
use starknet::core::types::Felt;

use crate::config::get_config;
use crate::constants::{
    VRF_BALANCE, VRF_REQUESTS_COUNT, VRF_TIME_SINCE_LAST_HANDLE_REQUEST,
    VRF_TIME_SINCE_OLDEST_REQUEST_IN_PENDING_STATUS,
};
use crate::diesel::QueryDsl;
use crate::error::MonitoringError;
use crate::models::VrfRequest;
use crate::monitoring::get_on_chain_balance;
use crate::schema::vrf_requests::dsl as vrf_dsl;

#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
enum VrfStatus {
    Uninitialized,
    Received,
    Fulfilled,
    Cancelled,
    OutOfGas,
    Refunded,
}

impl From<VrfStatus> for BigDecimal {
    fn from(val: VrfStatus) -> Self {
        BigDecimal::from(val as i32)
    }
}

pub async fn check_vrf_balance(vrf_address: Felt) -> Result<(), MonitoringError> {
    let config = get_config(None).await;
    let balance = get_on_chain_balance(vrf_address).await?;
    let network_env = &config.network_str();
    VRF_BALANCE.with_label_values(&[network_env]).set(balance);
    Ok(())
}

pub async fn check_vrf_request_count(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
) -> Result<(), MonitoringError> {
    let config = get_config(None).await;
    let network = config.network_str();

    let mut conn = pool.get().await.map_err(MonitoringError::Connection)?;
    let counts = vrf_dsl::vrf_requests
        .filter(vrf_dsl::network.eq(&network))
        .group_by(vrf_dsl::status)
        .select((vrf_dsl::status, diesel::dsl::count(vrf_dsl::status)))
        .load::<(BigDecimal, i64)>(&mut conn)
        .await?;

    for (status, count) in counts {
        VRF_REQUESTS_COUNT
            .with_label_values(&[network, &status.to_string()])
            .set(count as f64);
    }

    Ok(())
}

pub async fn check_vrf_time_since_last_handle(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
) -> Result<(), MonitoringError> {
    let config = get_config(None).await;
    let network = config.network_str();

    let now = Utc::now().naive_utc();

    let mut conn = pool.get().await.map_err(MonitoringError::Connection)?;
    let last_handle_time: Option<chrono::NaiveDateTime> = vrf_dsl::vrf_requests
        .filter(vrf_dsl::network.eq(&network))
        .select(max(vrf_dsl::updated_at))
        .first::<Option<chrono::NaiveDateTime>>(&mut conn)
        .await?;

    if let Some(last_time) = last_handle_time {
        let duration = now.signed_duration_since(last_time).num_seconds();
        VRF_TIME_SINCE_LAST_HANDLE_REQUEST
            .with_label_values(&[network])
            .set(duration as f64);
    }
    Ok(())
}

pub async fn check_vrf_oldest_request_pending_status_duration(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
) -> Result<(), MonitoringError> {
    let config = get_config(None).await;
    let network = config.network_str();

    let now = Utc::now().naive_utc();

    let mut conn = pool.get().await.map_err(MonitoringError::Connection)?;
    let oldest_uninitialized_request: Option<VrfRequest> = vrf_dsl::vrf_requests
        .filter(vrf_dsl::network.eq(&network))
        .filter(vrf_dsl::status.eq(BigDecimal::from(VrfStatus::Uninitialized)))
        .order_by(vrf_dsl::created_at.asc())
        .first::<VrfRequest>(&mut conn)
        .await
        .optional()?;

    if let Some(request) = oldest_uninitialized_request {
        let duration = now.signed_duration_since(request.created_at).num_seconds();
        VRF_TIME_SINCE_OLDEST_REQUEST_IN_PENDING_STATUS
            .with_label_values(&[network])
            .set(duration as f64);
    }
    Ok(())
}
