import * as anchor from "@anchor-lang/core";
import { Program } from "@anchor-lang/core";
import { DiceGame } from "../target/types/dice_game";
import {
  Ed25519Program,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  sendAndConfirmTransaction,
  SystemProgram,
  SYSVAR_INSTRUCTIONS_PUBKEY,
  Transaction,
} from "@solana/web3.js";
import { assert } from "chai";
import { createHash, randomBytes } from "crypto";
import { BN } from "bn.js";

const BET_ROLL = 90;
const BET_AMOUNT = BigInt(LAMPORTS_PER_SOL / 100);
const HOUSE_EDGE_BPS = 150n;

const commitment = "confirmed";

const confirmTx = async (
  connection: anchor.web3.Connection,
  signature: string,
  operationLabel: string,
) => {
  const latestBlockHash = await connection.getLatestBlockhash();

  await connection.confirmTransaction(
    {
      signature,
      ...latestBlockHash,
    },
    commitment,
  );
  console.log(`${operationLabel} signature: ${signature}`);
};
const readU128Le = (buf: Buffer, offset: number): bigint => {
  let v = 0n;
  for (let i = 0; i < 16; i++) {
    v |= BigInt(buf[offset + i]!) << BigInt(8 * i);
  }
  return v;
};

/** Same as on-chain: hash(sig) → roll in 1..100 */
const resolveRoll = (signature: Uint8Array): number => {
  const hash = createHash("sha256").update(signature).digest();
  const lower = readU128Le(hash, 0);
  const upper = readU128Le(hash, 16);
  const U128 = 1n << 128n;
  const sum = (lower + upper) % U128; // wrapping_add
  return Number(sum % 100n) + 1;
};

const resolveRollFromEntropy = (
  entropy: Uint8Array,
  betPubkey: PublicKey,
  slot: BN,
): number => {
  const preimage = Buffer.concat([
    Buffer.from(entropy),
    betPubkey.toBuffer(),
    slot.toArrayLike(Buffer, "le", 8),
  ]);
  const hash = createHash("sha256").update(preimage).digest();
  const lower = readU128Le(hash, 0);
  const upper = readU128Le(hash, 16);
  const U128 = 1n << 128n;
  const sum = (lower + upper) % U128;
  return Number(sum % 100n) + 1;
};
/** Same as on-chain payout when bet.roll > roll */
const calculatePayout = (amount: bigint, targetRoll: number): bigint => {
  const winningNumbers = BigInt(targetRoll - 1);
  return (amount * (10_000n - HOUSE_EDGE_BPS)) / winningNumbers / 100n;
};
const formatSol = (lamports: bigint): string =>
  (Number(lamports) / LAMPORTS_PER_SOL).toFixed(4);

const logBetSummary = (
  targetRoll: number,
  amount: bigint,
  signature: Uint8Array,
) => {
  const roll = resolveRoll(signature);
  const won = targetRoll > roll;
  const payout = won ? calculatePayout(amount, targetRoll) : 0n;

  console.log(
    `bet summary: target < ${targetRoll} | resolved roll: ${roll} ` +
      `${won ? "WON" : "Lost"}, payout: ${formatSol(
        payout,
      )} SOL (${payout} lamports)`,
  );
};

describe("dice_game", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const provider = anchor.getProvider();
  const connection = provider.connection;

  const program = anchor.workspace.diceGame as Program<DiceGame>;
  const house = Keypair.generate();
  const player = Keypair.generate();
  const seed = new BN(randomBytes(16));
  // pdas
  const vault = PublicKey.findProgramAddressSync(
    [Buffer.from("vault"), house.publicKey.toBuffer()],
    program.programId,
  )[0];

  const bet = PublicKey.findProgramAddressSync(
    [
      Buffer.from("bet"),
      vault.toBuffer(),
      player.publicKey.toBuffer(),
      seed.toBuffer("le", 16),
    ],
    program.programId,
  )[0];

  it("airdrop", async () => {
    await Promise.all(
      [house, player].map(async (key) => {
        return connection
          .requestAirdrop(key.publicKey, 1000 * LAMPORTS_PER_SOL)
          .then((sig) =>
            confirmTx(
              connection,
              sig,
              `aidrop ${key.publicKey.toBase58().slice(6)}`,
            ),
          );
      }),
    );
  });

  it("Is initialized!", async () => {
    await program.methods
      .initialize(new BN(100 * LAMPORTS_PER_SOL))
      .accountsStrict({
        house: house.publicKey,
        vault,
        systemProgram: SystemProgram.programId,
      })
      .signers([house])
      .rpc()
      .then((sig) => confirmTx(connection, sig, "initialization"));
  });

  it("Places a bet", async () => {
    await program.methods
      .placeBet(seed, new BN(BET_AMOUNT), BET_ROLL)
      .accountsStrict({
        player: player.publicKey,
        house: house.publicKey,
        vault,
        bet,
        systemProgram: SystemProgram.programId,
      })
      .signers([player])
      .rpc()
      .then((sig) => confirmTx(connection, sig, "placing a bet"));
  });

  it("Resolve a bet", async () => {
    // pull bet pda
    const betpda = await connection.getAccountInfo(bet, "confirmed");
    if (!betpda) throw new Error("Bet account not found");

    const sig_ix = Ed25519Program.createInstructionWithPrivateKey({
      privateKey: house.secretKey,
      message: betpda.data.subarray(8), //assuming discriminator is 8 bytes long and excluding it
    });

    // the Ed25519 instruction is packed like this for one signature:
    // [0..16):   header with offsets
    // [16..48):  public key
    // [48..112): signature
    // [122..end): signed message
    // The onchain program receives only the 64-byte signature here,
    // then reads this full Ed25519 instruction from instructions sysvar
    const ed25519Signature = Buffer.from(sig_ix.data.subarray(48, 112));

    const resolve_ix = await program.methods
      .resolveBet(ed25519Signature)
      .accountsStrict({
        player: player.publicKey,
        house: house.publicKey,
        vault,
        bet,
        instructionSysvar: SYSVAR_INSTRUCTIONS_PUBKEY,
        systemProgram: SystemProgram.programId,
      })
      .signers([house])
      .instruction();

    const tx = new Transaction().add(sig_ix).add(resolve_ix);

    try {
      await sendAndConfirmTransaction(connection, tx, [house]);
      logBetSummary(BET_ROLL, BET_AMOUNT, ed25519Signature);
    } catch (error) {
      console.log(error);
      throw error;
    }
  });
});

describe("resolve from entropy", () => {
  anchor.setProvider(anchor.AnchorProvider.env());

  const provider = anchor.getProvider();
  const connection = provider.connection;
  const program = anchor.workspace.diceGame as Program<DiceGame>;

  const house = Keypair.generate();
  const player = Keypair.generate();
  const seed = new BN(randomBytes(16));

  const vault = PublicKey.findProgramAddressSync(
    [Buffer.from("vault"), house.publicKey.toBuffer()],
    program.programId,
  )[0];

  const bet = PublicKey.findProgramAddressSync(
    [
      Buffer.from("bet"),
      vault.toBuffer(),
      player.publicKey.toBuffer(),
      seed.toBuffer("le", 16),
    ],
    program.programId,
  )[0];

  it("airdrop", async () => {
    await Promise.all(
      [house, player].map(async (key) => {
        return connection
          .requestAirdrop(key.publicKey, 1000 * LAMPORTS_PER_SOL)
          .then((sig) =>
            confirmTx(
              connection,
              sig,
              `entropy airdrop ${key.publicKey.toBase58().slice(6)}`,
            ),
          );
      }),
    );
  });

  it("initializes vault", async () => {
    await program.methods
      .initialize(new BN(100 * LAMPORTS_PER_SOL))
      .accountsStrict({
        house: house.publicKey,
        vault,
        systemProgram: SystemProgram.programId,
      })
      .signers([house])
      .rpc()
      .then((sig) => confirmTx(connection, sig, "entropy init"));
  });

  it("places a bet", async () => {
    await program.methods
      .placeBet(seed, new BN(BET_AMOUNT), BET_ROLL)
      .accountsStrict({
        player: player.publicKey,
        house: house.publicKey,
        vault,
        bet,
        systemProgram: SystemProgram.programId,
      })
      .signers([player])
      .rpc()
      .then((sig) => confirmTx(connection, sig, "entropy place bet"));
  });

  it("resolve from entropy", async () => {
    const betBefore = await program.account.bet.fetch(bet);
    const playerBalanceBefore = await connection.getBalance(
      player.publicKey,
      commitment,
    );
    // should call drand api
    const entropy = randomBytes(32);
    const expectedRoll = resolveRollFromEntropy(entropy, bet, betBefore.slot);
    const expectWin = BET_ROLL > expectedRoll;
    const expectedPayout = expectWin
      ? calculatePayout(BET_AMOUNT, BET_ROLL)
      : 0n;

    const commitIx = await program.methods
      .commitEntropy([...entropy])
      .accountsStrict({
        house: house.publicKey,
        player: player.publicKey,
        vault,
        bet,
      })
      .signers([house])
      .instruction();

    const resolveIx = await program.methods
      .resolveFromEntropy()
      .accountsStrict({
        house: house.publicKey,
        player: player.publicKey,
        vault,
        bet,
        instructionSysvar: SYSVAR_INSTRUCTIONS_PUBKEY,
        systemProgram: SystemProgram.programId,
      })
      .signers([house])
      .instruction();

    const tx = new Transaction().add(commitIx).add(resolveIx);
    await sendAndConfirmTransaction(connection, tx, [house], {
      commitment,
    });

    const betAfter = await connection.getAccountInfo(bet, commitment);
    assert.isNull(betAfter, "bet account should be closed after resolve");

    const playerBalanceAfter = await connection.getBalance(
      player.publicKey,
      commitment,
    );
    const balanceDelta = BigInt(playerBalanceAfter - playerBalanceBefore);

    console.log(
      `entropy resolve: roll=${expectedRoll} target=${BET_ROLL} ` +
        `${
          expectWin ? "WON" : "LOST"
        } expected payout ${expectedPayout} lamports`,
    );

    if (expectWin) {
      assert.isTrue(
        balanceDelta >= expectedPayout,
        `player should receive at least payout (delta=${balanceDelta}, expected>=${expectedPayout})`,
      );
    } else {
      assert.isTrue(
        balanceDelta >= 0n,
        "player should not lose principal beyond the placed bet",
      );
    }
  });
});
