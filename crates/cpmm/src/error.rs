#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpmmError {
    InvalidAmount,
    ZeroBalance,
    InvalidFee,
    SlippageExceeded,
    Overflow,
    Underflow,
}

pub type CpmmResult<T> = core::result::Result<T, CpmmError>;
