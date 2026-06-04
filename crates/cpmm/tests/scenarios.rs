//! Worked examples for token flows (pool: 100M X / 100M Y / 100M LP, fee 30 bps).
//!
//! These tests document *who sends what* on-chain — not just final balances.

use cpmm::{PoolState, Side};

const RESERVE: u64 = 100_000_000;
const FEE_BPS: u16 = 30;

fn equal_pool() -> PoolState {
    PoolState::new(RESERVE, RESERVE, RESERVE, FEE_BPS)
}

// --- Scenario 1: imbalanced balanced deposit ---

#[test]
fn scenario_imbalanced_balanced_deposit() {
    // User offers 2M X + 1M Y on a 1:1 pool. Only 1M of each is used; 1M X stays in wallet.
    let pool = equal_pool();
    let quote = pool
        .deposit(Some(2_000_000), Some(1_000_000), Side::Balanced, 0)
        .unwrap();

    assert_eq!(quote.deposit_x, 1_000_000);
    assert_eq!(quote.deposit_y, 1_000_000);
    assert_eq!(quote.lp_minted, 1_000_000);
    assert_eq!(quote.swap_in_x, 0);

    // On-chain: transfer exactly deposit_x / deposit_y (not the full 2M / 1M args).
    // User wallet delta: -1M X, -1M Y. Leftover: 1M X never leaves wallet.
}

// --- Scenario 2: single-sided X deposit (SwapDeposit) ---

#[test]
fn scenario_single_sided_deposit_x_flow() {
    // User offers 20M X only. Program: swap half → deposit balanced with swap output.
    let pool = equal_pool();
    let quote = pool.deposit(Some(20_000_000), None, Side::X, 0).unwrap();

    assert_eq!(quote.swap_in_x, 10_000_000);
    assert_eq!(quote.swap_out_y, 9_066_108);
    assert_eq!(quote.deposit_x, 9_999_999);
    assert_eq!(quote.deposit_y, 8_266_717);
    assert_eq!(quote.lp_minted, 9_090_909);

    let user_x_spent = quote.swap_in_x + quote.deposit_x;
    assert_eq!(user_x_spent, 19_999_999, "1 lamport of X stays in wallet");

    let user_y_refund = quote.swap_out_y - quote.deposit_y;
    assert_eq!(user_y_refund, 799_391, "unused swap Y stays in wallet");

    // Step-by-step CPI order (deposit instruction):
    //   1. user → vault   10_000_000 X   (swap in)
    //   2. vault → user    9_066_108 Y   (swap out)
    //   3. user → vault    9_999_999 X   (deposit leg)
    //   4. user → vault    8_266_717 Y   (deposit leg)
    //   5. mint            9_090_909 LP
    //
    // User net: -19_999_999 X, +799_391 Y, +9_090_909 LP
    // Vault net: +19_999_999 X, -799_391 Y
}

// --- Scenario 3: single-sided X withdraw (pro-rata + swap) ---

#[test]
fn scenario_single_sided_withdraw_x_flow() {
    // Burn 10M LP (10% of pool). User wants X only.
    let pool = equal_pool();
    let quote = pool.withdraw(10_000_000, Side::X, 0, 0).unwrap();

    assert_eq!(quote.pro_rata_x, 10_000_000);
    assert_eq!(quote.pro_rata_y, 10_000_000);
    assert_eq!(quote.swap_in_y, 10_000_000);
    assert_eq!(quote.swap_out_x, 8_975_692);
    assert_eq!(quote.withdraw_x, 18_975_692);
    assert_eq!(quote.withdraw_y, 0);

    // Step-by-step CPI order (withdraw instruction):
    //   1. vault → user  10_000_000 X   (pro-rata X)
    //   2. vault → user  10_000_000 Y   (pro-rata Y)
    //   3. user → vault  10_000_000 Y   (swap in)
    //   4. vault → user   8_975_692 X   (swap out)
    //   5. burn           10_000_000 LP
    //
    // User net: +18_975_692 X, 0 Y, -10M LP
    // Vault net: -18_975_692 X, 0 Y  (pro-rata Y out cancels swap Y in)
}

// --- Scenario 4: single-sided Y withdraw (mirror of scenario 3) ---

#[test]
fn scenario_single_sided_withdraw_y_flow() {
    let pool = equal_pool();
    let quote = pool.withdraw(10_000_000, Side::Y, 0, 0).unwrap();

    assert_eq!(quote.pro_rata_x, 10_000_000);
    assert_eq!(quote.pro_rata_y, 10_000_000);
    assert_eq!(quote.swap_in_x, 10_000_000);
    assert_eq!(quote.swap_out_y, 8_975_692);
    assert_eq!(quote.withdraw_x, 0);
    assert_eq!(quote.withdraw_y, 18_975_692);

    // User net: 0 X, +18_975_692 Y, -10M LP
    // Vault net: 0 X, -18_975_692 Y
}

// --- Scenario 5: balanced withdraw (no swap leg) ---

#[test]
fn scenario_balanced_withdraw_flow() {
    let pool = equal_pool();
    let quote = pool.withdraw(25_000_000, Side::Balanced, 0, 0).unwrap();

    assert_eq!(quote.withdraw_x, 25_000_000);
    assert_eq!(quote.withdraw_y, 25_000_000);
    assert_eq!(quote.pro_rata_x, quote.withdraw_x);
    assert_eq!(quote.pro_rata_y, quote.withdraw_y);
    assert_eq!(quote.swap_in_x, 0);
    assert_eq!(quote.swap_in_y, 0);

    // Two transfers + LP burn. No swap passthrough, no token burn.
}
