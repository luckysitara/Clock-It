import { PublicKey, Connection } from '@solana/web3.js';
import { Buffer } from 'buffer';

export const PROGRAM_ID = new PublicKey('HAjGxuih14imCMaWvCnJQ3nSdWmS8PQKzp74gyAgjsH3');
export const DEVNET_RPC = 'https://api.devnet.solana.com';

export const POOL_SEED = Buffer.from('pool');
export const VAULT_SEED = Buffer.from('vault');
export const LOAN_SEED = Buffer.from('loan');
export const ESCROW_SEED = Buffer.from('escrow');
export const P2P_SEED = Buffer.from('p2p_offer');
export const PROFILE_SEED = Buffer.from('profile');
export const TREASURY_SEED = Buffer.from('treasury');
export const SKR_ESCROW_SEED = Buffer.from('skr_escrow');

export function writeU64LE(val: number | bigint): Buffer {
  const buf = Buffer.alloc(8);
  const big = BigInt(val);
  buf.writeUInt32LE(Number(big & 0xffffffffn), 0);
  buf.writeUInt32LE(Number((big >> 32n) & 0xffffffffn), 4);
  return buf;
}

export function getPoolPDA(authority: PublicKey, poolId: number): [PublicKey, number] {
  const buf = writeU64LE(poolId);
  return PublicKey.findProgramAddressSync([POOL_SEED, authority.toBuffer(), buf], PROGRAM_ID);
}

export function getVaultPDA(poolPDA: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([VAULT_SEED, poolPDA.toBuffer()], PROGRAM_ID);
}

export function getLoanPDA(poolPDA: PublicKey, borrower: PublicKey, loanId: number): [PublicKey, number] {
  const buf = writeU64LE(loanId);
  return PublicKey.findProgramAddressSync([LOAN_SEED, poolPDA.toBuffer(), borrower.toBuffer(), buf], PROGRAM_ID);
}

export function getEscrowPDA(accountPDA: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([ESCROW_SEED, accountPDA.toBuffer()], PROGRAM_ID);
}

export function getProfilePDA(user: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([PROFILE_SEED, user.toBuffer()], PROGRAM_ID);
}

export function getP2POfferPDA(creator: PublicKey, offerId: number): [PublicKey, number] {
  const buf = writeU64LE(offerId);
  return PublicKey.findProgramAddressSync([P2P_SEED, creator.toBuffer(), buf], PROGRAM_ID);
}

export function getTreasuryPDA(): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([TREASURY_SEED], PROGRAM_ID);
}

export function getSkrEscrowPDA(user: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([SKR_ESCROW_SEED, user.toBuffer()], PROGRAM_ID);
}

export const connection = new Connection(DEVNET_RPC, 'confirmed');
