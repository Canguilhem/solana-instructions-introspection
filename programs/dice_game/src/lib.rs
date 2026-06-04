use anchor_lang::prelude::*;

pub mod error;
pub mod instructions;
pub mod state;

pub use error::*;
pub use instructions::*;
pub use state::*;

declare_id!("3hvpNKz6QeYsALNMwF18ViCT9K31wsvNqS5STbfDZMso");

#[program]
pub mod dice_game {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, amount: u64) -> Result<()> {
        ctx.accounts.init(amount)
    }

    pub fn place_bet(ctx: Context<PlaceBet>, seed: u128, amount: u64, roll: u8) -> Result<()> {
        ctx.accounts.create_bet(&ctx.bumps, seed, roll, amount)?;
        ctx.accounts.deposit(amount)
    }

    pub fn refund_bet(ctx: Context<RefundBet>) -> Result<()> {
        ctx.accounts.refund_bet(&ctx.bumps)
    }

    pub fn resolve_bet(ctx: Context<ResolveBet>, sig: Vec<u8>) -> Result<()> {
        ctx.accounts.verify_ed25519_signature(&sig)?;
        ctx.accounts.resolve_bet(&ctx.bumps, &sig)
    }

    #[instruction(discriminator = 0)]
    pub fn commit_entropy(ctx: Context<CommitEntropy>, entropy: [u8; 32]) -> Result<()> {
        ctx.accounts.commit_entropy(entropy)
    }

    pub fn resolve_from_entropy(ctx: Context<ResolveFromEntropy>) -> Result<()> {
        let entropy = ctx.accounts.verify_entropy_commit()?;
        ctx.accounts.resolve_bet(&ctx.bumps, &entropy)
    }
}
