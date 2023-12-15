use phf::phf_map;

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
};
