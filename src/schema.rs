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
        block_timestamp -> Timestamp,
        #[max_length = 255]
        transaction_hash -> Varchar,
        price -> Numeric,
        timestamp -> Timestamp,
        #[max_length = 255]
        publisher -> Varchar,
        #[max_length = 255]
        source -> Varchar,
        volume -> Numeric,
        expiration_timestamp -> Nullable<Timestamp>,
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
        block_timestamp -> Timestamp,
        #[max_length = 255]
        transaction_hash -> Varchar,
        price -> Numeric,
        timestamp -> Timestamp,
        #[max_length = 255]
        publisher -> Varchar,
        #[max_length = 255]
        source -> Varchar,
        volume -> Numeric,
        expiration_timestamp -> Nullable<Timestamp>,
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
        block_timestamp -> Timestamp,
        #[max_length = 255]
        transaction_hash -> Varchar,
        price -> Numeric,
        timestamp -> Timestamp,
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
        block_timestamp -> Timestamp,
        #[max_length = 255]
        transaction_hash -> Varchar,
        price -> Numeric,
        timestamp -> Timestamp,
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
        block_timestamp -> Timestamp,
        #[max_length = 255]
        transaction_hash -> Varchar,
        price -> Numeric,
        #[max_length = 255]
        sender_address -> Varchar,
        aggregation_mode -> Numeric,
        _cursor -> Int8,
        timestamp -> Timestamp,
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
        block_timestamp -> Timestamp,
        #[max_length = 255]
        transaction_hash -> Varchar,
        price -> Numeric,
        #[max_length = 255]
        sender_address -> Varchar,
        aggregation_mode -> Numeric,
        _cursor -> Int8,
        timestamp -> Timestamp,
        nb_sources_aggregated -> Numeric,
    }
}

diesel::table! {
    vrf_requests (data_id) {
        #[max_length = 255]
        network -> Varchar,
        request_id -> Numeric,
        seed -> Numeric,
        created_at -> Timestamp,
        created_at_tx -> Varchar,
        #[max_length = 255]
        callback_address -> Varchar,
        callback_fee_limit -> Numeric,
        num_words -> Numeric,
        requestor_address -> Varchar,
        updated_at -> Timestamp,
        updated_at_tx -> Varchar,
        status -> Numeric,
        minimum_block_number -> Numeric,
        _cursor -> Int8range,
        data_id -> Varchar,
    }
}


diesel::table! {
    oo_requests (data_id) {
        #[max_length = 255]
        network -> Varchar,
        data_id -> Varchar,
        assertion_id -> Numeric,
        domain_id -> Numeric,
        claim -> Text,
        #[max_length = 255]
        asserter -> Varchar,
        #[max_length = 255]
        disputer -> Varchar,
        disputed -> Bool,
        #[max_length = 255]
        callback_recipient -> Varchar,
        #[max_length = 255]
        escalation_manager -> Varchar,
        #[max_length = 255]
        caller -> Varchar,
        expiration_timestamp -> Timestamp,
        settlement_resolution -> Bool,
        #[max_length = 255]
        settle_caller -> Varchar,
        #[max_length = 255]
        currency -> Varchar,
        bond -> Numeric,
        _cursor -> Int8range,
        identifier -> Varchar,
        updated_at -> Timestamp,
        #[max_length = 255]
        updated_at_tx -> Varchar,
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
