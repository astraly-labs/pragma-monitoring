use std::time::Duration;

use starknet::{
    accounts::{single_owner::SignError, Account, AccountError, Call, SingleOwnerAccount},
    core::{
        chain_id,
        types::{FieldElement, InvokeTransactionResult},
        utils::{cairo_short_string_to_felt, get_selector_from_name},
    },
    providers::{jsonrpc::HttpTransport, JsonRpcClient},
    signers::local_wallet::SignError as SigningError,
    signers::{LocalWallet, SigningKey},
};

use super::constants::PUBLISHER_ADDRESS;

/// Wait for a condition to be true, with a timeout.
pub async fn wait_for_expect<F, T>(
    mut condition: F,
    timeout: Duration,
    interval: Duration,
) -> Option<T>
where
    F: FnMut() -> Option<T>,
{
    let start = tokio::time::Instant::now();
    while tokio::time::Instant::now() - start < timeout {
        if let Some(result) = condition() {
            return Some(result);
        }
        tokio::time::sleep(interval).await;
    }
    None
}

type RpcAccount<'a> = SingleOwnerAccount<&'a JsonRpcClient<HttpTransport>, LocalWallet>;

pub fn build_single_owner_account<'a>(
    rpc: &'a JsonRpcClient<HttpTransport>,
    private_key: &str,
    account_address: &str,
    is_legacy: bool,
) -> RpcAccount<'a> {
    let signer = LocalWallet::from(SigningKey::from_secret_scalar(
        FieldElement::from_hex_be(private_key).unwrap(),
    ));
    let account_address =
        FieldElement::from_hex_be(account_address).expect("Invalid Contract Address");
    let execution_encoding = if is_legacy {
        starknet::accounts::ExecutionEncoding::Legacy
    } else {
        starknet::accounts::ExecutionEncoding::New
    };
    SingleOwnerAccount::new(
        rpc,
        signer,
        account_address,
        chain_id::TESTNET,
        execution_encoding,
    )
}

pub async fn publish_data(
    provider: &JsonRpcClient<HttpTransport>,
    oracle_address: FieldElement,
    pair_id: &str,
    timestamp: &str,
    price: &str,
    source: &str,
    publisher: &str,
) -> Result<InvokeTransactionResult, AccountError<SignError<SigningError>>> {
    let publisher_account = build_single_owner_account(
        provider,
        &std::env::var("SIGNER_PRIVATE").expect("SIGNER_PRIVATE env var not set"),
        PUBLISHER_ADDRESS,
        false,
    );

    let pair_id = cairo_short_string_to_felt(pair_id).expect("Invalid pair id");
    let timestamp = FieldElement::from_dec_str(timestamp).expect("Invalid timestamp");
    let price = FieldElement::from_dec_str(price).expect("Invalid price");
    let source = cairo_short_string_to_felt(source).expect("Invalid source");
    let publisher = cairo_short_string_to_felt(publisher).expect("Invalid publisher");

    let calls = vec![Call {
        to: oracle_address,
        selector: get_selector_from_name("publish_data").unwrap(),
        calldata: vec![
            FieldElement::ZERO,
            timestamp,
            source,
            publisher,
            price,
            pair_id,
            FieldElement::ZERO,
        ],
    }];
    let tx = publisher_account.execute(calls);
    tx.send().await
}
