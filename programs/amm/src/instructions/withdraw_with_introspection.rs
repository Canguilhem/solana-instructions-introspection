use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{transfer, Mint, Token, TokenAccount, Transfer},
};
use solana_instructions_sysvar::get_instruction_relative;

use crate::{
    CONFIG_SEED, Config, LP_SEED, OperationSide, PoolState, WithdrawQuote, error::AmmError, instruction::BurnLpTokens
};

#[derive(Accounts)]
pub struct WithdrawWithIntrospection<'info> {
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

    /// CHECK: This should be safe
    #[account(
        address= solana_sdk_ids::sysvar::instructions::ID
    )]
    pub instruction_sysvar: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> WithdrawWithIntrospection<'info> {
    pub fn withdraw_w_introspection(
        &mut self,
        lp_amount: u64,
        side: OperationSide,
        min_x: u64,
        min_y: u64,
    ) -> Result<()> {
        require!(!self.config.locked, AmmError::PoolLocked);

        self.confirm_burn(lp_amount)?;

        let pool = PoolState::new(
            self.vault_x.amount,
            self.vault_y.amount,
            // now that burn and lp are two different instructions
            //  we need to use lp_supply_before_burn
            self.mint_lp
                .supply
                .checked_add(lp_amount)
                .ok_or(AmmError::Overflow)?,
            self.config.fee,
        );

        let quote = pool
            .withdraw(lp_amount, side.into(), min_x, min_y)
            .map_err(AmmError::from)?;

        self.execute_withdraw_transfers(side, &quote)?;
        // self.burn_lp_tokens(lp_amount)
        Ok(())
    }

    fn execute_withdraw_transfers(&self, side: OperationSide, quote: &WithdrawQuote) -> Result<()> {
        match side {
            OperationSide::Balanced => {
                self.transfer_vault_to_user(true, quote.withdraw_x)?;
                self.transfer_vault_to_user(false, quote.withdraw_y)?;
            }
            OperationSide::X | OperationSide::Y => {
                self.transfer_vault_to_user(true, quote.pro_rata_x)?;
                self.transfer_vault_to_user(false, quote.pro_rata_y)?;
                self.transfer_user_to_vault(true, quote.swap_in_x)?;
                self.transfer_user_to_vault(false, quote.swap_in_y)?;
                self.transfer_vault_to_user(true, quote.swap_out_x)?;
                self.transfer_vault_to_user(false, quote.swap_out_y)?;
            }
        }

        Ok(())
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

    pub fn confirm_burn(&self, amount: u64) -> Result<()> {
        let ix = get_instruction_relative(-1, &self.instruction_sysvar.to_account_info())
            .map_err(|_| error!(AmmError::MissingPriorInstruction))?;
        // // confirm ix originate from our program
        require_eq!(ix.program_id, crate::ID, AmmError::InvalidProgramId);

        // confirm instruction::discriminator
        require!(
            &ix.data[..8] == BurnLpTokens::DISCRIMINATOR,
            AmmError::UnexpectedDiscriminator
        );

        // confirm ix refs accounts
        self.confrim_burn_accounts(ix.accounts)?;

        // confirm data is 16 bytes (discriminator: 8 + u64: 8)
        require_eq!(ix.data.len(), 16, AmmError::InvalidDataLength);

        // read burn_amount used
        let burn_amount = u64::from_le_bytes(
            ix.data[8..16]
                .try_into()
                .map_err(|_| AmmError::InvalidDataLength)?,
        );
        require_eq!(burn_amount, amount, AmmError::InvalidAmount);

        Ok(())
    }

    pub fn confrim_burn_accounts(&self, accounts: Vec<AccountMeta>) -> Result<()> {
        const EXPECTED_COUNT: usize = 9;

        require_eq!(accounts.len(), EXPECTED_COUNT, AmmError::WrongAccountsCount);

        // should we also confirm the rest of accountmetdata? {is_signer, is_writtable}
        let expected_accounts: [Pubkey; EXPECTED_COUNT] = [
            self.user.key(),
            self.mint_x.key(),
            self.mint_y.key(),
            self.config.key(),
            self.mint_lp.key(),
            self.user_lp.key(),
            self.token_program.key(),
            self.system_program.key(),
            self.associated_token_program.key(),
        ];

        for (index, key) in expected_accounts.iter().enumerate() {
            require_keys_eq!(accounts[index].pubkey, *key, AmmError::InvalidKey);
        }

        Ok(())
    }
}
