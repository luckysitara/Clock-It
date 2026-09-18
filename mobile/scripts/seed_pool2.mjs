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

function u64LE(val) {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(BigInt(val), 0);
  return buf;
}

const poolId = 2;
const [poolPDA] = PublicKey.findProgramAddressSync(
  [Buffer.from('pool'), payer.publicKey.toBuffer(), u64LE(poolId)],
  PROGRAM_ID
);
const [vaultPDA] = PublicKey.findProgramAddressSync(
  [Buffer.from('vault'), poolPDA.toBuffer()],
  PROGRAM_ID
);

const existingAccount = await connection.getAccountInfo(poolPDA);
if (existingAccount) {
  console.log('Pool #2 already exists!');
  process.exit(0);
}

const data = Buffer.alloc(1 + 8 + 1 + 2 + 2 + 8 + 8 + 32);
let offset = 0;
data.writeUInt8(0, offset); offset += 1; // Variant 0
data.writeBigUInt64LE(BigInt(poolId), offset); offset += 8;
data.writeUInt8(0, offset); offset += 1; // 0 = Individual
data.writeUInt16LE(800, offset); offset += 2; // 8.0%
data.writeUInt16LE(8000, offset); offset += 2; // 80% LTV
data.writeBigInt64LE(BigInt(86400 * 3), offset); offset += 8;
data.writeBigInt64LE(BigInt(86400 * 30), offset); offset += 8;

const nameBuf = Buffer.from('Tokyo Whale Desk');
nameBuf.copy(data, offset);

const liquidityMint = new PublicKey('EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v');

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
const sig = await connection.sendTransaction(tx, [payer]);
await connection.confirmTransaction(sig, 'confirmed');
console.log('🎉 Created Pool #2 on Devnet:', poolPDA.toBase58(), 'Sig:', sig);
