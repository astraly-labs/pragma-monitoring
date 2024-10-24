use alloy::network::Ethereum;
use alloy::providers::fillers::{ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller};
use alloy::providers::{Identity, RootProvider};
use alloy::transports::http::{Client, Http};
use alloy::{providers::fillers::BlobGasFiller, sol};

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    interface IPragma {
        struct Metadata {
            bytes32 feedId;
            uint64 timestamp;
            uint16 numberOfSources;
            uint8 decimals;
        }

        struct SpotMedian {
            Metadata metadata;
            uint256 price;
            uint256 volume;
        }

        mapping(bytes32 => SpotMedian) public spotMedianFeeds;
    }
);

pub type PragmaContract = IPragma::IPragmaInstance<
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
