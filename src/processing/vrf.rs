extern crate diesel;
extern crate dotenv;

use bigdecimal::BigDecimal;
use chrono::Utc;
use deadpool::managed::Pool;
use diesel::dsl::max;
use diesel::ExpressionMethods;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;
use diesel_async::RunQueryDsl;

use crate::constants::{
    VRF_REQUESTS_COUNT, VRF_TIME_IN_RECEIVED_STATUS, VRF_TIME_SINCE_LAST_HANDLE_REQUEST,
};
use crate::diesel::QueryDsl;
use crate::error::MonitoringError;
use crate::models::VrfRequest;
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

pub async fn check_vrf_request_count(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
) -> Result<(), MonitoringError> {
    let mut conn = pool.get().await.map_err(MonitoringError::Connection)?;

    let networks: Vec<String> = vrf_dsl::vrf_requests
        .select(vrf_dsl::network)
        .distinct()
        .load::<String>(&mut conn)
        .await?;

    for network in networks {
        let counts = vrf_dsl::vrf_requests
            .filter(vrf_dsl::network.eq(&network))
            .group_by(vrf_dsl::status)
            .select((vrf_dsl::status, diesel::dsl::count(vrf_dsl::status)))
            .load::<(BigDecimal, i64)>(&mut conn)
            .await?;

        for (status, count) in counts {
            VRF_REQUESTS_COUNT
                .with_label_values(&[&network, &status.to_string()])
                .set(count as f64);
        }
    }

    Ok(())
}

pub async fn check_vrf_time_since_last_handle(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
) -> Result<(), MonitoringError> {
    let mut conn = pool.get().await.map_err(MonitoringError::Connection)?;

    let networks: Vec<String> = vrf_dsl::vrf_requests
        .select(vrf_dsl::network)
        .distinct()
        .load::<String>(&mut conn)
        .await?;

    let now = Utc::now().naive_utc();
    for network in networks {
        let last_handle_time: Option<chrono::NaiveDateTime> = vrf_dsl::vrf_requests
            .filter(vrf_dsl::network.eq(&network))
            .select(max(vrf_dsl::updated_at))
            .first::<Option<chrono::NaiveDateTime>>(&mut conn)
            .await?;

        if let Some(last_time) = last_handle_time {
            let duration = now.signed_duration_since(last_time).num_seconds();
            VRF_TIME_SINCE_LAST_HANDLE_REQUEST
                .with_label_values(&[&network])
                .set(duration as f64);
        }
    }
    Ok(())
}

pub async fn check_vrf_received_status_duration(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
) -> Result<(), MonitoringError> {
    let mut conn = pool.get().await.map_err(MonitoringError::Connection)?;

    // Clear the gauge so we only check current received requests for each network
    VRF_TIME_IN_RECEIVED_STATUS.reset();

    let networks: Vec<String> = vrf_dsl::vrf_requests
        .select(vrf_dsl::network)
        .distinct()
        .load::<String>(&mut conn)
        .await?;

    let now = Utc::now().naive_utc();

    for network in networks {
        let requests: Vec<VrfRequest> = vrf_dsl::vrf_requests
            .filter(vrf_dsl::network.eq(&network))
            .filter(vrf_dsl::status.eq(BigDecimal::from(VrfStatus::Received)))
            .load::<VrfRequest>(&mut conn)
            .await?;

        for request in requests {
            let duration = now.signed_duration_since(request.created_at).num_seconds();
            VRF_TIME_IN_RECEIVED_STATUS
                .with_label_values(&[&network, &request.request_id.to_string()])
                .set(duration as f64);
        }
    }

    Ok(())
}
