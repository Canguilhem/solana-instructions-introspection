use cpmm::{isqrt, CpmmError, PoolState, Side};

macro_rules! assert_cpmm_err {
    ($result:expr, $code:ident) => {
        let err = $result.expect_err(concat!("expected CpmmError::", stringify!($code)));
        assert_eq!(err, CpmmError::$code);
    };
}

fn equal_pool(fee_bps: u16) -> PoolState {
    PoolState::new(100_000_000, 100_000_000, 100_000_000, fee_bps)
}

// --- swap ---

#[test]
fn swap_y_for_x_matches_uniswap_tutorial() {
    let pool = PoolState::new(1_000_000_000, 1_000_000_000_000, 31_622_776, 30);
    let quote = pool.swap(1_000_000_000, Side::Y, 0).unwrap();
    assert_eq!(quote.amount_out, 996_006);
}

#[test]
fn swap_x_for_y_zero_fee_exact() {
    let pool = PoolState::new(20, 30, 0, 0);
    let quote = pool.swap(5, Side::X, 0).unwrap();
    assert_eq!(quote.amount_in, 5);
    assert_eq!(quote.amount_out, 6);
}

#[test]
fn swap_x_for_y_with_fee_exact() {
    let pool = equal_pool(30);
    let quote = pool.swap(10_000_000, Side::X, 0).unwrap();
    assert_eq!(quote.amount_out, 9_066_108);
}

#[test]
fn swap_rejects_slippage_limit_exceeded() {
    let pool = equal_pool(30);
    assert_cpmm_err!(
        pool.swap(10_000_000, Side::X, 9_066_109),
        SlippageExceeded
    );
}

#[test]
fn swap_rejects_zero_amount_in() {
    let pool = equal_pool(30);
    assert_cpmm_err!(pool.swap(0, Side::X, 0), InvalidAmount);
}

#[test]
fn swap_rejects_balanced_side() {
    let pool = equal_pool(30);
    assert_cpmm_err!(pool.swap(1, Side::Balanced, 0), InvalidAmount);
}

#[test]
fn swap_rejects_empty_pool() {
    let pool = PoolState::new(0, 0, 0, 30);
    assert_cpmm_err!(pool.swap(1, Side::X, 0), ZeroBalance);
}

#[test]
fn swap_zero_fee_preserves_k() {
    let pool = PoolState::new(20, 30, 0, 0);
    let k_before = 20u128 * 30;
    let quote = pool.swap(5, Side::X, 0).unwrap();
    let k_after = (20 + quote.amount_in) as u128 * (30 - quote.amount_out) as u128;
    assert_eq!(k_before, k_after);
}

// --- deposit ---

#[test]
fn balanced_deposit_first_mint() {
    let pool = PoolState::new(0, 0, 0, 30);
    let quote = pool
        .deposit(Some(100_000_000), Some(100_000_000), Side::Balanced, 0)
        .unwrap();
    assert_eq!(quote.lp_minted, 100_000_000);
    assert_eq!(quote.deposit_x, 100_000_000);
    assert_eq!(quote.deposit_y, 100_000_000);
}

#[test]
fn balanced_deposit_subsequent_mint() {
    let pool = equal_pool(30);
    let quote = pool
        .deposit(Some(1_000_000), Some(1_000_000), Side::Balanced, 0)
        .unwrap();
    assert_eq!(quote.lp_minted, 1_000_000);
    assert_eq!(quote.deposit_x, 1_000_000);
    assert_eq!(quote.deposit_y, 1_000_000);
}

#[test]
fn balanced_deposit_imbalanced_amounts_use_limiting_side() {
    let pool = equal_pool(30);
    let quote = pool
        .deposit(
            Some(200_000_000),
            Some(100_000_000),
            Side::Balanced,
            0,
        )
        .unwrap();
    assert_eq!(quote.lp_minted, 100_000_000);
    assert_eq!(quote.deposit_x, 100_000_000);
    assert_eq!(quote.deposit_y, 100_000_000);
}

#[test]
fn deposit_rejects_min_lp_slippage() {
    let pool = equal_pool(30);
    assert_cpmm_err!(
        pool.deposit(
            Some(1_000_000),
            Some(1_000_000),
            Side::Balanced,
            1_000_001,
        ),
        SlippageExceeded
    );
}

#[test]
fn deposit_rejects_invalid_side_combination() {
    let pool = equal_pool(30);
    assert_cpmm_err!(
        pool.deposit(None, None, Side::Balanced, 0),
        InvalidAmount
    );
    assert_cpmm_err!(
        pool.deposit(Some(1), None, Side::Balanced, 0),
        InvalidAmount
    );
}

#[test]
fn deposit_single_x_succeeds() {
    let pool = equal_pool(30);
    let quote = pool
        .deposit(Some(20_000_000), None, Side::X, 0)
        .unwrap();
    assert_eq!(quote.lp_minted, 9_090_909);
    assert_eq!(quote.deposit_x, 9_999_999);
    assert_eq!(quote.deposit_y, 8_266_717);
}

#[test]
fn deposit_single_y_succeeds() {
    let pool = equal_pool(30);
    let quote = pool
        .deposit(None, Some(20_000_000), Side::Y, 0)
        .unwrap();
    assert_eq!(quote.lp_minted, 9_090_909);
    assert_eq!(quote.deposit_x, 8_266_717);
    assert_eq!(quote.deposit_y, 9_999_999);
}

// --- withdraw ---

#[test]
fn balanced_withdraw_pro_rata() {
    let pool = equal_pool(30);
    let quote = pool
        .withdraw(25_000_000, Side::Balanced, 0, 0)
        .unwrap();
    assert_eq!(quote.withdraw_x, 25_000_000);
    assert_eq!(quote.withdraw_y, 25_000_000);
}

#[test]
fn balanced_withdraw_one_percent_of_pool() {
    let pool = equal_pool(30);
    let quote = pool
        .withdraw(1_000_000, Side::Balanced, 0, 0)
        .unwrap();
    assert_eq!(quote.withdraw_x, 1_000_000);
    assert_eq!(quote.withdraw_y, 1_000_000);
}

#[test]
fn withdraw_rejects_slippage_on_x() {
    let pool = equal_pool(30);
    assert_cpmm_err!(
        pool.withdraw(25_000_000, Side::Balanced, 25_000_001, 0),
        SlippageExceeded
    );
}

#[test]
fn withdraw_rejects_zero_lp_amount() {
    let pool = equal_pool(30);
    assert_cpmm_err!(pool.withdraw(0, Side::Balanced, 0, 0), InvalidAmount);
}

#[test]
fn withdraw_rejects_empty_pool() {
    let pool = PoolState::new(100, 100, 0, 30);
    assert_cpmm_err!(pool.withdraw(1, Side::Balanced, 0, 0), ZeroBalance);
}

#[test]
fn withdraw_single_x_succeeds() {
    let pool = equal_pool(30);
    let quote = pool
        .withdraw(10_000_000, Side::X, 0, 0)
        .unwrap();
    assert_eq!(quote.withdraw_x, 18_975_692);
    assert_eq!(quote.withdraw_y, 0);
}

#[test]
fn withdraw_single_y_succeeds() {
    let pool = equal_pool(30);
    let quote = pool
        .withdraw(10_000_000, Side::Y, 0, 0)
        .unwrap();
    assert_eq!(quote.withdraw_x, 0);
    assert_eq!(quote.withdraw_y, 18_975_692);
}

// --- isqrt ---

#[test]
fn isqrt_edge_cases() {
    assert_eq!(isqrt(0), 0);
    assert_eq!(isqrt(1), 1);
    assert_eq!(isqrt(4), 2);
    assert_eq!(isqrt(10), 3);
    assert_eq!(isqrt(100_000_000u128 * 100_000_000), 100_000_000);
}

#[test]
fn first_mint_lp_uses_isqrt() {
    let pool = PoolState::new(0, 0, 0, 30);
    let quote = pool
        .deposit(Some(200), Some(800), Side::Balanced, 0)
        .unwrap();
    assert_eq!(quote.lp_minted, 400);
}

// --- fee boundaries ---

#[test]
fn swap_with_max_valid_fee() {
    let pool = PoolState::new(1_000_000, 1_000_000, 0, 9_999);
    let quote = pool.swap(100_000, Side::X, 0).unwrap();
    assert!(quote.amount_out > 0);
    assert_eq!(quote.amount_in, 100_000);
}

#[test]
fn swap_rejects_max_fee() {
    let pool = PoolState::new(100, 100, 0, 10_000);
    assert_cpmm_err!(pool.swap(10, Side::X, 0), InvalidFee);
}

// --- round-trip sanity ---

#[test]
fn deposit_then_withdraw_same_lp_returns_same_amounts() {
    let pool = equal_pool(30);
    let deposit = pool
        .deposit(
            Some(10_000_000),
            Some(10_000_000),
            Side::Balanced,
            0,
        )
        .unwrap();

    let pool_after = PoolState::new(
        pool.reserve_x + deposit.deposit_x,
        pool.reserve_y + deposit.deposit_y,
        pool.lp_supply + deposit.lp_minted,
        pool.fee_bps,
    );

    let withdraw = pool_after
        .withdraw(deposit.lp_minted, Side::Balanced, 0, 0)
        .unwrap();

    assert_eq!(withdraw.withdraw_x, deposit.deposit_x);
    assert_eq!(withdraw.withdraw_y, deposit.deposit_y);
}
