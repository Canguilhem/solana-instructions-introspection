use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{mint_to, transfer, Mint, MintTo, Token, TokenAccount, Transfer},
};

use crate::{error::AmmError, Config, DepositQuote, OperationSide, PoolState, CONFIG_SEED, LP_SEED};

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    pub mint_x: Account<'info, Mint>,
    pub mint_y: Account<'info, Mint>,
    #[account(
        has_one= mint_x,
        has_one= mint_y,
        seeds= [CONFIG_SEED,config.seed.to_le_bytes().as_ref()],
        bump
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds=[LP_SEED,config.key().as_ref()],
        bump= config.lp_bump
    )]
    pub mint_lp: Account<'info, Mint>,
    #[account(
        mut,
        associated_token::mint= mint_x,
        associated_token::authority= config
    )]
    pub vault_x: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint= mint_y,
        associated_token::authority= config
    )]
    pub vault_y: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint= mint_x,
        associated_token::authority= user
    )]
    pub user_x: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint= mint_y,
        associated_token::authority= user
    )]
    pub user_y: Box<Account<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer=user,
        associated_token::mint= mint_lp,
        associated_token::authority= user
    )]
    pub user_lp: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> Deposit<'info> {
    pub fn deposit(
        &mut self,
        token_x: Option<u64>,
        token_y: Option<u64>,
        side: OperationSide,
        min_lp: u64,
    ) -> Result<()> {
        require!(!self.config.locked, AmmError::PoolLocked);

        let pool = PoolState::new(
            self.vault_x.amount,
            self.vault_y.amount,
            self.mint_lp.supply,
            self.config.fee,
        );

        let quote = pool
            .deposit(token_x, token_y, side.into(), min_lp)
            .map_err(AmmError::from)?;

        self.execute_deposit_transfers(side, &quote)?;
        self.mint_lp_tokens(quote.lp_minted)
    }

    fn execute_deposit_transfers(&self, side: OperationSide, quote: &DepositQuote) -> Result<()> {
        match side {
            OperationSide::Balanced => {
                self.transfer_user_to_vault(true, quote.deposit_x)?;
                self.transfer_user_to_vault(false, quote.deposit_y)?;
            }
            OperationSide::X => {
                self.transfer_user_to_vault(true, quote.swap_in_x)?;
                self.transfer_vault_to_user(false, quote.swap_out_y)?;
                self.transfer_user_to_vault(true, quote.deposit_x)?;
                self.transfer_user_to_vault(false, quote.deposit_y)?;
            }
            OperationSide::Y => {
                self.transfer_user_to_vault(false, quote.swap_in_y)?;
                self.transfer_vault_to_user(true, quote.swap_out_x)?;
                self.transfer_user_to_vault(true, quote.deposit_x)?;
                self.transfer_user_to_vault(false, quote.deposit_y)?;
            }
        }

        Ok(())
    }

    fn transfer_user_to_vault(&self, is_x: bool, amount: u64) -> Result<()> {
        if amount == 0 {
            return Ok(());
        }

        let program = self.token_program.key();

        let (from, to) = match is_x {
            true => (
                self.user_x.to_account_info(),
                self.vault_x.to_account_info(),
            ),
            false => (
                self.user_y.to_account_info(),
                self.vault_y.to_account_info(),
            ),
        };

        let accounts = Transfer {
            from,
            to,
            authority: self.user.to_account_info(),
        };

        let ctx = CpiContext::new(program, accounts);
        transfer(ctx, amount)
    }

    fn transfer_vault_to_user(&self, is_x: bool, amount: u64) -> Result<()> {
        if amount == 0 {
            return Ok(());
        }

        let program = self.token_program.key();

        let (from, to) = match is_x {
            true => (
                self.vault_x.to_account_info(),
                self.user_x.to_account_info(),
            ),
            false => (
                self.vault_y.to_account_info(),
                self.user_y.to_account_info(),
            ),
        };

        let accounts = Transfer {
            from,
            to,
            authority: self.config.to_account_info(),
        };

        let signer_seeds: &[&[&[u8]]] = &[&[
            CONFIG_SEED,
            &self.config.seed.to_le_bytes(),
            &[self.config.config_bump],
        ]];

        let ctx = CpiContext::new_with_signer(program, accounts, signer_seeds);
        transfer(ctx, amount)
    }

    pub fn mint_lp_tokens(&self, amount: u64) -> Result<()> {
        let program = self.token_program.key();

        let accounts = MintTo {
            mint: self.mint_lp.to_account_info(),
            to: self.user_lp.to_account_info(),
            authority: self.config.to_account_info(),
        };

        let signer_seeds: &[&[&[u8]]] = &[&[
            CONFIG_SEED,
            &self.config.seed.to_le_bytes(),
            &[self.config.config_bump],
        ]];

        let ctx = CpiContext::new_with_signer(program, accounts, signer_seeds);

        mint_to(ctx, amount)
    }
}
