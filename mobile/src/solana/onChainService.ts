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
  PROGRAM_ID,
  DEVNET_RPC,
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

export const MAINNET_RPCS = [
  'https://api.mainnet-beta.solana.com',
];

export const devnetConnection = new Connection(DEVNET_RPCS[0], 'confirmed');
export const mainnetConnection = new Connection(MAINNET_RPCS[0], 'confirmed');

export function getConnection(network: SolanaNetwork = 'devnet'): Connection {
  return network === 'mainnet-beta' ? mainnetConnection : devnetConnection;
}

// Default export connection for backwards compatibility
export const connection = devnetConnection;

export { PROGRAM_ID };

export const TOKEN_PROGRAM_ID = new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
export const TOKEN_2022_PROGRAM_ID = new PublicKey('TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb');
export const MEMO_PROGRAM_ID = new PublicKey('MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr');
export const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL');

// M-04: Canonical Solana Devnet USDC Mint
export const USDC_DEVNET_MINT = new PublicKey('4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU');
export const SKR_DEVNET_MINT = new PublicKey('SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3');

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

// Seeded verified Devnet pool addresses for instantaneous retrieval
const SEEDED_POOLS = [
  new PublicKey('6EK2pPKZyqZZfUj6MxJ1cS2jYQzXppMAVf9Wm1VsPgoi'), // Seeker Genesis Circle
  new PublicKey('AgMT2TRQ9xdTqf2CV8Gnjg564hDdKXBumhkhwi8egAKy'), // Tokyo Whale Desk
];

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

// Local hybrid cache for persistent loan orders
export async function getCachedOrders(borrowerPubkey: string): Promise<LoanOrder[]> {
  try {
    const raw = await SecureStore.getItemAsync(`clocklend_orders_${borrowerPubkey}`);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) return parsed;
    }
  } catch (err) {
    console.warn('Error reading cached orders:', err);
  }
  return [];
}

export async function setCachedOrders(borrowerPubkey: string, orders: LoanOrder[]): Promise<void> {
  try {
    await SecureStore.setItemAsync(`clocklend_orders_${borrowerPubkey}`, JSON.stringify(orders));
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

  const successRate = loansOriginated > 0 ? (loansRepaid / loansOriginated) * 100 : 100;

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
    successRate: parseFloat(successRate.toFixed(1)),
    isVerifiedMerchant: stakedSkrAmount > 0 || poolType === 'Circle' || poolType === 'Institutional',
  };
}

// Fetch all live Lending Pools from the deployed Devnet contract
export async function fetchLivePools(): Promise<LendingPool[]> {
  const poolsMap = new Map<string, LendingPool>();

  // 1. Fast path: load known seeded pools via getMultipleAccountsInfo (~300ms)
  try {
    const accounts = await devnetConnection.getMultipleAccountsInfo(SEEDED_POOLS);
    accounts.forEach((acc, idx) => {
      if (acc && acc.data) {
        const pool = parsePoolData(SEEDED_POOLS[idx].toBase58(), Buffer.from(acc.data), poolsMap.size + 1);
        if (pool) poolsMap.set(SEEDED_POOLS[idx].toBase58(), pool);
      }
    });
  } catch (err) {
    console.warn('Fast pool query notice:', err);
  }

  // 2. Full scan to find any additional pools created by individuals
  try {
    const accounts = await devnetConnection.getProgramAccounts(PROGRAM_ID);
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

// Fetch live user loan orders directly from Solana Devnet contract & on-chain state
export async function fetchLiveUserOrders(borrower: PublicKey): Promise<LoanOrder[]> {
  const borrowerPubkey = borrower.toBase58();
  const cachedOrders = await getCachedOrders(borrowerPubkey);
  const cachedBySig = new Map<string, LoanOrder>();
  const cachedById = new Map<number, LoanOrder>();
  for (const co of cachedOrders) {
    if (co.txSignature) cachedBySig.set(co.txSignature, co);
    cachedById.set(co.id, co);
  }

  const ordersMap = new Map<number, LoanOrder>();

  // 1. Scan on-chain PDA accounts first (for liquid pool PDA loans)
  try {
    const accounts = await devnetConnection.getProgramAccounts(PROGRAM_ID);
    for (const acc of accounts) {
      if (acc.account.data.length === 170 || acc.account.data.length === 154) {
        const data = Buffer.from(acc.account.data);
        const isV2 = data.length === 170;
        const isActive = data.readUInt8(isV2 ? 8 : 0) === 1;
        if (!isActive) continue;

        const borrowerOnChain = new PublicKey(data.subarray(isV2 ? 17 : 9, isV2 ? 49 : 41));
        if (!borrowerOnChain.equals(borrower)) continue;

        const loanId = Number(data.readBigUInt64LE(isV2 ? 9 : 1));
        const poolPubkey = new PublicKey(data.subarray(isV2 ? 49 : 41, isV2 ? 81 : 73));
        const principalAmount = Number(data.readBigUInt64LE(isV2 ? 81 : 73)) / 1_000_000;
        const collateralMint = new PublicKey(data.subarray(isV2 ? 89 : 81, isV2 ? 121 : 113)).toBase58();
        const collateralAmount = Number(data.readBigUInt64LE(isV2 ? 121 : 113)) / 1_000_000_000;
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
          borrower: borrowerPubkey,
          principalAmount,
          collateralName: `${collateralAmount.toFixed(2)} SOL`,
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

  // 2. Scan Solana Devnet blockchain transactions & memos for ground-truth borrow/repay history
  try {
    const signatures = await devnetConnection.getSignaturesForAddress(borrower, { limit: 60 });
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

          // Check if already in cache for this signature
          const cached = cachedBySig.get(m.sig);
          if (cached) {
            orderId = cached.id;
          } else {
            const digits = m.sig.replace(/\D/g, '');
            orderId = parseInt(digits.slice(-4)) || Math.floor(1000 + Math.random() * 9000);
          }
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

    // Convert open candidates into LoanOrder records
    const nowSec = Math.floor(Date.now() / 1000);
    for (const cand of openCandidates) {
      // If locally marked repaid, skip
      const cached = cachedById.get(cand.id) || cachedBySig.get(cand.sig);
      if (cached && cached.status === 'Repaid') continue;

      const dueTime = cand.time + 7 * 86400;
      let status: LoanStatus = 'Active';
      let gracePeriodExpires = 0;
      if (nowSec > dueTime) {
        status = 'InGracePeriod';
        gracePeriodExpires = dueTime + 86400;
      } else if (cached?.status === 'InGracePeriod') {
        status = 'InGracePeriod';
        gracePeriodExpires = cached.gracePeriodExpires || (dueTime + 86400);
      }

      const isSol = cand.collateral.toUpperCase().includes('SOL');
      const collUnits = parseFloat(cand.collateral) || 0;
      const collateralMint = isSol
        ? 'So11111111111111111111111111111111111111112'
        : 'SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3';

      // M-03: Align interest math to smart contract exact integer formula
      const principalMicro = BigInt(Math.round(cand.principal * 1_000_000));
      const exactInterestMicro = calculateExactInterestDue(principalMicro, 800, 7 * 86400, false);
      const interestDue = Number(exactInterestMicro) / 1_000_000;
      const poolAuth = cand.poolId === 1 ? 'BEmX1nfeZT5i4VpSEeZmhiYxpZ9z4Y1LQLjAtPR9c3re' : '5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1';
      const [poolPDA] = getPoolPDA(new PublicKey(poolAuth), cand.poolId);
      const [loanPDA] = getLoanPDA(poolPDA, borrower, cand.id);
      const [escrowPDA] = getEscrowPDA(loanPDA);

      ordersMap.set(cand.id, {
        id: cand.id,
        poolId: cand.poolId,
        poolName: cand.poolId === 1 ? 'Seeker Genesis Circle' : 'Tokyo Whale Desk',
        borrower: borrowerPubkey,
        principalAmount: cand.principal,
        collateralName: cand.collateral.includes(' ') ? cand.collateral : `${cand.collateral} ${isSol ? 'SOL' : 'SKR'}`,
        collateralMint,
        collateralAmount: collUnits,
        interestDue,
        originationTime: cand.time,
        dueTime,
        gracePeriodExpires,
        status,
        txSignature: cand.sig,
        escrowAddress: escrowPDA.toBase58(),
        solscanUrl: `https://solscan.io/tx/${cand.sig}?cluster=devnet`,
      });
    }
  } catch (sigErr) {
    console.warn('Devnet signature scan notice:', sigErr);
  }

  // 3. Fallback: if blockchain was temporarily unreachable or slow, keep active cached orders
  if (ordersMap.size === 0 && cachedOrders.length > 0) {
    for (const co of cachedOrders) {
      if (co.status === 'Active' || co.status === 'InGracePeriod') {
        ordersMap.set(co.id, co);
      }
    }
  }

  const finalOrders = Array.from(ordersMap.values());
  // Save verified active orders back to SecureStore hybrid cache
  await setCachedOrders(borrowerPubkey, finalOrders);

  return finalOrders;
}

// Fetch live P2P pawn offers directly from Devnet contract
export async function fetchLiveP2POffers(): Promise<P2POffer[]> {
  try {
    const accounts = await devnetConnection.getProgramAccounts(PROGRAM_ID);
    const offers: P2POffer[] = [];

    for (const acc of accounts) {
      if (acc.account.data.length === 170 || acc.account.data.length === 168 || acc.account.data.length === 162) {
        const data = Buffer.from(acc.account.data);
        const isV2 = data.length >= 168;
        const isListed = data.readUInt8(isV2 ? 8 : 0) === 1;
        if (!isListed) continue;

        const offerId = Number(data.readBigUInt64LE(isV2 ? 9 : 1));
        const creator = new PublicKey(data.subarray(isV2 ? 17 : 9, isV2 ? 49 : 41)).toBase58();
        const funder = new PublicKey(data.subarray(isV2 ? 49 : 41, isV2 ? 81 : 73)).toBase58();
        const collateralMint = new PublicKey(data.subarray(isV2 ? 81 : 73, isV2 ? 113 : 105)).toBase58();
        const collateralLamports = Number(data.readBigUInt64LE(isV2 ? 113 : 105));
        const requestedLamports = Number(data.readBigUInt64LE(isV2 ? 121 : 113));
        const interestLamports = Number(data.readBigUInt64LE(isV2 ? 129 : 121));
        const durationSeconds = Number(data.readBigInt64LE(isV2 ? 137 : 129));
        const createdAt = Number(data.readBigInt64LE(isV2 ? 145 : 137));
        const dueTime = Number(data.readBigInt64LE(isV2 ? 153 : 145));
        const statusByte = data.readUInt8(isV2 ? (data.length === 170 ? 169 : 167) : 161);

        const requestedAmount = requestedLamports / 1_000_000;
        const interestOffered = interestLamports / 1_000_000;
        const durationDays = Math.max(1, Math.round(durationSeconds / 86400));
        const collateralAmount = collateralLamports / 1_000_000_000;

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
          collateralName: `${collateralAmount > 0 ? collateralAmount.toFixed(2) : '1.0'} SOL`,
          collateralType: 'Token',
          collateralAmount: collateralAmount > 0 ? collateralAmount : 1,
          collateralMint,
          requestedAmount,
          interestOffered,
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
export async function fetchLiveUserProfile(userPubkey: PublicKey, skrHandle: string): Promise<UserProfile> {
  try {
    const [profilePDA] = getProfilePDA(userPubkey);
    const accountInfo = await devnetConnection.getAccountInfo(profilePDA);

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
    reputationScore: 10000,
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
  if (mint === '4Zao8ocPhmMgq7PdsYWyxvqySMGx7xb9cMftPMkEokRG') {
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
  if (mint === '4Zao8ocPhmMgq7PdsYWyxvqySMGx7xb9cMftPMkEokRG') {
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

function calculateTokenUsd(mint: string, amount: number, prices = livePrices): number {
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
  network: SolanaNetwork = 'devnet'
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

      // Detect Seeker Genesis Token (Soulbound / Frozen Token-2022)
      if (
        state === 'frozen' ||
        mint === '4Zao8ocPhmMgq7PdsYWyxvqySMGx7xb9cMftPMkEokRG' ||
        mint.toLowerCase().includes('seeker') ||
        mint.toLowerCase().includes('sgt')
      ) {
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
    hasSeekerGenesisToken: hasSeekerGenesisToken || true,
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
  isPoolLiquid: boolean = true
): Promise<{ tx: Transaction; escrowPDA: PublicKey; loanId: number }> {
  const [poolPDA] = getPoolPDA(poolAuthority, poolId);
  const [vaultPDA] = getVaultPDA(poolPDA);
  const loanId = Math.floor(1000 + Math.random() * 900000);
  const [loanPDA] = getLoanPDA(poolPDA, borrower, loanId);
  const [escrowPDA] = getEscrowPDA(loanPDA);
  const [profilePDA] = getProfilePDA(borrower);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 100_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // Layout: 1 byte tag (3) + 8 bytes loan_id + 8 bytes borrow_amount + 8 bytes collateral_amount + 8 bytes duration_seconds = 33 bytes
  const data = Buffer.alloc(33);
  data.writeUInt8(3, 0); // Instruction 3: BorrowFromPool
  writeU64LE(BigInt(loanId)).copy(data, 1);
  writeU64LE(BigInt(Math.round(borrowAmountUsdc * 1_000_000))).copy(data, 9);
  writeU64LE(BigInt(collateralAmountLamports)).copy(data, 17);
  writeU64LE(BigInt(durationDays * 86400)).copy(data, 25);

  const borrowerUsdcAccount = getAssociatedTokenAddress(USDC_DEVNET_MINT, borrower);
  const isNativeSol = collateralName.toUpperCase().includes('SOL');
  const borrowerCollateralAccount = isNativeSol
    ? borrower
    : getAssociatedTokenAddress(SKR_DEVNET_MINT, borrower);
  const collateralMint = isNativeSol
    ? SystemProgram.programId
    : SKR_DEVNET_MINT;

  const [treasuryPDA] = getTreasuryPDA();
  const treasuryUsdcAccount = getAssociatedTokenAddress(USDC_DEVNET_MINT, treasuryPDA);
  const [oraclePDA] = getOraclePDA(collateralMint);

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
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
    ],
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
  collateralName: string = 'SOL'
): Promise<Transaction> {
  const [poolPDA] = getPoolPDA(poolAuthority, poolId);
  const [vaultPDA] = getVaultPDA(poolPDA);
  const [loanPDA] = getLoanPDA(poolPDA, borrower, orderId);
  const [escrowPDA] = getEscrowPDA(loanPDA);
  const [profilePDA] = getProfilePDA(borrower);
  const borrowerUsdcAccount = getAssociatedTokenAddress(USDC_DEVNET_MINT, borrower);
  const [treasuryPDA] = getTreasuryPDA();
  const treasuryUsdcAccount = getAssociatedTokenAddress(USDC_DEVNET_MINT, treasuryPDA);

  const isNativeSol = collateralName.toUpperCase().includes('SOL');
  const borrowerCollateralAccount = isNativeSol
    ? borrower
    : getAssociatedTokenAddress(SKR_DEVNET_MINT, borrower);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 80_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // Layout: 1 byte tag (6) + 8 bytes repay_amount = 9 bytes
  const data = Buffer.alloc(9);
  data.writeUInt8(6, 0); // Instruction 6: RepayLoan
  writeU64LE(BigInt(Math.round(repayAmountUsdc * 1_000_000))).copy(data, 1);

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

// Request 1 Devnet SOL airdrop directly to the user's connected wallet
export async function requestDevnetAirdrop(userPubkey: PublicKey): Promise<string> {
  const sig = await devnetConnection.requestAirdrop(userPubkey, 1_000_000_000);
  const { blockhash, lastValidBlockHeight } = await devnetConnection.getLatestBlockhash('confirmed');
  await devnetConnection.confirmTransaction(
    {
      signature: sig,
      blockhash,
      lastValidBlockHeight,
    },
    'confirmed'
  );
  return sig;
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
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 100_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // Check if asset specifies SOL collateral amount (e.g. "0.5 SOL", "1 SOL") or SKR
  const solMatch = assetName.match(/([0-9]*\.?[0-9]+)\s*SOL/i);
  const skrMatch = assetName.match(/([0-9]*\.?[0-9]+)\s*SKR/i);
  const isNativeSol = solMatch !== null || !assetName.toUpperCase().includes('SKR');

  let collateralAmount = 1_000_000_000; // default 1 SOL
  if (solMatch && solMatch[1]) {
    collateralAmount = Math.round(parseFloat(solMatch[1]) * 1_000_000_000);
  } else if (skrMatch && skrMatch[1]) {
    collateralAmount = Math.round(parseFloat(skrMatch[1]) * 1_000_000);
  }

  const collateralMint = isNativeSol ? SystemProgram.programId : SKR_DEVNET_MINT;
  const creatorCollateralAccount = isNativeSol
    ? creator
    : getAssociatedTokenAddress(SKR_DEVNET_MINT, creator);

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
  const funderUsdcAccount = getAssociatedTokenAddress(USDC_DEVNET_MINT, funder);
  const creatorUsdcAccount = getAssociatedTokenAddress(USDC_DEVNET_MINT, creator);

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
  const borrowerUsdcAccount = getAssociatedTokenAddress(USDC_DEVNET_MINT, borrower);
  const funderUsdcAccount = getAssociatedTokenAddress(USDC_DEVNET_MINT, funder);

  const isNativeSol = offer.collateralName.toUpperCase().includes('SOL');
  const borrowerCollateralAccount = isNativeSol
    ? borrower
    : getAssociatedTokenAddress(SKR_DEVNET_MINT, borrower);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 80_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  const totalDue = parseFloat((offer.requestedAmount + offer.interestOffered).toFixed(2));
  // ClockLendInstruction::RepayLoan (Variant 6)
  const data = Buffer.alloc(9);
  data.writeUInt8(6, 0);
  writeU64LE(BigInt(Math.round(totalDue * 1_000_000))).copy(data, 1);

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

  const memoText = `ClockLend: Repay P2P Pawn #${offer.id} | Repaid: $${totalDue} USDC | Collateral ${offer.collateralName} Released from Escrow`;
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
  const creatorCollateralAccount = isNativeSol ? creator : getAssociatedTokenAddress(SKR_DEVNET_MINT, creator);

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
  const data = Buffer.alloc(1 + 8 + 1 + 2 + 2 + 8 + 8 + 32);
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
  nameBuf.copy(data, offset);

  const liquidityMint = USDC_DEVNET_MINT;

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
  const userSkrAccount = getAssociatedTokenAddress(SKR_DEVNET_MINT, user);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 100_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // ClockLendInstruction::StakeSKR (Variant 2):
  // 1 byte tag (2) + 8 bytes amount = 9 bytes
  const data = Buffer.alloc(9);
  data.writeUInt8(2, 0); // Instruction 2: StakeSKR
  writeU64LE(BigInt(Math.round(amountSkr * 1_000_000))).copy(data, 1);

  // Transfer 0.002 SOL for rent-exempt UserProfile / Escrow PDA
  tx.add(
    SystemProgram.transfer({
      fromPubkey: user,
      toPubkey: escrowPDA,
      lamports: 2_000_000,
    })
  );

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
        { pubkey: SKR_DEVNET_MINT, isSigner: false, isWritable: false },
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
  const userSkrAccount = getAssociatedTokenAddress(SKR_DEVNET_MINT, user);

  const tx = new Transaction();
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 100_000 }));
  tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000 }));

  // ClockLendInstruction::UnstakeSKR (Variant 11):
  // 1 byte tag (11) + 8 bytes amount = 9 bytes
  const data = Buffer.alloc(9);
  data.writeUInt8(11, 0); // Instruction 11: UnstakeSKR
  writeU64LE(BigInt(Math.round(amountSkr * 1_000_000))).copy(data, 1);

  // Execute on-chain UnstakeSKR instruction
  tx.add(
    new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: user, isSigner: true, isWritable: true },
        { pubkey: profilePDA, isSigner: false, isWritable: true },
        { pubkey: userSkrAccount, isSigner: false, isWritable: true },
        { pubkey: escrowPDA, isSigner: false, isWritable: true },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      ],
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




