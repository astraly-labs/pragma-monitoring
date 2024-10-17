use std::{collections::HashMap, fmt::Display};

use num_bigint::BigUint;
use starknet::core::types::Felt;
use tokio::task::JoinError;

use crate::constants::LONG_TAIL_ASSETS;

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
    for result in &results {
        match result {
            Ok(data) => match data {
                Ok(_) => log::info!("[{category}]: Task finished successfully",),
                Err(e) => log::error!("[{category}]: Task failed with error: {e}"),
            },
            Err(e) => log::error!("[{category}]: Task failed with error: {:?}", e),
        }
    }
}

/// Process or output the results of tokio monitoring tasks
#[allow(dead_code)]
pub(crate) fn log_monitoring_results(results: HashMap<String, Result<(), tokio::task::JoinError>>) {
    for (task_name, result) in results {
        match result {
            Ok(_) => log::info!("[{}] Monitoring completed successfully", task_name),
            Err(e) => log::error!("[{}] Monitoring failed: {:?}", task_name, e),
        }
    }
}

/// Check if the provided pair in a long tail asset.
#[allow(dead_code)]
pub(crate) fn is_long_tail_asset(pair: &str) -> bool {
    LONG_TAIL_ASSETS.contains_key(pair)
}

#[allow(dead_code)]
pub fn get_time_diff(timestamp: u64) -> f64 {
    let now = chrono::Utc::now().timestamp() as f64;

    now - timestamp as f64
}
