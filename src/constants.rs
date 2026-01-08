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
    "ETH/STRK" => "ethereum",
    "LBTC/USD" => "lombard-staked-btc",
    "UNIBTC/USD" => "universal-btc",
    "MRE7YIELD/USD" => "midas-mre7yield",
    "MRE7BTC/USD" => "midas-mre7btc-2",
    "BROTHER/USDPLUS" => "starknet-brother",
};

#[allow(unused)]
pub const FEE_TOKEN_DECIMALS: u32 = 18;
#[allow(unused)]
pub const FEE_TOKEN_ADDRESS: &str =
    "0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d";

pub const CONFIG_UPDATE_INTERVAL: u64 = 3 * 3600;
