// In lst_tests.rs

use super::{init_test_config, set_test_config};
use crate::{
    constants::LST_CONVERSION_RATE, error::MonitoringError,
    monitoring::lst::process_lst_data_by_pair,
};
use mockall::predicate;
use starknet::{
    core::types::{BlockId, BlockTag, Felt, StarknetError},
    providers::ProviderError,
};

#[tokio::test]
async fn test_lst_conversion_rate_success() {
    let pair = "XSTRK/STRK".to_string();
    let mock_rate = 1.2; // Valid rate > 1.0
    let mock_decimals = 8;
    let mock_rate_felt = Felt::from((mock_rate * 10f64.powi(mock_decimals)) as u64);

    let mut mock_provider = init_test_config().await;
    mock_provider
        .expect_call()
        .with(
            predicate::always(),
            predicate::eq(BlockId::Tag(BlockTag::Latest)),
        )
        .returning(move |_, _| Ok(vec![mock_rate_felt]));

    set_test_config(&mock_provider).await;

    let result = process_lst_data_by_pair(pair).await;
    assert!(result.is_ok());

    let metric = LST_CONVERSION_RATE
        .with_label_values(&["testnet", "XSTRK/STRK"])
        .get();
    assert!((metric - mock_rate).abs() < f64::EPSILON);
}

#[tokio::test]
async fn test_lst_conversion_rate_below_one() {
    let pair = "XSTRK/STRK".to_string();
    let mock_rate = 0.9; // Invalid rate < 1.0
    let mock_decimals = 8;
    let mock_rate_felt = Felt::from((mock_rate * 10f64.powi(mock_decimals)) as u64);

    let mut mock_provider = init_test_config().await;
    mock_provider
        .expect_call()
        .with(
            predicate::always(),
            predicate::eq(BlockId::Tag(BlockTag::Latest)),
        )
        .returning(move |_, _| Ok(vec![mock_rate_felt]));

    set_test_config(&mock_provider).await;

    let result = process_lst_data_by_pair(pair).await;
    assert!(matches!(
        result,
        Err(MonitoringError::Price(msg)) if msg.contains("<= 1")
    ));
}

#[tokio::test]
async fn test_lst_non_lst_pair() {
    let pair = "BTC/USD".to_string();

    let mock_provider = init_test_config().await;
    set_test_config(&mock_provider).await;

    let result = process_lst_data_by_pair(pair).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_lst_provider_error() {
    let pair = "XSTRK/STRK".to_string();

    let mut mock_provider = init_test_config().await;
    mock_provider
        .expect_call()
        .with(
            predicate::always(),
            predicate::eq(BlockId::Tag(BlockTag::Latest)),
        )
        .returning(|_, _| {
            Err(ProviderError::StarknetError(
                StarknetError::ValidationFailure("Test error".to_string()),
            ))
        });

    set_test_config(&mock_provider).await;

    let result = process_lst_data_by_pair(pair).await;
    assert!(matches!(result, Err(MonitoringError::OnChain(_))));
}
