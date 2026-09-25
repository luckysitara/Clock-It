// ClockLend RPC scalability benchmark.
//
// Measures the client data layer's heaviest calls against a live cluster:
//   1. the UNFILTERED getProgramAccounts scan (what the app did pre-Phase-1)
//   2. the discriminator-filtered scans the app uses now (CLK_POOL memcmp +
//      legacy 182-byte dataSize; CLK_PAWN memcmp + legacy 162-byte dataSize)
//   3. the borrower-offset scan used for a user's loans
//
// Usage: node mobile/scripts/benchmark-accounts.mjs [rpcUrl] [borrowerPubkey]
//   e.g. node mobile/scripts/benchmark-accounts.mjs https://api.devnet.solana.com
//
// Payload projections assume ~200 bytes per account (typical ClockLend
// account) and report what a single screen load would download at 10k /
// 100k / 1M accounts of that type.

import { Connection, PublicKey } from '@solana/web3.js';

const RPC = process.argv[2] || process.env.SOLANA_RPC_URL || 'https://api.devnet.solana.com';
const BORROWER = process.argv[3];
if (!BORROWER) { console.error('Usage: node benchmark-accounts.mjs [rpcUrl] [borrowerPubkey]'); process.exit(1); }
const PROGRAM_ID = new PublicKey('HAjGxuih14imCMaWvCnJQ3nSdWmS8PQKzp74gyAgjsH3');
const B58_CLK_POOL = 'CFskzA4CnMh';
const B58_CLK_PAWN = 'CFskzA486E1';

const conn = new Connection(RPC, 'confirmed');

async function timed(label, fn) {
  const t0 = Date.now();
  try {
    const value = await fn();
    const ms = Date.now() - t0;
    const size = Array.isArray(value)
      ? value.reduce((n, a) => n + (a.account?.data?.length ?? 0), 0)
      : 0;
    console.log(`  ${label}: ${value.length} accounts, ${size.toLocaleString()} bytes payload, ${ms}ms`);
    return value;
  } catch (e) {
    console.log(`  ${label}: FAILED (${e?.message || e})`);
    return [];
  }
}

console.log(`RPC: ${RPC}\nProgram: ${PROGRAM_ID.toBase58()}\n`);

console.log('1) What the app used to do (pre-Phase-1):');
const unfiltered = await timed('   unfiltered getProgramAccounts (full program scan)',
  () => conn.getProgramAccounts(PROGRAM_ID, { dataSlice: { offset: 0, length: 0 } }));

console.log('\n2) What the app does now (Phase-1 filters):');
const [poolsModern, poolsLegacy, pawnsModern, pawnsLegacy] = await Promise.all([
  timed('   CLK_POOL memcmp filter            ',
    () => conn.getProgramAccounts(PROGRAM_ID, { filters: [{ memcmp: { offset: 0, bytes: B58_CLK_POOL } }], dataSlice: { offset: 0, length: 0 } })),
  timed('   legacy 182-byte pools (dataSize)  ',
    () => conn.getProgramAccounts(PROGRAM_ID, { filters: [{ dataSize: 182 }], dataSlice: { offset: 0, length: 0 } })),
  timed('   CLK_PAWN memcmp filter            ',
    () => conn.getProgramAccounts(PROGRAM_ID, { filters: [{ memcmp: { offset: 0, bytes: B58_CLK_PAWN } }], dataSlice: { offset: 0, length: 0 } })),
  timed('   legacy 162-byte offers (dataSize)',
    () => conn.getProgramAccounts(PROGRAM_ID, { filters: [{ dataSize: 162 }], dataSlice: { offset: 0, length: 0 } })),
]);

console.log('\n3) Borrower loan scan (memcmp on LoanOrder.borrower @17):');
const loans = await timed('   loans for one borrower           ',
  () => conn.getProgramAccounts(PROGRAM_ID, {
    filters: [{ memcmp: { offset: 17, bytes: BORROWER } }],
    dataSlice: { offset: 0, length: 0 },
  }));

const totalNow = poolsModern.length + poolsLegacy.length + pawnsModern.length + pawnsLegacy.length + loans.length;
console.log('\n=== Projection ===');
console.log(`Account count on this cluster (unfiltered scan): ${unfiltered.length}`);
console.log(`Accounts fetched for one full app load (Phase-1):  ${totalNow}`);

// The unfiltered scan grows with the program's TOTAL account count (pools +
// loans + offers + profiles + yield positions); the filtered scan grows only
// with the requested type. Model a realistic mix: 1 pool per 100 total
// accounts (loans and offers dominate), 200 bytes/account.
const bytesPerAccount = 200;
const TOTAL_PER_POOL = Number(process.env.TOTAL_PER_POOL || 100);
console.log(`\nAssumed mix: ${TOTAL_PER_POOL} total accounts per pool, ${bytesPerAccount} bytes/account.`);
for (const pools of [10_000, 100_000, 1_000_000]) {
  const total = pools * TOTAL_PER_POOL;
  console.log(
    `  ${pools.toLocaleString()} pools (${total.toLocaleString()} program accounts):\n` +
    `    unfiltered pools-screen load ≈ ${((total * bytesPerAccount) / 1e6).toFixed(0)} MB\n` +
    `    filtered   pools-screen load ≈ ${((pools * bytesPerAccount) / 1e6).toFixed(0)} MB`
  );
}
console.log('\nRun with TOTAL_PER_POOL=<n> to model a different mix.');
