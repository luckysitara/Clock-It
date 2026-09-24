import '../polyfill';
import {
  Connection,
  PublicKey,
  Transaction,
  TransactionInstruction,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  SYSVAR_CLOCK_PUBKEY,
  ComputeBudgetProgram,
} from '@solana/web3.js';
import { Buffer } from 'buffer';
import * as SecureStore from 'expo-secure-store';
import {
  buildPythAttachment,
  pythFeedIdForCollateral,
  tryCanonicalSolUpdateAccount,
} from './pyth';
import {
  PROGRAM_ID,
  writeU64LE,
  getPoolPDA,
  getVaultPDA,
  getLoanPDA,
  getEscrowPDA,
  getProfilePDA,
  getP2POfferPDA,
  getSkrEscrowPDA,
  getTreasuryPDA,
  getOraclePDA,
  getPoolOraclePDA,
  getAdminPDA,
} from './program';
import {
  PoolType,
  LendingPool,
  LoanOrder,
  P2POffer,
  UserProfile,
  LoanStatus,
  OfferStatus,
  WalletAssets,
  SolanaNetwork,
  TokenAssetItem,
} from '../types';

export const DEVNET_RPCS = [
  'https://api.devnet.solana.com',
];

const GATEKEEPER_RPC = process.env.EXPO_PUBLIC_HELIUS_GATEKEEPER_RPC_URL;
const SOLANA_RPC = process.env.EXPO_PUBLIC_SOLANA_RPC_URL;
export const MAINNET_RPCS = [
  ...(GATEKEEPER_RPC ? [GATEKEEPER_RPC] : []),
  ...(SOLANA_RPC ? [SOLANA_RPC] : []),
  'https://solana-rpc.publicnode.com',
  'https://api.mainnet-beta.solana.com',
];

// Base58-encoded 8-byte account discriminators for RPC-side gPA filters.
// Filtering server-side cuts the scan payload from "every account the program
// ever created" to just the requested type — the difference between a screen
// loading in 200ms and in minutes at 1M+ accounts.
const B58_CLK_POOL = 'CFskzA4CnMh';
const B58_CLK_PAWN = 'CFskzA486E1';

// Small TTL cache for protocol reads: screen loads within a few seconds of
// each other (tab switches, re-renders) share one RPC result. Post-mutation
// refreshes pass { force: true } and bypass it.
const FETCH_CACHE_TTL_MS = 10_000;
const fetchCache = new Map<string, { at: number; value: unknown; inflight?: Promise<unknown> }>();
export function cachedFetch<T>(
  key: string,
  fn: () => Promise<T>,
  opts?: { force?: boolean }
): Promise<T> {
  const entry = fetchCache.get(key);
  if (!opts?.force && entry?.inflight) return entry.inflight as Promise<T>;
  if (!opts?.force && entry && Date.now() - entry.at < FETCH_CACHE_TTL_MS) {
    return Promise.resolve(entry.value as T);
  }
  const p = fn().then((v) => {
    fetchCache.set(key, { at: Date.now(), value: v });
    return v;
  });
  fetchCache.set(key, { at: entry?.at ?? 0, value: entry?.value, inflight: p });
  return p;
}

export const devnetConnection = new Connection(DEVNET_RPCS[0], 'confirmed');
export const mainnetConnection = new Connection(MAINNET_RPCS[0], 'confirmed');

export function getConnection(network: SolanaNetwork = 'mainnet-beta'): Connection {
  return network === 'mainnet-beta' ? mainnetConnection : devnetConnection;
}

// Default export connection for backwards compatibility
export const connection = mainnetConnection;

export { PROGRAM_ID };

/**
 * Attach a Pyth price update to a transaction (borrow + create-offer paths).
 *
 * Order is load-bearing: web3.js partialSign throws on a transaction without
 * a recentBlockhash, and the ephemeral signature must cover the FINAL message,
 * so the blockhash/feePayer are set first and the receiver instruction is
 * unshifted BEFORE the signer collects its signature. If signing fails for any
 * reason, the instruction is removed again so no phantom postUpdateAtomic
 * (whose ephemeral signer nobody else can sign) survives in the transaction.
 *
 * On any failure the caller falls back: SOL collateral references Pyth's
 * canonical cranked account (re-verified on-chain by the program); SKR has no
 * canonical account, so it falls back to the admin feed's freshness window.
 */
async function attachPythOrFallback(
  tx: Transaction,
  connection: Connection,
  feePayer: PublicKey,
  collateralName: string
): Promise<{ pythAccount?: PublicKey; pythCu: number }> {
  try {
    const att = await buildPythAttachment(connection, feePayer, pythFeedIdForCollateral(collateralName));
    if (!tx.recentBlockhash) {
      const { blockhash } = await connection.getLatestBlockhash('confirmed');
      tx.recentBlockhash = blockhash;
    }
    if (!tx.feePayer) tx.feePayer = feePayer;
    tx.instructions.unshift(...att.instructions);
    try {
      if (att.signers.length > 0) tx.partialSign(...att.signers);
    } catch (signErr) {
      tx.instructions.splice(0, att.instructions.length);
      throw signErr;
    }
    return { pythAccount: att.priceUpdateAccount, pythCu: att.computeUnits };
  } catch (err) {
    console.warn('[Pyth] attach skipped, fallback:', (err as any)?.message || err);
    if (collateralName.toUpperCase().includes('SOL')) {
      const canonical = await tryCanonicalSolUpdateAccount(connection);
      if (canonical) {
        console.warn('[Pyth] using canonical SOL update account');
        return { pythAccount: canonical, pythCu: 0 };
      }
    }
    return { pythCu: 0 };
  }
}

export const TOKEN_PROGRAM_ID = new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

// Seeker Genesis Token mint (Soulbound Token-2022). Single source of truth:
// the SGT badge is granted by EXACT mint match only — never by substring or
// frozen-account heuristics.
export const SGT_MINT = '4Zao8ocPhmMgq7PdsYWyxvqySMGx7xb9cMftPMkEokRG';
export const TOKEN_2022_PROGRAM_ID = new PublicKey('TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb');
export const MEMO_PROGRAM_ID = new PublicKey('MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr');
export const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL');

// M-04: Legacy devnet USDC mint (kept for parsing legacy devnet accounts)
export const USDC_DEVNET_MINT = new PublicKey('4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU');
export const USDC_MAINNET_MINT = new PublicKey('EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v');
export const SKR_MINT = new PublicKey('SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3');
export const NATIVE_SOL_MINT = new PublicKey('So11111111111111111111111111111111111111112');

export const SKR_YIELD_VAULT_SEED = Buffer.from('skr_yield_vault');
export const SKR_YIELD_TOKEN_SEED = Buffer.from('skr_yield_token');
export const USER_YIELD_SEED = Buffer.from('skr_yield_user');

// M-03: Integer Interest Calculation Helper matching Smart Contract exactly
export function calculateExactInterestDue(
  borrowAmountMicro: bigint,
  rateBps: number,
  durationSeconds: number,
  hasSkrDiscount: boolean = false
): bigint {
  const effectiveBps = hasSkrDiscount ? Math.floor((rateBps * 50) / 100) : rateBps;
  const numerator = borrowAmountMicro * BigInt(effectiveBps) * BigInt(durationSeconds);
  const denominator = BigInt(10000) * BigInt(31536000);
  return numerator / denominator;
}

export function getAssociatedTokenAddress(mint: PublicKey, owner: PublicKey): PublicKey {
  const [address] = PublicKey.findProgramAddressSync(
    [owner.toBuffer(), TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    ASSOCIATED_TOKEN_PROGRAM_ID
  );
  return address;
}

/** Associated Token Program createIdempotent (tag 1) — no spl-token dep. */
export function createAssociatedTokenAccountIdempotentInstruction(
  payer: PublicKey,
  ata: PublicKey,
  owner: PublicKey,
  mint: PublicKey
): TransactionInstruction {
  return new TransactionInstruction({
    programId: ASSOCIATED_TOKEN_PROGRAM_ID,
    keys: [
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: ata, isSigner: false, isWritable: true },
      { pubkey: owner, isSigner: false, isWritable: false },
      { pubkey: mint, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([1]),
  });
}

// Seeded pool addresses for instantaneous retrieval (populated after mainnet desk creation)
const SEEDED_POOLS: PublicKey[] = [];

// Helper to query with automatic fallback to secondary RPC endpoints
export async function queryRpcWithFallback<T>(
  network: SolanaNetwork,
  queryFn: (conn: Connection) => Promise<T>
): Promise<T> {
  const rpcList = network === 'mainnet-beta' ? MAINNET_RPCS : DEVNET_RPCS;
  let lastError: any = null;

  for (const rpc of rpcList) {
    try {
      const conn = new Connection(rpc, 'confirmed');
      return await queryFn(conn);
    } catch (err: any) {
      lastError = err;
      console.warn(`[RPC Fallback] ${rpc} notice:`, err?.message || err);
    }
  }

  throw lastError || new Error(`All RPC endpoints failed for ${network}`);
}

// Local hybrid cache for persistent loan orders (strictly namespaced by network)
export async function getCachedOrders(
  borrowerPubkey: string,
  network: SolanaNetwork = 'mainnet-beta'
): Promise<LoanOrder[]> {
  try {
    const raw = await SecureStore.getItemAsync(`clocklend_orders_${network}_${borrowerPubkey}`);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) return parsed;
    }
  } catch (err) {
    console.warn('Error reading cached orders:', err);
  }
  return [];
}

export async function setCachedOrders(
  borrowerPubkey: string,
  orders: LoanOrder[],
  network: SolanaNetwork = 'mainnet-beta'
): Promise<void> {
  try {
    await SecureStore.setItemAsync(`clocklend_orders_${network}_${borrowerPubkey}`, JSON.stringify(orders));
  } catch (err) {
    console.warn('Error saving cached orders:', err);
  }
}

// Helper to decode null-terminated on-chain string from 32-byte array
function decodeName(bytes: Uint8Array): string {
  let end = bytes.indexOf(0);
  if (end === -1) end = bytes.length;
  const decoded = new TextDecoder().decode(bytes.slice(0, end)).trim();
  return decoded.length > 0 ? decoded : 'Community Pool';
}

function parsePoolData(pubkey: string, data: Buffer, id: number): LendingPool | null {
  if (data.length !== 200 && data.length !== 182) return null;
  const isV2 = data.length === 200;
  // NEW-4: discriminator-gated parsing — never parse a non-pool account as a pool
  if (isV2 && data.subarray(0, 8).toString() !== 'CLK_POOL') return null;
  const offset = isV2 ? 8 : 0;
  const isInitialized = data.readUInt8(offset) === 1;
  if (!isInitialized) return null;

  const poolId = isV2 ? Number(data.readBigUInt64LE(9)) : id;
  const poolTypeByte = data.readUInt8(isV2 ? 17 : 1);
  const poolType: PoolType = poolTypeByte === 2 ? 'Institutional' : poolTypeByte === 1 ? 'Circle' : 'Individual';
  const authority = new PublicKey(data.subarray(isV2 ? 18 : 2, isV2 ? 50 : 34)).toBase58();
  const liquidityMint = new PublicKey(data.subarray(isV2 ? 50 : 34, isV2 ? 82 : 66)).toBase58();
  const totalLiquidity = Number(data.readBigUInt64LE(isV2 ? 114 : 98));
  const totalBorrowed = Number(data.readBigUInt64LE(isV2 ? 122 : 106));
  const stakedSkrAmount = Number(data.readBigUInt64LE(isV2 ? 130 : 114));
  const interestRateBps = data.readUInt16LE(isV2 ? 138 : 122);
  const maxLtvBps = data.readUInt16LE(isV2 ? 140 : 124);
  const minDurationDays = Math.round(Number(data.readBigInt64LE(isV2 ? 142 : 126)) / 86400);
  const maxDurationDays = Math.round(Number(data.readBigInt64LE(isV2 ? 150 : 134)) / 86400);
  const loansOriginated = data.readUInt32LE(isV2 ? 158 : 142);
  const loansRepaid = data.readUInt32LE(isV2 ? 162 : 146);
  const name = decodeName(data.subarray(isV2 ? 166 : 150, isV2 ? 198 : 182));

  // null (rendered as '—') until the desk actually has loan history.
  const successRate: number | null = loansOriginated > 0 ? (loansRepaid / loansOriginated) * 100 : null;

  return {
    id: poolId,
    poolType,
    authority,
    name,
    liquidityMint,
    totalLiquidity: totalLiquidity / 1_000_000,
    totalBorrowed: totalBorrowed / 1_000_000,
    stakedSkrAmount,
    interestRateBps,
    maxLtvBps,
    minDurationDays: minDurationDays || 1,
    maxDurationDays: maxDurationDays || 30,
    loansOriginated,
    loansRepaid,
    successRate: successRate === null ? null : parseFloat(successRate.toFixed(1)),
    isVerifiedMerchant: stakedSkrAmount > 0,
  };
}

// Fetch all live Lending Pools from the deployed contract
export async function fetchLivePools(
  network: SolanaNetwork = 'mainnet-beta',
  opts?: { force?: boolean }
): Promise<LendingPool[]> {
  return cachedFetch(`pools:${network}`, () => fetchLivePoolsUncached(network), opts);
}

async function fetchLivePoolsUncached(network: SolanaNetwork): Promise<LendingPool[]> {
  const poolsMap = new Map<string, LendingPool>();
  const rpcConn = getConnection(network);

  // 1. Fast path: load known seeded pools via getMultipleAccountsInfo (~300ms)
  try {
    const accounts = await queryRpcWithFallback(network, (c) => c.getMultipleAccountsInfo(SEEDED_POOLS));
    accounts.forEach((acc, idx) => {
      if (acc && acc.data) {
        const pool = parsePoolData(SEEDED_POOLS[idx].toBase58(), Buffer.from(acc.data), poolsMap.size + 1);
        if (pool) poolsMap.set(SEEDED_POOLS[idx].toBase58(), pool);
      }
    });
  } catch (err) {
    console.warn('Fast pool query notice:', err);
  }

  // 2. Filtered scan for additional pools: RPC-side discriminator filter for
  // modern 200-byte pools + a legacy 182-byte dataSize query. This fetches
  // only pool accounts instead of the program's entire account history.
  try {
    const accounts = await queryRpcWithFallback(network, async (c) => {
      const [modern, legacy] = await Promise.all([
        c.getProgramAccounts(PROGRAM_ID, { filters: [{ memcmp: { offset: 0, bytes: B58_CLK_POOL } }] }),
        // web3.js 1.99's config type lacks the dataSize filter variant even
        // though the RPC supports it — cast the (runtime-correct) result.
        (c.getProgramAccounts(PROGRAM_ID, { filters: [{ dataSize: 182 }] } as any) as unknown) as Promise<
          Awaited<ReturnType<Connection['getProgramAccounts']>>
        >,
      ]);
      return [...modern, ...legacy];
    });
    for (const acc of accounts) {
      if (acc.account.data.length === 200 || acc.account.data.length === 182) {
        const pubkeyStr = acc.pubkey.toBase58();
        if (!poolsMap.has(pubkeyStr)) {
          const pool = parsePoolData(pubkeyStr, Buffer.from(acc.account.data), poolsMap.size + 1);
          if (pool) poolsMap.set(pubkeyStr, pool);
        }
      }
    }
  } catch (err) {
    console.warn('Full pool scan notice:', err);
  }

  return Array.from(poolsMap.values());
}

// Fetch live user loan orders directly from the contract & on-chain state
export async function fetchLiveUserOrders(borrower: PublicKey, network: SolanaNetwork = 'mainnet-beta'): Promise<LoanOrder[]> {
  const rpcConn = getConnection(network);
  const borrowerPubkey = borrower.toBase58();
  const cachedOrders = await getCachedOrders(borrowerPubkey, network);
  const cachedBySig = new Map<string, LoanOrder>();
  const cachedById = new Map<number, LoanOrder>();
  for (const co of cachedOrders) {
    if (co.txSignature) cachedBySig.set(co.txSignature, co);
    cachedById.set(co.id, co);
  }

  const ordersMap = new Map<number, LoanOrder>();

  // 1. Scan on-chain PDA accounts first (for liquid pool PDA loans)
  try {
    const accounts = await queryRpcWithFallback(network, (c) => c.getProgramAccounts(PROGRAM_ID));
    for (const acc of accounts) {
      if (acc.account.data.length === 170 || acc.account.data.length === 154) {
        const data = Buffer.from(acc.account.data);
        const isV2 = data.length === 170;
        // NEW-4: discriminator-gated parsing — never parse a non-loan account as a loan
        if (isV2 && data.subarray(0, 8).toString() !== 'CLK_LOAN') continue;
        const isActive = data.readUInt8(isV2 ? 8 : 0) === 1;
        if (!isActive) continue;

        const borrowerOnChain = new PublicKey(data.subarray(isV2 ? 17 : 9, isV2 ? 49 : 41));
        if (!borrowerOnChain.equals(borrower)) continue;

        const loanId = Number(data.readBigUInt64LE(isV2 ? 9 : 1));
        const poolPubkey = new PublicKey(data.subarray(isV2 ? 49 : 41, isV2 ? 81 : 73));
        const principalAmount = Number(data.readBigUInt64LE(isV2 ? 81 : 73)) / 1_000_000;
        const collateralMint = new PublicKey(data.subarray(isV2 ? 89 : 81, isV2 ? 121 : 113)).toBase58();
        const isSkr = collateralMint === SKR_MINT.toBase58();
        const rawCollateral = Number(data.readBigUInt64LE(isV2 ? 121 : 113));
        const collateralAmount = isSkr ? rawCollateral / 1_000_000 : rawCollateral / 1_000_000_000;
        const collateralName = isSkr ? `${collateralAmount.toLocaleString()} SKR` : `${collateralAmount.toFixed(2)} SOL`;
        const interestDue = Number(data.readBigUInt64LE(isV2 ? 129 : 121)) / 1_000_000;
        const originationTime = Number(data.readBigInt64LE(isV2 ? 137 : 129));
        const dueTime = Number(data.readBigInt64LE(isV2 ? 145 : 137));
        const gracePeriodExpires = Number(data.readBigInt64LE(isV2 ? 153 : 145));
        const statusByte = data.readUInt8(isV2 ? 161 : 153);

        let status: LoanStatus = 'Active';
        if (statusByte === 1) status = 'InGracePeriod';
        else if (statusByte === 2) status = 'Repaid';
        else if (statusByte === 3) status = 'Defaulted';

        if (status === 'Repaid') continue;

        const id = loanId;
        ordersMap.set(id, {
          id,
          poolId: 1,
          poolName: 'Seeker Genesis Circle',
          poolPubkey: poolPubkey.toBase58(),
          borrower: borrowerPubkey,
          principalAmount,
          collateralName,
          collateralMint,
          collateralAmount,
          interestDue,
          originationTime,
          dueTime: dueTime > 0 ? dueTime : Math.floor(Date.now() / 1000) + 86400 * 7,
          gracePeriodExpires,
          status,
        });
      }
    }
  } catch (err) {
    console.warn('Program account query notice:', err);
  }

  const finalizeOrders = async (): Promise<LoanOrder[]> => {
    // 3. Fallback: if blockchain was temporarily unreachable or slow, keep active cached orders
    if (ordersMap.size === 0 && cachedOrders.length > 0) {
      for (const co of cachedOrders) {
        if (co.status === 'Active' || co.status === 'InGracePeriod') {
          ordersMap.set(co.id, co);
        }
      }
    }
    const finalOrders = Array.from(ordersMap.values());
    await setCachedOrders(borrowerPubkey, finalOrders, network);
    return finalOrders;
  };

  // 2. Scan blockchain transactions & memos for ground-truth borrow/repay history.
  // Every on-chain loan is a PDA account (found by the filtered scan above), so
  // this heavy signature walk only runs as a LAST-RESORT fallback when the PDA
  // scan found nothing — and with a bounded window. It no longer duplicates the
  // PDA results (memo candidates must resolve to a real loan PDA anyway).
  if (ordersMap.size > 0) {
    return finalizeOrders();
  }
  try {
    const signatures = await queryRpcWithFallback(network, (c) => c.getSignaturesForAddress(borrower, { limit: 20 }));
    const memos = signatures
      .filter((s) => Boolean(s.memo))
      .map((s) => ({
        time: s.blockTime || Math.floor(Date.now() / 1000),
        memo: s.memo!.replace(/^\[\d+\]\s*/, '').trim(),
        sig: s.signature,
      }));

    // Sort chronologically from oldest to newest to pair repayments with borrows accurately
    memos.sort((a, b) => a.time - b.time);

    interface OpenBorrowCandidate {
      id: number;
      principal: number;
      collateral: string;
      poolId: number;
      time: number;
      sig: string;
    }
    const openCandidates: OpenBorrowCandidate[] = [];

    for (const m of memos) {
      // Check for borrow memo with explicit Order/Loan ID
      const borrowWithId = m.memo.match(
        /ClockLend:\s*Borrow\s*#?(\d+)\s*\$?([\d.]+)\s*USDC.*?Collateral:\s*([^\s|]+(?:\s+[^\s|]+)?)\s*(?:locked)?.*?Pool\s*#(\d+)/i
      );
      // Check for borrow memo without explicit ID (legacy format)
      const borrowLegacy = !borrowWithId
        ? m.memo.match(
            /ClockLend:\s*Borrow\s*\$?([\d.]+)\s*USDC.*?Collateral:\s*([^\s|]+(?:\s+[^\s|]+)?)\s*(?:locked)?.*?Pool\s*#(\d+)/i
          )
        : null;

      if (borrowWithId || borrowLegacy) {
        let orderId = 0;
        let principal = 0;
        let collateral = '';
        let poolId = 1;

        if (borrowWithId) {
          orderId = parseInt(borrowWithId[1]);
          principal = parseFloat(borrowWithId[2]);
          collateral = borrowWithId[3].trim();
          poolId = parseInt(borrowWithId[4]);
        } else if (borrowLegacy) {
          principal = parseFloat(borrowLegacy[1]);
          collateral = borrowLegacy[2].trim();
          poolId = parseInt(borrowLegacy[3]);

          // Check if already in cache for this signature. A legacy memo with no
          // cached id cannot be resolved to a loan PDA — skip it rather than
          // invent an identifier (the PDA derivation would target the wrong loan).
          const cached = cachedBySig.get(m.sig);
          if (!cached) continue;
          orderId = cached.id;
        }

        openCandidates.push({
          id: orderId,
          principal,
          collateral,
          poolId,
          time: m.time,
          sig: m.sig,
        });
        continue;
      }

      // Check for repay memo
      const repayMatch = m.memo.match(/ClockLend:\s*Repay.*?Order\s*#?(\d+)\s*Closed/i);
      if (repayMatch) {
        const closedId = parseInt(repayMatch[1]);
        const exactIdx = openCandidates.findIndex((b) => b.id === closedId);
        if (exactIdx !== -1) {
          openCandidates.splice(exactIdx, 1);
        } else {
          // Pair with the most recent open candidate prior to this repay
          const priorCandidates = openCandidates.filter((b) => b.time <= m.time);
          if (priorCandidates.length > 0) {
            const lastCandidate = priorCandidates[priorCandidates.length - 1];
            const idx = openCandidates.indexOf(lastCandidate);
            if (idx !== -1) openCandidates.splice(idx, 1);
          }
        }
      }
    }

    // Convert open candidates into LoanOrder records — from the CHAIN, never
    // from fabricated identifiers. A memo-derived candidate is only shown when
    // its pool resolves to a real on-chain desk AND its loan PDA exists with
    // the expected borrower; everything else is skipped rather than invented.
    const nowSec = Math.floor(Date.now() / 1000);
    const livePools = await fetchLivePools(network);
    const poolsById = new Map(livePools.map((p) => [p.id, p]));
    for (const cand of openCandidates) {
      // If locally marked repaid, skip
      const cached = cachedById.get(cand.id) || cachedBySig.get(cand.sig);
      if (cached && cached.status === 'Repaid') continue;

      const pool = poolsById.get(cand.poolId);
      if (!pool) {
        console.warn(`[Orders] memo candidate pool #${cand.poolId} has no on-chain desk — skipped`);
        continue;
      }
      const [poolPDA] = getPoolPDA(new PublicKey(pool.authority), pool.id);
      const [loanPDA] = getLoanPDA(poolPDA, borrower, cand.id);
      const [escrowPDA] = getEscrowPDA(loanPDA);

      // The loan PDA is the single source of truth for amounts, terms and status.
      const info = await rpcConn.getAccountInfo(loanPDA);
      const data = info?.data ? Buffer.from(info.data) : undefined;
      if (!data || data.length < 170 || data.subarray(0, 8).toString() !== 'CLK_LOAN') {
        console.warn(`[Orders] memo candidate #${cand.id} has no on-chain loan PDA — skipped`);
        continue;
      }
      if (data.readUInt8(8) !== 1) continue; // not active
      const onChainBorrower = new PublicKey(data.subarray(17, 49));
      if (!onChainBorrower.equals(borrower)) continue;

      // LoanOrder layout (170-byte): discriminator[0..8], is_active[8],
      // loan_id[9..17], borrower[17..49], pool[49..81], principal[81..89],
      // collateral_mint[89..121], collateral_amount[121..129],
      // interest_due[129..137], origination[137..145], due[145..153],
      // grace_expires[153..161], status[161], locked_skr[162..170].
      const principalMicro = Number(data.readBigUInt64LE(81));
      const interestMicro = Number(data.readBigUInt64LE(129));
      const dueTime = Number(data.readBigInt64LE(145));
      const gracePeriodExpires = Number(data.readBigInt64LE(153));
      const statusByte = data.readUInt8(161);
      const collateralMint = new PublicKey(data.subarray(89, 121)).toBase58();
      const collateralAmountRaw = Number(data.readBigUInt64LE(121));

      const isSol = collateralMint === NATIVE_SOL_MINT.toBase58() ||
        collateralMint === SystemProgram.programId.toBase58();
      const collUnits = isSol ? collateralAmountRaw / 1_000_000_000 : collateralAmountRaw / 1_000_000;
      let status: LoanStatus = 'Active';
      if (statusByte === 1) status = 'InGracePeriod';
      else if (statusByte === 2) status = 'Repaid';
      else if (statusByte === 3) status = 'Defaulted';

      ordersMap.set(cand.id, {
        id: cand.id,
        poolId: pool.id,
        poolName: pool.name,
        borrower: borrowerPubkey,
        principalAmount: principalMicro / 1_000_000,
        collateralName: `${collUnits > 0 ? collUnits.toFixed(2) : '1.0'} ${isSol ? 'SOL' : 'SKR'}`,
        collateralMint,
        collateralAmount: collUnits > 0 ? collUnits : 1,
        interestDue: interestMicro / 1_000_000,
        originationTime: cand.time,
        dueTime,
        gracePeriodExpires,
        status,
        txSignature: cand.sig,
        escrowAddress: escrowPDA.toBase58(),
        solscanUrl: `https://solscan.io/tx/${cand.sig}`,
      });
    }
  } catch (sigErr) {
    console.warn('Signature scan notice:', sigErr);
  }

  return finalizeOrders();
}

// Fetch live P2P pawn offers directly from Devnet contract
export async function fetchLiveP2POffers(
  network: SolanaNetwork = 'mainnet-beta',
  opts?: { force?: boolean }
): Promise<P2POffer[]> {
  return cachedFetch(`offers:${network}`, () => fetchLiveP2POffersUncached(network), opts);
}

async function fetchLiveP2POffersUncached(network: SolanaNetwork): Promise<P2POffer[]> {
  const rpcConn = getConnection(network);
  try {
    // RPC-side discriminator filter (modern CLK_PAWN) + legacy 162-byte
    // dataSize query — only offer accounts cross the wire.
    const accounts = await queryRpcWithFallback(network, async (c) => {
      const [modern, legacy] = await Promise.all([
        c.getProgramAccounts(PROGRAM_ID, { filters: [{ memcmp: { offset: 0, bytes: B58_CLK_PAWN } }] }),
        // same cast as the pool scan: the RPC supports dataSize, the TS type
        // in this web3.js version does not.
        (c.getProgramAccounts(PROGRAM_ID, { filters: [{ dataSize: 162 }] } as any) as unknown) as Promise<
          Awaited<ReturnType<Connection['getProgramAccounts']>>
        >,
      ]);
      return [...modern, ...legacy];
    });
    const offers: P2POffer[] = [];

    for (const acc of accounts) {
      if (acc.account.data.length >= 162) {
        const data = Buffer.from(acc.account.data);
        let isInitialized = false;
        let offerId = 0;
        let creator = '';
        let funder = '';
        let collateralMint = '';
        let liquidityMint = USDC_MAINNET_MINT.toBase58();
        let collateralLamports = 0;
        let requestedRaw: string | undefined;
        let interestRaw: string | undefined;
        let durationSeconds = 0;
        let createdAt = 0;
        let dueTime = 0;
        let statusByte = 0;

        // NEW-4: discriminator-gated parsing — only CLK_PAWN accounts are offers
        const kind = data.subarray(0, 8).toString();
        if (data.length >= 200 && kind === 'CLK_PAWN') {
          isInitialized = data.readUInt8(8) === 1;
          offerId = Number(data.readBigUInt64LE(9));
          creator = new PublicKey(data.subarray(17, 49)).toBase58();
          funder = new PublicKey(data.subarray(49, 81)).toBase58();
          collateralMint = new PublicKey(data.subarray(81, 113)).toBase58();
          liquidityMint = new PublicKey(data.subarray(113, 145)).toBase58();
          collateralLamports = Number(data.readBigUInt64LE(145));
          requestedRaw = data.readBigUInt64LE(153).toString();
          interestRaw = data.readBigUInt64LE(161).toString();
          durationSeconds = Number(data.readBigInt64LE(169));
          createdAt = Number(data.readBigInt64LE(177));
          dueTime = Number(data.readBigInt64LE(185));
          statusByte = data.readUInt8(data.length >= 202 ? 201 : data.length - 1);
        } else if (data.length >= 168 && kind === 'CLK_PAWN') {
          isInitialized = data.readUInt8(8) === 1;
          offerId = Number(data.readBigUInt64LE(9));
          creator = new PublicKey(data.subarray(17, 49)).toBase58();
          funder = new PublicKey(data.subarray(49, 81)).toBase58();
          collateralMint = new PublicKey(data.subarray(81, 113)).toBase58();
          collateralLamports = Number(data.readBigUInt64LE(113));
          requestedRaw = data.readBigUInt64LE(121).toString();
          interestRaw = data.readBigUInt64LE(129).toString();
          durationSeconds = Number(data.readBigInt64LE(137));
          createdAt = Number(data.readBigInt64LE(145));
          dueTime = Number(data.readBigInt64LE(153));
          statusByte = data.readUInt8(data.length === 170 ? 169 : 167);
        } else if (data.length === 162) {
          isInitialized = data.readUInt8(0) === 1;
          offerId = Number(data.readBigUInt64LE(1));
          creator = new PublicKey(data.subarray(9, 41)).toBase58();
          funder = new PublicKey(data.subarray(41, 73)).toBase58();
          collateralMint = new PublicKey(data.subarray(73, 105)).toBase58();
          collateralLamports = Number(data.readBigUInt64LE(105));
          requestedRaw = data.readBigUInt64LE(113).toString();
          interestRaw = data.readBigUInt64LE(121).toString();
          durationSeconds = Number(data.readBigInt64LE(129));
          createdAt = Number(data.readBigInt64LE(137));
          dueTime = Number(data.readBigInt64LE(145));
          statusByte = data.readUInt8(161);
        } else {
          continue;
        }

        if (!isInitialized) continue;

        const requestedLamports = requestedRaw ? Number(BigInt(requestedRaw)) : 0;
        const interestLamports = interestRaw ? Number(BigInt(interestRaw)) : 0;
        const requestedAmount = requestedLamports / 1_000_000;
        const interestOffered = interestLamports / 1_000_000;
        const durationDays = Math.max(1, Math.round(durationSeconds / 86400));
        const isSkr = collateralMint === SKR_MINT.toBase58();
        const collateralDecimals = isSkr ? 1_000_000 : 1_000_000_000;
        const collateralAmount = collateralLamports / collateralDecimals;
        const collateralName = isSkr
          ? `${collateralAmount.toLocaleString()} SKR`
          : `${collateralAmount > 0 ? collateralAmount.toFixed(2) : '1.0'} SOL`;

        let status: OfferStatus = 'Open';
        if (statusByte === 1) status = 'Funded';
        else if (statusByte === 2) status = 'InGracePeriod';
        else if (statusByte === 3) status = 'Repaid';
        else if (statusByte === 4) status = 'Defaulted';

        const [escrowPDA] = getEscrowPDA(acc.pubkey);

        offers.push({
          id: offerId,
          creator,
          funder: funder === PublicKey.default.toBase58() ? undefined : funder,
          collateralName,
          collateralType: 'Token',
          collateralAmount: collateralAmount > 0 ? collateralAmount : 1,
          collateralMint,
          liquidityMint,
          requestedAmount,
          interestOffered,
          requestedAmountRaw: requestedRaw,
          interestOfferedRaw: interestRaw,
          durationDays: durationDays || 7,
          createdAt,
          dueTime: dueTime > 0 ? dueTime : undefined,
          status,
          escrowAddress: escrowPDA.toBase58(),
        });
      }
    }

    return offers;
  } catch (err) {
    console.warn('Error querying live offers:', err);
    return [];
  }
}

// Fetch live UserProfile PDA from Devnet contract
export async function fetchLiveUserProfile(userPubkey: PublicKey, skrHandle: string, network: SolanaNetwork = 'mainnet-beta'): Promise<UserProfile> {
  const rpcConn = getConnection(network);
  try {
    const [profilePDA] = getProfilePDA(userPubkey);
    const accountInfo = await queryRpcWithFallback(network, (c) => c.getAccountInfo(profilePDA));

    if (accountInfo && (accountInfo.data.length === 67 || accountInfo.data.length >= 51)) {
      const data = Buffer.from(accountInfo.data);
      const isV2 = data.length === 67;
      const isInitialized = data.readUInt8(isV2 ? 8 : 0) === 1;

      if (isInitialized) {
        const stakedSkr = Number(data.readBigUInt64LE(isV2 ? 41 : 33)) / 1_000_000;
        const totalLoansCompleted = data.readUInt32LE(isV2 ? 49 : 41);
        const totalLoansDefaulted = data.readUInt32LE(isV2 ? 53 : 45);
        const reputationScore = data.readUInt16LE(isV2 ? 57 : 49);

        let tier: 'Diamond' | 'Gold' | 'Silver' | 'Standard' = 'Standard';
        let aprDiscount = 0;

        if (stakedSkr >= 5000 || reputationScore >= 9000) {
          tier = 'Diamond';
          aprDiscount = 50;
        } else if (stakedSkr >= 2000 || reputationScore >= 7500) {
          tier = 'Gold';
          aprDiscount = 25;
        } else if (stakedSkr >= 500 || reputationScore >= 5000) {
          tier = 'Silver';
          aprDiscount = 10;
        }

        return {
          pubkey: userPubkey.toBase58(),
          stakedSkr,
          totalLoansCompleted,
          totalLoansDefaulted,
          reputationScore,
          tier,
          aprDiscount,
        };
      }
    }
  } catch (err) {
    console.warn('Profile PDA not yet initialized, returning default profile:', err);
  }

  return {
    pubkey: userPubkey.toBase58(),
    stakedSkr: 0,
    totalLoansCompleted: 0,
    totalLoansDefaulted: 0,
    reputationScore: 0,
    tier: 'Standard',
    aprDiscount: 0,
  };
}

// Live crypto market price cache with automatic 2-minute updates
export let livePrices = {
  sol: 101.12,
  skr: 0.0192,
  usdc: 1.0,
};
let lastPriceFetchTime = 0;

export async function fetchLivePrices(): Promise<{ sol: number; skr: number; usdc: number }> {
  const now = Date.now();
  if (now - lastPriceFetchTime < 120_000) {
    return livePrices;
  }
  // Jupiter price API first (configured via EXPO_PUBLIC_JUPITER_API_URL),
  // CoinGecko as the fallback source.
  const JUPITER_API_URL = process.env.EXPO_PUBLIC_JUPITER_API_URL;
  const JUPITER_API_KEY = process.env.EXPO_PUBLIC_JUPITER_API_KEY;
  const mintIds = [
    'So11111111111111111111111111111111111111112', // SOL
    'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v', // USDC
    'SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3', // SKR
  ].join(',');
  // Jupiter Price API — proven shape from the reimagine stack is /price/v3
  // (top-level token-id keys with usdPrice). v2 and bare shapes are kept as
  // fallback candidates; CoinGecko is the last resort.
  const vsToken = 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v';
  const base = (JUPITER_API_URL || 'https://api.jup.ag').replace(/\/+$/, '');
  const candidates = [
    `${base}/price/v3?ids=${mintIds}`,
    `${base}/price/v2?ids=${mintIds}&vsToken=${vsToken}`,
    `${base}?ids=${mintIds}&vsToken=${vsToken}`,
  ];
  try {
    let jupRes: Response | null = null;
    for (const url of candidates) {
      const r = await fetch(url, {
        headers: JUPITER_API_KEY ? { 'x-api-key': JUPITER_API_KEY } : undefined,
      });
      if (r.ok) { jupRes = r; break; }
    }
    if (jupRes && jupRes.ok) {
      const data = await jupRes.json();
      const byId: Record<string, number> = {};
      if (data?.data && typeof data.data === 'object') {
        // v2 shape: { data: { [id]: { price } } }
        for (const [k, v] of Object.entries(data.data)) {
          byId[k] = Number((v as any)?.price);
        }
      } else {
        // v3 shape: { [id]: { usdPrice } }
        for (const [k, v] of Object.entries(data)) {
          byId[k] = Number((v as any)?.usdPrice ?? (v as any)?.price);
        }
      }
      if (byId['So11111111111111111111111111111111111111112']) {
        livePrices.sol = byId['So11111111111111111111111111111111111111112'];
      }
      if (byId['EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v']) {
        livePrices.usdc = byId['EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v'];
      }
      if (byId['SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3']) {
        livePrices.skr = byId['SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3'];
      }
      lastPriceFetchTime = now;
      return livePrices;
    }
  } catch (err) {
    // fall through to CoinGecko
  }
  try {
    const res = await fetch(
      'https://api.coingecko.com/api/v3/simple/price?ids=solana,seeker,usd-coin&vs_currencies=usd'
    );
    if (res.ok) {
      const data = await res.json();
      if (data?.solana?.usd) livePrices.sol = Number(data.solana.usd);
      if (data?.seeker?.usd) livePrices.skr = Number(data.seeker.usd);
      if (data?.['usd-coin']?.usd) livePrices.usdc = Number(data['usd-coin'].usd);
      lastPriceFetchTime = now;
    }
  } catch (err) {
    // Graceful fallback to confirmed market baseline
  }
  return livePrices;
}

function getKnownTokenSymbol(mint: string): string {
  if (mint === 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v' || mint === '4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU') {
    return 'USDC';
  }
  if (mint === 'DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263') {
    return 'BONK';
  }
  if (mint === SGT_MINT) {
    return 'SGT';
  }
  if (
    mint === 'SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3' ||
    mint.toLowerCase().includes('skr') ||
    mint === 'G55PoQUF8yqZeZrrQi8bdmBWtzbo9NPgAx27v1zz2daM'
  ) {
    return 'SKR';
  }
  return mint.slice(0, 4) + '..' + mint.slice(-4);
}

function getKnownTokenName(mint: string): string {
  if (mint === 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v' || mint === '4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU') {
    return 'USD Coin';
  }
  if (mint === 'DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263') {
    return 'Bonk';
  }
  if (mint === SGT_MINT) {
    return 'Seeker Genesis Token';
  }
  if (
    mint === 'SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3' ||
    mint.toLowerCase().includes('skr') ||
    mint === 'G55PoQUF8yqZeZrrQi8bdmBWtzbo9NPgAx27v1zz2daM'
  ) {
    return 'Seeker Token';
  }
  return 'Solana Token';
}

function calculateTokenUsd(mint: string, amount: number, prices: { sol: number; skr: number; usdc: number } = livePrices): number {
  if (mint === 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v' || mint === '4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU') {
    return parseFloat((amount * prices.usdc).toFixed(2));
  }
  if (
    mint === 'SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3' ||
    mint.toLowerCase().includes('skr') ||
    mint === 'G55PoQUF8yqZeZrrQi8bdmBWtzbo9NPgAx27v1zz2daM'
  ) {
    return parseFloat((amount * prices.skr).toFixed(2));
  }
  if (mint === 'DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263') {
    return parseFloat((amount * 0.00002).toFixed(2));
  }
  return 0;
}

// Query live user balances across SOL and all SPL token holdings on specified network with multi-RPC fallback
export async function fetchLiveWalletAssets(
  userPubkey: PublicKey,
  network: SolanaNetwork = 'mainnet-beta'
): Promise<WalletAssets> {
  // 1. Query native SOL balance with RPC fallback
  let solLamports = 0;
  try {
    solLamports = await queryRpcWithFallback(network, async (conn) => {
      return await conn.getBalance(userPubkey, 'confirmed');
    });
  } catch (err) {
    console.warn(`[WalletAssets] Error querying SOL balance on ${network}:`, err);
  }
  const solBalance = solLamports / 1_000_000_000;

  // 2. Query SPL tokens from both standard Token Program and Token-2022
  let usdcBalance = 0;
  let skrBalance = 0;
  let bonkBalance = 0;
  let hasSeekerGenesisToken = false;
  const tokenList: TokenAssetItem[] = [];
  const prices = await fetchLivePrices();

  try {
    const allAccounts = await queryRpcWithFallback(network, async (conn) => {
      const [standardResult, token2022Result] = await Promise.allSettled([
        conn.getParsedTokenAccountsByOwner(userPubkey, { programId: TOKEN_PROGRAM_ID }),
        conn.getParsedTokenAccountsByOwner(userPubkey, { programId: TOKEN_2022_PROGRAM_ID }),
      ]);

      const list: any[] = [];
      if (standardResult.status === 'fulfilled' && standardResult.value?.value) {
        list.push(...standardResult.value.value.map((a) => ({ ...a, isToken2022: false })));
      }
      if (token2022Result.status === 'fulfilled' && token2022Result.value?.value) {
        list.push(...token2022Result.value.value.map((a) => ({ ...a, isToken2022: true })));
      }
      return list;
    });

    for (const item of allAccounts) {
      const info = item.account?.data?.parsed?.info;
      if (!info) continue;

      const mint: string = info.mint || '';
      const amount: number = info.tokenAmount?.uiAmount || 0;
      const decimals: number = info.tokenAmount?.decimals || 0;
      const state: string = info.state || '';

      // Detect the Seeker Genesis Token by EXACT mint match only. Substring
      // heuristics ('seeker'/'sgt') can match unrelated mints, and a frozen
      // Token-2022 account is not proof of SGT ownership — never overclaim
      // the trust signal.
      if (mint === SGT_MINT) {
        hasSeekerGenesisToken = true;
      }

      // Check for USDC
      if (
        mint === 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v' ||
        mint === '4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU'
      ) {
        usdcBalance += amount;
      }
      // Check for SKR
      else if (
        mint === 'SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3' ||
        mint.toLowerCase().includes('skr') ||
        mint === 'G55PoQUF8yqZeZrrQi8bdmBWtzbo9NPgAx27v1zz2daM'
      ) {
        skrBalance += amount;
      }
      // Check for BONK
      else if (
        mint === 'DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263' ||
        mint.toLowerCase().includes('bonk')
      ) {
        bonkBalance += amount;
      }

      if (amount > 0 || state === 'frozen') {
        tokenList.push({
          mint,
          name: getKnownTokenName(mint),
          symbol: getKnownTokenSymbol(mint),
          amount,
          decimals,
          usdValue: calculateTokenUsd(mint, amount, prices),
          isToken2022: item.isToken2022,
        });
      }
    }
  } catch (err) {
    console.warn(`[WalletAssets] Error querying token accounts on ${network}:`, err);
  }

  const solUsd = solBalance * prices.sol;
  const usdcUsd = usdcBalance * prices.usdc;
  const skrUsd = skrBalance * prices.skr;
  const bonkUsd = bonkBalance * 0.00002;
  const totalUsdValue = parseFloat((solUsd + usdcUsd + skrUsd + bonkUsd).toFixed(2));

  return {
    network,
    solBalance,
    usdcBalance,
    skrBalance,
    bonkBalance,
    hasSeekerGenesisToken,
    totalUsdValue,
    tokenList,
  };
}

// Build Borrow Transaction instruction
export async function buildBorrowTx(
  borrower: PublicKey,
  poolAuthority: PublicKey,
  poolId: number,
  borrowAmountUsdc: number,
  collateralAmountLamports: number,
  durationDays: number,
  collateralName: string = 'SOL',
  isPoolLiquid: boolean = true,
  liquidityMint: PublicKey = USDC_MAINNET_MINT
): Promise<{ tx: Transaction; escrowPDA: PublicKey; loanId: number }> {
  const [poolPDA] = getPoolPDA(poolAuthority, poolId);
  const [vaultPDA] = getVaultPDA(poolPDA);
  const loanId = Math.floor(1000 + Math.random() * 900000);
  const [loanPDA] = getLoanPDA(poolPDA, borrower, loanId);
  const [escrowPDA] = getEscrowPDA(loanPDA);
  const [profilePDA] = getProfilePDA(borrower);

  const tx = new Transaction();

  // Layout: 1 byte tag (3) + 8 bytes loan_id + 8 bytes borrow_amount + 8 bytes collateral_amount + 8 bytes duration_seconds = 33 bytes
  const data = Buffer.alloc(33);
  data.writeUInt8(3, 0); // Instruction 3: BorrowFromPool
  writeU64LE(BigInt(loanId)).copy(data, 1);
  writeU64LE(BigInt(Math.round(borrowAmountUsdc * 1_000_000))).copy(data, 9);
  writeU64LE(BigInt(collateralAmountLamports)).copy(data, 17);
  writeU64LE(BigInt(durationDays * 86400)).copy(data, 25);

  const borrowerUsdcAccount = getAssociatedTokenAddress(liquidityMint, borrower);
  const isNativeSol = collateralName.toUpperCase() === 'SOL';
  const collateralMint = isNativeSol ? SystemProgram.programId : SKR_MINT;
  const borrowerCollateralAccount = isNativeSol
    ? borrower
    : getAssociatedTokenAddress(collateralMint, borrower);

  const [treasuryPDA] = getTreasuryPDA();
  const treasuryUsdcAccount = getAssociatedTokenAddress(liquidityMint, treasuryPDA);
  const oracleMint = isNativeSol ? NATIVE_SOL_MINT : collateralMint;
  const [oraclePDA] = getOraclePDA(oracleMint);

  // Pyth pull-oracle (best effort): when the Hermes fetch succeeds, prepend the
  // receiver postUpdate instructions and pass the verified price account. On any
  // failure the program falls back to the admin price feed (or Pyth's canonical
  // cranked SOL account), so borrowing still works while that feed is fresh.
  const pyth = await attachPythOrFallback(
    tx,
    getConnection('mainnet-beta'),
    borrower,
    collateralName
  );
  const pythCu = pyth.pythCu;
  const pythAccount = pyth.pythAccount;

  // H-3: size the tx-wide compute budget for the receiver's postUpdateAtomic
  // (~170k CU) plus the program instructions; budget must precede the body.
  tx.instructions.unshift(
    ComputeBudgetProgram.setComputeUnitLimit({ units: 100_000 + pythCu }),
    ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 })
  );

  const keys: any[] = [
    { pubkey: borrower, isSigner: true, isWritable: true },
    { pubkey: poolPDA, isSigner: false, isWritable: true },
    { pubkey: loanPDA, isSigner: false, isWritable: true },
    { pubkey: vaultPDA, isSigner: false, isWritable: true },
    { pubkey: borrowerUsdcAccount, isSigner: false, isWritable: true },
    { pubkey: borrowerCollateralAccount, isSigner: false, isWritable: true },
    { pubkey: escrowPDA, isSigner: false, isWritable: true },
    { pubkey: collateralMint, isSigner: false, isWritable: false },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    { pubkey: profilePDA, isSigner: false, isWritable: true },
    { pubkey: treasuryUsdcAccount, isSigner: false, isWritable: true },
    { pubkey: oraclePDA, isSigner: false, isWritable: false },
  ];
  if (pythAccount) {
    keys.push({ pubkey: pythAccount, isSigner: false, isWritable: false });
  }
  // Route 50% of the origination fee to the SKR yield vault when it exists
  // and is initialized. If the read fails or the vault is absent, the whole
  // fee goes to the treasury (program behavior) — never revert the borrow
  // over a missing vault.
  try {
    const yieldVault = await fetchSkrYieldVault('mainnet-beta', liquidityMint);
    if (yieldVault?.initialized) {
      keys.push({ pubkey: yieldVault.vaultPDA, isSigner: false, isWritable: true });
      keys.push({ pubkey: yieldVault.vaultTokenPDA, isSigner: false, isWritable: true });
    }
  } catch (_e) {
    // graceful: no fee split
  }

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys,
    data,
  });
  tx.add(ix);

  const collateralLabel = isNativeSol
    ? `${(collateralAmountLamports / 1e9).toFixed(3)} SOL`
    : `${collateralAmountLamports} SKR`;
  const memoText = `ClockLend: Borrow #${loanId} $${borrowAmountUsdc} USDC | Collateral: ${collateralLabel} | Pool #${poolId}`;
  tx.add(
    new TransactionInstruction({
      programId: MEMO_PROGRAM_ID,
      keys: [{ pubkey: borrower, isSigner: true, isWritable: false }],
      data: Buffer.from(memoText, 'utf-8'),
    })
  );

  return { tx, escrowPDA, loanId };
}

// Build Initialize Admin Transaction instruction (ClockLend Instruction 13)
export async function buildInitializeAdminTx(
  admin: PublicKey
): Promise<Transaction> {
  const [adminPDA] = getAdminPDA();
  const tx = new Transaction();
  const data = Buffer.alloc(1);
  data.writeUInt8(13, 0); // Instruction 13: InitializeAdmin

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: admin, isSigner: true, isWritable: true },
      { pubkey: adminPDA, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data,
  });
  tx.add(ix);
  return tx;
}

// Build Set Price Feed Transaction instruction (ClockLend Instruction 12)
export async function buildSetPriceFeedTx(
  authority: PublicKey,
  mint: PublicKey,
  priceMicroUsd: number | bigint,
  decimals: number,
  poolPDA?: PublicKey
): Promise<Transaction> {
  const [oraclePDA] = poolPDA ? getPoolOraclePDA(poolPDA, mint) : getOraclePDA(mint);
  const [adminPDA] = getAdminPDA();
  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 80_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // Layout: 1 byte tag (12) + 8 bytes price_micro_usd + 1 byte decimals = 10 bytes
  const data = Buffer.alloc(10);
  data.writeUInt8(12, 0); // Instruction 12: SetPriceFeed
  writeU64LE(BigInt(priceMicroUsd)).copy(data, 1);
  data.writeUInt8(decimals, 9);

  const keys = [
    { pubkey: authority, isSigner: true, isWritable: true },
    { pubkey: oraclePDA, isSigner: false, isWritable: true },
    { pubkey: mint, isSigner: false, isWritable: false },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    { pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
  ];
  if (poolPDA) {
    keys.push({ pubkey: poolPDA, isSigner: false, isWritable: false });
  } else {
    keys.push({ pubkey: adminPDA, isSigner: false, isWritable: false });
  }

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys,
    data,
  });
  tx.add(ix);
  return tx;
}

// Build Repay Transaction instruction
export async function buildRepayTx(
  borrower: PublicKey,
  poolAuthority: PublicKey,
  poolId: number,
  orderId: number,
  repayAmountUsdc: number,
  isPoolLiquid: boolean = true,
  collateralName: string = 'SOL',
  poolPubkeyOverride?: PublicKey,
  liquidityMint: PublicKey = USDC_MAINNET_MINT
): Promise<Transaction> {
  // NEW-2: bind repayment to the loan's actual pool pubkey (read from the loan
  // PDA) instead of re-deriving the pool PDA from a menu-driven (authority, id).
  const [poolPDA] = poolPubkeyOverride
    ? [poolPubkeyOverride]
    : getPoolPDA(poolAuthority, poolId);
  const [vaultPDA] = getVaultPDA(poolPDA);
  const [loanPDA] = getLoanPDA(poolPDA, borrower, orderId);
  const [escrowPDA] = getEscrowPDA(loanPDA);
  const [profilePDA] = getProfilePDA(borrower);
  const borrowerUsdcAccount = getAssociatedTokenAddress(liquidityMint, borrower);
  const [treasuryPDA] = getTreasuryPDA();
  const treasuryUsdcAccount = getAssociatedTokenAddress(liquidityMint, treasuryPDA);

  const isNativeSol = collateralName.toUpperCase().includes('SOL');
  const borrowerCollateralAccount = isNativeSol
    ? borrower
    : getAssociatedTokenAddress(SKR_MINT, borrower);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 80_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // Query on-chain loan state to guarantee exact repayment down to the micro-unit
  let exactRepayLamports = BigInt(Math.round(repayAmountUsdc * 1_000_000));
  try {
    const loanInfo = await getConnection('mainnet-beta').getAccountInfo(loanPDA);
    const loanData = loanInfo?.data;
    // NEW-4: discriminator-gated read of the loan PDA for the exact amount
    if (loanData && loanData.length === 170 && loanData.subarray(0, 8).toString() === 'CLK_LOAN') {
      const principal = loanData.readBigUInt64LE(81);
      const interest = loanData.readBigUInt64LE(129);
      exactRepayLamports = BigInt(principal.toString()) + BigInt(interest.toString());
    } else if (loanData && loanData.length === 154) {
      const principal = loanData.readBigUInt64LE(73);
      const interest = loanData.readBigUInt64LE(121);
      exactRepayLamports = BigInt(principal.toString()) + BigInt(interest.toString());
    }
  } catch (err) {
    console.warn('[Repay] exact-amount read failed, using caller amount:', (err as any)?.message || err);
  }

  // Layout: 1 byte tag (6) + 8 bytes repay_amount = 9 bytes
  const data = Buffer.alloc(9);
  data.writeUInt8(6, 0); // Instruction 6: RepayLoan
  writeU64LE(exactRepayLamports).copy(data, 1);

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: borrower, isSigner: true, isWritable: true },
      { pubkey: loanPDA, isSigner: false, isWritable: true },
      { pubkey: borrowerUsdcAccount, isSigner: false, isWritable: true },
      { pubkey: vaultPDA, isSigner: false, isWritable: true },
      { pubkey: escrowPDA, isSigner: false, isWritable: true },
      { pubkey: borrowerCollateralAccount, isSigner: false, isWritable: true },
      { pubkey: poolPDA, isSigner: false, isWritable: true },
      { pubkey: profilePDA, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: treasuryUsdcAccount, isSigner: false, isWritable: true },
    ],
    data,
  });
  tx.add(ix);

  const memoText = `ClockLend: Repay $${repayAmountUsdc} USDC | Order #${orderId} Closed | Collateral Released`;
  tx.add(
    new TransactionInstruction({
      programId: MEMO_PROGRAM_ID,
      keys: [{ pubkey: borrower, isSigner: true, isWritable: false }],
      data: Buffer.from(memoText, 'utf-8'),
    })
  );

  return tx;
}

// Build Withdraw Liquidity Transaction instruction (Pool Authority only)
export async function buildWithdrawLiquidityTx(
  authority: PublicKey,
  poolId: number,
  amountUsdc: number,
  authorityTokenAccount: PublicKey
): Promise<Transaction> {
  const [poolPDA] = getPoolPDA(authority, poolId);
  const [vaultPDA] = getVaultPDA(poolPDA);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 60_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // Layout: 1 byte tag (9) + 8 bytes amount = 9 bytes
  const data = Buffer.alloc(9);
  data.writeUInt8(9, 0); // Instruction 9: WithdrawLiquidity
  writeU64LE(BigInt(Math.round(amountUsdc * 1_000_000))).copy(data, 1);

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: authority, isSigner: true, isWritable: true },
      { pubkey: poolPDA, isSigner: false, isWritable: true },
      { pubkey: vaultPDA, isSigner: false, isWritable: true },
      { pubkey: authorityTokenAccount, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
  tx.add(ix);

  return tx;
}


// Build Create P2P Pawn Offer Transaction
export async function buildCreateP2POfferTx(
  creator: PublicKey,
  offerId: number,
  assetName: string,
  requestedAmountUsdc: number,
  profitAmountUsdc: number,
  durationDays: number
): Promise<{ tx: Transaction; offerPDA: PublicKey; escrowPDA: PublicKey }> {
  const [offerPDA] = getP2POfferPDA(creator, offerId);
  const [escrowPDA] = getEscrowPDA(offerPDA);

  const tx = new Transaction();

  // Check if asset specifies collateral amount (SOL or SKR)
  // Collateral spec: an EXPLICIT "amount SYMBOL" with an allowlisted symbol.
  // Free-text must never silently map to a different asset — "2 JitoSOL" used
  // to escrow 1.0 native SOL, and a bare "SKR" used to escrow 1000 SKR.
  const solMatch = assetName.match(/^([0-9]*\.?[0-9]+)\s*(SOL)\b/i);
  const skrMatch = assetName.match(/^([0-9]*\.?[0-9]+)\s*(SKR)\b/i);

  let isNativeSol: boolean;
  let collateralMint: PublicKey;
  let collateralAmount: number;
  if (solMatch && solMatch[1]) {
    isNativeSol = true;
    collateralMint = SystemProgram.programId;
    collateralAmount = Math.round(parseFloat(solMatch[1]) * 1_000_000_000);
  } else if (skrMatch && skrMatch[1]) {
    isNativeSol = false;
    collateralMint = SKR_MINT;
    collateralAmount = Math.round(parseFloat(skrMatch[1]) * 1_000_000);
  } else {
    throw new Error(
      `Unsupported collateral: "${assetName}". Only explicit amounts of SOL or SKR are accepted (e.g. "1.5 SOL", "500 SKR").`
    );
  }
  if (collateralAmount <= 0) {
    throw new Error('Collateral amount must be positive');
  }

  const creatorCollateralAccount = isNativeSol
    ? creator
    : getAssociatedTokenAddress(collateralMint, creator);

  const oracleMint = isNativeSol ? NATIVE_SOL_MINT : collateralMint;
  const [oraclePDA] = getOraclePDA(oracleMint);

  // Pyth pull-oracle (best effort) with the admin feed as the program fallback.
  const pyth = await attachPythOrFallback(
    tx,
    getConnection('mainnet-beta'),
    creator,
    isNativeSol ? 'SOL' : 'SKR'
  );
  const pythCu = pyth.pythCu;
  const pythAccount = pyth.pythAccount;

  // H-3: size the tx-wide compute budget for the receiver's postUpdateAtomic
  // (~170k CU) plus the program instructions; budget must precede the body.
  tx.instructions.unshift(
    ComputeBudgetProgram.setComputeUnitLimit({ units: 100_000 + pythCu }),
    ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 })
  );

  // ClockLendInstruction::CreateP2POffer (Variant 4):
  // 1 byte tag (4) + 8 bytes offer_id + 8 bytes requested_amount + 8 bytes collateral_amount + 8 bytes interest_offered + 8 bytes duration_seconds = 41 bytes
  const data = Buffer.alloc(41);
  data.writeUInt8(4, 0);
  writeU64LE(BigInt(offerId)).copy(data, 1);
  writeU64LE(BigInt(Math.round(requestedAmountUsdc * 1_000_000))).copy(data, 9);
  writeU64LE(BigInt(collateralAmount)).copy(data, 17);
  writeU64LE(BigInt(Math.round(profitAmountUsdc * 1_000_000))).copy(data, 25);
  writeU64LE(BigInt(durationDays * 86400)).copy(data, 33);

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: creator, isSigner: true, isWritable: true },
      { pubkey: offerPDA, isSigner: false, isWritable: true },
      { pubkey: creatorCollateralAccount, isSigner: false, isWritable: true },
      { pubkey: escrowPDA, isSigner: false, isWritable: true },
      { pubkey: collateralMint, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: oraclePDA, isSigner: false, isWritable: false },
      { pubkey: USDC_MAINNET_MINT, isSigner: false, isWritable: false },
      ...(pythAccount ? [{ pubkey: pythAccount, isSigner: false, isWritable: false }] : []),
    ],
    data,
  });
  tx.add(ix);

  const memoText = `ClockLend: Create P2P Pawn Offer #${offerId} | Asset: ${assetName} | Request: $${requestedAmountUsdc} USDC | Profit: $${profitAmountUsdc} | Duration: ${durationDays}d | Escrow: ${escrowPDA.toBase58().slice(0, 8)}...`;
  tx.add(
    new TransactionInstruction({
      programId: MEMO_PROGRAM_ID,
      keys: [{ pubkey: creator, isSigner: true, isWritable: false }],
      data: Buffer.from(memoText, 'utf-8'),
    })
  );

  return { tx, offerPDA, escrowPDA };
}

// Build Fund P2P Pawn Offer Transaction
export async function buildFundP2POfferTx(
  funder: PublicKey,
  offer: P2POffer
): Promise<Transaction> {
  const creator = new PublicKey(offer.creator);
  const [offerPDA] = getP2POfferPDA(creator, offer.id);
  const offerMint = offer.liquidityMint ? new PublicKey(offer.liquidityMint) : USDC_MAINNET_MINT;
  const funderUsdcAccount = getAssociatedTokenAddress(offerMint, funder);
  const creatorUsdcAccount = getAssociatedTokenAddress(offerMint, creator);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 80_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // ClockLendInstruction::FundP2POffer (Variant 5)
  const data = Buffer.alloc(1);
  data.writeUInt8(5, 0);

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: funder, isSigner: true, isWritable: true },
      { pubkey: offerPDA, isSigner: false, isWritable: true },
      { pubkey: funderUsdcAccount, isSigner: false, isWritable: true },
      { pubkey: creatorUsdcAccount, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
  tx.add(ix);

  const memoText = `ClockLend: Fund P2P Pawn #${offer.id} | Principal: $${offer.requestedAmount} USDC | Expected Yield: +$${offer.interestOffered} USDC | Funder: ${funder.toBase58().slice(0, 8)}...`;
  tx.add(
    new TransactionInstruction({
      programId: MEMO_PROGRAM_ID,
      keys: [{ pubkey: funder, isSigner: true, isWritable: false }],
      data: Buffer.from(memoText, 'utf-8'),
    })
  );

  return tx;
}

// Build Repay P2P Pawn Offer Transaction (Borrower repays principal + yield to release collateral)
export async function buildRepayPawnOfferTx(
  borrower: PublicKey,
  offer: P2POffer
): Promise<Transaction> {
  if (!offer.funder) {
    throw new Error('Offer has not been funded yet');
  }
  const [offerPDA] = getP2POfferPDA(borrower, offer.id);
  const [escrowPDA] = getEscrowPDA(offerPDA);
  const funder = new PublicKey(offer.funder);
  const offerMint = offer.liquidityMint ? new PublicKey(offer.liquidityMint) : USDC_MAINNET_MINT;
  const borrowerUsdcAccount = getAssociatedTokenAddress(offerMint, borrower);
  const funderUsdcAccount = getAssociatedTokenAddress(offerMint, funder);

  const isNativeSol = offer.collateralName.toUpperCase().includes('SOL');
  const borrowerCollateralAccount = isNativeSol
    ? borrower
    : getAssociatedTokenAddress(SKR_MINT, borrower);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 80_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // H-4: exact repay down to the base unit. Prefer the raw account-derived
  // strings (never round through Number), then the on-chain read, and only as
  // a last resort the caller-supplied float.
  let exactRepayLamports =
    offer.requestedAmountRaw && offer.interestOfferedRaw
      ? BigInt(offer.requestedAmountRaw) + BigInt(offer.interestOfferedRaw)
      : BigInt(Math.round((offer.requestedAmount + offer.interestOffered) * 1_000_000));
  try {
    const offerInfo = await getConnection('mainnet-beta').getAccountInfo(offerPDA);
    const data = offerInfo?.data;
    // NEW-1: read the CURRENT (202-byte) offer layout — requested_amount @153,
    // interest_offered @161 — with the discriminator gate. The legacy 170-byte
    // layout (121/129) is only used for pre-v3 offers.
    if (data && data.subarray(0, 8).toString() === 'CLK_PAWN') {
      if (data.length >= 200) {
        const requested = data.readBigUInt64LE(153);
        const interest = data.readBigUInt64LE(161);
        exactRepayLamports = BigInt(requested.toString()) + BigInt(interest.toString());
      } else if (data.length >= 168) {
        const requested = data.readBigUInt64LE(121);
        const interest = data.readBigUInt64LE(129);
        exactRepayLamports = BigInt(requested.toString()) + BigInt(interest.toString());
      }
    }
  } catch (err) {
    console.warn('[Repay] exact-amount read failed, using caller amount:', (err as any)?.message || err);
  }

  // ClockLendInstruction::RepayLoan (Variant 6)
  const data = Buffer.alloc(9);
  data.writeUInt8(6, 0);
  writeU64LE(exactRepayLamports).copy(data, 1);

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: borrower, isSigner: true, isWritable: true },
      { pubkey: offerPDA, isSigner: false, isWritable: true },
      { pubkey: borrowerUsdcAccount, isSigner: false, isWritable: true },
      { pubkey: funderUsdcAccount, isSigner: false, isWritable: true },
      { pubkey: escrowPDA, isSigner: false, isWritable: true },
      { pubkey: borrowerCollateralAccount, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data,
  });
  tx.add(ix);

  const memoText = `ClockLend: Repay P2P Pawn #${offer.id} | Repaid: $${(Number(exactRepayLamports) / 1_000_000).toFixed(2)} USDC | Collateral ${offer.collateralName} Released from Escrow`;
  tx.add(
    new TransactionInstruction({
      programId: MEMO_PROGRAM_ID,
      keys: [{ pubkey: borrower, isSigner: true, isWritable: false }],
      data: Buffer.from(memoText, 'utf-8'),
    })
  );

  return tx;
}

// Build Cancel P2P Pawn Offer Transaction (Creator withdraws collateral before anyone funds)
export async function buildCancelPawnOfferTx(
  creator: PublicKey,
  offer: P2POffer
): Promise<Transaction> {
  const [offerPDA] = getP2POfferPDA(creator, offer.id);
  const [escrowPDA] = getEscrowPDA(offerPDA);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 80_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // Instruction 10: CancelP2POffer (1 byte tag)
  const data = Buffer.alloc(1);
  data.writeUInt8(10, 0);

  const isNativeSol = offer.collateralName.toUpperCase().includes('SOL');
  const creatorCollateralAccount = isNativeSol ? creator : getAssociatedTokenAddress(SKR_MINT, creator);

  const keys = [
    { pubkey: creator, isSigner: true, isWritable: true },
    { pubkey: offerPDA, isSigner: false, isWritable: true },
    { pubkey: escrowPDA, isSigner: false, isWritable: true },
    { pubkey: creatorCollateralAccount, isSigner: false, isWritable: true },
  ];

  if (!isNativeSol) {
    keys.push({ pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false });
  }
  keys.push({ pubkey: SystemProgram.programId, isSigner: false, isWritable: false });

  tx.add(
    new TransactionInstruction({
      programId: PROGRAM_ID,
      keys,
      data,
    })
  );

  const memoText = `ClockLend: Cancel P2P Pawn #${offer.id} | Collateral ${offer.collateralName} Withdrawn & Rent Refunded`;
  tx.add(
    new TransactionInstruction({
      programId: MEMO_PROGRAM_ID,
      keys: [{ pubkey: creator, isSigner: true, isWritable: false }],
      data: Buffer.from(memoText, 'utf-8'),
    })
  );

  return tx;
}

// Build Create Lending Desk / Pool (Individual or Circle) Transaction
export async function buildCreatePoolTx(
  authority: PublicKey,
  poolId: number,
  poolType: 'Individual' | 'Circle',
  name: string,
  interestRateBps: number,
  maxLtvBps: number,
  minDurationDays: number,
  maxDurationDays: number
): Promise<{ tx: Transaction; poolPDA: PublicKey; vaultPDA: PublicKey }> {
  const [poolPDA] = getPoolPDA(authority, poolId);
  const [vaultPDA] = getVaultPDA(poolPDA);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 100_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // ClockLendInstruction::InitializePool:
  // Variant tag: 0 (1 byte)
  // pool_id: u64 (8 bytes)
  // pool_type: u8 (1 byte: 0 = Individual, 1 = Circle)
  // interest_rate_bps: u16 (2 bytes)
  // max_ltv_bps: u16 (2 bytes)
  // min_duration: i64 (8 bytes)
  // max_duration: i64 (8 bytes)
  // name: [u8; 32]
  // is_oracle_free: bool (1 byte)
  //
  // Explicit, not inferred from `name`. Keep false so the pool requires a live
  // price feed; true prices collateral from hardcoded baselines and should only
  // be used for a deliberate oracle-free (devnet / test) pool.
  const isOracleFree = false;
  const data = Buffer.alloc(1 + 8 + 1 + 2 + 2 + 8 + 8 + 32 + 1);
  let offset = 0;
  data.writeUInt8(0, offset); offset += 1;
  writeU64LE(BigInt(poolId)).copy(data, offset); offset += 8;
  data.writeUInt8(poolType === 'Circle' ? 1 : 0, offset); offset += 1;
  data.writeUInt16LE(interestRateBps, offset); offset += 2;
  data.writeUInt16LE(maxLtvBps, offset); offset += 2;
  writeU64LE(BigInt(minDurationDays * 86400)).copy(data, offset); offset += 8;
  writeU64LE(BigInt(maxDurationDays * 86400)).copy(data, offset); offset += 8;

  const nameBuf = Buffer.alloc(32);
  Buffer.from(name.slice(0, 32), 'utf-8').copy(nameBuf);
  nameBuf.copy(data, offset); offset += 32;
  data.writeUInt8(isOracleFree ? 1 : 0, offset);

  const liquidityMint = USDC_MAINNET_MINT;

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: authority, isSigner: true, isWritable: true },
      { pubkey: poolPDA, isSigner: false, isWritable: true },
      { pubkey: liquidityMint, isSigner: false, isWritable: false },
      { pubkey: vaultPDA, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: SYSVAR_RENT_PUBKEY, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });

  tx.add(ix);

  const memoText = `ClockLend: Create Lending Desk #${poolId} "${name}" | Type: ${poolType} | APR: ${(interestRateBps / 100).toFixed(1)}% | Max LTV: ${(maxLtvBps / 100).toFixed(0)}%`;
  tx.add(
    new TransactionInstruction({
      programId: MEMO_PROGRAM_ID,
      keys: [{ pubkey: authority, isSigner: true, isWritable: false }],
      data: Buffer.from(memoText, 'utf-8'),
    })
  );

  return { tx, poolPDA, vaultPDA };
}

// Build Stake SKR Reputation Bond Transaction
export async function buildStakeSkrTx(
  user: PublicKey,
  amountSkr: number
): Promise<{ tx: Transaction; profilePDA: PublicKey; escrowPDA: PublicKey }> {
  const [profilePDA] = getProfilePDA(user);
  const [escrowPDA] = getSkrEscrowPDA(user);
  const userSkrAccount = getAssociatedTokenAddress(SKR_MINT, user);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 100_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // ClockLendInstruction::StakeSKR (Variant 2):
  // 1 byte tag (2) + 8 bytes amount = 9 bytes
  const data = Buffer.alloc(9);
  data.writeUInt8(2, 0); // Instruction 2: StakeSKR
  writeU64LE(BigInt(Math.round(amountSkr * 1_000_000))).copy(data, 1);

  // No client-side SOL transfer: the program funds both the profile and the
  // skr_escrow PDAs itself (create_or_allocate_pda from the signer's wallet).
  // An extra transfer here would be a permanent, unrecoverable donation.

  // Execute on-chain StakeSKR instruction
  tx.add(
    new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: user, isSigner: true, isWritable: true },
        { pubkey: profilePDA, isSigner: false, isWritable: true },
        { pubkey: userSkrAccount, isSigner: false, isWritable: true },
        { pubkey: escrowPDA, isSigner: false, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        { pubkey: SKR_MINT, isSigner: false, isWritable: false },
      ],
      data,
    })
  );

  // Memo instruction for on-chain proof & Solscan verification
  const memoText = `ClockLend: Stake ${amountSkr.toLocaleString()} SKR Reputation Bond | User: ${user.toBase58().slice(0, 8)}... | Unlocks 90% LTV & APR Discounts`;
  tx.add(
    new TransactionInstruction({
      programId: MEMO_PROGRAM_ID,
      keys: [{ pubkey: user, isSigner: true, isWritable: false }],
      data: Buffer.from(memoText, 'utf-8'),
    })
  );

  return { tx, profilePDA, escrowPDA };
}

// Build Unstake SKR Reputation Bond Transaction
export async function buildUnstakeSkrTx(
  user: PublicKey,
  amountSkr: number
): Promise<{ tx: Transaction; profilePDA: PublicKey; escrowPDA: PublicKey }> {
  const [profilePDA] = getProfilePDA(user);
  const [escrowPDA] = getSkrEscrowPDA(user);
  const userSkrAccount = getAssociatedTokenAddress(SKR_MINT, user);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 100_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // ClockLendInstruction::UnstakeSKR (Variant 11):
  // 1 byte tag (11) + 8 bytes amount = 9 bytes
  const data = Buffer.alloc(9);
  data.writeUInt8(11, 0); // Instruction 11: UnstakeSKR
  writeU64LE(BigInt(Math.round(amountSkr * 1_000_000))).copy(data, 1);

  // Execute on-chain UnstakeSKR instruction. When the yield vault exists,
  // append it + the user's position so the program syncs the position DOWN —
  // a recycled stake must never keep earning ghost shares.
  const keys: any[] = [
    { pubkey: user, isSigner: true, isWritable: true },
    { pubkey: profilePDA, isSigner: false, isWritable: true },
    { pubkey: userSkrAccount, isSigner: false, isWritable: true },
    { pubkey: escrowPDA, isSigner: false, isWritable: true },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
  ];
  try {
    const yieldVault = await fetchSkrYieldVault('mainnet-beta', USDC_MAINNET_MINT);
    if (yieldVault?.initialized) {
      keys.push({ pubkey: yieldVault.vaultPDA, isSigner: false, isWritable: true });
      keys.push({ pubkey: getUserYieldPDA(user, new PublicKey(yieldVault.rewardMint)), isSigner: false, isWritable: true });
    }
  } catch (_e) {
    // graceful: no position sync
  }
  tx.add(
    new TransactionInstruction({
      programId: PROGRAM_ID,
      keys,
      data,
    })
  );

  const memoText = `ClockLend: Unstake ${amountSkr.toLocaleString()} SKR | User: ${user.toBase58().slice(0, 8)}...`;
  tx.add(
    new TransactionInstruction({
      programId: MEMO_PROGRAM_ID,
      keys: [{ pubkey: user, isSigner: true, isWritable: false }],
      data: Buffer.from(memoText, 'utf-8'),
    })
  );

  return { tx, profilePDA, escrowPDA };
}

// Build Withdraw Treasury Transaction instruction (Admin only - H-2)
export async function buildWithdrawTreasuryTx(
  admin: PublicKey,
  amountLamports: bigint,
  destination: PublicKey,
  isSpl: boolean = false,
  mint?: PublicKey
): Promise<Transaction> {
  const [adminPDA] = getAdminPDA();
  const [treasuryPDA] = getTreasuryPDA();

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 60_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // Variant 14: WithdrawTreasury { amount: u64 } -> 1 byte tag (14) + 8 bytes = 9 bytes
  const data = Buffer.alloc(9);
  data.writeUInt8(14, 0);
  writeU64LE(amountLamports).copy(data, 1);

  const keys = [
    { pubkey: admin, isSigner: true, isWritable: true },
    { pubkey: adminPDA, isSigner: false, isWritable: false },
    { pubkey: treasuryPDA, isSigner: false, isWritable: true },
    { pubkey: destination, isSigner: false, isWritable: true },
  ];

  if (isSpl && mint) {
    const treasuryTokenAcc = getAssociatedTokenAddress(mint, treasuryPDA);
    keys.push(
      { pubkey: treasuryTokenAcc, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }
    );
  } else {
    keys.push(
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }
    );
  }

  tx.add(
    new TransactionInstruction({
      programId: PROGRAM_ID,
      keys,
      data,
    })
  );

  return tx;
}

// Build Deposit Liquidity Transaction instruction (ClockLend Instruction 1 - Pool Authority only)
export async function buildDepositLiquidityTx(
  authority: PublicKey,
  poolId: number,
  amountUsdc: number,
  authorityTokenAccount?: PublicKey
): Promise<Transaction> {
  const [poolPDA] = getPoolPDA(authority, poolId);
  const [vaultPDA] = getVaultPDA(poolPDA);
  const userTokenAcc = authorityTokenAccount || getAssociatedTokenAddress(USDC_MAINNET_MINT, authority);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 80_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // Instruction 1: DepositLiquidity { amount: u64 } -> 1 byte tag (1) + 8 bytes amount = 9 bytes
  const data = Buffer.alloc(9);
  data.writeUInt8(1, 0);
  writeU64LE(BigInt(Math.round(amountUsdc * 1_000_000))).copy(data, 1);

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: authority, isSigner: true, isWritable: true },
      { pubkey: poolPDA, isSigner: false, isWritable: true },
      { pubkey: userTokenAcc, isSigner: false, isWritable: true },
      { pubkey: vaultPDA, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
  tx.add(ix);

  const memoText = `ClockLend: Deposit $${amountUsdc} USDC Liquidity into Pool #${poolId}`;
  tx.add(
    new TransactionInstruction({
      programId: MEMO_PROGRAM_ID,
      keys: [{ pubkey: authority, isSigner: true, isWritable: false }],
      data: Buffer.from(memoText, 'utf-8'),
    })
  );

  return tx;
}

// Build Trigger Grace Period Transaction instruction (ClockLend Instruction 7)
export async function buildTriggerGracePeriodTx(
  caller: PublicKey,
  targetPDA: PublicKey,
  poolPDA?: PublicKey
): Promise<Transaction> {
  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 60_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // Instruction 7: TriggerGracePeriod -> 1 byte tag (7)
  const data = Buffer.alloc(1);
  data.writeUInt8(7, 0);

  const keys = [
    { pubkey: caller, isSigner: true, isWritable: true },
    { pubkey: targetPDA, isSigner: false, isWritable: true },
  ];
  if (poolPDA) {
    keys.push({ pubkey: poolPDA, isSigner: false, isWritable: false });
  }
  keys.push({ pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false });

  tx.add(
    new TransactionInstruction({
      programId: PROGRAM_ID,
      keys,
      data,
    })
  );

  const memoText = `ClockLend: Trigger 24h Social Grace Period for ${targetPDA.toBase58().slice(0, 8)}...`;
  tx.add(
    new TransactionInstruction({
      programId: MEMO_PROGRAM_ID,
      keys: [{ pubkey: caller, isSigner: true, isWritable: false }],
      data: Buffer.from(memoText, 'utf-8'),
    })
  );

  return tx;
}

// Build Claim Default Transaction instruction (ClockLend Instruction 8)
export async function buildClaimDefaultTx(
  caller: PublicKey,
  targetPDA: PublicKey,
  escrowPDA: PublicKey,
  destinationCollateralAccount: PublicKey,
  options: {
    poolPDA?: PublicKey;
    borrower?: PublicKey;
    isNativeSol?: boolean;
    slashSkrDestination?: PublicKey;
  } = {}
): Promise<Transaction> {
  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 100_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // Instruction 8: ClaimDefault -> 1 byte tag (8)
  const data = Buffer.alloc(1);
  data.writeUInt8(8, 0);

  const keys = [
    { pubkey: caller, isSigner: true, isWritable: true },
    { pubkey: targetPDA, isSigner: false, isWritable: true },
    { pubkey: escrowPDA, isSigner: false, isWritable: true },
    { pubkey: destinationCollateralAccount, isSigner: false, isWritable: true },
  ];

  if (options.poolPDA) {
    keys.push({ pubkey: options.poolPDA, isSigner: false, isWritable: true });
    if (options.borrower) {
      const [profilePDA] = getProfilePDA(options.borrower);
      keys.push({ pubkey: profilePDA, isSigner: false, isWritable: true });
      const [skrEscrowPDA] = getSkrEscrowPDA(options.borrower);
      keys.push({ pubkey: skrEscrowPDA, isSigner: false, isWritable: true });
    }
    const [treasuryPDA] = getTreasuryPDA();
    keys.push({ pubkey: treasuryPDA, isSigner: false, isWritable: true });
    if (options.slashSkrDestination) {
      keys.push({ pubkey: options.slashSkrDestination, isSigner: false, isWritable: true });
    }
  }

  keys.push(
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    { pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false }
  );

  tx.add(
    new TransactionInstruction({
      programId: PROGRAM_ID,
      keys,
      data,
    })
  );

  const memoText = `ClockLend: Claim Default & Liquidate Collateral for ${targetPDA.toBase58().slice(0, 8)}...`;
  tx.add(
    new TransactionInstruction({
      programId: MEMO_PROGRAM_ID,
      keys: [{ pubkey: caller, isSigner: true, isWritable: false }],
      data: Buffer.from(memoText, 'utf-8'),
    })
  );

  return tx;
}

// Build Claim SKR Protocol Fee Dividends Transaction (Zero Cooldown, Instant USDC Payout)
// ---------------- SKR yield vault (round-9 hardened ABI) ----------------

export interface SkrYieldVaultState {
  vaultPDA: PublicKey;
  vaultTokenPDA: PublicKey;
  initialized: boolean;
  authority: string;
  rewardMint: string;
  totalStakedSkr: number;        // SKR base units
  accRewardPerShare: bigint;     // 1e12-scaled
  totalRewardsDistributed: number;
  pendingRewards: number;
  unallocatedRewards: number;
}

export interface UserYieldPositionState {
  positionPDA: PublicKey;
  initialized: boolean;
  stakedSkr: number;             // SKR base units
  rewardDebt: bigint;            // 1e12-scaled
  accruedRewards: number;        // reward-mint base units
  totalClaimed: number;
  lastInteractionTime: number;
}

export function getSkrYieldVaultPDA(rewardMint: PublicKey): PublicKey {
  const [pda] = PublicKey.findProgramAddressSync([SKR_YIELD_VAULT_SEED, rewardMint.toBuffer()], PROGRAM_ID);
  return pda;
}
export function getSkrYieldTokenPDA(rewardMint: PublicKey): PublicKey {
  const [pda] = PublicKey.findProgramAddressSync([SKR_YIELD_TOKEN_SEED, rewardMint.toBuffer()], PROGRAM_ID);
  return pda;
}
export function getUserYieldPDA(user: PublicKey, rewardMint: PublicKey): PublicKey {
  const [pda] = PublicKey.findProgramAddressSync([USER_YIELD_SEED, user.toBuffer(), rewardMint.toBuffer()], PROGRAM_ID);
  return pda;
}

/** Read the on-chain vault (121-byte CLK_SYLD layout); undefined if absent. */
export async function fetchSkrYieldVault(
  network: SolanaNetwork,
  rewardMint: PublicKey,
  opts?: { force?: boolean }
): Promise<SkrYieldVaultState | undefined> {
  return cachedFetch(`yieldvault:${network}:${rewardMint.toBase58()}`, () => fetchSkrYieldVaultUncached(network, rewardMint), opts);
}

async function fetchSkrYieldVaultUncached(
  network: SolanaNetwork,
  rewardMint: PublicKey
): Promise<SkrYieldVaultState | undefined> {
  try {
    const vaultPDA = getSkrYieldVaultPDA(rewardMint);
    const info = await queryRpcWithFallback(network, (c) => c.getAccountInfo(vaultPDA));
    if (!info || info.data.length < 121 || Buffer.from(info.data.subarray(0, 8)).toString() !== 'CLK_SYLD') {
      return undefined;
    }
    const d = Buffer.from(info.data);
    return {
      vaultPDA,
      vaultTokenPDA: getSkrYieldTokenPDA(rewardMint),
      initialized: d.readUInt8(8) === 1,
      authority: new PublicKey(d.subarray(9, 41)).toBase58(),
      rewardMint: new PublicKey(d.subarray(41, 73)).toBase58(),
      totalStakedSkr: Number(d.readBigUInt64LE(73)),
      accRewardPerShare: BigInt(d.readBigUInt64LE(81).toString()) + BigInt(d.readBigUInt64LE(89).toString()) * 18446744073709551616n,
      totalRewardsDistributed: Number(d.readBigUInt64LE(97)),
      pendingRewards: Number(d.readBigUInt64LE(105)),
      unallocatedRewards: Number(d.readBigUInt64LE(113)),
    };
  } catch (err) {
    console.warn('[Yield] vault read failed:', (err as any)?.message || err);
    return undefined;
  }
}

/** Read the user's yield position (121-byte CLK_UYLD layout); undefined if absent. */
export async function fetchUserYieldPosition(
  network: SolanaNetwork,
  user: PublicKey,
  rewardMint: PublicKey,
  opts?: { force?: boolean }
): Promise<UserYieldPositionState | undefined> {
  return cachedFetch(
    `yieldpos:${network}:${user.toBase58()}:${rewardMint.toBase58()}`,
    () => fetchUserYieldPositionUncached(network, user, rewardMint),
    opts
  );
}

async function fetchUserYieldPositionUncached(
  network: SolanaNetwork,
  user: PublicKey,
  rewardMint: PublicKey
): Promise<UserYieldPositionState | undefined> {
  try {
    const positionPDA = getUserYieldPDA(user, rewardMint);
    const info = await queryRpcWithFallback(network, (c) => c.getAccountInfo(positionPDA));
    if (!info || info.data.length < 121 || Buffer.from(info.data.subarray(0, 8)).toString() !== 'CLK_UYLD') {
      return undefined;
    }
    const d = Buffer.from(info.data);
    return {
      positionPDA,
      initialized: d.readUInt8(8) === 1,
      stakedSkr: Number(d.readBigUInt64LE(73)),
      rewardDebt: BigInt(d.readBigUInt64LE(81).toString()) + BigInt(d.readBigUInt64LE(89).toString()) * 18446744073709551616n,
      accruedRewards: Number(d.readBigUInt64LE(97)),
      totalClaimed: Number(d.readBigUInt64LE(105)),
      lastInteractionTime: Number(d.readBigInt64LE(113)),
    };
  } catch (err) {
    console.warn('[Yield] position read failed:', (err as any)?.message || err);
    return undefined;
  }
}

/** Instruction 15: admin-gated vault init (requires AdminConfig PDA). */
export async function buildInitializeSkrYieldVaultTx(
  authority: PublicKey,
  rewardMint: PublicKey = USDC_MAINNET_MINT
): Promise<Transaction> {
  const vaultPDA = getSkrYieldVaultPDA(rewardMint);
  const vaultTokenPDA = getSkrYieldTokenPDA(rewardMint);
  const [adminPDA] = getAdminPDA();

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 100_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));
  tx.add(
    new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: authority, isSigner: true, isWritable: true },
        { pubkey: vaultPDA, isSigner: false, isWritable: true },
        { pubkey: rewardMint, isSigner: false, isWritable: false },
        { pubkey: vaultTokenPDA, isSigner: false, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: SYSVAR_RENT_PUBKEY, isSigner: false, isWritable: false },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        { pubkey: adminPDA, isSigner: false, isWritable: false },
      ],
      data: Buffer.from([15]),
    })
  );
  return tx;
}

/** Instruction 16: authority-gated dividend deposit. */
export async function buildDepositSkrYieldTx(
  depositor: PublicKey,
  amountUsdc: number,
  rewardMint: PublicKey = USDC_MAINNET_MINT
): Promise<Transaction> {
  const vaultPDA = getSkrYieldVaultPDA(rewardMint);
  const vaultTokenPDA = getSkrYieldTokenPDA(rewardMint);
  const depositorToken = getAssociatedTokenAddress(rewardMint, depositor);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 100_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));
  const data = Buffer.alloc(9);
  data.writeUInt8(16, 0);
  writeU64LE(BigInt(Math.round(amountUsdc * 1_000_000))).copy(data, 1);
  tx.add(
    new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: depositor, isSigner: true, isWritable: true },
        { pubkey: vaultPDA, isSigner: false, isWritable: true },
        { pubkey: depositorToken, isSigner: false, isWritable: true },
        { pubkey: vaultTokenPDA, isSigner: false, isWritable: true },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      ],
      data,
    })
  );
  return tx;
}

/** Instruction 17: claim dividends. Stake is read from the SKR escrow token
 *  account (account 7). The reward ATA is created idempotently. */
export async function buildClaimSkrYieldTx(
  user: PublicKey,
  rewardMint: PublicKey = USDC_MAINNET_MINT
): Promise<Transaction> {
  const vaultPDA = getSkrYieldVaultPDA(rewardMint);
  const vaultTokenPDA = getSkrYieldTokenPDA(rewardMint);
  const userYieldPDA = getUserYieldPDA(user, rewardMint);
  const [escrowPDA] = getSkrEscrowPDA(user);
  const userRewardAccount = getAssociatedTokenAddress(rewardMint, user);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 150_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));
  // The program unpacks the reward account whenever there is something to
  // claim — create it idempotently so the first real claim never reverts.
  tx.add(
    createAssociatedTokenAccountIdempotentInstruction(
      user,
      userRewardAccount,
      user,
      rewardMint
    )
  );
  // Instruction 17: ClaimSkrYield
  tx.add(
    new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: user, isSigner: true, isWritable: true },
        { pubkey: vaultPDA, isSigner: false, isWritable: true },
        { pubkey: userYieldPDA, isSigner: false, isWritable: true },
        { pubkey: vaultTokenPDA, isSigner: false, isWritable: true },
        { pubkey: userRewardAccount, isSigner: false, isWritable: true },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: escrowPDA, isSigner: false, isWritable: false },
      ],
      data: Buffer.from([17]),
    })
  );

  const memoText = `ClockLend: Claim SKR Protocol Fee Yield Dividends`;
  tx.add(
    new TransactionInstruction({
      programId: MEMO_PROGRAM_ID,
      keys: [{ pubkey: user, isSigner: true, isWritable: false }],
      data: Buffer.from(memoText, 'utf-8'),
    })
  );

  return tx;
}
