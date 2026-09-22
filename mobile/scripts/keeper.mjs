import fs from 'fs';
import {
  Connection, Keypair, PublicKey, Transaction, TransactionInstruction,
  SystemProgram, SYSVAR_CLOCK_PUBKEY, sendAndConfirmTransaction,
} from '@solana/web3.js';

// Load the repo-root .env (no external deps) so server-side scripts can use
// SOLANA_RPC_URL / HELIUS_RPC_URL without exporting them manually.
function loadEnv() {
  // Try the repo-root .env first (server keys), then the app's mobile/.env
  let envPath = new URL('../../.env', import.meta.url).pathname;
  if (!fs.existsSync(envPath)) envPath = new URL('../.env', import.meta.url).pathname;
  try {
    for (const line of fs.readFileSync(envPath, 'utf8').split('\n')) {
      const m = line.match(/^([A-Z_][A-Z0-9_]*)=(.*)$/);
      if (m && !(m[1] in process.env)) {
        process.env[m[1]] = m[2].trim().replace(/^['"]|['"]$/g, '');
      }
    }
  } catch (_e) { /* optional */ }
}
loadEnv();

const PROGRAM_ID = new PublicKey('HAjGxuih14imCMaWvCnJQ3nSdWmS8PQKzp74gyAgjsH3');
const SKR_MINT = new PublicKey('SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3');
const NATIVE_MINT = new PublicKey('So11111111111111111111111111111111111111112');
const ORACLE_SEED = Buffer.from('oracle');
const ADMIN_SEED = Buffer.from('admin');

const args = process.argv.slice(2);
const network = args.includes('--network') ? args[args.indexOf('--network') + 1] : 'mainnet-beta';
const skrPrice = parseFloat(args.includes('--skr-price') ? args[args.indexOf('--skr-price') + 1] : '0');
const solPriceOverride = parseFloat(args.includes('--sol-price') ? args[args.indexOf('--sol-price') + 1] : '0');

const RPC = network === 'devnet'
  ? 'https://api.devnet.solana.com'
  : process.env.SOLANA_RPC_URL || process.env.HELIUS_RPC_URL || 'https://api.mainnet-beta.solana.com';
// After --rotate-oracle, feeds are signed by the keeper key (the rotated
// oracle_authority), never by the full admin/upgrade key.
const keypairPath = process.env.KEEPER_KEY || process.env.ORACLE_KEY || `${process.env.HOME}/.config/solana/mainnet-keeper.json`;
const keypair = Keypair.fromSecretKey(new Uint8Array(JSON.parse(fs.readFileSync(keypairPath, 'utf8'))));
const conn = new Connection(RPC, 'confirmed');

const w64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };

async function main() {
  const [adminPda] = PublicKey.findProgramAddressSync([ADMIN_SEED], PROGRAM_ID);

  const solUsd = solPriceOverride > 0
    ? solPriceOverride
    : await fetchJupUsdPrice(NATIVE_MINT.toBase58()).catch(() => fetchUsdPrice('solana'));
  await setFeed(NATIVE_MINT, Math.round(solUsd * 1e6), 9, adminPda, `SOL $${solUsd}`);

  // SKR is listed on Jupiter (jup.ag/tokens/SKRbvo6Gf…); default to the live
  // market price unless --skr-price is passed as a manual override.
  if (skrPrice > 0 || network === 'mainnet-beta') {
    const skrUsd = skrPrice > 0
      ? skrPrice
      : await fetchJupUsdPrice(SKR_MINT.toBase58()).catch(() => 0);
    if (skrUsd > 0) {
      await setFeed(SKR_MINT, Math.round(skrUsd * 1e6), 6, adminPda, `SKR $${skrUsd}`);
    } else {
      console.log('  SKR price unavailable from Jupiter — feed left unchanged.');
    }
  }
  console.log(`${new Date().toISOString()} keeper run complete (${network})`);
}

async function setFeed(mint, priceMicroUsd, decimals, adminPda, label) {
  const [oraclePda] = PublicKey.findProgramAddressSync([ORACLE_SEED, mint.toBuffer()], PROGRAM_ID);
  const data = Buffer.concat([Buffer.from([12]), w64(priceMicroUsd), Buffer.from([decimals])]);
  try {
    const sig = await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: keypair.publicKey, isSigner: true, isWritable: false },
        { pubkey: oraclePda, isSigner: false, isWritable: true },
        { pubkey: mint, isSigner: false, isWritable: false },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
        { pubkey: adminPda, isSigner: false, isWritable: false },
      ],
      data,
    })), [keypair], { commitment: 'confirmed' });
    console.log(`  feed updated (${label}): ${sig}`);
  } catch (e) {
    console.error(`  FAILED to update ${label}: ${e.message}`);
  }
}

async function fetchJupUsdPrice(mint) {
  const base = (process.env.JUPITER_API_URL || 'https://api.jup.ag').replace(/\/+$/, '');
  const headers = process.env.JUPITER_API_KEY ? { 'x-api-key': process.env.JUPITER_API_KEY } : {};
  const res = await fetch(`${base}/price/v3?ids=${mint}`, { headers });
  if (!res.ok) throw new Error(`Jupiter price failed: ${res.status}`);
  const j = await res.json();
  const usd = j?.[mint]?.usdPrice;
  if (!usd) throw new Error(`No Jupiter price for ${mint}`);
  return Number(usd);
}

async function fetchUsdPrice(coingeckoId) {
  const res = await fetch(`https://api.coingecko.com/api/v3/simple/price?ids=${coingeckoId}&vs_currencies=usd`);
  if (!res.ok) throw new Error(`CoinGecko failed: ${res.status}`);
  const j = await res.json();
  const price = j[coingeckoId]?.usd;
  if (!price) throw new Error(`No price for ${coingeckoId}`);
  return price;
}

main().catch((e) => { console.error('FAILED:', e.message || e); process.exit(1); });
