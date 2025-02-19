use phf::phf_map;

#[allow(unused)]
pub(crate) static COINGECKO_IDS: phf::Map<&'static str, &'static str> = phf_map! {
    "BTC/USD" => "bitcoin",
    "ETH/USD" => "ethereum",
    "LUSD/USD" => "liquity-usd",
    "WBTC/USD" => "wrapped-bitcoin",
    "DAI/USD" => "dai",
    "USDC/USD" => "usd-coin",
    "USDT/USD" => "tether",
    "WSTETH/USD" => "wrapped-steth",
    "LORDS/USD" => "lords",
    "STRK/USD" => "starknet",
    "ZEND/USD" => "zklend-2",
    "NSTR/USD" => "nostra",
    "EKUBO/USD" => "ekubo-protocol",
    "XSTRK/USD" => "endur-fi-staked-strk",
};

#[allow(unused)]
pub const FEE_TOKEN_DECIMALS: u32 = 18;
#[allow(unused)]
pub const FEE_TOKEN_ADDRESS: &str =
    "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7";

pub const CONFIG_UPDATE_INTERVAL: u64 = 3 * 3600;
