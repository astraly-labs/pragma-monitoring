use std::{error::Error as StdError, fmt};

#[derive(Debug)]
pub enum MonitoringError {
    Price(String),
    Time(String),
    Database(diesel::result::Error),
    Connection(String),
}

impl StdError for MonitoringError {}

impl fmt::Display for MonitoringError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MonitoringError::Price(e) => write!(f, "Price Error: {}", e),
            MonitoringError::Time(e) => write!(f, "Time Error: {}", e),
            MonitoringError::Database(e) => write!(f, "Database Error: {}", e),
            MonitoringError::Connection(e) => write!(f, "Connection Error: {}", e),
        }
    }
}

// Convert diesel error to our custom error
impl From<diesel::result::Error> for MonitoringError {
    fn from(err: diesel::result::Error) -> MonitoringError {
        MonitoringError::Database(err)
    }
}
