use anchor_lang::AccountDeserialize;
use anchor_spl::associated_token;
use litesvm::{types::FailedTransactionMetadata, LiteSVM};
use litesvm_token::{CreateMint, MintTo};
use solana_keypair::Keypair;
use solana_message::{Instruction, Message, VersionedMessage};
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

mod utils;
use utils::create_initialize_ix;

use crate::utils::{
    create_burn_ix, create_deposit_ix, create_swap_ix, create_withdraw_ix, get_user_atas, token_balance, update_config_ix
};

const SEED: u64 = 123;
const FEE_BPS: u16 = 30;
const INITIAL_DEPOSIT: u64 = 100_000_000;

struct InitializedPool {
    svm: LiteSVM,
    payer: Keypair,
    mint_x: Pubkey,
    mint_y: Pubkey,
    config: Pubkey,
    mint_lp: Pubkey,
    vault_x: Pubkey,
    vault_y: Pubkey,
    user_x: Pubkey,
    user_y: Pubkey,
    user_lp: Pubkey,
}

fn send(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
) -> Result<litesvm::types::TransactionMetadata, FailedTransactionMetadata> {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx)
}

fn assert_log_contains(failure: &FailedTransactionMetadata, needle: &str) {
    let joined = failure.meta.logs.join("\n");
    assert!(
        joined.contains(needle),
        "expected logs to contain {needle:?}, got:\n{joined}"
    );
}

fn assert_tx_err(
    result: Result<litesvm::types::TransactionMetadata, FailedTransactionMetadata>,
    err_name: &str,
) {
    let err = result.expect_err("expected transaction to fail");
    assert_log_contains(&err, err_name);
}

fn setup() -> (
    LiteSVM,
    Keypair,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
) {
    let program_id = amm::id();
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/amm.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

    let mint_x = CreateMint::new(&mut svm, &payer)
        .decimals(6)
        .authority(&payer.pubkey())
        .send()
        .unwrap();
    let mint_y = CreateMint::new(&mut svm, &payer)
        .decimals(6)
        .authority(&payer.pubkey())
        .send()
        .unwrap();

    let config = Pubkey::find_program_address(&[b"config", &SEED.to_le_bytes()], &program_id).0;
    let mint_lp = Pubkey::find_program_address(&[b"lp", config.as_ref()], &program_id).0;
    let vault_x = associated_token::get_associated_token_address(&config, &mint_x);
    let vault_y = associated_token::get_associated_token_address(&config, &mint_y);

    (
        svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y,
    )
}

fn fund_user(svm: &mut LiteSVM, payer: &Keypair, mint_x: Pubkey, mint_y: Pubkey, user_x: Pubkey, user_y: Pubkey) {
    MintTo::new(svm, payer, &mint_x, &user_x, 1_000_000_000)
        .send()
        .unwrap();
    MintTo::new(svm, payer, &mint_y, &user_y, 1_000_000_000)
        .send()
        .unwrap();
}

fn setup_initialized_pool() -> InitializedPool {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();

    let init = create_initialize_ix(
        &mut svm, &payer, mint_x, mint_y, config, SEED, FEE_BPS, mint_lp, vault_x, vault_y,
    );

    let (user_x, user_y, user_lp) = get_user_atas(&mut svm, &payer, mint_x, mint_y, mint_lp);
    fund_user(&mut svm, &payer, mint_x, mint_y, user_x, user_y);

    let deposit = create_deposit_ix(
        &mut svm,
        &payer,
        mint_x,
        mint_y,
        config,
        mint_lp,
        vault_x,
        vault_y,
        user_x,
        user_y,
        user_lp,
        Some(INITIAL_DEPOSIT),
        Some(INITIAL_DEPOSIT),
        amm::OperationSide::Balanced,
        INITIAL_DEPOSIT,
    );

    send(&mut svm, &[init, deposit], &payer, &[&payer]).expect("initialize + deposit");

    InitializedPool {
        svm,
        payer,
        mint_x,
        mint_y,
        config,
        mint_lp,
        vault_x,
        vault_y,
        user_x,
        user_y,
        user_lp,
    }
}

fn lock_pool(pool: &mut InitializedPool) {
    let update = update_config_ix(
        &pool.payer,
        pool.config,
        SEED,
        true,
        Some(pool.payer.pubkey()),
        FEE_BPS,
    );
    send(&mut pool.svm, &[update], &pool.payer, &[&pool.payer]).expect("lock pool");
}

fn cpmm_pool(pool: &InitializedPool) -> amm::PoolState {
    amm::PoolState::new(
        token_balance(&pool.svm, &pool.vault_x),
        token_balance(&pool.svm, &pool.vault_y),
        token_balance(&pool.svm, &pool.user_lp),
        FEE_BPS,
    )
}

#[test]
fn test_initialize() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();
    let init = create_initialize_ix(
        &mut svm, &payer, mint_x, mint_y, config, SEED, FEE_BPS, mint_lp, vault_x, vault_y,
    );

    let result = send(&mut svm, &[init], &payer, &[&payer]);
    assert!(result.is_ok());

    let config_account = svm.get_account(&config).unwrap();
    let config_state =
        amm::state::Config::try_deserialize(&mut config_account.data.as_ref()).unwrap();

    assert_eq!(config_state.authority, Some(payer.pubkey()));
    assert_eq!(config_state.locked, false);
    assert_eq!(config_state.mint_x, mint_x);
    assert_eq!(config_state.mint_y, mint_y);
    assert_eq!(config_state.fee, FEE_BPS);
    assert_eq!(config_state.seed, SEED);

    let (config_pda, config_bump) =
        Pubkey::find_program_address(&[b"config", &SEED.to_le_bytes()], &amm::id());
    let (_mint_lp_pda, lp_bump) =
        Pubkey::find_program_address(&[b"lp", config_pda.as_ref()], &amm::id());
    assert_eq!(config_state.config_bump, config_bump);
    assert_eq!(config_state.lp_bump, lp_bump);
}

#[test]
fn test_deposit() {
    let pool = setup_initialized_pool();

    assert_eq!(token_balance(&pool.svm, &pool.vault_x), INITIAL_DEPOSIT);
    assert_eq!(token_balance(&pool.svm, &pool.vault_y), INITIAL_DEPOSIT);
    assert_eq!(token_balance(&pool.svm, &pool.user_lp), INITIAL_DEPOSIT);
}

#[test]
fn test_withdraw() {
    let mut pool = setup_initialized_pool();
    let withdraw_amount = 25_000_000;

    let withdraw = create_withdraw_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.vault_x,
        pool.vault_y,
        pool.user_x,
        pool.user_y,
        pool.user_lp,
        withdraw_amount,
        amm::OperationSide::Balanced,
        withdraw_amount,
        withdraw_amount,
    );

    assert_tx_err(
        send(&mut pool.svm, &[withdraw.clone()], &pool.payer, &[&pool.payer]),
        "MissingPriorInstruction",  // Anchor Error Code name in logs
    );

    let burn_ix = create_burn_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.user_lp,
        withdraw_amount,
    );

    send(&mut pool.svm, &[burn_ix, withdraw], &pool.payer, &[&pool.payer]).expect("Burn + withdraw -> OK");

    assert_eq!(
        token_balance(&pool.svm, &pool.user_lp),
        INITIAL_DEPOSIT - withdraw_amount,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.vault_x),
        token_balance(&pool.svm, &pool.vault_y),
    );
}

#[test]
fn test_swap() {
    let mut pool = setup_initialized_pool();
    let user_x_before = token_balance(&pool.svm, &pool.user_x);
    let user_y_before = token_balance(&pool.svm, &pool.user_y);

    let swap_amount = 10_000_000;
    let min_amount = 9_000_000;

    let swap = create_swap_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.vault_x,
        pool.vault_y,
        pool.user_x,
        pool.user_y,
        amm::OperationSide::X,
        swap_amount,
        min_amount,
    );

    send(&mut pool.svm, &[swap], &pool.payer, &[&pool.payer]).expect("swap");

    let y_from_swap = token_balance(&pool.svm, &pool.user_y)
        .saturating_add(INITIAL_DEPOSIT)
        .saturating_sub(user_y_before);
    assert!(y_from_swap >= min_amount);

    assert_eq!(
        token_balance(&pool.svm, &pool.user_x),
        user_x_before - swap_amount,
    );

    let k_before = INITIAL_DEPOSIT as u128 * INITIAL_DEPOSIT as u128;
    let k_after = token_balance(&pool.svm, &pool.vault_x) as u128
        * token_balance(&pool.svm, &pool.vault_y) as u128;
    assert!(k_after >= k_before);
    assert_eq!(token_balance(&pool.svm, &pool.user_lp), INITIAL_DEPOSIT);
}

#[test]
fn test_update_config() {
    let mut pool = setup_initialized_pool();
    let new_admin = Keypair::new().pubkey();

    let update = update_config_ix(
        &pool.payer,
        pool.config,
        SEED,
        true,
        Some(new_admin),
        FEE_BPS * 2,
    );

    send(&mut pool.svm, &[update], &pool.payer, &[&pool.payer]).expect("update config");

    let config_account = pool.svm.get_account(&pool.config).unwrap();
    let config_state =
        amm::state::Config::try_deserialize(&mut config_account.data.as_ref()).unwrap();

    assert_eq!(config_state.authority, Some(new_admin));
    assert_eq!(config_state.locked, true);
    assert_eq!(config_state.fee, FEE_BPS * 2);
}

#[test]
fn test_locked_pool_rejects_swap() {
    let mut pool = setup_initialized_pool();
    lock_pool(&mut pool);

    let swap = create_swap_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.vault_x,
        pool.vault_y,
        pool.user_x,
        pool.user_y,
        amm::OperationSide::X,
        1_000_000,
        0,
    );

    assert_tx_err(
        send(&mut pool.svm, &[swap], &pool.payer, &[&pool.payer]),
        "PoolLocked",
    );
}

#[test]
fn test_locked_pool_rejects_deposit() {
    let mut pool = setup_initialized_pool();
    lock_pool(&mut pool);

    let deposit = create_deposit_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.vault_x,
        pool.vault_y,
        pool.user_x,
        pool.user_y,
        pool.user_lp,
        Some(1_000_000),
        Some(1_000_000),
        amm::OperationSide::Balanced,
        0,
    );

    assert_tx_err(
        send(&mut pool.svm, &[deposit], &pool.payer, &[&pool.payer]),
        "PoolLocked",
    );
}

#[test]
fn test_locked_pool_rejects_withdraw() {
    let mut pool = setup_initialized_pool();
    lock_pool(&mut pool);

    let withdraw = create_withdraw_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.vault_x,
        pool.vault_y,
        pool.user_x,
        pool.user_y,
        pool.user_lp,
        1_000_000,
        amm::OperationSide::Balanced,
        0,
        0,
    );

    assert_tx_err(
        send(&mut pool.svm, &[withdraw], &pool.payer, &[&pool.payer]),
        "PoolLocked",
    );
}

#[test]
fn test_swap_slippage_exceeded() {
    let mut pool = setup_initialized_pool();
    let swap_amount = 10_000_000;

    let cpmm_pool = amm::PoolState::new(
        token_balance(&pool.svm, &pool.vault_x),
        token_balance(&pool.svm, &pool.vault_y),
        token_balance(&pool.svm, &pool.user_lp),
        FEE_BPS,
    );
    let quote = cpmm_pool
        .swap(swap_amount, amm::Side::X, 0)
        .expect("cpmm quote");

    let swap = create_swap_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.vault_x,
        pool.vault_y,
        pool.user_x,
        pool.user_y,
        amm::OperationSide::X,
        swap_amount,
        quote.amount_out + 1,
    );

    assert_tx_err(
        send(&mut pool.svm, &[swap], &pool.payer, &[&pool.payer]),
        "SlippageLimitExceeded",
    );
}

#[test]
fn test_unauthorized_update_config() {
    let mut pool = setup_initialized_pool();
    let stranger = Keypair::new();
    pool.svm
        .airdrop(&stranger.pubkey(), 1_000_000_000)
        .unwrap();

    let update = update_config_ix(
        &stranger,
        pool.config,
        SEED,
        false,
        Some(stranger.pubkey()),
        FEE_BPS,
    );

    assert_tx_err(
        send(&mut pool.svm, &[update], &stranger, &[&stranger]),
        "Unauthorized",
    );
}

#[test]
fn test_second_deposit_mints_expected_lp() {
    let mut pool = setup_initialized_pool();
    let second_deposit = 1_000_000;

    let vault_x_before = token_balance(&pool.svm, &pool.vault_x);
    let vault_y_before = token_balance(&pool.svm, &pool.vault_y);
    let user_lp_before = token_balance(&pool.svm, &pool.user_lp);

    let deposit = create_deposit_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.vault_x,
        pool.vault_y,
        pool.user_x,
        pool.user_y,
        pool.user_lp,
        Some(second_deposit),
        Some(second_deposit),
        amm::OperationSide::Balanced,
        second_deposit,
    );

    send(&mut pool.svm, &[deposit], &pool.payer, &[&pool.payer]).expect("second deposit");

    assert_eq!(
        token_balance(&pool.svm, &pool.vault_x),
        vault_x_before + second_deposit,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.vault_y),
        vault_y_before + second_deposit,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.user_lp),
        user_lp_before + second_deposit,
    );
}

#[test]
fn test_swap_y_for_x() {
    let mut pool = setup_initialized_pool();
    let user_x_before = token_balance(&pool.svm, &pool.user_x);
    let user_y_before = token_balance(&pool.svm, &pool.user_y);
    let swap_amount = 10_000_000;

    let cpmm_pool = amm::PoolState::new(
        token_balance(&pool.svm, &pool.vault_x),
        token_balance(&pool.svm, &pool.vault_y),
        token_balance(&pool.svm, &pool.user_lp),
        FEE_BPS,
    );
    let quote = cpmm_pool
        .swap(swap_amount, amm::Side::Y, 0)
        .expect("cpmm quote");

    let swap = create_swap_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.vault_x,
        pool.vault_y,
        pool.user_x,
        pool.user_y,
        amm::OperationSide::Y,
        swap_amount,
        quote.amount_out,
    );

    send(&mut pool.svm, &[swap], &pool.payer, &[&pool.payer]).expect("swap Y for X");

    assert_eq!(
        token_balance(&pool.svm, &pool.user_y),
        user_y_before - swap_amount,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.user_x),
        user_x_before + quote.amount_out,
    );
    assert_eq!(token_balance(&pool.svm, &pool.user_lp), INITIAL_DEPOSIT);
}

#[test]
fn test_imbalanced_balanced_deposit_keeps_excess_in_wallet() {
    let mut pool = setup_initialized_pool();
    let offered_x = 2_000_000;
    let offered_y = 1_000_000;

    let user_x_before = token_balance(&pool.svm, &pool.user_x);
    let user_y_before = token_balance(&pool.svm, &pool.user_y);
    let vault_x_before = token_balance(&pool.svm, &pool.vault_x);
    let vault_y_before = token_balance(&pool.svm, &pool.vault_y);
    let user_lp_before = token_balance(&pool.svm, &pool.user_lp);

    let quote = cpmm_pool(&pool)
        .deposit(
            Some(offered_x),
            Some(offered_y),
            amm::Side::Balanced,
            0,
        )
        .expect("cpmm quote");

    let deposit = create_deposit_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.vault_x,
        pool.vault_y,
        pool.user_x,
        pool.user_y,
        pool.user_lp,
        Some(offered_x),
        Some(offered_y),
        amm::OperationSide::Balanced,
        0,
    );

    send(&mut pool.svm, &[deposit], &pool.payer, &[&pool.payer]).expect("imbalanced deposit");

    assert_eq!(
        token_balance(&pool.svm, &pool.user_x),
        user_x_before - quote.deposit_x,
        "excess X should remain in the user wallet",
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.user_y),
        user_y_before - quote.deposit_y,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.vault_x),
        vault_x_before + quote.deposit_x,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.vault_y),
        vault_y_before + quote.deposit_y,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.user_lp),
        user_lp_before + quote.lp_minted,
    );
}

#[test]
fn test_single_sided_deposit_x_preserves_user_funds() {
    let mut pool = setup_initialized_pool();
    let token_x = 20_000_000;

    let user_x_before = token_balance(&pool.svm, &pool.user_x);
    let user_y_before = token_balance(&pool.svm, &pool.user_y);
    let vault_x_before = token_balance(&pool.svm, &pool.vault_x);
    let vault_y_before = token_balance(&pool.svm, &pool.vault_y);
    let user_lp_before = token_balance(&pool.svm, &pool.user_lp);

    let quote = cpmm_pool(&pool)
        .deposit(Some(token_x), None, amm::Side::X, 0)
        .expect("cpmm quote");

    let user_x_spent = quote.swap_in_x + quote.deposit_x;
    assert!(
        user_x_spent <= token_x,
        "program must not pull more X than the user offered",
    );

    let deposit = create_deposit_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.vault_x,
        pool.vault_y,
        pool.user_x,
        pool.user_y,
        pool.user_lp,
        Some(token_x),
        None,
        amm::OperationSide::X,
        0,
    );

    send(&mut pool.svm, &[deposit], &pool.payer, &[&pool.payer]).expect("single-sided X deposit");

    assert_eq!(
        token_balance(&pool.svm, &pool.user_x),
        user_x_before - user_x_spent,
        "leftover X must stay with the user",
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.user_y),
        user_y_before + quote.swap_out_y - quote.deposit_y,
        "unused swap output Y must stay with the user",
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.vault_x),
        vault_x_before + quote.swap_in_x + quote.deposit_x,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.vault_y),
        vault_y_before - quote.swap_out_y + quote.deposit_y,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.user_lp),
        user_lp_before + quote.lp_minted,
    );
}

#[test]
fn test_single_sided_deposit_y_preserves_user_funds() {
    let mut pool = setup_initialized_pool();
    let token_y = 20_000_000;

    let user_x_before = token_balance(&pool.svm, &pool.user_x);
    let user_y_before = token_balance(&pool.svm, &pool.user_y);
    let vault_x_before = token_balance(&pool.svm, &pool.vault_x);
    let vault_y_before = token_balance(&pool.svm, &pool.vault_y);
    let user_lp_before = token_balance(&pool.svm, &pool.user_lp);

    let quote = cpmm_pool(&pool)
        .deposit(None, Some(token_y), amm::Side::Y, 0)
        .expect("cpmm quote");

    let user_y_spent = quote.swap_in_y + quote.deposit_y;
    assert!(user_y_spent <= token_y);

    let deposit = create_deposit_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.vault_x,
        pool.vault_y,
        pool.user_x,
        pool.user_y,
        pool.user_lp,
        None,
        Some(token_y),
        amm::OperationSide::Y,
        0,
    );

    send(&mut pool.svm, &[deposit], &pool.payer, &[&pool.payer]).expect("single-sided Y deposit");

    assert_eq!(
        token_balance(&pool.svm, &pool.user_x),
        user_x_before + quote.swap_out_x - quote.deposit_x,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.user_y),
        user_y_before - user_y_spent,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.vault_x),
        vault_x_before - quote.swap_out_x + quote.deposit_x,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.vault_y),
        vault_y_before + quote.swap_in_y + quote.deposit_y,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.user_lp),
        user_lp_before + quote.lp_minted,
    );
}

#[test]
fn test_single_sided_withdraw_x_preserves_user_funds() {
    let mut pool = setup_initialized_pool();
    let lp_amount = 10_000_000;

    let user_x_before = token_balance(&pool.svm, &pool.user_x);
    let user_y_before = token_balance(&pool.svm, &pool.user_y);
    let vault_x_before = token_balance(&pool.svm, &pool.vault_x);
    let vault_y_before = token_balance(&pool.svm, &pool.vault_y);
    let user_lp_before = token_balance(&pool.svm, &pool.user_lp);

    let quote = cpmm_pool(&pool)
        .withdraw(lp_amount, amm::Side::X, 0, 0)
        .expect("cpmm quote");

    let withdraw = create_withdraw_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.vault_x,
        pool.vault_y,
        pool.user_x,
        pool.user_y,
        pool.user_lp,
        lp_amount,
        amm::OperationSide::X,
        0,
        0,
    );

    let burn_ix = create_burn_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.user_lp,
        lp_amount,
    );

    send(&mut pool.svm, &[burn_ix, withdraw], &pool.payer, &[&pool.payer]).expect("Burn + withdraw -> OK");

    assert_eq!(
        token_balance(&pool.svm, &pool.user_x),
        user_x_before + quote.withdraw_x,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.user_y),
        user_y_before,
        "single-sided X withdraw must not strand Y in the user wallet",
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.vault_x),
        vault_x_before - quote.withdraw_x,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.vault_y),
        vault_y_before,
        "pro-rata Y out and swap Y in cancel on single-sided X withdraw",
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.user_lp),
        user_lp_before - lp_amount,
    );
}

#[test]
fn test_single_sided_withdraw_y_preserves_user_funds() {
    let mut pool = setup_initialized_pool();
    let lp_amount = 10_000_000;

    let user_x_before = token_balance(&pool.svm, &pool.user_x);
    let user_y_before = token_balance(&pool.svm, &pool.user_y);
    let vault_x_before = token_balance(&pool.svm, &pool.vault_x);
    let vault_y_before = token_balance(&pool.svm, &pool.vault_y);
    let user_lp_before = token_balance(&pool.svm, &pool.user_lp);

    let quote = cpmm_pool(&pool)
        .withdraw(lp_amount, amm::Side::Y, 0, 0)
        .expect("cpmm quote");

    let withdraw = create_withdraw_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.vault_x,
        pool.vault_y,
        pool.user_x,
        pool.user_y,
        pool.user_lp,
        lp_amount,
        amm::OperationSide::Y,
        0,
        0,
    );

    let burn_ix = create_burn_ix(
        &mut pool.svm,
        &pool.payer,
        pool.mint_x,
        pool.mint_y,
        pool.config,
        pool.mint_lp,
        pool.user_lp,
        lp_amount,
    );


    send(&mut pool.svm, &[burn_ix,withdraw], &pool.payer, &[&pool.payer]).expect("single-sided Y withdraw");

    assert_eq!(
        token_balance(&pool.svm, &pool.user_x),
        user_x_before,
        "single-sided Y withdraw must not strand X in the user wallet",
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.user_y),
        user_y_before + quote.withdraw_y,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.vault_x),
        vault_x_before,
        "pro-rata X out and swap X in cancel on single-sided Y withdraw",
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.vault_y),
        vault_y_before - quote.withdraw_y,
    );
    assert_eq!(
        token_balance(&pool.svm, &pool.user_lp),
        user_lp_before - lp_amount,
    );
}
