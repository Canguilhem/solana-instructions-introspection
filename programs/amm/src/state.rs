use anchor_lang::prelude::*;

use cpmm::Side;

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub seed: u64,
    pub authority: Option<Pubkey>,
    pub mint_x: Pubkey,
    pub mint_y: Pubkey,
    pub fee: u16,
    pub locked: bool,
    pub config_bump: u8,
    pub lp_bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum OperationSide {
    X,
    Y,
    Balanced,
}

impl From<OperationSide> for Side {
    fn from(value: OperationSide) -> Self {
        match value {
            OperationSide::X => Side::X,
            OperationSide::Y => Side::Y,
            OperationSide::Balanced => Side::Balanced,
        }
    }
}
