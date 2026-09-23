// Pyth pull-oracle client: fetches verified price updates from Hermes and
// hand-builds the receiver postUpdateAtomic instruction — deliberately WITHOUT
// the @pythnetwork/pyth-solana-receiver package, whose @coral-xyz/anchor
// dependency breaks Metro bundling on React Native.
//
// Layout sources (verified against the Rust receiver + SDK crates):
//   - instruction discriminator sha256("global:post_update_atomic")[..8]
//   - data = borsh({ vaa: Vec<u8>, merkle_price_update: { message: Vec<u8>,
//             proof: Vec<[u8;20]> }, treasury_id: u8 })
//   - Borsh encodes EVERY sequence length as u32 LE — including `message`
//     (PrefixedVec<u16, u8> derives Borsh as a struct wrapping a plain Vec,
//     pinned by the crate's own round-trip test) and `proof` (Vec<[u8;20]>).
//   - accounts = [payer(s,w), guardianSet(r), config(r), treasury(w),
//             priceUpdateAccount(w), systemProgram(r), writeAuthority(s)]
//   - PDA seeds: config ["config"], guardian set ["GuardianSet", u32BE(idx)]
//     (wormhole program), treasury ["treasury", u8 id] (receiver program)
//
// Fail-safe policy: an attachment is only built when the client can PROVE it
// will be accepted — the on-chain guardian set must exist and the VAA must
// carry >= quorum(keys) signatures (the program accepts Full only). Anything
// short of that throws, and the caller falls back to the canonical account /
// admin feed. Hermes v2 now requires an API key (EXPO_PUBLIC_HERMES_API_KEY);
// without one the fetch fails loudly and the fallback runs.
//
import { PublicKey, Keypair, TransactionInstruction, Connection, SystemProgram } from '@solana/web3.js';
import { parseAccumulatorUpdateData } from '@pythnetwork/price-service-sdk';
import { Buffer } from 'buffer';

export const PYTH_RECEIVER_ID = new PublicKey('rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ');
export const WORMHOLE_CORE_BRIDGE_ID = new PublicKey('HDwcJBJXjL9FpJ7UBsYBtaDjsBUhuLCUYoz3zr8SWWaQ');

// Canonical PythNet feed ids (must match the program's pyth.rs constants).
export const SOL_USD_FEED_ID = '0xef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d';
export const SKR_USD_FEED_ID = '0x38846ec4d0dbe808091817f5c0d6ab8058e25422348ddf97db52b6c378a93bf9';

const POST_UPDATE_ATOMIC_DISCRIMINATOR = Buffer.from([0x31, 0xac, 0x54, 0xc0, 0xaf, 0xb4, 0x34, 0xea]);
const POST_UPDATE_ATOMIC_COMPUTE_BUDGET = 170_000;
const TREASURY_ID = 0;
// Solana transaction payload limit (1232 bytes) minus room for the 8-byte
// discriminator, treasury id byte and instruction overhead.
const MAX_ATOMIC_DATA_SIZE = 1100;

export interface PythAttachment {
  instructions: TransactionInstruction[];
  signers: Keypair[];
  priceUpdateAccount: PublicKey;
  computeUnits: number;
}

/**
 * Pyth's canonical, continuously-cranked SOL/USD price UPDATE account. When
 * Hermes is unreachable, the client can reference this account directly
 * (no posting needed) — the program re-verifies it on-chain. Ages observed
 * 17-54s; the program's SOL freshness window is 120s.
 */
export const PYTH_CANONICAL_SOL_UPDATE_ACCOUNT = new PublicKey(
  '7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE'
);

/** Feed id for a collateral name (program allowlist: SOL or SKR only). */
export function pythFeedIdForCollateral(collateralName: string): string {
  return collateralName.toUpperCase().includes('SOL') ? SOL_USD_FEED_ID : SKR_USD_FEED_ID;
}

/**
 * Resolve the canonical on-chain SOL price update account if it is currently
 * receiver-owned (sanity; the program performs the full verification).
 */
export async function tryCanonicalSolUpdateAccount(
  connection: Connection
): Promise<PublicKey | undefined> {
  try {
    const info = await connection.getAccountInfo(PYTH_CANONICAL_SOL_UPDATE_ACCOUNT);
    if (info && info.owner.equals(PYTH_RECEIVER_ID)) {
      return PYTH_CANONICAL_SOL_UPDATE_ACCOUNT;
    }
  } catch (_e) {
    // fall through
  }
  return undefined;
}

/**
 * Fetch fully-verified price updates (base64 VAAs) from Hermes v2. An API key
 * is required (EXPO_PUBLIC_HERMES_API_KEY); keyless requests now return 401.
 * Fail loudly — callers catch and fall back to the admin feed / canonical
 * account, never to an unverified price.
 */
export async function fetchPythPriceUpdates(feedIds: string[]): Promise<string[]> {
  const apiKey = process.env.EXPO_PUBLIC_HERMES_API_KEY;
  if (!apiKey) throw new Error('Pyth Hermes: EXPO_PUBLIC_HERMES_API_KEY is not configured');
  const q = feedIds.map((id) => `ids[]=${encodeURIComponent(id)}`).join('&');
  const res = await fetch(`https://hermes.pyth.network/v2/updates/price/latest?${q}&encoding=base64`, {
    headers: { Authorization: apiKey },
    signal: AbortSignal.timeout(5000),
  });
  if (!res.ok) throw new Error(`Pyth Hermes fetch failed: ${res.status}`);
  const j = await res.json();
  const data = j?.binary?.data;
  if (!Array.isArray(data) || data.length === 0 || typeof data[0] !== 'string') {
    throw new Error('Pyth Hermes: unexpected response shape');
  }
  return data;
}

/** Wormhole quorum: n - floor((n-1)/3), i.e. ceil(2n/3). */
export function quorumFor(guardianCount: number): number {
  return guardianCount - Math.floor((guardianCount - 1) / 3);
}

/**
 * Read the on-chain guardian set the VAA references and return the number of
 * guardian keys. Handles both the Anchor layout (8-byte discriminator, index
 * u32, keys_len u32) and the legacy wormhole-native layout (index u32,
 * keys_len u32) — the PDA derivation matches either way. Returns undefined if
 * the account does not exist or parses to an implausible key count.
 */
export async function guardianSetKeyCount(
  connection: Connection,
  guardianSetIndex: number
): Promise<number | undefined> {
  const [guardianSetPda] = pdaGuardianSet(guardianSetIndex);
  const info = await connection.getAccountInfo(guardianSetPda);
  if (!info) return undefined;
  const d = info.data;
  const parse = (lenOff: number): number | undefined => {
    if (d.length < lenOff + 4) return undefined;
    const n = d.readUInt32LE(lenOff);
    if (n < 1 || n > 100 || d.length < lenOff + 4 + n * 20) return undefined;
    return n;
  };
  // Anchor GuardianSet: discriminator[0..8], index[8..12], len[12..16].
  return parse(12) ?? parse(4);
}

function pdaConfig(): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([Buffer.from('config')], PYTH_RECEIVER_ID);
}

function pdaTreasury(treasuryId: number): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from('treasury'), Buffer.from([treasuryId])],
    PYTH_RECEIVER_ID
  );
}

function pdaGuardianSet(index: number): [PublicKey, number] {
  const buf = Buffer.alloc(4);
  buf.writeUInt32BE(index, 0);
  return PublicKey.findProgramAddressSync([Buffer.from('GuardianSet'), buf], WORMHOLE_CORE_BRIDGE_ID);
}

function borshBytes(value: Buffer): Buffer {
  const len = Buffer.alloc(4);
  len.writeUInt32LE(value.length, 0);
  return Buffer.concat([len, value]);
}

// Borsh encodes every sequence length as u32 LE. Both merkle fields go
// through plain Vecs in the receiver's Borsh layout (see header comment).
function borshU32Prefixed20s(items: number[][]): Buffer {
  const count = Buffer.alloc(4);
  count.writeUInt32LE(items.length, 0);
  const body = Buffer.concat(items.map((it) => Buffer.from(it)));
  return Buffer.concat([count, body]);
}

/**
 * Build the receiver postUpdateAtomic instruction for one feed, plus the
 * ephemeral price-update account keypair. Metro-safe: no anchor imports.
 *
 * Only builds an attachment that is PROVABLY acceptable to ClockLend's
 * Full-verification gate: the VAA's guardian set must exist on-chain and the
 * VAA must carry >= quorum(keys) signatures (the VAA is passed through
 * UNTRIMMED — rewriting the signature count would change the guardian-signed
 * digest and be rejected anyway). If the VAA can't satisfy Full verification
 * or the payload would exceed the transaction size limit, this throws and the
 * caller falls back to the canonical account / admin feed.
 */
export async function buildPythAttachment(
  connection: Connection,
  payer: PublicKey,
  feedId: string
): Promise<PythAttachment> {
  const [vaaB64] = await fetchPythPriceUpdates([feedId]);
  if (!vaaB64) throw new Error('Pyth: no price update returned for feed');

  const payload = Buffer.from(vaaB64, 'base64');
  const parsed = parseAccumulatorUpdateData(payload);
  const vaa = Buffer.from(parsed.vaa);
  if (vaa.length < 6) throw new Error('Pyth: malformed VAA');
  const update = parsed.updates[0];
  if (!update) throw new Error('Pyth: no update in accumulator message');

  const signatureCount = vaa[5];
  const guardianSetIndex = vaa.readUInt32BE(1);
  const keys = await guardianSetKeyCount(connection, guardianSetIndex);
  if (keys === undefined) {
    throw new Error(`Pyth: guardian set ${guardianSetIndex} not found on-chain`);
  }
  if (signatureCount < quorumFor(keys)) {
    // The program accepts Full only; posting a VAA that will be stored as
    // Partial{..} guarantees a revert, so refuse and let the caller fall back.
    throw new Error(
      `Pyth: VAA has ${signatureCount} signatures, quorum for ${keys} guardians is ${quorumFor(keys)}`
    );
  }

  const [configPda] = pdaConfig();
  const [treasuryPda] = pdaTreasury(TREASURY_ID);
  const [guardianSetPda] = pdaGuardianSet(guardianSetIndex);

  const merkle = Buffer.concat([
    borshBytes(Buffer.from(update.message)),
    borshU32Prefixed20s(update.proof),
  ]);
  const data = Buffer.concat([
    POST_UPDATE_ATOMIC_DISCRIMINATOR,
    borshBytes(vaa),
    merkle,
    Buffer.from([TREASURY_ID]),
  ]);
  if (data.length > MAX_ATOMIC_DATA_SIZE) {
    throw new Error(`Pyth: atomic payload too large for a transaction (${data.length} bytes)`);
  }

  const priceUpdateKeypair = Keypair.generate();
  const ix = new TransactionInstruction({
    programId: PYTH_RECEIVER_ID,
    keys: [
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: guardianSetPda, isSigner: false, isWritable: false },
      { pubkey: configPda, isSigner: false, isWritable: false },
      { pubkey: treasuryPda, isSigner: false, isWritable: true },
      { pubkey: priceUpdateKeypair.publicKey, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: priceUpdateKeypair.publicKey, isSigner: true, isWritable: false },
    ],
    data,
  });

  return {
    instructions: [ix],
    signers: [priceUpdateKeypair],
    priceUpdateAccount: priceUpdateKeypair.publicKey,
    computeUnits: POST_UPDATE_ATOMIC_COMPUTE_BUDGET,
  };
}
