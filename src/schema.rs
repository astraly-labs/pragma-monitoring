// @generated automatically by Diesel CLI.

diesel::table! {
    future_entry (data_id) {
        #[max_length = 255]
        network -> Varchar,
        #[max_length = 255]
        pair_id -> Varchar,
        #[max_length = 255]
        data_id -> Varchar,
        #[max_length = 255]
        block_hash -> Varchar,
        block_number -> Int8,
        block_timestamp -> Timestamptz,
        #[max_length = 255]
        transaction_hash -> Varchar,
        price -> Numeric,
        timestamp -> Timestamptz,
        #[max_length = 255]
        publisher -> Varchar,
        #[max_length = 255]
        source -> Varchar,
        volume -> Numeric,
        expiration_timestamp -> Nullable<Timestamptz>,
        _cursor -> Int8,
    }
}

diesel::table! {
    mainnet_future_entry (data_id) {
        #[max_length = 255]
        network -> Varchar,
        #[max_length = 255]
        pair_id -> Varchar,
        #[max_length = 255]
        data_id -> Varchar,
        #[max_length = 255]
        block_hash -> Varchar,
        block_number -> Int8,
        block_timestamp -> Timestamptz,
        #[max_length = 255]
        transaction_hash -> Varchar,
        price -> Numeric,
        timestamp -> Timestamptz,
        #[max_length = 255]
        publisher -> Varchar,
        #[max_length = 255]
        source -> Varchar,
        volume -> Numeric,
        expiration_timestamp -> Nullable<Timestamptz>,
        _cursor -> Int8,
    }
}

diesel::table! {
    mainnet_spot_entry (data_id) {
        #[max_length = 255]
        network -> Varchar,
        #[max_length = 255]
        pair_id -> Varchar,
        #[max_length = 255]
        data_id -> Varchar,
        #[max_length = 255]
        block_hash -> Varchar,
        block_number -> Int8,
        block_timestamp -> Timestamptz,
        #[max_length = 255]
        transaction_hash -> Varchar,
        price -> Numeric,
        timestamp -> Timestamptz,
        #[max_length = 255]
        publisher -> Varchar,
        #[max_length = 255]
        source -> Varchar,
        volume -> Numeric,
        _cursor -> Int8,
    }
}

diesel::table! {
    spot_entry (data_id) {
        #[max_length = 255]
        network -> Varchar,
        #[max_length = 255]
        pair_id -> Varchar,
        #[max_length = 255]
        data_id -> Varchar,
        #[max_length = 255]
        block_hash -> Varchar,
        block_number -> Int8,
        block_timestamp -> Timestamptz,
        #[max_length = 255]
        transaction_hash -> Varchar,
        price -> Numeric,
        timestamp -> Timestamptz,
        #[max_length = 255]
        publisher -> Varchar,
        #[max_length = 255]
        source -> Varchar,
        volume -> Numeric,
        _cursor -> Int8,
    }
}

diesel::table! {
    mainnet_spot_checkpoints (pair_id) {
        #[max_length = 255]
        network -> Varchar,
        #[max_length = 255]
        pair_id -> Varchar,
        #[max_length = 255]
        data_id -> Varchar,
        #[max_length = 255]
        block_hash -> Varchar,
        block_number -> Int8,
        block_timestamp -> Timestamptz,
        #[max_length = 255]
        transaction_hash -> Varchar,
        price -> Numeric,
        #[max_length = 255]
        sender_address -> Varchar,
        aggregation_mode -> Numeric,
        _cursor -> Int8,
        timestamp -> Timestamptz,
        nb_sources_aggregated -> Numeric,
    }
}

diesel::table! {
    spot_checkpoints (data_id) {
        #[max_length = 255]
        network -> Varchar,
        #[max_length = 255]
        pair_id -> Varchar,
        #[max_length = 255]
        data_id -> Varchar,
        #[max_length = 255]
        block_hash -> Varchar,
        block_number -> Int8,
        block_timestamp -> Timestamptz,
        #[max_length = 255]
        transaction_hash -> Varchar,
        price -> Numeric,
        #[max_length = 255]
        sender_address -> Varchar,
        aggregation_mode -> Numeric,
        _cursor -> Int8,
        timestamp -> Timestamptz,
        nb_sources_aggregated -> Numeric,
    }
}

diesel::table! {
    vrf_requests (data_id) {
        #[max_length = 255]
        network -> Varchar,
        request_id -> Numeric,
        seed -> Numeric,
        created_at -> Timestamptz,
        created_at_tx -> Varchar,
        #[max_length = 255]
        callback_address -> Varchar,
        callback_fee_limit -> Numeric,
        num_words -> Numeric,
        requestor_address -> Varchar,
        updated_at -> Timestamptz,
        updated_at_tx -> Varchar,
        status -> Numeric,
        minimum_block_number -> Numeric,
        _cursor -> Int8range,
        data_id -> Varchar,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    future_entry,
    mainnet_future_entry,
    mainnet_spot_checkpoints,
    mainnet_spot_entry,
    spot_checkpoints,
    spot_entry,
    vrf_requests,
);
