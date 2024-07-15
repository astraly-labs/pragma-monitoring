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

use crate::constants::{VRF_REQUESTS_COUNT, VRF_TIME_SINCE_LAST_HANDLE_REQUEST};
use crate::diesel::QueryDsl;
use crate::error::MonitoringError;
use crate::schema::vrf_requests::dsl as vrf_dsl;

pub async fn check_vrf_request_count(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
) -> Result<(), MonitoringError> {
    let mut conn = pool
        .get()
        .await
        .map_err(|_| MonitoringError::Connection("Failed to get connection".to_string()))?;

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
    let mut conn = pool
        .get()
        .await
        .map_err(|_| MonitoringError::Connection("Failed to get connection".to_string()))?;

    let networks: Vec<String> = vrf_dsl::vrf_requests
        .select(vrf_dsl::network)
        .distinct()
        .load::<String>(&mut conn)
        .await?;

    for network in networks {
        let last_handle_time: Option<chrono::NaiveDateTime> = vrf_dsl::vrf_requests
            .filter(vrf_dsl::network.eq(&network))
            .select(max(vrf_dsl::updated_at))
            .first::<Option<chrono::NaiveDateTime>>(&mut conn)
            .await?;

        if let Some(last_time) = last_handle_time {
            let now = Utc::now().naive_utc();
            let duration = now.signed_duration_since(last_time).num_seconds();
            VRF_TIME_SINCE_LAST_HANDLE_REQUEST
                .with_label_values(&[&network])
                .set(duration as f64);
        }
    }

    Ok(())
}
