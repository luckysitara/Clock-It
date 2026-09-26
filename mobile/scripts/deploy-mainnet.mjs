import fs from 'fs';
import {
  Connection, Keypair, PublicKey, Transaction, TransactionInstruction,
  SystemProgram, SYSVAR_RENT_PUBKEY, SYSVAR_CLOCK_PUBKEY,
  sendAndConfirmTransaction,
} from '@solana/web3.js';
import { execSync } from 'child_process';

// No @solana/spl-token dependency: the ATA helpers are hand-rolled (the same
// derivation the mobile app uses) so the script runs on a bare checkout.
function getAssociatedTokenAddress(mint, owner) {
  const [address] = PublicKey.findProgramAddressSync(
    [owner.toBuffer(), TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    ASSOC_TOKEN_PROGRAM
  );
  return address;
}
function createAssociatedTokenAccountIdempotentInstruction(payer, ata, owner, mint) {
  return new TransactionInstruction({
    programId: ASSOC_TOKEN_PROGRAM,
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
const TOKEN_PROGRAM_ID = new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
const ASSOC_TOKEN_PROGRAM = new PublicKey('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL');
const USDC_MAINNET_MINT = new PublicKey('EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v');
const SKR_MINT = new PublicKey('SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3');
const NATIVE_MINT = new PublicKey('So11111111111111111111111111111111111111112');
const BPF_LOADER = new PublicKey('BPFLoaderUpgradeab1e11111111111111111111111');
const ORACLE_SEED = Buffer.from('oracle');
const POOL_SEED = Buffer.from('pool');
const VAULT_SEED = Buffer.from('vault');
const ADMIN_SEED = Buffer.from('admin');
const TREASURY_SEED = Buffer.from('treasury');

const RPC = process.env.MAINNET_RPC || process.env.SOLANA_RPC_URL || process.env.HELIUS_RPC_URL || 'https://api.mainnet-beta.solana.com';
const keypairPath = process.env.DEPLOYER_KEY || `${process.env.HOME}/.config/solana/mainnet-deployer.json`;
const keeperKeyPath = process.env.ORACLE_KEY || `${process.env.HOME}/.config/solana/mainnet-keeper.json`;
// Cluster for the solana CLI subprocess calls (web3.js calls always use RPC).
// Set SOLANA_CLUSTER=devnet for a dry run.
const CLI_NETWORK = process.env.SOLANA_CLUSTER || 'mainnet-beta';
const keypair = Keypair.fromSecretKey(new Uint8Array(JSON.parse(fs.readFileSync(keypairPath, 'utf8'))));
const conn = new Connection(RPC, 'confirmed');

const w64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const w64s = (n) => { const b = Buffer.alloc(8); b.writeBigInt64LE(BigInt(n)); return b; };
const u16 = (n) => { const b = Buffer.alloc(2); b.writeUInt16LE(n); return b; };

// Resolve the program-id keypair. A FIRST deploy (the program account does not
// exist on the cluster) requires the keypair for the program id — the CLI
// rejects a bare address for initial deployments. Upgrades accept an address.
function resolveProgramKeypairPath() {
  const candidates = [
    process.env.PROGRAM_KEYPAIR,
    `${process.env.HOME}/.config/solana/clock-lend-program.json`,
    new URL('../program/target/deploy/clock_lend-keypair.json', import.meta.url).pathname,
  ].filter(Boolean);
  for (const p of candidates) {
    if (!fs.existsSync(p)) continue;
    try {
      const pub = execSync(`solana-keygen pubkey ${p}`, { encoding: 'utf8' }).trim();
      if (pub === PROGRAM_ID.toBase58()) return p;
    } catch (_e) { /* try next candidate */ }
  }
  return undefined;
}

async function main() {
  const args = process.argv.slice(2);
  const skrPrice = parseFloat(args[args.indexOf('--skr-price') + 1] || '0');
  const createPool = args.includes('--create-pool');
  const rotateOracle = args.includes('--rotate-oracle');
  const unknown = args.filter((a) => a.startsWith('--') && !['--create-pool', '--rotate-oracle', '--skr-price', '--skip-build'].includes(a));
  if (unknown.length) throw new Error(`Unknown flags: ${unknown.join(', ')}`);

  // C-3 preflight: fail BEFORE spending lamports if the program-id keypair is
  // not on this machine (first deploy) — the CLI's error comes only after the
  // write-buffer step, which wastes rent and leaves a partial bootstrap.
  const programKeypairPath = resolveProgramKeypairPath();
  const programExists = await (async () => {
    try { return !!(await conn.getAccountInfo(PROGRAM_ID)); } catch (_e) { return false; }
  })();
  if (!programExists && !programKeypairPath) {
    throw new Error(
      `Program account ${PROGRAM_ID.toBase58()} does not exist on ${CLI_NETWORK} and no keypair for it ` +
      'was found (checked PROGRAM_KEYPAIR, ~/.config/solana/clock-lend-program.json, ' +
      'program/target/deploy/clock_lend-keypair.json). Recover the keypair from backup and set ' +
      'PROGRAM_KEYPAIR=/path/to/keypair.json — or pick a new program id and update lib.rs/program.ts.'
    );
  }

  // 1. Deploy the program (write-buffer + upgrade) — same program id on mainnet
  const soPath = new URL('../program/target/deploy/clock_lend.so', import.meta.url).pathname;

  // C-3 (round 9): build + freshness preflight. Shipping a stale ELF silently
  // omits whatever the sources gained since the last build. Hard-fail unless --skip-build is passed.
  const skipBuild = args.includes('--skip-build');
  if (!skipBuild) {
    console.log('Building the program from source (cargo build-sbf)...');
    execSync(`cd ${new URL('../program', import.meta.url).pathname} && cargo build-sbf`, { stdio: 'inherit' });
  }
  if (!fs.existsSync(soPath)) {
    throw new Error(`${soPath} not found — run: cd program && cargo build-sbf`);
  }
  const soStat = fs.statSync(soPath);
  let newestSource = 0;
  for (const dir of ['src']) {
    const walk = (d) => {
      for (const e of fs.readdirSync(d, { withFileTypes: true })) {
        const p = `${d}/${e.name}`;
        if (e.isDirectory()) walk(p);
        else if (/\.[a-z]+$/i.test(e.name)) newestSource = Math.max(newestSource, fs.statSync(p).mtimeMs);
      }
    };
    const root = new URL(`../program/${dir}`, import.meta.url).pathname;
    if (fs.existsSync(root)) walk(root);
  }
  const tomlPath = new URL('../program/Cargo.toml', import.meta.url).pathname;
  const lockPath = new URL('../program/Cargo.lock', import.meta.url).pathname;
  newestSource = Math.max(newestSource, fs.statSync(tomlPath).mtimeMs);
  if (fs.existsSync(lockPath)) newestSource = Math.max(newestSource, fs.statSync(lockPath).mtimeMs);
  if (soStat.mtimeMs < newestSource) {
    throw new Error(
      `${soPath} is OLDER than the program sources — a stale build would ship silently. ` +
      'Rebuild (cargo build-sbf) or pass --skip-build only if you are certain.'
    );
  }
  const soMd5 = execSync(`md5sum ${soPath}`, { encoding: 'utf8' }).split(' ')[0];
  console.log(`Artifact: ${soPath} (${soStat.size} bytes, md5 ${soMd5})`);

  // Dynamic rent preflight: calculate exact required rent for the ProgramData buffer (size + 45 bytes header)
  // plus 0.1 SOL buffer for write-buffer transaction fees and priority fees.
  const requiredRent = await conn.getMinimumBalanceForRentExemption(soStat.size + 45);
  const deployFloor = requiredRent + 100_000_000;
  const balance = await conn.getBalance(keypair.publicKey);
  console.log(`Deployer ${keypair.publicKey.toBase58()} balance: ${(balance / 1e9).toFixed(4)} SOL (min needed: ${(deployFloor / 1e9).toFixed(4)} SOL, rent: ${(requiredRent / 1e9).toFixed(4)} SOL)`);
  if (balance < deployFloor) {
    throw new Error(
      `Insufficient SOL — deployer has ${(balance / 1e9).toFixed(4)} SOL, but required rent is ${(requiredRent / 1e9).toFixed(4)} SOL ` +
      `(${(deployFloor / 1e9).toFixed(4)} SOL needed with tx buffer). Fund the deployer wallet first.`
    );
  }

  console.log('Writing program buffer...');
  const bufOut = execSync(`solana program write-buffer --url ${CLI_NETWORK} --keypair ${keypairPath} ${soPath}`, { encoding: 'utf8' });
  const bufMatch = bufOut.match(/Buffer: (\w+)/);
  if (!bufMatch) throw new Error(`Could not parse buffer from write-buffer output:\n${bufOut}`);
  const buffer = bufMatch[1];
  console.log('Upgrading program...');
  const programIdArg = programKeypairPath || PROGRAM_ID.toBase58();
  execSync(`solana program deploy --url ${CLI_NETWORK} --keypair ${keypairPath} --program-id ${programIdArg} --buffer ${buffer}`, { stdio: 'inherit' });
  // No close needed: `deploy --buffer` transfers the buffer's lamports into the
  // new/upgraded ProgramData as its rent — nothing is left to refund.

  // 2. InitializeAdmin (sole root = on-chain upgrade authority via ProgramData)
  const [adminPda] = PublicKey.findProgramAddressSync([ADMIN_SEED], PROGRAM_ID);
  const [programDataPda] = PublicKey.findProgramAddressSync([PROGRAM_ID.toBuffer()], BPF_LOADER);
  console.log('Initializing AdminConfig...');
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: keypair.publicKey, isSigner: true, isWritable: true },
      { pubkey: adminPda, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: programDataPda, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([13]), // InitializeAdmin (tag 13)
  })), [keypair], { commitment: 'confirmed' });
  console.log(`Admin initialized: ${adminPda.toBase58()}`);

  // 2b. Optional: rotate the oracle authority to a separate, lower-privilege
  // keeper key so the price keeper never needs the full admin/upgrade key.
  // InitializeAdmin's rotation is authorized by the upgrade-authority proof
  // above; trailing accounts [new_admin, new_oracle] set the roles.
  if (rotateOracle) {
    const keeperPubkey = Keypair.fromSecretKey(
      new Uint8Array(JSON.parse(fs.readFileSync(keeperKeyPath, 'utf8')))
    ).publicKey;
    console.log(`Rotating oracle_authority to keeper key ${keeperPubkey.toBase58()}...`);
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: keypair.publicKey, isSigner: true, isWritable: true }, // writable: first-time feed creation pays rent
        { pubkey: adminPda, isSigner: false, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: programDataPda, isSigner: false, isWritable: false },
        { pubkey: keypair.publicKey, isSigner: false, isWritable: false }, // keep admin
        { pubkey: keeperPubkey, isSigner: false, isWritable: false },     // new oracle authority
      ],
      data: Buffer.from([13]), // InitializeAdmin (rotation)
    })), [keypair], { commitment: 'confirmed' });
    console.log('oracle_authority rotated. Keeper may sign feeds with the keeper key only.');
  }

  // 3. Create the treasury USDC token account (owner = treasury PDA)
  const [treasuryPda] = PublicKey.findProgramAddressSync([TREASURY_SEED], PROGRAM_ID);
  const treasuryAta = await getAssociatedTokenAddress(USDC_MAINNET_MINT, treasuryPda);
  console.log('Creating treasury USDC ATA...');
  await sendAndConfirmTransaction(conn, new Transaction().add(
    createAssociatedTokenAccountIdempotentInstruction(
      keypair.publicKey, treasuryAta, treasuryPda, USDC_MAINNET_MINT
    )
  ), [keypair], { commitment: 'confirmed' });
  console.log(`Treasury USDC ATA: ${treasuryAta.toBase58()}`);

  // 4. Publish the global SOL price feed (Jupiter first, CoinGecko fallback)
  const solUsd = await fetchJupUsdPrice(NATIVE_MINT.toBase58()).catch(() => fetchUsdPrice('solana'));
  console.log(`Publishing SOL feed at $${solUsd}...`);
  await setFeed(NATIVE_MINT, Math.round(solUsd * 1e6), 9, adminPda);

  // 5. Publish the SKR feed — Jupiter market price by default (SKR is listed
  // at jup.ag/tokens/SKRbvo6Gf…), --skr-price overrides manually.
  const skrUsd = skrPrice > 0
    ? skrPrice
    : await fetchJupUsdPrice(SKR_MINT.toBase58()).catch(() => 0);
  if (skrUsd > 0) {
    console.log(`Publishing SKR feed at $${skrUsd}...`);
    await setFeed(SKR_MINT, Math.round(skrUsd * 1e6), 6, adminPda);
  } else {
    console.log('SKR feed skipped — no Jupiter price and no --skr-price override.');
  }

  // 6. Optionally create the first desk (mainnet USDC)
  if (createPool) {
    const poolId = 1n;
    const [poolPda] = PublicKey.findProgramAddressSync(
      [POOL_SEED, keypair.publicKey.toBuffer(), w64(poolId)], PROGRAM_ID);
    const [vaultPda] = PublicKey.findProgramAddressSync([VAULT_SEED, poolPda.toBuffer()], PROGRAM_ID);
    const name = Buffer.alloc(32);
    Buffer.from('Seeker Genesis Circle').copy(name);
    const data = Buffer.concat([
      Buffer.from([0]), w64(poolId), Buffer.from([1]), u16(350), u16(9000),
      w64s(3 * 86400), w64s(30 * 86400), name,
      Buffer.from([0]), // is_oracle_free: false — require a live oracle (63rd byte, added in F7)
    ]);
    console.log('Creating "Seeker Genesis Circle" desk...');
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: keypair.publicKey, isSigner: true, isWritable: true },
        { pubkey: poolPda, isSigner: false, isWritable: true },
        { pubkey: USDC_MAINNET_MINT, isSigner: false, isWritable: false },
        { pubkey: vaultPda, isSigner: false, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: SYSVAR_RENT_PUBKEY, isSigner: false, isWritable: false },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      ],
      data,
    })), [keypair], { commitment: 'confirmed' });
    console.log(`Desk created: ${poolPda.toBase58()}`);
  }

  // 7. Initialize the SKR yield vault (admin-gated, tag 15). The vault funds
  // itself via the 50/50 origination-fee split once the app appends its PDAs.
  console.log('Initializing SKR yield vault (reward mint = USDC)...');
  {
    const [yieldVaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from('skr_yield_vault'), USDC_MAINNET_MINT.toBuffer()], PROGRAM_ID);
    const [yieldTokenPda] = PublicKey.findProgramAddressSync(
      [Buffer.from('skr_yield_token'), USDC_MAINNET_MINT.toBuffer()], PROGRAM_ID);
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: keypair.publicKey, isSigner: true, isWritable: true },
        { pubkey: yieldVaultPda, isSigner: false, isWritable: true },
        { pubkey: USDC_MAINNET_MINT, isSigner: false, isWritable: false },
        { pubkey: yieldTokenPda, isSigner: false, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: SYSVAR_RENT_PUBKEY, isSigner: false, isWritable: false },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        { pubkey: adminPda, isSigner: false, isWritable: false },
      ],
      data: Buffer.from([15]),
    })), [keypair], { commitment: 'confirmed' });
    console.log(`SkrYieldVault: ${yieldVaultPda.toBase58()} (token ${yieldTokenPda.toBase58()})`);
  }

  console.log('\nMAINNET BOOTSTRAP COMPLETE');
  console.log('Pricing is ADMIN-FEED-ONLY (Pyth removed): the feeds just published are the sole');
  console.log('price source, bound to 600s freshness. GET THE KEEPER RUNNING NOW:');
  console.log('  KEEPER_KEY=~/.config/solana/mainnet-keeper.json node mobile/scripts/keeper.mjs --network mainnet-beta');
  console.log('Schedule it at <10-minute cadence (cron or .github/workflows/keeper.yml) or borrows');
  console.log('revert with StaleOraclePrice after 600s.');
}

async function setFeed(mint, priceMicroUsd, decimals, adminPda) {
  const [oraclePda] = PublicKey.findProgramAddressSync([ORACLE_SEED, mint.toBuffer()], PROGRAM_ID);
  const data = Buffer.concat([Buffer.from([12]), w64(priceMicroUsd), Buffer.from([decimals])]);
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: keypair.publicKey, isSigner: true, isWritable: true }, // writable: first-time feed creation pays rent
      { pubkey: oraclePda, isSigner: false, isWritable: true },
      { pubkey: mint, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
      { pubkey: adminPda, isSigner: false, isWritable: false },
    ],
    data,
  })), [keypair], { commitment: 'confirmed' });
  console.log(`Feed set: ${oraclePda.toBase58()} = ${priceMicroUsd} micro-USD (${decimals} decimals)`);
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
