use alloy::network::Ethereum;
use alloy::providers::fillers::{ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller};
use alloy::providers::{Identity, RootProvider};
use alloy::transports::http::{Client, Http};
use alloy::{providers::fillers::BlobGasFiller, sol};

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    Pragma,
    "abi/Pragma.json"
);

pub type PragmaContract = Pragma::PragmaInstance<
    Http<Client>,
    FillProvider<
        JoinFill<
            Identity,
            JoinFill<GasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>,
        >,
        RootProvider<Http<Client>>,
        Http<Client>,
        Ethereum,
    >,
>;
