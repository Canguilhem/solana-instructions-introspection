use anchor_lang::prelude::*;

use cpmm::CpmmError;

#[error_code]
pub enum AmmError {
    #[msg("Custom error message")]
    CustomError,
    #[msg("Pool is locked")]
    PoolLocked,
    #[msg("Invalid precision")]
    InvalidPrecision,
    #[msg("Overflow")]
    Overflow,
    #[msg("Underflow")]
    Underflow,
    #[msg("Invalid fee amount")]
    InvalidFeeAmount,
    #[msg("Insufficient balance")]
    InsufficientBalance,
    #[msg("Zero Balance")]
    ZeroBalance,
    #[msg("Slippage exceeded")]
    SlippageLimitExceeded,
    #[msg("Invalid Amount")]
    InvalidAmount,
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("Invalid ProgramId")]
    InvalidProgramId,
    #[msg("Invalid Accounts count")]
    WrongAccountsCount,
    #[msg("Invalid Key")]
    InvalidKey,
    #[msg("Invalid data length")]
    InvalidDataLength,
    #[msg("Missing Prior instructions")]
    MissingPriorInstruction,
}

impl From<CpmmError> for AmmError {
    fn from(err: CpmmError) -> Self {
        match err {
            CpmmError::InvalidAmount => AmmError::InvalidAmount,
            CpmmError::ZeroBalance => AmmError::ZeroBalance,
            CpmmError::InvalidFee => AmmError::InvalidFeeAmount,
            CpmmError::SlippageExceeded => AmmError::SlippageLimitExceeded,
            CpmmError::Overflow => AmmError::Overflow,
            CpmmError::Underflow => AmmError::Underflow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpmm_error_maps_to_amm_error() {
        match AmmError::from(CpmmError::SlippageExceeded) {
            AmmError::SlippageLimitExceeded => {}
            other => panic!("unexpected mapping: {other:?}"),
        }
        match AmmError::from(CpmmError::InvalidFee) {
            AmmError::InvalidFeeAmount => {}
            other => panic!("unexpected mapping: {other:?}"),
        }
    }
}
