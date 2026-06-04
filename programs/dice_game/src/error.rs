use anchor_lang::prelude::*;

#[error_code]
pub enum DiceError {
    #[msg("Bet should be greater than minimum value")]
    MinimumBet,
    #[msg("Roll should be greater than minimum value")]
    MinimumRoll,
    #[msg("Roll should be lower than maximum value")]
    MaximumRoll,
    #[msg("Maths doesnt math")]
    MathError,
    #[msg("Time out not reached")]
    TimeoutNotReached,
    #[msg("Invalid data length")]
    Ed25519DataLength,
    #[msg("Invalid offset")]
    Ed25519SignatureOffset,
    #[msg("Signature must be one")]
    Ed25519SignatureMustBeOne,
    #[msg("Invalid Header")]
    Ed25519Header,
    #[msg("Invalid program ID")]
    Ed25519Program,
    #[msg("Invalid accounts count")]
    Ed25519Accounts,
    #[msg("Invalid pubkey")]
    Ed25519Pubkey,
    #[msg("Invalid signature")]
    Ed25519Signature,
    #[msg("Unexpected message")]
    Ed25519Message,
    #[msg("Unexpected programId")]
    InvalidProgramId,
    #[msg("Wrong accounts count")]
    WrongAccountsCount,
    #[msg("invalid discriminator")]
    InvalidDiscriminator,
    #[msg("invalid key")]
    InvalidKey,
    #[msg("invalid data length")]
    InvalidDataLength,
}
