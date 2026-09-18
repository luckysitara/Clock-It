import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
} from '@solana/web3.js';
import fs from 'fs';

const PROGRAM_ID = new PublicKey('HAjGxuih14imCMaWvCnJQ3nSdWmS8PQKzp74gyAgjsH3');
const connection = new Connection('https://api.devnet.solana.com', 'confirmed');

import os from 'os';
import path from 'path';

// Load wallet
const walletPath = process.env.ANCHOR_WALLET || path.join(os.homedir(), '.config/solana/id.json');
const keypairBytes = JSON.parse(fs.readFileSync(walletPath, 'utf-8'));
const payer = Keypair.fromSecretKey(Uint8Array.from(keypairBytes));

console.log('Payer address:', payer.publicKey.toBase58());

// Helper for 8-byte LE buffer
function u64LE(val) {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(BigInt(val), 0);
  return buf;
}

// PDAs
const poolId = 1;
const [poolPDA, poolBump] = PublicKey.findProgramAddressSync(
  [Buffer.from('pool'), payer.publicKey.toBuffer(), u64LE(poolId)],
  PROGRAM_ID
);

const [vaultPDA, vaultBump] = PublicKey.findProgramAddressSync(
  [Buffer.from('vault'), poolPDA.toBuffer()],
  PROGRAM_ID
);

console.log('Pool PDA:', poolPDA.toBase58());
console.log('Vault PDA:', vaultPDA.toBase58());

// Check if pool already exists
const existingAccount = await connection.getAccountInfo(poolPDA);
if (existingAccount) {
  console.log('Pool account already exists on Devnet! Data length:', existingAccount.data.length);
  process.exit(0);
}

// Serialize InitializePool instruction
// Variant tag 0 (1 byte)
// pool_id: u64 (8 bytes)
// pool_type: u8 (1 byte: 1 = Circle)
// interest_rate_bps: u16 (2 bytes: 350 = 3.5%)
// max_ltv_bps: u16 (2 bytes: 9000 = 90%)
// min_duration: i64 (8 bytes: 86400 * 3)
// max_duration: i64 (8 bytes: 86400 * 30)
// name: [u8; 32]

const data = Buffer.alloc(1 + 8 + 1 + 2 + 2 + 8 + 8 + 32);
let offset = 0;
data.writeUInt8(0, offset); offset += 1; // Variant 0
data.writeBigUInt64LE(BigInt(poolId), offset); offset += 8;
data.writeUInt8(1, offset); offset += 1; // Circle
data.writeUInt16LE(350, offset); offset += 2; // 3.5%
data.writeUInt16LE(9000, offset); offset += 2; // 90% LTV
data.writeBigInt64LE(BigInt(86400 * 3), offset); offset += 8;
data.writeBigInt64LE(BigInt(86400 * 30), offset); offset += 8;

const nameBuf = Buffer.from('Seeker Genesis Circle');
nameBuf.copy(data, offset);

const liquidityMint = new PublicKey('EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v'); // USDC

const ix = new TransactionInstruction({
  programId: PROGRAM_ID,
  keys: [
    { pubkey: payer.publicKey, isSigner: true, isWritable: true },
    { pubkey: poolPDA, isSigner: false, isWritable: true },
    { pubkey: liquidityMint, isSigner: false, isWritable: false },
    { pubkey: vaultPDA, isSigner: false, isWritable: true },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    { pubkey: SYSVAR_RENT_PUBKEY, isSigner: false, isWritable: false },
  ],
  data,
});

const tx = new Transaction().add(ix);
console.log('Sending transaction to Solana Devnet...');
const sig = await connection.sendTransaction(tx, [payer]);
console.log('Transaction sent! Signature:', sig);

await connection.confirmTransaction(sig, 'confirmed');
console.log('🎉 Successfully created on-chain LendingPool on Devnet!');
console.log('Explorer link: https://explorer.solana.com/tx/' + sig + '?cluster=devnet');
