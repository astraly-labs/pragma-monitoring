use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};

use crate::{
    config::DataType,
    models::{FutureEntry, SpotEntry},
};

pub trait Entry {
    fn pair_id(&self) -> &str;
    fn source(&self) -> &str;
    fn timestamp(&self) -> DateTime<Utc>;
    fn block_number(&self) -> i64;
    fn price(&self) -> BigDecimal;
    fn expiration_timestamp(&self) -> Option<DateTime<Utc>>;
    fn data_type(&self) -> DataType;
}

impl Entry for SpotEntry {
    fn pair_id(&self) -> &str {
        &self.pair_id
    }

    fn source(&self) -> &str {
        &self.source
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn block_number(&self) -> i64 {
        self.block_number
    }

    fn price(&self) -> BigDecimal {
        self.price.clone()
    }

    fn expiration_timestamp(&self) -> Option<DateTime<Utc>> {
        None
    }

    fn data_type(&self) -> DataType {
        DataType::Spot
    }
}

impl Entry for FutureEntry {
    fn pair_id(&self) -> &str {
        &self.pair_id
    }

    fn source(&self) -> &str {
        &self.source
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn block_number(&self) -> i64 {
        self.block_number
    }

    fn price(&self) -> BigDecimal {
        self.price.clone()
    }

    fn expiration_timestamp(&self) -> Option<DateTime<Utc>> {
        self.expiration_timestamp
    }

    fn data_type(&self) -> DataType {
        DataType::Future
    }
}
