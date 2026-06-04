# CPMM — Constant Product Market Maker

Anchor-free constant-product math for the AMM program. Implements Uniswap V2-style swap, deposit, and withdraw formulas with integer arithmetic (`u64` / `u128`).

Based on the [Uniswap V2 math tutorial (Coinmonks / UniswapPy)](https://medium.com/coinmonks/uniswap-v2-math-tutorial-using-uniswappy-abb23cdef005).

## Design

| Layer | Location | Responsibility |
| ----- | -------- | -------------- |
| **CPMM** | `crates/cpmm/` | Pure quotes; returns `CpmmResult<T>` |
| **Instructions** | `instructions/` | Read accounts, call cpmm, execute CPIs |
| **Program errors** | `error.rs` | `CpmmError` → `AmmError` mapping |

CPMM has **no** `anchor_lang` dependency. Instruction args use `OperationSide` (Anchor-serializable in `state.rs`); convert with `.into()` to cpmm `Side`.

## Types

### `PoolState`

Snapshot of pool reserves at quote time:

```rust
PoolState {
    reserve_x: u64,   // vault X balance
    reserve_y: u64,   // vault Y balance
    lp_supply: u64,   // total LP mint supply
    fee_bps: u16,     // swap fee, 0–9999 (e.g. 30 = 0.30%)
}
```

### `Side`

| Variant | Use |
| ------- | --- |
| `X` | Pay X, receive Y (swap) — or single-sided X deposit |
| `Y` | Pay Y, receive X (swap) — or single-sided Y deposit |
| `Balanced` | Double-sided deposit / pro-rata withdraw |

### Quote structs

- **`SwapQuote`** — `{ amount_in, amount_out }`
- **`DepositQuote`** — `{ lp_minted, deposit_x, deposit_y }`
- **`WithdrawQuote`** — `{ withdraw_x, withdraw_y }`

## API

```rust
impl PoolState {
    fn swap(&self, amount_in: u64, side: Side, min_out: u64) -> CpmmResult<SwapQuote>;
    fn deposit(&self, token_x: Option<u64>, token_y: Option<u64>, side: Side, min_lp: u64) -> CpmmResult<DepositQuote>;
    fn withdraw(&self, lp_amount: u64, side: Side, min_x: u64, min_y: u64) -> CpmmResult<WithdrawQuote>;
}
```

All slippage checks happen inside cpmm (`min_out`, `min_lp`, `min_x` / `min_y`). Instructions pass user-supplied floors through unchanged.

## Math

Fee factor (basis points):

\[
\gamma = \frac{10\,000 - \text{fee\_bps}}{10\,000}
\]

All division is **integer floor division** on `u128` intermediates.

### Swap

Invariant: \(k = x \cdot y\). Fee applied to **input**; output computed so the post-swap reserves sit on the curve.

**Pay Y, receive X** (`Side::Y`):

\[
\Delta x = \frac{\gamma \cdot x \cdot \Delta y}{10\,000 \cdot y + \gamma \cdot \Delta y}
\]

**Pay X, receive Y** (`Side::X`):

\[
\Delta y = \frac{\gamma \cdot y \cdot \Delta x}{10\,000 \cdot x + \gamma \cdot \Delta x}
\]

Example (0.3% fee): pool `x = 1000`, `y = 1_000_000`, pay `dy = 1000` → receive `dx ≈ 0.99601`.

Zero-fee check: pool `x = 20`, `y = 30`, pay `5` X → receive `6` Y; \(k\) unchanged.

### Deposit — balanced (`Side::Balanced`, both tokens)

**First mint** (`lp_supply == 0`):

\[
L = \lfloor\sqrt{x \cdot y}\rfloor
\]

Deposits the full `token_x` and `token_y`; mints `L` LP.

**Subsequent mints** — maintain pool ratio; limiting side wins:

\[
L = \min\left(\frac{\Delta x \cdot L_{\text{supply}}}{x},\ \frac{\Delta y \cdot L_{\text{supply}}}{y}\right)
\]

\[
\text{deposit\_x} = \frac{L \cdot x}{L_{\text{supply}}},\quad
\text{deposit\_y} = \frac{L \cdot y}{L_{\text{supply}}}
\]

If the user offers more than needed on one side, the excess is not consumed (instruction only transfers quoted amounts).

### Deposit — single-sided (`Side::X` or `Side::Y`)

Composed **SwapDeposit** (not in core Uniswap V2 contracts):

1. Swap half of the input token for the other via the swap formula.
2. Add liquidity with `(remaining_input, swapped_output)` at the **post-swap** pool ratio.

Approximates “deposit with one asset only.” On-chain instruction support uses the same quote; token flows must cover both legs.

### Withdraw — balanced (`Side::Balanced`)

Pro-rata exit; price unchanged:

\[
\Delta x = \frac{x \cdot \Delta L}{L},\quad
\Delta y = \frac{y \cdot \Delta L}{L}
\]

### Withdraw — single-sided (`Side::X` or `Side::Y`)

Composed **WithdrawSwap**:

1. Pro-rata withdraw \((\Delta x_0, \Delta y_0)\) for burning \(\Delta L\).
2. Swap the undesired token leg on post-withdraw reserves into the desired token.
3. User receives one asset only (`withdraw_x = 0` or `withdraw_y = 0` in quote).

## Errors

`CpmmError` is independent of Anchor:

| `CpmmError` | Mapped `AmmError` | When |
| ----------- | ----------------- | ---- |
| `InvalidAmount` | `InvalidAmount` | Zero input, bad side combo, zero output |
| `ZeroBalance` | `ZeroBalance` | Empty reserves or zero LP supply |
| `InvalidFee` | `InvalidFeeAmount` | `fee_bps >= 10_000` |
| `SlippageExceeded` | `SlippageLimitExceeded` | Quote below user floor |
| `Overflow` | `Overflow` | `checked_*` arithmetic failed (product too large) |
| `Underflow` | `Underflow` | Reserve would go negative in composed ops |

Mapping is defined in `programs/amm/src/error.rs`:

```rust
impl From<CpmmError> for AmmError { ... }
```

Instructions convert at the boundary:

```rust
let quote = pool.swap(amount, side.into(), min_out).map_err(AmmError::from)?;
```

## Usage in instructions

```rust
let pool = PoolState::new(
    self.vault_x.amount,
    self.vault_y.amount,
    self.mint_lp.supply,
    self.config.fee,
);

let quote = pool
    .swap(amount, side.into(), min_out)
    .map_err(AmmError::from)?;
// CPI: transfer quote.amount_in in, quote.amount_out out
```

## Client / off-chain quoting

Use the same `PoolState` inputs as on-chain (read vault ATAs + LP mint supply + config fee):

```rust
use amm::{PoolState, Side};

let pool = PoolState::new(vault_x_amount, vault_y_amount, lp_supply, fee_bps);
let swap = pool.swap(10_000_000, Side::X, 0)?;
// set min_out = swap.amount_out * (10_000 - slippage_bps) / 10_000
```

## Tests

`tests.rs` in this module (29 unit tests). Covers:

- Tutorial swap example (exact output)
- Zero-fee swap preserving \(k\)
- First mint / subsequent / imbalanced deposits
- Pro-rata and single-sided withdraw quotes
- Slippage and invalid-input errors
- Fee boundary (9999 bps valid, 10000 rejected)
- Deposit → withdraw round-trip
- `CpmmError` → `AmmError` mapping

```bash
cargo test -p cpmm
```

Program-level LiteSVM tests in `programs/amm/tests/tests.rs` verify CPI accounting and guards (`PoolLocked`, `Unauthorized`) end-to-end.

## File layout

```
crates/cpmm/
  src/lib.rs     PoolState impl, formulas, isqrt
  src/error.rs   CpmmError, CpmmResult
  tests/cpmm.rs  integration tests
  README.md      this file
```
