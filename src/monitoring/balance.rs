use bigdecimal::ToPrimitive;
use starknet::{
    core::types::{BlockId, BlockTag, Felt, FunctionCall},
    macros::selector,
    providers::Provider,
};

use crate::constants::{FEE_TOKEN_ADDRESS, FEE_TOKEN_DECIMALS};
use crate::{config::get_config, error::MonitoringError};

/// Returns the balance of a given adress
/// Note: Currently only reads ETH balance
pub async fn get_on_chain_balance(address: Felt) -> Result<f64, MonitoringError> {
    let config = get_config(None).await;

    let client = &config.network().provider;
    let token_balance = client
        .call(
            FunctionCall {
                contract_address: Felt::from_hex_unchecked(FEE_TOKEN_ADDRESS),
                entry_point_selector: selector!("balanceOf"),
                calldata: vec![address],
            },
            BlockId::Tag(BlockTag::Latest),
        )
        .await
        .map_err(|e| MonitoringError::OnChain(e.to_string()))?;

    let on_chain_balance = token_balance
        .first()
        .ok_or(MonitoringError::OnChain("No data".to_string()))?
        .to_bigint()
        .to_f64()
        .ok_or(MonitoringError::Conversion(
            "Failed to convert to f64".to_string(),
        ))?
        / 10_f64.powi(FEE_TOKEN_DECIMALS as i32);

    Ok(on_chain_balance)
}
