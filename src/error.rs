use diesel::result::Error as DieselError;
use diesel_async::pooled_connection::deadpool::PoolError;
use starknet::providers::ProviderError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MonitoringError {
    #[error("Price error: {0}")]
    #[allow(dead_code)]
    Price(String),

    #[error("Database error: {0}")]
    Database(#[from] DieselError),

    #[error("Connection pool error: {0}")]
    Connection(#[from] PoolError),

    #[error("External API error: {0}")]
    Api(String),

    #[error("Data conversion error: {0}")]
    Conversion(String),

    #[error("On-chain RPC error: {0}")]
    OnChain(String),

    #[error("Starknet provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("Invalid timestamp: {0}")]
    InvalidTimestamp(u64),
}
