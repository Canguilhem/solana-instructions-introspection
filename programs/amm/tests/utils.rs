use anchor_lang::{
    solana_program::program_pack::Pack, system_program::ID as SYSTEM_PROGRAM_ID, InstructionData,
    ToAccountMetas,
};
use anchor_spl::{
    associated_token::{self, ID as ASSOCIATED_PROGRAM_ID},
    token::spl_token::state::Account as SplTokenAccount,
    token::ID as TOKEN_PROGRAM_ID,
};
use litesvm::LiteSVM;
use litesvm_token::CreateAssociatedTokenAccountIdempotent;
use solana_keypair::Keypair;
use solana_message::Instruction;
use solana_pubkey::Pubkey;
use solana_signer::Signer;

pub fn get_user_atas(
    svm: &mut LiteSVM,
    payer: &Keypair,
    mint_x: Pubkey,
    mint_y: Pubkey,
    mint_lp: Pubkey,
) -> (Pubkey, Pubkey, Pubkey) {
    let user_x = CreateAssociatedTokenAccountIdempotent::new(svm, payer, &mint_x)
        .send()
        .unwrap();
    let user_y = CreateAssociatedTokenAccountIdempotent::new(svm, payer, &mint_y)
        .send()
        .unwrap();
    let user_lp = associated_token::get_associated_token_address(&payer.pubkey(), &mint_lp);

    (user_x, user_y, user_lp)
}

pub fn token_balance(svm: &LiteSVM, ata: &Pubkey) -> u64 {
    let acc = svm.get_account(ata).expect("SPL token account missing");
    let data: &[u8] = acc.data.as_ref();
    SplTokenAccount::unpack(data)
        .expect("unpack SPL token account")
        .amount
}

pub fn create_initialize_ix(
    mut _svm: &mut LiteSVM,
    payer: &Keypair,
    mint_x: Pubkey,
    mint_y: Pubkey,
    config: Pubkey,
    seed: u64,
    fee: u16,
    mint_lp: Pubkey,
    vault_x: Pubkey,
    vault_y: Pubkey,
) -> Instruction {
    let maker = payer.pubkey();

    Instruction {
        program_id: amm::id(),
        accounts: amm::accounts::Initialize {
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
            initializer: maker,
            mint_x,
            mint_y,
            mint_lp,
            vault_x,
            vault_y,
            config,
            associated_token_program: ASSOCIATED_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: amm::instruction::Initialize {
            seed,
            fee,
            authority: Some(maker),
        }
        .data(),
    }
}

pub fn create_deposit_ix(
    mut _svm: &mut LiteSVM,
    payer: &Keypair,
    mint_x: Pubkey,
    mint_y: Pubkey,
    config: Pubkey,
    mint_lp: Pubkey,
    vault_x: Pubkey,
    vault_y: Pubkey,
    user_x: Pubkey,
    user_y: Pubkey,
    user_lp: Pubkey,
    token_x: Option<u64>,
    token_y: Option<u64>,
    side: amm::OperationSide,
    min_lp: u64,
) -> Instruction {
    let maker = payer.pubkey();

    Instruction {
        program_id: amm::id(),
        accounts: amm::accounts::Deposit {
            user: maker,
            mint_x,
            mint_y,
            config,
            mint_lp,
            vault_x,
            vault_y,
            user_x,
            user_y,
            user_lp,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
            associated_token_program: ASSOCIATED_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: amm::instruction::Deposit {
            token_x,
            token_y,
            side,
            min_lp,
        }
        .data(),
    }
}

pub fn create_withdraw_w_introspection_ix(
    _svm: &mut LiteSVM,
    payer: &Keypair,
    mint_x: Pubkey,
    mint_y: Pubkey,
    config: Pubkey,
    mint_lp: Pubkey,
    vault_x: Pubkey,
    vault_y: Pubkey,
    user_x: Pubkey,
    user_y: Pubkey,
    user_lp: Pubkey,
    lp_amount: u64,
    side: amm::OperationSide,
    min_x: u64,
    min_y: u64,
) -> Instruction {
    let maker = payer.pubkey();

    Instruction {
        program_id: amm::id(),
        accounts: amm::accounts::WithdrawWithIntrospection {
            user: maker,
            mint_x,
            mint_y,
            config,
            mint_lp,
            vault_x,
            vault_y,
            user_x,
            user_y,
            user_lp,
            instruction_sysvar: solana_instructions_sysvar::ID,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
            associated_token_program: ASSOCIATED_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: amm::instruction::WithdrawWIntrospection {
            lp_amount,
            side,
            min_x,
            min_y,
        }
        .data(),
    }
}

pub fn create_withdraw_ix(
    _svm: &mut LiteSVM,
    payer: &Keypair,
    mint_x: Pubkey,
    mint_y: Pubkey,
    config: Pubkey,
    mint_lp: Pubkey,
    vault_x: Pubkey,
    vault_y: Pubkey,
    user_x: Pubkey,
    user_y: Pubkey,
    user_lp: Pubkey,
    lp_amount: u64,
    side: amm::OperationSide,
    min_x: u64,
    min_y: u64,
) -> Instruction {
    let maker = payer.pubkey();

    Instruction {
        program_id: amm::id(),
        accounts: amm::accounts::Withdraw {
            user: maker,
            mint_x,
            mint_y,
            config,
            mint_lp,
            vault_x,
            vault_y,
            user_x,
            user_y,
            user_lp,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
            associated_token_program: ASSOCIATED_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: amm::instruction::Withdraw {
            lp_amount,
            side,
            min_x,
            min_y,
        }
        .data(),
    }
}

pub fn create_burn_ix(
    _svm: &mut LiteSVM,
    payer: &Keypair,
    mint_x: Pubkey,
    mint_y: Pubkey,
    config: Pubkey,
    mint_lp: Pubkey,
    user_lp: Pubkey,
    lp_amount: u64,
) -> Instruction {
    let maker = payer.pubkey();

    Instruction {
        program_id: amm::id(),
        accounts: amm::accounts::BurnLp {
            user: maker,
            mint_x,
            mint_y,
            config,
            mint_lp,
            user_lp,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
            associated_token_program: ASSOCIATED_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: amm::instruction::BurnLpTokens { amount: lp_amount }.data(),
    }
}

pub fn create_swap_ix(
    _svm: &mut LiteSVM,
    payer: &Keypair,
    mint_x: Pubkey,
    mint_y: Pubkey,
    config: Pubkey,
    mint_lp: Pubkey,
    vault_x: Pubkey,
    vault_y: Pubkey,
    user_x: Pubkey,
    user_y: Pubkey,
    side: amm::OperationSide,
    amount: u64,
    min_out: u64,
) -> Instruction {
    let maker = payer.pubkey();

    Instruction {
        program_id: amm::id(),
        accounts: amm::accounts::Swap {
            user: maker,
            mint_x,
            mint_y,
            config,
            mint_lp,
            vault_x,
            vault_y,
            user_x,
            user_y,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
            associated_token_program: ASSOCIATED_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: amm::instruction::Swap {
            amount,
            side,
            min_out,
        }
        .data(),
    }
}

pub fn update_config_ix(
    payer: &Keypair,
    config: Pubkey,
    seed: u64,
    locked: bool,
    authority: Option<Pubkey>,
    fee: u16,
) -> Instruction {
    let maker = payer.pubkey();

    Instruction {
        program_id: amm::id(),
        accounts: amm::accounts::UpdateConfig {
            user: maker,
            config,
        }
        .to_account_metas(None),
        data: amm::instruction::UpdateConfig {
            _seed: seed,
            authority,
            fee,
            locked,
        }
        .data(),
    }
}
