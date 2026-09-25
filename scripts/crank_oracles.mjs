import fs from 'fs';
import path from 'path';

// Portable web3 import: resolve standard node module or relative from mobile
let web3;
try {
  web3 = await import('@solana/web3.js');
} catch (_e) {
  web3 = await import('../mobile/node_modules/@solana/web3.js/lib/index.cjs.js');
}

const {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
  SystemProgram,
  sendAndConfirmTransaction,
} = web3;

// Load repository .env if present
function loadEnv() {
  const candidates = [
    new URL('../.env', import.meta.url).pathname,
    new URL('../mobile/.env', import.meta.url).pathname,
  ];
  for (const envPath of candidates) {
    if (!fs.existsSync(envPath)) continue;
    try {
      for (const line of fs.readFileSync(envPath, 'utf8').split('\n')) {
        const m = line.match(/^([A-Z_][A-Z0-9_]*)=(.*)$/);
        if (m && !(m[1] in process.env)) {
          process.env[m[1]] = m[2].trim().replace(/^['"]|['"]$/g, '');
        }
      }
    } catch (_e) {}
  }
}
loadEnv();

const PROGRAM_ID = new PublicKey('HAjGxuih14imCMaWvCnJQ3nSdWmS8PQKzp74gyAgjsH3');
const NATIVE_SOL_MINT = new PublicKey('So11111111111111111111111111111111111111112');
const SKR_MINT = new PublicKey('SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3');

const RPC = process.env.CRANK_RPC || 'https://api.devnet.solana.com'; // explicit: the repo .env carries a MAINNET rpc
const conn = new Connection(RPC, 'confirmed');

// Explicit key only — never default to the CLI god key (id.json), which on
// many machines is a funded mainnet key. Usage:
//   ORACLE_KEY=~/.config/solana/id.json node scripts/crank_oracles.mjs   # devnet only
const keypairPath = process.env.ORACLE_KEY || process.env.KEEPER_KEY;
if (!keypairPath || !fs.existsSync(keypairPath)) {
  throw new Error(`Oracle keypair not found at ${keypairPath}. Set ORACLE_KEY or KEEPER_KEY.`);
}
const secretKey = Uint8Array.from(JSON.parse(fs.readFileSync(keypairPath, 'utf-8')));
const adminKeypair = Keypair.fromSecretKey(secretKey);

function writeU64LE(val) {
  const buf = Buffer.alloc(8);
  const big = BigInt(val);
  buf.writeUInt32LE(Number(big & 0xffffffffn), 0);
  buf.writeUInt32LE(Number((big >> 32n) & 0xffffffffn), 4);
  return buf;
}

export async function crankOraclePrices() {
  const genesis = await conn.getGenesisHash();
  if (genesis !== 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG') {
    throw new Error(`REFUSING TO RUN: RPC ${RPC} is not devnet (genesis ${genesis})`);
  }
  console.log('=== ClockLend Live Oracle Crank ===');
  console.log('Admin:', adminKeypair.publicKey.toBase58());
  console.log('RPC:', RPC);

  const [adminPDA] = PublicKey.findProgramAddressSync([Buffer.from('admin')], PROGRAM_ID);
  const [solOraclePDA] = PublicKey.findProgramAddressSync([Buffer.from('oracle'), NATIVE_SOL_MINT.toBuffer()], PROGRAM_ID);
  const [skrOraclePDA] = PublicKey.findProgramAddressSync([Buffer.from('oracle'), SKR_MINT.toBuffer()], PROGRAM_ID);

  // 1. Fetch live market price from Jupiter
  const jupKey = process.env.JUPITER_API_KEY || process.env.EXPO_PUBLIC_JUPITER_API_KEY;
  const headers = jupKey ? { 'x-api-key': jupKey } : {};

  let solPrice = null;
  let skrPrice = null;

  try {
    const jupRes = await fetch(
      'https://api.jup.ag/price/v3?ids=So11111111111111111111111111111111111111112,SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3',
      { headers }
    );
    if (jupRes.ok) {
      const jupData = await jupRes.json();
      if (jupData['So11111111111111111111111111111111111111112']?.usdPrice) {
        solPrice = Number(jupData['So11111111111111111111111111111111111111112'].usdPrice);
      }
      if (jupData['SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3']?.usdPrice) {
        skrPrice = Number(jupData['SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3'].usdPrice);
      }
    } else {
      console.warn(`Jupiter fetch failed with status ${jupRes.status}: ${jupRes.statusText}`);
    }
  } catch (err) {
    console.warn('Jupiter fetch error, trying CoinGecko fallback:', err);
  }

  // Fallback to CoinGecko if Jupiter didn't supply both prices
  if (solPrice == null || skrPrice == null) {
    try {
      const cgRes = await fetch('https://api.coingecko.com/api/v3/simple/price?ids=solana,seeker&vs_currencies=usd');
      if (cgRes.ok) {
        const cgData = await cgRes.json();
        if (solPrice == null && cgData?.solana?.usd) solPrice = Number(cgData.solana.usd);
        if (skrPrice == null && cgData?.seeker?.usd) skrPrice = Number(cgData.seeker.usd);
      } else {
        console.warn(`CoinGecko fetch failed with status ${cgRes.status}: ${cgRes.statusText}`);
      }
    } catch (err) {
      console.warn('CoinGecko fetch error:', err);
    }
  }

  if (solPrice == null || skrPrice == null || isNaN(solPrice) || isNaN(skrPrice) || solPrice <= 0 || skrPrice <= 0) {
    throw new Error(`Unable to fetch valid live market prices for oracle crank. SOL: ${solPrice}, SKR: ${skrPrice}. Refusing to publish stale/mock prices.`);
  }

  console.log(`Target Oracle Prices: SOL=$${solPrice}, SKR=$${skrPrice}`);

  const solPriceMicro = BigInt(Math.round(solPrice * 1e6));
  const skrPriceMicro = BigInt(Math.round(skrPrice * 1e6));

  // Variant 12: SetPriceFeed
  const solData = Buffer.alloc(10);
  solData.writeUInt8(12, 0);
  writeU64LE(solPriceMicro).copy(solData, 1);
  solData.writeUInt8(9, 9); // 9 decimals

  const solIx = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: adminKeypair.publicKey, isSigner: true, isWritable: true },
      { pubkey: solOraclePDA, isSigner: false, isWritable: true },
      { pubkey: NATIVE_SOL_MINT, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: adminPDA, isSigner: false, isWritable: false },
    ],
    data: solData,
  });

  const skrData = Buffer.alloc(10);
  skrData.writeUInt8(12, 0);
  writeU64LE(skrPriceMicro).copy(skrData, 1);
  skrData.writeUInt8(6, 9); // 6 decimals

  const skrIx = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: adminKeypair.publicKey, isSigner: true, isWritable: true },
      { pubkey: skrOraclePDA, isSigner: false, isWritable: true },
      { pubkey: SKR_MINT, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: adminPDA, isSigner: false, isWritable: false },
    ],
    data: skrData,
  });

  const tx = new Transaction().add(solIx, skrIx);
  const sig = await sendAndConfirmTransaction(conn, tx, [adminKeypair]);
  console.log(`Success! Devnet Oracle PDAs updated to SOL=$${solPrice}, SKR=$${skrPrice}. Tx: ${sig}`);
}

if (process.argv[1] && import.meta.url.endsWith(process.argv[1].split('/').pop())) {
  crankOraclePrices().catch((e) => {
    console.error('FAILED:', e.message || e);
    process.exit(1);
  });
}
