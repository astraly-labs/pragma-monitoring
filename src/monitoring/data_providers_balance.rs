use bigdecimal::ToPrimitive;
use starknet::{
    core::{
        types::{BlockId, BlockTag, FieldElement, FunctionCall},
        utils::cairo_short_string_to_felt,
    },
    macros::selector,
    providers::Provider,
};

use crate::constants::DECIMALS;
use crate::{config::get_config, error::MonitoringError};

/// Calculates the deviation from the on-chain price
/// Returns the deviation and the number of sources aggregated
pub async fn data_provider_balance(data_provider: FieldElement) -> Result<f64, MonitoringError> {
    let config = get_config(None).await;

    let client = &config.network().provider;
    let token_address =
        std::env::var("BALANCE_TOKEN_ADDRESS").expect("BALANCE_TOKEN_ADDRESS must be set");
    let token_balance = client
        .call(
            FunctionCall {
                contract_address: FieldElement::from_hex_be(&token_address)
                    .expect("failed to convert token address"),
                entry_point_selector: selector!("balanceOf"),
                calldata: vec![data_provider.clone().into()],
            },
            BlockId::Tag(BlockTag::Latest),
        )
        .await
        .map_err(|e| MonitoringError::OnChain(e.to_string()))?;

    // Decimals are not obliged to be fetched since they are set to 18 decimals by default
    // let decimals = client
    // .call(
    //     FunctionCall {
    //         contract_address: FieldElement::from_hex_be(&token_address).expect("failed to convert token address"),
    //         entry_point_selector: selector!("decimals"),
    //         calldata: vec![],
    //     },
    //     BlockId::Tag(BlockTag::Latest),
    // )
    // .await
    // .map_err(|e| MonitoringError::OnChain(e.to_string()))?;

    let on_chain_balance = token_balance
        .first()
        .ok_or(MonitoringError::OnChain("No data".to_string()))?
        .to_big_decimal(DECIMALS)
        .to_f64()
        .ok_or(MonitoringError::Conversion(
            "Failed to convert to f64".to_string(),
        ))?;

    Ok((on_chain_balance))
}
