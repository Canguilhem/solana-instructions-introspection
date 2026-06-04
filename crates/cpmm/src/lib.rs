mod error;

pub use error::{CpmmError, CpmmResult};

pub const BPS: u128 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    X,
    Y,
    Balanced,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolState {
    pub reserve_x: u64,
    pub reserve_y: u64,
    pub lp_supply: u64,
    pub fee_bps: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwapQuote {
    pub amount_in: u64,
    pub amount_out: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DepositQuote {
    pub lp_minted: u64,
    pub deposit_x: u64,
    pub deposit_y: u64,
    /// User → vault swap input (single-sided deposit).
    pub swap_in_x: u64,
    pub swap_in_y: u64,
    /// Vault → user swap output before the balanced deposit leg.
    pub swap_out_x: u64,
    pub swap_out_y: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WithdrawQuote {
    pub withdraw_x: u64,
    pub withdraw_y: u64,
    /// Pro-rata vault → user before the swap leg (0 for unused side on balanced).
    pub pro_rata_x: u64,
    pub pro_rata_y: u64,
    /// User → vault swap input (single-sided withdraw).
    pub swap_in_x: u64,
    pub swap_in_y: u64,
    /// Vault → user swap output.
    pub swap_out_x: u64,
    pub swap_out_y: u64,
}

macro_rules! cpmm_require {
    ($cond:expr, $err:expr) => {
        if !$cond {
            return Err($err);
        }
    };
}

impl PoolState {
    pub fn new(reserve_x: u64, reserve_y: u64, lp_supply: u64, fee_bps: u16) -> Self {
        Self {
            reserve_x,
            reserve_y,
            lp_supply,
            fee_bps,
        }
    }

    /// Constant-product swap with fee on input.
    ///
    /// Invariant: `k = x * y`. Fee factor `γ = (10_000 - fee_bps) / 10_000`.
    ///
    /// - `Side::X` (pay X, receive Y): `Δy = (γ * y * Δx) / (10_000 * x + γ * Δx)`
    /// - `Side::Y` (pay Y, receive X): `Δx = (γ * x * Δy) / (10_000 * y + γ * Δy)`
    ///
    /// Fails if `amount_out < min_out`.
    pub fn swap(&self, amount_in: u64, side: Side, min_out: u64) -> CpmmResult<SwapQuote> {
        cpmm_require!(amount_in > 0, CpmmError::InvalidAmount);
        cpmm_require!(
            matches!(side, Side::X | Side::Y),
            CpmmError::InvalidAmount
        );
        cpmm_require!(
            self.reserve_x > 0 && self.reserve_y > 0,
            CpmmError::ZeroBalance
        );
        cpmm_require!(self.fee_bps < 10_000, CpmmError::InvalidFee);

        let amount_out = self.swap_out(amount_in, side)?;
        cpmm_require!(amount_out >= min_out, CpmmError::SlippageExceeded);

        Ok(SwapQuote {
            amount_in,
            amount_out,
        })
    }

    /// Add liquidity and quote LP to mint.
    ///
    /// - `Side::Balanced` + `(Some(x), Some(y))`: double-sided deposit
    /// - `Side::X` + `(Some(x), None)`: SwapDeposit (swap half of X, then add balanced)
    /// - `Side::Y` + `(None, Some(y))`: SwapDeposit (swap half of Y, then add balanced)
    ///
    /// Fails if minted LP `< min_lp`.
    pub fn deposit(
        &self,
        token_x: Option<u64>,
        token_y: Option<u64>,
        side: Side,
        min_lp: u64,
    ) -> CpmmResult<DepositQuote> {
        cpmm_require!(self.fee_bps < 10_000, CpmmError::InvalidFee);

        match (token_x, token_y, side) {
            (Some(x), Some(y), Side::Balanced) => self.deposit_balanced(x, y, min_lp),
            (Some(x), None, Side::X) => self.deposit_single_x(x, min_lp),
            (None, Some(y), Side::Y) => self.deposit_single_y(y, min_lp),
            _ => Err(CpmmError::InvalidAmount),
        }
    }

    /// Burn LP and quote token amounts returned.
    ///
    /// - `Side::Balanced`: pro-rata `Δx = x * ΔL / L`, `Δy = y * ΔL / L`
    /// - `Side::X` / `Side::Y`: WithdrawSwap (pro-rata + swap undesired leg)
    ///
    /// Fails if `withdraw_x < min_x` or `withdraw_y < min_y`.
    pub fn withdraw(
        &self,
        lp_amount: u64,
        side: Side,
        min_x: u64,
        min_y: u64,
    ) -> CpmmResult<WithdrawQuote> {
        cpmm_require!(lp_amount > 0, CpmmError::InvalidAmount);
        cpmm_require!(self.lp_supply > 0, CpmmError::ZeroBalance);
        cpmm_require!(
            self.reserve_x > 0 && self.reserve_y > 0,
            CpmmError::ZeroBalance
        );

        let quote = match side {
            Side::Balanced => self.withdraw_balanced(lp_amount)?,
            Side::X => self.withdraw_single_x(lp_amount)?,
            Side::Y => self.withdraw_single_y(lp_amount)?,
        };

        cpmm_require!(quote.withdraw_x >= min_x, CpmmError::SlippageExceeded);
        cpmm_require!(quote.withdraw_y >= min_y, CpmmError::SlippageExceeded);

        Ok(quote)
    }

    /// Fee numerator: `γ = 10_000 - fee_bps` (denominator is always `10_000`).
    fn gamma(&self) -> CpmmResult<u128> {
        BPS.checked_sub(self.fee_bps as u128)
            .ok_or(CpmmError::InvalidFee)
    }

    /// Output amount for a swap; see [`Self::swap`] for formulas.
    fn swap_out(&self, amount_in: u64, side: Side) -> CpmmResult<u64> {
        let gamma = self.gamma()?;
        let x = self.reserve_x as u128;
        let y = self.reserve_y as u128;
        let amount = amount_in as u128;

        let out = match side {
            // Δx = (γ * x * Δy) / (10_000 * y + γ * Δy)
            Side::Y => gamma
                .checked_mul(x)
                .and_then(|v| v.checked_mul(amount))
                .and_then(|num| {
                    let den = BPS
                        .checked_mul(y)
                        .and_then(|v| v.checked_add(gamma.checked_mul(amount)?))?;
                    num.checked_div(den)
                }),
            // Δy = (γ * y * Δx) / (10_000 * x + γ * Δx)
            Side::X => gamma
                .checked_mul(y)
                .and_then(|v| v.checked_mul(amount))
                .and_then(|num| {
                    let den = BPS
                        .checked_mul(x)
                        .and_then(|v| v.checked_add(gamma.checked_mul(amount)?))?;
                    num.checked_div(den)
                }),
            Side::Balanced => return Err(CpmmError::InvalidAmount),
        }
        .ok_or(CpmmError::Overflow)? as u64;

        cpmm_require!(out > 0, CpmmError::InvalidAmount);
        Ok(out)
    }

    /// Double-sided deposit at the current pool ratio.
    ///
    /// First mint (`lp_supply == 0`): `L = isqrt(x * y)`, deposit all of `token_x` / `token_y`.
    ///
    /// Later mints — limiting side wins:
    /// ```text
    /// L         = min(Δx * L_supply / x,  Δy * L_supply / y)
    /// deposit_x = L * x / L_supply
    /// deposit_y = L * y / L_supply
    /// ```
    fn deposit_balanced(&self, token_x: u64, token_y: u64, min_lp: u64) -> CpmmResult<DepositQuote> {
        cpmm_require!(token_x > 0 && token_y > 0, CpmmError::InvalidAmount);

        if self.lp_supply == 0 {
            // L = sqrt(x * y)
            let lp_minted = isqrt(token_x as u128 * token_y as u128) as u64;
            cpmm_require!(lp_minted >= min_lp, CpmmError::SlippageExceeded);
            cpmm_require!(lp_minted > 0, CpmmError::InvalidAmount);
            return Ok(DepositQuote {
                lp_minted,
                deposit_x: token_x,
                deposit_y: token_y,
                swap_in_x: 0,
                swap_in_y: 0,
                swap_out_x: 0,
                swap_out_y: 0,
            });
        }

        let l = self.lp_supply as u128;
        let x = self.reserve_x as u128;
        let y = self.reserve_y as u128;
        let dx = token_x as u128;
        let dy = token_y as u128;

        // L = min(Δx * L / x, Δy * L / y)
        let lp_from_x = dx
            .checked_mul(l)
            .and_then(|v| v.checked_div(x))
            .ok_or(CpmmError::Overflow)?;
        let lp_from_y = dy
            .checked_mul(l)
            .and_then(|v| v.checked_div(y))
            .ok_or(CpmmError::Overflow)?;
        let lp = lp_from_x.min(lp_from_y);

        // deposit_x = L * x / L,  deposit_y = L * y / L
        let deposit_x = x
            .checked_mul(lp)
            .and_then(|v| v.checked_div(l))
            .ok_or(CpmmError::Overflow)? as u64;
        let deposit_y = y
            .checked_mul(lp)
            .and_then(|v| v.checked_div(l))
            .ok_or(CpmmError::Overflow)? as u64;
        let lp_minted = lp as u64;

        cpmm_require!(lp_minted >= min_lp, CpmmError::SlippageExceeded);
        cpmm_require!(deposit_x > 0 && deposit_y > 0, CpmmError::InvalidAmount);

        Ok(DepositQuote {
            lp_minted,
            deposit_x,
            deposit_y,
            swap_in_x: 0,
            swap_in_y: 0,
            swap_out_x: 0,
            swap_out_y: 0,
        })
    }

    /// SwapDeposit with X only: swap `token_x / 2` for Y, then balanced-deposit the rest + Y out.
    fn deposit_single_x(&self, token_x: u64, min_lp: u64) -> CpmmResult<DepositQuote> {
        cpmm_require!(token_x > 1, CpmmError::InvalidAmount);
        cpmm_require!(self.lp_supply > 0, CpmmError::ZeroBalance);

        let swap_amount = token_x / 2;
        let remaining_x = token_x - swap_amount;
        let y_out = self.swap_out(swap_amount, Side::X)?;

        let pool_after_swap = PoolState::new(
            self.reserve_x
                .checked_add(swap_amount)
                .ok_or(CpmmError::Overflow)?,
            self.reserve_y.checked_sub(y_out).ok_or(CpmmError::Underflow)?,
            self.lp_supply,
            self.fee_bps,
        );

        let mut quote = pool_after_swap.deposit_balanced(remaining_x, y_out, min_lp)?;
        quote.swap_in_x = swap_amount;
        quote.swap_out_y = y_out;
        Ok(quote)
    }

    /// SwapDeposit with Y only: swap `token_y / 2` for X, then balanced-deposit the rest + X out.
    fn deposit_single_y(&self, token_y: u64, min_lp: u64) -> CpmmResult<DepositQuote> {
        cpmm_require!(token_y > 1, CpmmError::InvalidAmount);
        cpmm_require!(self.lp_supply > 0, CpmmError::ZeroBalance);

        let swap_amount = token_y / 2;
        let remaining_y = token_y - swap_amount;
        let x_out = self.swap_out(swap_amount, Side::Y)?;

        let pool_after_swap = PoolState::new(
            self.reserve_x.checked_sub(x_out).ok_or(CpmmError::Underflow)?,
            self.reserve_y
                .checked_add(swap_amount)
                .ok_or(CpmmError::Overflow)?,
            self.lp_supply,
            self.fee_bps,
        );

        let mut quote = pool_after_swap.deposit_balanced(x_out, remaining_y, min_lp)?;
        quote.swap_in_y = swap_amount;
        quote.swap_out_x = x_out;
        Ok(quote)
    }

    /// Pro-rata withdraw: `Δx = x * ΔL / L`, `Δy = y * ΔL / L` (price unchanged).
    fn withdraw_balanced(&self, lp_amount: u64) -> CpmmResult<WithdrawQuote> {
        let l = self.lp_supply as u128;
        let dl = lp_amount as u128;
        let x = self.reserve_x as u128;
        let y = self.reserve_y as u128;

        let withdraw_x = x
            .checked_mul(dl)
            .and_then(|v| v.checked_div(l))
            .ok_or(CpmmError::Overflow)? as u64;
        let withdraw_y = y
            .checked_mul(dl)
            .and_then(|v| v.checked_div(l))
            .ok_or(CpmmError::Overflow)? as u64;

        cpmm_require!(withdraw_x > 0 && withdraw_y > 0, CpmmError::InvalidAmount);

        Ok(WithdrawQuote {
            withdraw_x,
            withdraw_y,
            pro_rata_x: withdraw_x,
            pro_rata_y: withdraw_y,
            swap_in_x: 0,
            swap_in_y: 0,
            swap_out_x: 0,
            swap_out_y: 0,
        })
    }

    /// WithdrawSwap: receive X only.
    ///
    /// 1. Pro-rata: `wx0 = x * ΔL / L`, `wy = y * ΔL / L`
    /// 2. Swap `wy` on reduced reserves `(x - wx0, y - wy)` for additional X
    /// 3. Total X out: `wx0 + x_swap`
    fn withdraw_single_x(&self, lp_amount: u64) -> CpmmResult<WithdrawQuote> {
        let l = self.lp_supply as u128;
        let dl = lp_amount as u128;
        let x = self.reserve_x as u128;
        let y = self.reserve_y as u128;

        let wx0 = x
            .checked_mul(dl)
            .and_then(|v| v.checked_div(l))
            .ok_or(CpmmError::Overflow)?;
        let wy = y
            .checked_mul(dl)
            .and_then(|v| v.checked_div(l))
            .ok_or(CpmmError::Overflow)?;

        // reserves after removing pro-rata share (before swap)
        let x1 = x.checked_sub(wx0).ok_or(CpmmError::Underflow)?;
        let y1 = y.checked_sub(wy).ok_or(CpmmError::Underflow)?;

        let x_swap = swap_out_y_for_x(x1, y1, wy as u64, self.fee_bps)?;
        let total_x = wx0
            .checked_add(x_swap as u128)
            .ok_or(CpmmError::Overflow)? as u64;

        cpmm_require!(total_x > 0, CpmmError::InvalidAmount);

        Ok(WithdrawQuote {
            withdraw_x: total_x,
            withdraw_y: 0,
            pro_rata_x: wx0 as u64,
            pro_rata_y: wy as u64,
            swap_in_x: 0,
            swap_in_y: wy as u64,
            swap_out_x: x_swap,
            swap_out_y: 0,
        })
    }

    /// WithdrawSwap: receive Y only.
    ///
    /// 1. Pro-rata: `wx = x * ΔL / L`, `wy0 = y * ΔL / L`
    /// 2. Swap `wx` on reduced reserves for additional Y
    /// 3. Total Y out: `wy0 + y_swap`
    fn withdraw_single_y(&self, lp_amount: u64) -> CpmmResult<WithdrawQuote> {
        let l = self.lp_supply as u128;
        let dl = lp_amount as u128;
        let x = self.reserve_x as u128;
        let y = self.reserve_y as u128;

        let wx = x
            .checked_mul(dl)
            .and_then(|v| v.checked_div(l))
            .ok_or(CpmmError::Overflow)?;
        let wy0 = y
            .checked_mul(dl)
            .and_then(|v| v.checked_div(l))
            .ok_or(CpmmError::Overflow)?;

        let x1 = x.checked_sub(wx).ok_or(CpmmError::Underflow)?;
        let y1 = y.checked_sub(wy0).ok_or(CpmmError::Underflow)?;

        let y_swap = swap_out_x_for_y(x1, y1, wx as u64, self.fee_bps)?;
        let total_y = wy0
            .checked_add(y_swap as u128)
            .ok_or(CpmmError::Overflow)? as u64;

        cpmm_require!(total_y > 0, CpmmError::InvalidAmount);

        Ok(WithdrawQuote {
            withdraw_x: 0,
            withdraw_y: total_y,
            pro_rata_x: wx as u64,
            pro_rata_y: wy0 as u64,
            swap_in_x: wx as u64,
            swap_in_y: 0,
            swap_out_x: 0,
            swap_out_y: y_swap,
        })
    }
}

/// Pay Y on reserves `(x, y)`, receive X: `Δx = (γ * x * Δy) / (10_000 * y + γ * Δy)`.
fn swap_out_y_for_x(x: u128, y: u128, amount_y: u64, fee_bps: u16) -> CpmmResult<u64> {
    let gamma = BPS.checked_sub(fee_bps as u128).ok_or(CpmmError::InvalidFee)?;
    let dy = amount_y as u128;

    let out = gamma
        .checked_mul(x)
        .and_then(|v| v.checked_mul(dy))
        .and_then(|num| {
            let den = BPS
                .checked_mul(y)
                .and_then(|v| v.checked_add(gamma.checked_mul(dy)?))?;
            num.checked_div(den)
        })
        .ok_or(CpmmError::Overflow)? as u64;

    Ok(out)
}

/// Pay X on reserves `(x, y)`, receive Y: `Δy = (γ * y * Δx) / (10_000 * x + γ * Δx)`.
fn swap_out_x_for_y(x: u128, y: u128, amount_x: u64, fee_bps: u16) -> CpmmResult<u64> {
    let gamma = BPS.checked_sub(fee_bps as u128).ok_or(CpmmError::InvalidFee)?;
    let dx = amount_x as u128;

    let out = gamma
        .checked_mul(y)
        .and_then(|v| v.checked_mul(dx))
        .and_then(|num| {
            let den = BPS
                .checked_mul(x)
                .and_then(|v| v.checked_add(gamma.checked_mul(dx)?))?;
            num.checked_div(den)
        })
        .ok_or(CpmmError::Overflow)? as u64;

    Ok(out)
}

/// Integer square root for first-mint LP: `L = isqrt(x * y)`.
pub fn isqrt(n: u128) -> u128 {
    if n == 0 {
        return 0;
    }

    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}
