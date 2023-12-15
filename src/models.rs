extern crate bigdecimal;
extern crate chrono;

use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::{Queryable, QueryableByName, Selectable};

#[derive(Debug, Queryable, Selectable, QueryableByName)]
#[diesel(table_name = crate::schema::spot_entry)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct SpotEntry {
    pub network: String,
    pub pair_id: String,
    pub data_id: String,
    pub block_hash: String,
    pub block_number: i64,
    pub block_timestamp: Option<NaiveDateTime>,
    pub transaction_hash: String,
    pub price: BigDecimal,
    pub timestamp: Option<chrono::NaiveDateTime>,
    pub publisher: String,
    pub source: String,
    pub volume: BigDecimal,
    pub _cursor: i64,
}

#[derive(Debug, Queryable, Selectable, QueryableByName)]
#[diesel(table_name = crate::schema::future_entry)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct FutureEntry {
    pub network: String,
    pub pair_id: String,
    pub data_id: String,
    pub block_hash: String,
    pub block_number: i64,
    pub block_timestamp: Option<NaiveDateTime>,
    pub transaction_hash: String,
    pub price: BigDecimal,
    pub timestamp: Option<chrono::NaiveDateTime>,
    pub publisher: String,
    pub source: String,
    pub volume: BigDecimal,
    pub expiration_timestamp: Option<chrono::NaiveDateTime>,
    pub _cursor: i64,
}
