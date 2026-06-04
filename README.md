# Dice Game

An Anchor program where players bet SOL on a number (1–99). The house signs the bet and reveals an Ed25519 signature that deterministically sets the dice roll. Players win if their pick is **above** the roll; payout scales with risk (lower picks pay more).

Built with **Anchor 1.0** (`anchor-lang` 1.0.2, `@anchor-lang/core` for tests).

## Game rules

| Concept | Detail |
|--------|--------|
| Player pick (`roll`) | Integer **1–99** — bet that the drawn result lands **under** this number |
| Drawn roll | Derived from **SHA-256** of the house’s 64-byte Ed25519 signature → value in **1..100** |
| Win | `bet.roll > drawn_roll` (strictly greater) |
| Lose | `bet.roll <= drawn_roll` — stake stays in the vault (bet account is closed to player; no refund on resolve) |
| House edge | **150 bps** (1.5%) applied to winning payouts |

### Payout (on win)

```
payout = amount × (10_000 − HOUSE_EDGE_BPS) / (bet.roll − 1) / 100
```

- `bet.roll − 1` is the count of winning outcomes (e.g. pick 50 → 49 ways to win).
- **Lower pick** → fewer winning outcomes → **higher** multiplier if you win.
- **Higher pick** → more winning outcomes → **lower** multiplier if you win.

### Roll derivation (must match off-chain helpers)

```
hash = SHA256(signature_bytes)
lower = u128::from_le_bytes(hash[0..16])
upper = u128::from_le_bytes(hash[16..32])
drawn_roll = (lower + upper) % 100 + 1   // 1..100, wrapping arithmetic
```

## Accounts & PDAs

### Vault

- **Seeds:** `["vault", house_pubkey]`
- **Type:** System-owned PDA holding house liquidity and player stakes.
- Funded by the house via `initialize`; receives bet deposits; pays winners and refunds.

### Bet

- **Seeds:** `["bet", vault_pubkey, player_pubkey, seed.to_le_bytes()]`
- **Type:** Anchor account (`Bet` struct).

| Field | Description |
|-------|-------------|
| `player` | Bettor pubkey |
| `seed` | Client-chosen `u128` for unique PDA per bet |
| `slot` | Slot at placement (used for refund timeout) |
| `amount` | Lamports wagered |
| `roll` | Player’s target (1–99) |
| `bump` | Bet PDA bump |

`Bet::to_slice()` serializes account fields (no discriminator) for the Ed25519 signed message.

### Constants

| Constant | Value |
|----------|-------|
| `MIN_BET_LAMPORTS` | 10_000_000 (0.01 SOL) |
| `MIN_ROLL` / `MAX_ROLL` | 1 / 99 |
| `HOUSE_EDGE_BPS` | 150 |
| Refund delay | **> 1000 slots** after bet placement |

## Instruction flow

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌──────────────────────────────┐
│ initialize  │────▶│  place_bet  │────▶│ resolve_bet │     │ refund_bet (optional path)   │
│ house funds │     │ player stake│     │ house signs │     │ player, after 1000+ slots    │
│ vault       │     │ + Bet PDA   │     │ + payout?   │     │ full stake returned          │
└─────────────┘     └─────────────┘     └─────────────┘     └──────────────────────────────┘
```

### 1. `initialize(amount)`

- **Signer:** house
- Transfers `amount` lamports from house → vault PDA.
- Does not create the vault account explicitly; the vault must already exist or receive funds (first transfer to the PDA address).

### 2. `place_bet(seed, amount, roll)`

- **Signer:** player
- Validates `amount`, `roll` bounds.
- Creates `Bet` PDA, records slot/time and parameters.
- Transfers `amount` from player → vault.

### 3. `resolve_bet(sig)`

- **Signer:** house
- **Argument:** `sig` — 64-byte Ed25519 signature (must match the preceding verify instruction).
- **Requires:** Instructions sysvar; **previous instruction** in the same transaction must be the native **Ed25519 verify** program.

**Verification steps:**

1. Load instruction at index `-1` from the instructions sysvar.
2. Confirm it is the Ed25519 program with no accounts.
3. Parse instruction data (single signature, all index fields `u16::MAX` — data lives in that instruction only).
4. Pubkey in the Ed25519 ix must be the house.
5. Signature bytes must equal `sig`.
6. Signed message must equal `bet.to_slice()`.

**Resolution:**

1. Compute `drawn_roll` from `hash(sig)` (see above).
2. If `bet.roll > drawn_roll`, transfer payout from vault → player (vault PDA signs).
3. Close bet account to player (rent return).

**Typical transaction layout:**

```
[0] Ed25519Program.createInstructionWithPrivateKey({ message: bet_account_data_without_discriminator })
[1] resolve_bet(sig_bytes)
```

### 4. `refund_bet`

- **Signer:** player
- Requires more than **1000 slots** since `bet.slot`.
- Returns full `bet.amount` from vault → player (vault PDA signer).
- Closes bet account to player.

## Program ID

Localnet (from `Anchor.toml`):

```
3hvpNKz6QeYsALNMwF18ViCT9K31wsvNqS5STbfDZMso
```

## Development

### Prerequisites

- [Anchor](https://www.anchor-lang.com/) 1.0.x (`avm install 1.0.0` or newer)
- Solana / Agave toolchain compatible with Anchor 1.0
- Node + Yarn

### Build & test

```bash
anchor build
anchor test
```

Tests live in `tests/dice_game.ts` and use `@anchor-lang/core` + `@solana/web3.js`. The suite:

1. Airdrops SOL to house and player
2. Initializes vault liquidity
3. Places a bet
4. Resolves with a two-instruction transaction (Ed25519 + `resolve_bet`)

Off-chain helpers in the test file (`resolveRoll`, `calculatePayout`, `formatSol`) mirror on-chain math for logging.

### Custom test script

`Anchor.toml` runs `yarn run ts-mocha` for tests. Use `anchor test` (not raw `ts-mocha` alone) so the program is built and deployed to the test validator.

## Project layout

```
programs/dice_game/src/
  lib.rs              # Instruction entrypoints
  state/mod.rs        # Bet account, constants
  instructions/
    initialize.rs     # Fund vault
    place_bet.rs      # Create bet, deposit
    resolve_bet.rs    # Ed25519 check, roll, payout
    refund_bet.rs     # Timeout refund
tests/dice_game.ts    # Integration tests
```

## Security notes

- **Commitment:** The house commits to the bet by signing `bet.to_slice()`; the signature bytes double as the RNG seed after verification.
- **Ed25519 layout:** Only the standard packed instruction (all `*_instruction_index == u16::MAX`) is accepted to avoid cross-instruction data injection.
- **Resolve ordering:** `resolve_bet` must immediately follow the Ed25519 verify instruction (`get_instruction_relative(-1, ...)`).
- **House trust:** The house chooses when to resolve and what signature to publish; players rely on the house signing the agreed message and the deterministic hash mapping.

## Related

Deployment runbooks (Surfpool): see [`runbooks/README.md`](runbooks/README.md).
