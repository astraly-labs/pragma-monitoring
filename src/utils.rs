use std::{collections::HashMap, fmt::Display, time::Duration};

use deadpool::managed::Pool;
use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use num_bigint::BigUint;
use starknet::core::types::Felt;
use tokio::task::JoinError;
use tokio::time::sleep;

use crate::error::MonitoringError;

#[derive(Debug)]
pub enum FeltConversionError {
    Overflow,
}

pub(crate) fn try_felt_to_u32(felt: &Felt) -> Result<u32, FeltConversionError> {
    let biguint = felt.to_biguint();
    let u32_max = BigUint::from(u32::MAX);

    // assert!(biguint <= u32_max, "Felt value doesn't fit in u32");
    if biguint > u32_max {
        return Err(FeltConversionError::Overflow);
    }

    // Convert to u32, safe due to previous check
    Ok(biguint.to_u32_digits()[0])
}

/// Process or output the results of tokio tasks
#[allow(dead_code)]
pub(crate) fn log_tasks_results<T, E: Display>(
    category: &str,
    results: Vec<Result<Result<T, E>, JoinError>>,
) {
    let total = results.len();
    let success = results
        .iter()
        .filter(|r| matches!(r, Ok(Ok(_))))
        .count();

    if success == total {
        tracing::info!("✅ [{category}] All {total} tasks completed");
    } else {
        tracing::warn!("⚠️  [{category}] {success}/{total} tasks succeeded");
        for result in &results {
            if let Ok(Err(e)) = result {
                tracing::error!("   ❌ [{category}] Error: {e}");
            } else if let Err(e) = result {
                tracing::error!("   ❌ [{category}] Task panic: {:?}", e);
            }
        }
    }
}

/// Process or output the results of tokio monitoring tasks
#[allow(dead_code)]
pub(crate) fn log_monitoring_results(results: HashMap<String, Result<(), tokio::task::JoinError>>) {
    tracing::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    tracing::info!("📊 Monitoring Tasks Summary");
    tracing::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    for (task_name, result) in results {
        match result {
            Ok(_) => tracing::info!("   ✅ {} completed", task_name),
            Err(e) => tracing::error!("   ❌ {} failed: {:?}", task_name, e),
        }
    }
}

/// Get database connection with retry logic
#[allow(dead_code)]
pub(crate) async fn get_db_connection_with_retry(
    pool: &Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
    operation: &str,
) -> Result<
    deadpool::managed::Object<AsyncDieselConnectionManager<AsyncPgConnection>>,
    MonitoringError,
> {
    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY: Duration = Duration::from_millis(500);

    for attempt in 1..=MAX_RETRIES {
        match pool.get().await {
            Ok(conn) => return Ok(conn),
            Err(e) => {
                if attempt < MAX_RETRIES {
                    tracing::warn!(
                        "Failed to get database connection for {} (attempt {}/{}): {:?}. Retrying...",
                        operation,
                        attempt,
                        MAX_RETRIES,
                        e
                    );
                    sleep(RETRY_DELAY).await;
                } else {
                    tracing::error!(
                        "Failed to get database connection for {} after {} attempts: {:?}",
                        operation,
                        MAX_RETRIES,
                        e
                    );
                    return Err(MonitoringError::Connection(e));
                }
            }
        }
    }
    unreachable!()
}
