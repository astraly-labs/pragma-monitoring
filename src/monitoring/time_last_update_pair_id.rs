use crate::models::SpotEntry;
use chrono::{DateTime, TimeZone, Utc};
use std::time::SystemTime;
pub async fn time_last_update_pair_id(query: &SpotEntry) -> u64 {
    let datetime: DateTime<Utc> = TimeZone::from_utc_datetime(&Utc, &query.timestamp.unwrap());
    let timestamp = datetime.timestamp();
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
    now.unwrap().as_secs() - timestamp as u64
}
