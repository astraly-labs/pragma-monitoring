// @generated automatically by Diesel CLI.

diesel::table! {
    future_entry (data_id) {
        #[max_length = 255]
        network -> Nullable<Varchar>,
        #[max_length = 255]
        pair_id -> Nullable<Varchar>,
        #[max_length = 255]
        data_id -> Varchar,
        #[max_length = 255]
        block_hash -> Nullable<Varchar>,
        block_number -> Nullable<Int8>,
        block_timestamp -> Nullable<Timestamp>,
        #[max_length = 255]
        transaction_hash -> Nullable<Varchar>,
        price -> Nullable<Numeric>,
        timestamp -> Nullable<Timestamp>,
        #[max_length = 255]
        publisher -> Nullable<Varchar>,
        #[max_length = 255]
        source -> Nullable<Varchar>,
        volume -> Nullable<Numeric>,
        expiration_timestamp -> Nullable<Timestamp>,
        _cursor -> Nullable<Int8>,
    }
}

diesel::table! {
    mainnet_future_entry (data_id) {
        #[max_length = 255]
        network -> Nullable<Varchar>,
        #[max_length = 255]
        pair_id -> Nullable<Varchar>,
        #[max_length = 255]
        data_id -> Varchar,
        #[max_length = 255]
        block_hash -> Nullable<Varchar>,
        block_number -> Nullable<Int8>,
        block_timestamp -> Nullable<Timestamp>,
        #[max_length = 255]
        transaction_hash -> Nullable<Varchar>,
        price -> Nullable<Numeric>,
        timestamp -> Nullable<Timestamp>,
        #[max_length = 255]
        publisher -> Nullable<Varchar>,
        #[max_length = 255]
        source -> Nullable<Varchar>,
        volume -> Nullable<Numeric>,
        expiration_timestamp -> Nullable<Timestamp>,
        _cursor -> Nullable<Int8>,
    }
}

diesel::table! {
    mainnet_spot_entry (data_id) {
        #[max_length = 255]
        network -> Nullable<Varchar>,
        #[max_length = 255]
        pair_id -> Nullable<Varchar>,
        #[max_length = 255]
        data_id -> Varchar,
        #[max_length = 255]
        block_hash -> Nullable<Varchar>,
        block_number -> Nullable<Int8>,
        block_timestamp -> Nullable<Timestamp>,
        #[max_length = 255]
        transaction_hash -> Nullable<Varchar>,
        price -> Nullable<Numeric>,
        timestamp -> Nullable<Timestamp>,
        #[max_length = 255]
        publisher -> Nullable<Varchar>,
        #[max_length = 255]
        source -> Nullable<Varchar>,
        volume -> Nullable<Numeric>,
        _cursor -> Nullable<Int8>,
    }
}

diesel::table! {
    spot_entry (timestamp) {
        #[max_length = 255]
        network -> Nullable<Varchar>,
        #[max_length = 255]
        pair_id -> Nullable<Varchar>,
        #[max_length = 255]
        data_id -> Nullable<Varchar>,
        #[max_length = 255]
        block_hash -> Nullable<Varchar>,
        block_number -> Nullable<Int8>,
        block_timestamp -> Nullable<Timestamp>,
        #[max_length = 255]
        transaction_hash -> Nullable<Varchar>,
        price -> Nullable<Numeric>,
        timestamp -> Timestamp,
        #[max_length = 255]
        publisher -> Nullable<Varchar>,
        #[max_length = 255]
        source -> Nullable<Varchar>,
        volume -> Nullable<Numeric>,
        _cursor -> Nullable<Int8>,
    }
}

diesel::table! {
    vrf_requests (data_id) {
        #[max_length = 255]
        network -> Nullable<Varchar>,
        request_id -> Nullable<Numeric>,
        seed -> Nullable<Numeric>,
        created_at -> Nullable<Timestamp>,
        created_at_tx -> Nullable<Varchar>,
        #[max_length = 255]
        callback_address -> Nullable<Varchar>,
        callback_fee_limit -> Nullable<Numeric>,
        num_words -> Nullable<Numeric>,
        requestor_address -> Nullable<Varchar>,
        updated_at -> Nullable<Timestamp>,
        updated_at_tx -> Nullable<Varchar>,
        status -> Nullable<Numeric>,
        minimum_block_number -> Nullable<Numeric>,
        _cursor -> Nullable<Int8range>,
        data_id -> Varchar,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    future_entry,
    mainnet_future_entry,
    mainnet_spot_entry,
    spot_entry,
    vrf_requests,
);
