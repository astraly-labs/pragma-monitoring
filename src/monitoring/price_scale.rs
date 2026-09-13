/// Convert an integer price to its human-readable value without integer overflow.
pub(crate) fn normalize_price(price: f64, decimals: u32) -> f64 {
    price / 10_f64.powi(decimals as i32)
}

#[cfg(test)]
mod tests {
    use super::normalize_price;

    #[test]
    fn identical_eighteen_decimal_prices_do_not_report_extreme_deviation() {
        let source = 0.125;
        let median = normalize_price(125_000_000_000_000_000.0, 18);
        assert_eq!((source - median) / median, 0.0);
        let divergent_source = source * 1.3;
        assert!((divergent_source - median) / median > 0.25);
    }

    #[test]
    fn supports_zero_eight_and_twenty_seven_decimals() {
        assert_eq!(normalize_price(42.0, 0), 42.0);
        assert_eq!(normalize_price(12_500_000.0, 8), 0.125);
        assert!((normalize_price(1.25e26, 27) - 0.125).abs() < 1e-12);
    }
}
