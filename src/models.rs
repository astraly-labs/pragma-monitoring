extern crate chrono;
extern crate bigdecimal;


use chrono::NaiveDateTime;
use diesel::{Queryable, Selectable};
use uuid::Uuid;




#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = crate::schema::storage)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Storage {
    pub id: Uuid,
    pub network: String,
    pub data_type: String,
    pub block_hash: String,
    pub block_number: i64,
    pub block_timestamp: NaiveDateTime,
    pub transaction_hash: String,
    pub source: Option<String>,
    pub price: Option<f32>,
}
