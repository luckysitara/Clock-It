// Pyth pull-oracle client: fetches verified price updates from Hermes and
// hand-builds the receiver postUpdateAtomic instruction — deliberately WITHOUT
// the @pythnetwork/pyth-solana-receiver package, whose @coral-xyz/anchor
// dependency breaks Metro bundling on React Native.
//
// Layout sources (verified against the Rust receiver + SDK):
//   - instruction discriminator sha256("global:post_update_atomic")[..8]
//   - data = borsh({ vaa: Vec<u8>, merkle_price_update: { message: Vec<u8>,
//             proof: Vec<[u8;20]> }, treasury_id: u8 })
//   - accounts = [payer(s,w), guardianSet(r), config(r), treasury(w),
//             priceUpdateAccount(w), systemProgram(r), writeAuthority(s)]
//   - PDA seeds: config ["config"], guardian set ["GuardianSet", u32BE(idx)]
//     (wormhole program), treasury ["treasury", u8 id] (receiver program)
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
const REDUCED_GUARDIAN_SET_SIZE = 5;
const TREASURY_ID = 0;

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
 * Fetch fully-verified price updates (base64 VAAs) from the public Hermes
 * endpoint. No API key required.
 */
export async function fetchPythPriceUpdates(feedIds: string[]): Promise<string[]> {
  const q = feedIds.map((id) => `ids[]=${encodeURIComponent(id)}`).join('&');
  const res = await fetch(
    `https://hermes.pyth.network/v2/updates/price/latest?${q}&encoding=base64`
  );
  if (!res.ok) throw new Error(`Pyth Hermes fetch failed: ${res.status}`);
  const j = await res.json();
  return j?.binary?.data ?? [];
}

/** Keep the first n guardian signatures of a wormhole VAA (tx-size limit). */
function trimSignatures(vaa: Buffer, n: number): Buffer {
  const current = vaa[5];
  if (n > current) throw new Error('VAA has fewer signatures than requested');
  const trimmed = Buffer.concat([
    vaa.subarray(0, 6 + n * 66),
    vaa.subarray(6 + current * 66),
  ]);
  trimmed[5] = n;
  return trimmed;
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

function borshU16Prefixed(value: Buffer): Buffer {
  const len = Buffer.alloc(2);
  len.writeUInt16LE(value.length, 0);
  return Buffer.concat([len, value]);
}

function borshU16Prefixed20s(items: number[][]): Buffer {
  const count = Buffer.alloc(2);
  count.writeUInt16LE(items.length, 0);
  const body = Buffer.concat(items.map((it) => Buffer.from(it)));
  return Buffer.concat([count, body]);
}

/**
 * Build the receiver postUpdateAtomic instruction for one feed, plus the
 * ephemeral price-update account keypair. Metro-safe: no anchor imports.
 */
export async function buildPythAttachment(
  _connection: Connection,
  payer: PublicKey,
  feedId: string
): Promise<PythAttachment> {
  const [vaaB64] = await fetchPythPriceUpdates([feedId]);
  if (!vaaB64) throw new Error('Pyth: no price update returned for feed');

  const payload = Buffer.from(vaaB64, 'base64');
  const parsed = parseAccumulatorUpdateData(payload);
  const innerVaa = Buffer.from(parsed.vaa);
  const trimmedVaa = trimSignatures(innerVaa, REDUCED_GUARDIAN_SET_SIZE);
  const update = parsed.updates[0];
  if (!update) throw new Error('Pyth: no update in accumulator message');

  const guardianSetIndex = trimmedVaa.readUInt32BE(1);
  const [configPda] = pdaConfig();
  const [treasuryPda] = pdaTreasury(TREASURY_ID);
  const [guardianSetPda] = pdaGuardianSet(guardianSetIndex);

  const merkle = Buffer.concat([
    borshU16Prefixed(Buffer.from(update.message)),
    borshU16Prefixed20s(update.proof),
  ]);
  const data = Buffer.concat([
    POST_UPDATE_ATOMIC_DISCRIMINATOR,
    borshBytes(trimmedVaa),
    merkle,
    Buffer.from([TREASURY_ID]),
  ]);

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
