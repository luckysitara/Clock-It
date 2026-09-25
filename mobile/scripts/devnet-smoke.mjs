// Devnet end-to-end smoke test for the DEPLOYED ClockLend program.
//
// Self-contained (no @solana/spl-token dep). Uses the devnet authority
// (~/.config/solana/id.json) and exercises:
//   1. version probes that PROVE which program build is deployed
//      (tag 15 acceptance = round-9 build live; Custom 25 = reinit guard)
//   2. the money path with an oracle-free native-SOL pool (no mint authority
//      needed): pool init -> wSOL deposit -> borrow -> exact repay
//   3. P2P offer create + cancel (native SOL collateral)
//   4. notes on what devnet cannot exercise (SKR mint / USDC mint authority)
//
// Usage: SOLANA_RPC_URL=https://api.devnet.solana.com node mobile/scripts/devnet-smoke.mjs

import fs from 'fs';
import {
  Connection, Keypair, PublicKey, Transaction, TransactionInstruction,
  SystemProgram, SYSVAR_RENT_PUBKEY, SYSVAR_CLOCK_PUBKEY,
} from '@solana/web3.js';

const PROGRAM_ID = new PublicKey('HAjGxuih14imCMaWvCnJQ3nSdWmS8PQKzp74gyAgjsH3');
const TOKEN_PROGRAM_ID = new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
const ASSOC_TOKEN_PROGRAM = new PublicKey('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL');
const NATIVE_MINT = new PublicKey('So11111111111111111111111111111111111111112');
const USDC_DEVNET_MINT = new PublicKey('4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU');
const RPC = process.env.SOLANA_RPC_URL || 'https://api.devnet.solana.com';
const conn = new Connection(RPC, 'confirmed');

// Explicit key only — never default to id.json, and NEVER run this script on
// a mainnet RPC (the repo .env sets a mainnet SOLANA_RPC_URL; without this
// assertion the "devnet" smoke would run the money path on mainnet).
const KEY_PATH = process.env.ORACLE_KEY || process.env.KEEPER_KEY;
if (!KEY_PATH) {
  console.error('Set ORACLE_KEY (or KEEPER_KEY) to the devnet admin keypair path.');
  process.exit(1);
}
const authority = Keypair.fromSecretKey(
  new Uint8Array(JSON.parse(fs.readFileSync(KEY_PATH, 'utf8')))
);

const w64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const w64s = (n) => { const b = Buffer.alloc(8); b.writeBigInt64LE(BigInt(n)); return b; };
const u16 = (n) => { const b = Buffer.alloc(2); b.writeUInt16LE(n); return b; };

const pda = (seeds) => PublicKey.findProgramAddressSync(seeds, PROGRAM_ID)[0];
const POOL = (auth, id) => pda([Buffer.from('pool'), auth.toBuffer(), w64(id)]);
const VAULT = (pool) => pda([Buffer.from('vault'), pool.toBuffer()]);
const LOAN = (pool, borrower, id) => pda([Buffer.from('loan'), pool.toBuffer(), borrower.toBuffer(), w64(id)]);
const ESCROW = (loan) => pda([Buffer.from('escrow'), loan.toBuffer()]);
const PROFILE = (user) => pda([Buffer.from('profile'), user.toBuffer()]);
const TREASURY = () => pda([Buffer.from('treasury')]);
const ADMIN = () => pda([Buffer.from('admin')]);
const ORACLE = (mint) => pda([Buffer.from('oracle'), mint.toBuffer()]);
const OFFER = (creator, id) => pda([Buffer.from('p2p_offer'), creator.toBuffer(), w64(id)]);
const YIELD_VAULT = (mint) => pda([Buffer.from('skr_yield_vault'), mint.toBuffer()]);
const YIELD_TOKEN = (mint) => pda([Buffer.from('skr_yield_token'), mint.toBuffer()]);

function ata(mint, owner) {
  return PublicKey.findProgramAddressSync(
    [owner.toBuffer(), TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()], ASSOC_TOKEN_PROGRAM)[0];
}

function createAtaIx(payer, mint, owner) {
  return new TransactionInstruction({
    programId: ASSOC_TOKEN_PROGRAM,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: ata(mint, owner), isSigner: false, isWritable: true },
      { pubkey: owner, isSigner: false, isWritable: false },
      { pubkey: mint, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([1]),
  });
}

async function signSend(ixs, signers, payer) {
  try {
    const bh = await conn.getLatestBlockhash('confirmed');
    const tx = new Transaction().add(...ixs);
    tx.recentBlockhash = bh.blockhash;
    tx.feePayer = payer.publicKey;
    tx.sign(...signers);
    return await conn.sendRawTransaction(tx.serialize(), { skipPreflight: true });
  } catch (e) {
    return { __sendError: e?.message || String(e) };
  }
}

async function confirm(result, label) {
  if (typeof result === 'object' && result.__sendError) {
    console.log(`  ${label}: SEND FAILED (${result.__sendError})`);
    return { ok: false };
  }
  try {
    const res = await conn.confirmTransaction(result, 'confirmed');
    const err = res.value.err;
    if (err) {
      const custom = err.InstructionError?.[1]?.Custom ?? err.InstructionError?.[1];
      console.log(`  ${label}: REVERTED (${JSON.stringify(err).slice(0, 120)})`);
      return { ok: false, custom };
    }
    console.log(`  ${label}: OK (${String(result).slice(0, 12)}…)`);
    return { ok: true };
  } catch (e) {
    console.log(`  ${label}: CONFIRM FAILED (${e?.message || e})`);
    return { ok: false };
  }
}

const run = async (label, ixs, signers, payer) => confirm(await signSend(ixs, signers, payer), label);

const DEVNET_GENESIS = 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG';
const top = async () => {
try {
{
  const genesis = await conn.getGenesisHash();
  if (genesis !== DEVNET_GENESIS) {
    console.error(`REFUSING TO RUN: RPC ${RPC} has genesis ${genesis} (expected devnet).`);
    process.exit(1);
  }
}
console.log(`RPC: ${RPC}`);
console.log(`Authority: ${authority.publicKey.toBase58()} (${(await conn.getBalance(authority.publicKey)) / 1e9} SOL)\n`);

// ---------------------------------------------------------------------------
console.log('=== 1. Version probes (which build is deployed?) ===');
const adminPda = ADMIN();
const adminAcc = await conn.getAccountInfo(adminPda);
console.log(`  Admin PDA ${adminPda.toBase58()}: ${adminAcc ? `CLK_ADMN (${adminAcc.data.length}B)` : 'MISSING'}`);

// tag 15 exists only in the round-9 build. Accept: Ok (round-9 live) or
// Custom(25) PoolAlreadyInitialized (round-9 live, already initialized).
// Custom(0) InvalidInstruction = pre-round-9 build.
const yieldVaultPda = YIELD_VAULT(USDC_DEVNET_MINT);
const yieldTokenPda = YIELD_TOKEN(USDC_DEVNET_MINT);
const initVaultIx = () => new TransactionInstruction({
  programId: PROGRAM_ID,
  keys: [
    { pubkey: authority.publicKey, isSigner: true, isWritable: true },
    { pubkey: yieldVaultPda, isSigner: false, isWritable: true },
    { pubkey: USDC_DEVNET_MINT, isSigner: false, isWritable: false },
    { pubkey: yieldTokenPda, isSigner: false, isWritable: true },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    { pubkey: SYSVAR_RENT_PUBKEY, isSigner: false, isWritable: false },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: adminPda, isSigner: false, isWritable: false },
  ],
  data: Buffer.from([15]),
});
const vaultProbe = await run('InitializeSkrYieldVault (tag 15) probe', [initVaultIx()], [authority], authority);
if (vaultProbe.ok || vaultProbe.custom === 25) {
  console.log('  => ROUND-9-OR-LATER BUILD IS LIVE (tag 15 accepted; verify the ELF hash separately)');
  if (vaultProbe.custom === 25) console.log('  => vault already initialized — C-2 reinit guard firing on-chain');
} else if (vaultProbe.custom === 0) {
  console.log('  => DEPLOYED BUILD PREDATES ROUND 9 (tag 15 unknown) — redeploy required');
  process.exitCode = 1;
  return;
} else {
  console.log('  => unexpected result; continuing');
}

// ---------------------------------------------------------------------------
console.log('\n=== 2. Money path: oracle-free native-SOL pool ===');
// Fresh global SOL feed (harmless; the oracle-free path uses baselines).
const solOracle = ORACLE(NATIVE_MINT);
const feedIx = new TransactionInstruction({
  programId: PROGRAM_ID,
  keys: [
    { pubkey: authority.publicKey, isSigner: true, isWritable: true }, // writable: first-time feed creation pays rent
    { pubkey: solOracle, isSigner: false, isWritable: true },
    { pubkey: NATIVE_MINT, isSigner: false, isWritable: false },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    { pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
    { pubkey: adminPda, isSigner: false, isWritable: false },
  ],
  data: Buffer.concat([Buffer.from([12]), w64(150_000_000), Buffer.from([9])]),
});
await run('SetPriceFeed SOL $150/9dp', [feedIx], [authority], authority);

// Pool 9025, oracle-free. NOTE: native-SOL pools can only borrow when
// is_oracle_free=true — for a non-free native pool the pool-scoped liquidity
// oracle PDA ([oracle, pool, native_mint]) is always claimed by the collateral
// oracle branch of the account scan first, so the pool-oracle slot can never
// be filled and the borrow reverts InvalidOracleAccount. (Round-9 finding;
// see build-context.)
const POOL_ID = 9025n;
const poolPda = POOL(authority.publicKey, POOL_ID);
const vaultPda = VAULT(poolPda);
const name = Buffer.alloc(32);
Buffer.from('Devnet Smoke Desk').copy(name);
const initPoolIx = new TransactionInstruction({
  programId: PROGRAM_ID,
  keys: [
    { pubkey: authority.publicKey, isSigner: true, isWritable: true },
    { pubkey: poolPda, isSigner: false, isWritable: true },
    { pubkey: NATIVE_MINT, isSigner: false, isWritable: false },
    { pubkey: vaultPda, isSigner: false, isWritable: true },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    { pubkey: SYSVAR_RENT_PUBKEY, isSigner: false, isWritable: false },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
  ],
  data: Buffer.concat([Buffer.from([0]), w64(POOL_ID), Buffer.from([1]), u16(800), u16(6500), w64s(3 * 86400), w64s(30 * 86400), name, Buffer.from([1])]), // is_oracle_free = true
});
const initPoolRes = await run('InitializePool (native SOL, id 9025, oracle-free)', [initPoolIx], [authority], authority);
if (initPoolRes.custom === 25) console.log('  (pool already exists from a prior run — continuing)');

// wSOL for the authority + deposit.
const lpWsAta = ata(NATIVE_MINT, authority.publicKey);
await run('wrap 0.1 SOL (authority wSOL ATA)', [
  createAtaIx(authority, NATIVE_MINT, authority.publicKey),
  SystemProgram.transfer({ fromPubkey: authority.publicKey, toPubkey: lpWsAta, lamports: 100_000_000 }),
  new TransactionInstruction({
    programId: TOKEN_PROGRAM_ID,
    keys: [{ pubkey: lpWsAta, isSigner: false, isWritable: true }],
    data: Buffer.from([17]), // SyncNative
  }),
], [authority], authority);

const depositIx = new TransactionInstruction({
  programId: PROGRAM_ID,
  keys: [
    { pubkey: authority.publicKey, isSigner: true, isWritable: true },
    { pubkey: poolPda, isSigner: false, isWritable: true },
    { pubkey: lpWsAta, isSigner: false, isWritable: true },
    { pubkey: vaultPda, isSigner: false, isWritable: true },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
  ],
  data: Buffer.concat([Buffer.from([1]), w64(100_000_000)]),
});
await run('DepositLiquidity 0.1 wSOL', [depositIx], [authority], authority);

const treasuryPda = TREASURY();
await run('create treasury wSOL ATA (idempotent)', [createAtaIx(authority, NATIVE_MINT, treasuryPda)], [authority], authority);
const treasuryWsAta = ata(NATIVE_MINT, treasuryPda);

// Borrower: 0.02 wSOL against 0.2 SOL collateral (oracle-free baselines).
const borrower = Keypair.generate();
const loanId = BigInt(Math.floor(Date.now() % 1_000_000_000));
const loanPda = LOAN(poolPda, borrower.publicKey, loanId);
const escrowPda = ESCROW(loanPda);
const profilePda = PROFILE(borrower.publicKey);
await run('fund borrower + wSOL ATA', [
  SystemProgram.transfer({ fromPubkey: authority.publicKey, toPubkey: borrower.publicKey, lamports: 300_000_000 }),
  createAtaIx(authority, NATIVE_MINT, borrower.publicKey),
], [authority], authority);
const borrowerWsAta = ata(NATIVE_MINT, borrower.publicKey);

const borrowIx = new TransactionInstruction({
  programId: PROGRAM_ID,
  keys: [
    { pubkey: borrower.publicKey, isSigner: true, isWritable: true },
    { pubkey: poolPda, isSigner: false, isWritable: true },
    { pubkey: loanPda, isSigner: false, isWritable: true },
    { pubkey: vaultPda, isSigner: false, isWritable: true },
    { pubkey: borrowerWsAta, isSigner: false, isWritable: true },
    { pubkey: borrower.publicKey, isSigner: false, isWritable: true },
    { pubkey: escrowPda, isSigner: false, isWritable: true },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }, // native collateral mint
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    { pubkey: profilePda, isSigner: false, isWritable: true },
    { pubkey: treasuryWsAta, isSigner: false, isWritable: true },
  ],
  data: Buffer.concat([Buffer.from([3]), w64(loanId), w64(20_000_000), w64(200_000_000), w64s(7 * 86400)]),
});
const borrowRes = await run('BorrowFromPool 0.02 wSOL vs 0.2 SOL', [borrowIx], [borrower], borrower);

if (borrowRes.ok) {
  // The borrower received only the NET disbursement (origination fee was
  // deducted), so top up the wSOL ATA before repaying the full
  // principal + interest — the exact-equality repay would otherwise fail
  // with the token program's InsufficientFunds (Custom 1).
  await run('top up borrower wSOL (0.01)', [
    SystemProgram.transfer({ fromPubkey: borrower.publicKey, toPubkey: borrowerWsAta, lamports: 10_000_000 }),
    new TransactionInstruction({
      programId: TOKEN_PROGRAM_ID,
      keys: [{ pubkey: borrowerWsAta, isSigner: false, isWritable: true }],
      data: Buffer.from([17]), // SyncNative
    }),
  ], [borrower], borrower);

  const loanAcc = await conn.getAccountInfo(loanPda);
  if (loanAcc && loanAcc.data.length >= 170 && loanAcc.data.subarray(0, 8).toString() === 'CLK_LOAN') {
    const principal = loanAcc.data.readBigUInt64LE(81);
    const interest = loanAcc.data.readBigUInt64LE(129);
    console.log(`  loan state: principal=${principal} interest=${interest} status=${loanAcc.data.readUInt8(161)}`);
    const repayIx = new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: borrower.publicKey, isSigner: true, isWritable: true },
        { pubkey: loanPda, isSigner: false, isWritable: true },
        { pubkey: borrowerWsAta, isSigner: false, isWritable: true },
        { pubkey: vaultPda, isSigner: false, isWritable: true },
        { pubkey: escrowPda, isSigner: false, isWritable: true },
        { pubkey: borrower.publicKey, isSigner: false, isWritable: true },
        { pubkey: poolPda, isSigner: false, isWritable: true },
        { pubkey: profilePda, isSigner: false, isWritable: true },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: treasuryWsAta, isSigner: false, isWritable: true },
      ],
      data: Buffer.concat([Buffer.from([6]), w64(principal + interest)]),
    });
    await run('RepayLoan (exact principal+interest)', [repayIx], [borrower], borrower);
    const after = await conn.getAccountInfo(loanPda);
    if (after) console.log(`  loan status after repay: ${after.data.readUInt8(161)} (2 = Repaid)`);
  } else {
    console.log('  WARNING: loan account unreadable after borrow');
  }
}

// ---------------------------------------------------------------------------
console.log('\n=== 3. P2P offer (native SOL collateral) ===');
const offerId = BigInt(Math.floor(Date.now() % 1_000_000_000));
const offerPda = OFFER(authority.publicKey, offerId);
const offerEscrow = ESCROW(offerPda);
const createOfferIx = new TransactionInstruction({
  programId: PROGRAM_ID,
  keys: [
    { pubkey: authority.publicKey, isSigner: true, isWritable: true },
    { pubkey: offerPda, isSigner: false, isWritable: true },
    { pubkey: authority.publicKey, isSigner: false, isWritable: true },
    { pubkey: offerEscrow, isSigner: false, isWritable: true },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    { pubkey: solOracle, isSigner: false, isWritable: false },
  ],
  data: Buffer.concat([Buffer.from([4]), w64(offerId), w64(10_000_000), w64(100_000_000), w64(500_000), w64s(7 * 86400)]),
});
const offerRes = await run('CreateP2POffer 0.1 SOL vs $10 USDC', [createOfferIx], [authority], authority);
if (offerRes.ok) {
  const cancelIx = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: authority.publicKey, isSigner: true, isWritable: true },
      { pubkey: offerPda, isSigner: false, isWritable: true },
      { pubkey: offerEscrow, isSigner: false, isWritable: true },
      { pubkey: authority.publicKey, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([10]),
  });
  await run('CancelP2POffer (collateral returned)', [cancelIx], [authority], authority);
}

console.log('\n=== 4. Notes ===');
console.log('Stake/unstake + yield deposit/claim CANNOT run on devnet: the SKR mint at the');
console.log('canonical address does not exist on devnet and USDC_DEVNET_MINT has no local mint');
console.log('authority. Those paths are covered by the deterministic bank tests (10 yield + 44 integration).');
} catch (e) {
  console.log('\n=== SMOKE ABORTED ===');
  console.log(e?.message || e);
  process.exitCode = 1;
} finally {
  console.log('\nSmoke run complete.');
}
};
top();
