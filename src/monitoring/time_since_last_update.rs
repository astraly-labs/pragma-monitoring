use crate::{error::MonitoringError, types::Entry};
use chrono::{DateTime, LocalResult, TimeZone, Utc};
use std::time::SystemTime;

/// Calculate the time since the last update in seconds.
pub fn time_since_last_update<T: Entry>(query: &T) -> u64 {
    let datetime: DateTime<Utc> = TimeZone::from_utc_datetime(&Utc, &query.timestamp());
    let timestamp = datetime.timestamp();
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);

    now.unwrap().as_secs() - timestamp as u64
}

/// Calculate the raw time since the last update in seconds.
pub fn raw_time_since_last_update(timestamp: u64) -> Result<u64, MonitoringError> {
    let datetime = match Utc.timestamp_millis_opt(timestamp as i64) {
        LocalResult::Single(datetime) => datetime,
        _ => {
            return Err(MonitoringError::InvalidTimestamp(timestamp));
        }
    };
    let timestamp = datetime.timestamp();
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);

    Ok(now.unwrap().as_secs() - timestamp as u64)
}
