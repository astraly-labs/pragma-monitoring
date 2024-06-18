use num_bigint::BigUint;
use starknet::core::types::Felt;

#[derive(Debug)]
pub enum FeltConversionError {
    Overflow,
}

pub(crate) fn try_felt_to_u32(felt: &Felt) -> Result<u32, FeltConversionError> {
    let biguint = felt.to_biguint();
    let u32_max = BigUint::from(u32::MAX);

    // assert!(biguint <= u32_max, "Felt value doesn't fit in u32");
    if biguint > u32_max {
        return Err(FeltConversionError::Overflow);
    }

    // Convert to u32, safe due to previous check
    Ok(biguint.to_u32_digits()[0])
}
