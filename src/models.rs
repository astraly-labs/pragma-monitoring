// Generated by diesel_ext

#![allow(unused)]
#![allow(clippy::all)]

use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::{Queryable, QueryableByName, Selectable};
use num_bigint::BigInt;
use std::ops::Bound;

#[derive(Debug, Queryable, Selectable, QueryableByName)]
#[diesel(table_name = crate::schema::spot_entry)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct SpotEntry {
    pub network: String,
    pub pair_id: String,
    pub data_id: String,
    pub block_hash: String,
    pub block_number: i64,
    pub block_timestamp: NaiveDateTime,
    pub transaction_hash: String,
    pub price: BigDecimal,
    pub timestamp: NaiveDateTime,
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
    pub block_timestamp: NaiveDateTime,
    pub transaction_hash: String,
    pub price: BigDecimal,
    pub timestamp: NaiveDateTime,
    pub publisher: String,
    pub source: String,
    pub volume: BigDecimal,
    pub expiration_timestamp: Option<NaiveDateTime>,
    pub _cursor: i64,
}

#[derive(Queryable, Debug, QueryableByName, Selectable)]
#[diesel(primary_key(data_id))]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = crate::schema::spot_checkpoints)]
pub struct SpotCheckpoint {
    pub network: String,
    pub pair_id: String,
    pub data_id: String,
    pub block_hash: String,
    pub block_number: i64,
    pub block_timestamp: NaiveDateTime,
    pub transaction_hash: String,
    pub price: BigDecimal,
    pub sender_address: String,
    pub aggregation_mode: BigDecimal,
    pub _cursor: i64,
    pub timestamp: NaiveDateTime,
    pub nb_sources_aggregated: BigDecimal,
}

#[derive(Queryable, Debug, QueryableByName, Selectable)]
#[diesel(primary_key(data_id))]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = crate::schema::vrf_requests)]
pub struct VrfRequest {
    pub network: String,
    pub request_id: BigDecimal,
    pub seed: BigDecimal,
    pub created_at: NaiveDateTime,
    pub created_at_tx: String,
    pub callback_address: String,
    pub callback_fee_limit: BigDecimal,
    pub num_words: BigDecimal,
    pub requestor_address: String,
    pub updated_at: NaiveDateTime,
    pub updated_at_tx: String,
    pub status: BigDecimal,
    pub minimum_block_number: BigDecimal,
    pub _cursor: (Bound<i64>, Bound<i64>),
    pub data_id: String,
}

#[derive(Queryable, Debug, QueryableByName, Selectable)]
#[diesel(primary_key(data_id))]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = crate::schema::oo_requests)]
pub struct OORequest {
    pub network: String,
    pub data_id: String,
    pub assertion_id: String,
    pub domain_id: String,
    pub claim: String,
    pub asserter: String,
    pub disputer: Option<String>,
    pub disputed: Option<bool>,
    pub dispute_id: Option<String>,
    pub callback_recipient: String,
    pub escalation_manager: String,
    pub caller: String,
    pub expiration_timestamp: NaiveDateTime,
    pub settled: Option<bool>,
    pub settlement_resolution: Option<bool>,
    pub settle_caller: Option<String>,
    pub currency: String,
    pub bond: BigDecimal,
    pub _cursor: (Bound<i64>, Bound<i64>),
    pub identifier: String,
    pub updated_at: NaiveDateTime,
    pub updated_at_tx: String,
}
