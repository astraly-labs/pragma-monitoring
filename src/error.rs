use diesel::result::Error as DieselError;
use diesel_async::pooled_connection::deadpool::PoolError;
use starknet::providers::ProviderError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MonitoringError {
    #[error("Price error: {0}")]
    Price(String),

    #[error("Database error: {0}")]
    Database(#[from] DieselError),

    #[error("Connection error: {0}")]
    Connection(#[from] PoolError),

    #[error("API error: {0}")]
    Api(String),

    #[error("Conversion error: {0}")]
    Conversion(String),

    #[error("OnChain error: {0}")]
    OnChain(String),

    #[error("Provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("Invalid timestamp: {0}")]
    InvalidTimestamp(u64),
}
