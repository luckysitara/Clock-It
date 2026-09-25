# ClockLend — Mainnet Runbook

Status: program v10 (round-8 hardening + round-9 SKR yield vault + round-10 reaudit fixes:
immediate StakeSKR yield registration, unstake clamping & slot-collision fix, ClaimDefault
yield down-sync, admin vault-authority rotation, Hermes fetch timeout & price provenance).
**Devnet runs the round-10 build** — ProgramData `76MvPRVKdsthzyBQhFnfqgd7QtjebiAGCLNTA56nCgMB`,
ELF 354,712 bytes, md5 `540edc5c` (byte-verified against the local .so; upgrade tx
`4L2nAvLzqP2qTrh9JdswRj5kCNpB4vYcd3VxzfUXyKj6Kno6TzvkPqr9wwjn6vegY2UkS1qBtLr3K3Zfvy5KDAMy`).
Mainnet bootstrap is prepared but **not executed** (deployer wallet has 0 mainnet SOL).
Follow this checklist in order.

## 0. Pre-flight

- [ ] **Recover the program-id keypair for `HAjGxuih…`.** A FIRST deploy (the program
      account does not exist on mainnet) requires the keypair file — the CLI rejects a
      bare address for initial deployments. It is NOT on the deploy box (the build
      regenerates `program/target/deploy/clock_lend-keypair.json` with a random key, which
      is wrong). Restore it from backup and export `PROGRAM_KEYPAIR=<path>` — the deploy
      script now preflights it and aborts before spending lamports if it is missing.
      (If it is truly lost: pick a new program id and update `lib.rs declare_id!`,
      `program.ts`, this runbook, and all seed scripts.)
- [ ] **Fund the deployer wallet with ~2.5 SOL on mainnet-beta.** Breakdown: program rent
      is dynamically verified by `deploy-mainnet.mjs` against `getMinimumBalanceForRentExemption(soStat.size + 45)`
      (~1.73 SOL for the compiled ELF + 45-byte ProgramData header; permanent buffer transfer),
      plus ~0.1 SOL buffer for write-buffer transaction fees and priority fees,
      PDAs + feeds + first desk ~0.05 SOL. `solana balance -u m` should show
      ≥ 2.0 SOL (script dynamically enforces `rent + 0.1 SOL`).
- [x] **Fresh mainnet keypairs generated** (`~/.config/solana/`, chmod 600):
      - Deployer (upgrade authority / admin): `5avuk58DjBwBsyWkhgp6efC5WbnUKTFA5iLkbS8Aqv29`
      - Keeper (oracle_authority after `--rotate-oracle`): `HtiDpTkcWDDaQeRLSBvYDdw2sRJb5VvkD7EMvr5JWVzJ`
      ```bash
      export DEPLOYER_KEY=~/.config/solana/mainnet-deployer.json
      export ORACLE_KEY=~/.config/solana/mainnet-keeper.json
      ```
      The program derives the admin from the **on-chain upgrade authority** (no hardcoded
      key since F3), so the fresh key works cleanly.
- [ ] **SKR price source — RESOLVED.** SKR is listed on Jupiter with a real market
      (jup.ag/tokens/SKRbvo6Gf…, ~$0.021 at last check, ~$766K liquidity). The deploy
      script seeds the SKR feed from Jupiter's Price API v3 automatically; the keeper
      refreshes both SOL and SKR feeds from the same source every run.
      `--skr-price <usd>` still overrides manually if you want a policy floor.
- [ ] `cd program && cargo build-sbf && cargo test` (97/97 incl. fuzz invariants).
      Note: the deploy script now builds the ELF itself (`cargo build-sbf`) and hard-fails
      if the artifact is older than the sources — `--skip-build` overrides.

## 1. Deploy

```bash
cd /home/rootkit/lend
export DEPLOYER_KEY=~/.config/solana/mainnet-deployer.json
export ORACLE_KEY=~/.config/solana/mainnet-keeper.json
node mobile/scripts/deploy-mainnet.mjs --create-pool --rotate-oracle
```

This, in order: deploys the program (same id `HAjGxuih…`, upgradeable), calls
`InitializeAdmin` with ProgramData proof (admin = deployer key), rotates
`oracle_authority` to the keeper key, creates the treasury USDC ATA (`EPjFWdd5…`,
owner = treasury PDA), publishes the global SOL and SKR feeds (Jupiter Price API v3,
CoinGecko fallback for SOL), and creates the first desk.

## 2. Pricing (admin PriceFeed — keeper cranked, no Pyth)

Pyth pull oracles were REMOVED in round 11 (unreliable anchor, dead atomic path,
borrower-picks-the-price risk). Pricing is now **admin-feed-only**:

- The program prices borrows and P2P offers from the global / pool-scoped
  `PriceFeed` accounts, kept fresh by the keeper crank (`scripts/crank_oracles.mjs`,
  devnet) / `mobile/scripts/keeper.mjs` (mainnet), fed by **Jupiter + CoinGecko**
  (the same sources drive the app's WebSocket price pipeline).
- **Pricing staleness is 600s** regardless of the feed's stored 3600s window
  (round 11) — keep the crank cadence under 10 minutes. There is no scheduler in
  the tree: run it from cron/Actions with the exact command below.
- Pool authorities can publish **pool-scoped feeds** (`[b"oracle", pool, mint]`,
  via SetPriceFeed with the pool account appended); when `has_custom_oracle` is
  set the pool-scoped feed is REQUIRED and authoritative (a borrower cannot
  substitute another source — the round-11 precedence fix).
- Oracle-free pools use hardcoded baselines ($150/SOL, $0.02/SKR) and are
  **capped at 30% LTV** (round 11).
- Feed decimals are cross-checked against the mints on both sides (C-1 + the
  round-11 pool-side mirror).

Manual crank (devnet):
```bash
CRANK_RPC=https://api.devnet.solana.com ORACLE_KEY=~/.config/solana/id.json node scripts/crank_oracles.mjs
```

Mainnet keeper:
```bash
KEEPER_KEY=~/.config/solana/mainnet-keeper.json node mobile/scripts/keeper.mjs --network mainnet-beta
```
Schedule it (cron or Actions) at &lt;10-minute cadence — feeds stale for &gt;600s
revert borrows with `StaleOraclePrice` (fail-closed, no loss).

## 2b. SKR yield vault (dividends)

- The deploy script initializes the vault automatically (admin-gated tag 15, reward
  mint = USDC `EPjFWdd5…`). PDAs: vault `[skr_yield_vault, EPjFWdd5]`, vault token
  `[skr_yield_token, EPjFWdd5]`, user positions `[skr_yield_user, user, EPjFWdd5]`.
- Funding: the app appends the two vault PDAs to every borrow (when the vault is
  initialized), routing **50% of the origination fee** to yield holders — no keeper job.
  Manual alternative: the vault authority (deployer key) may call DepositSkrYield
  (tag 16, authority-gated).
- Claims: tag 17. The stake is read from the SKR escrow token account (the single
  source of truth — unstaking immediately removes shares). A **1-hour cooldown**
  applies after each payout or stake change (`YieldCooldown`, error 40). Deposits made
  while nobody is staked are parked in `unallocated_rewards` and folded into the next
  allocation — never stranded.
- Note: the SKR mint does not exist on devnet, so devnet yield testing requires a
  devnet SKR mint; mainnet has `SKRbvo6Gf…` (6 decimals).

## 3. Verify (after deploy)

- `solana program show --url m HAjGxuih14imCMaWvCnJQ3nSdWmS8PQKzp74gyAgjsH3`
  (Note on verification: ProgramData accounts on Solana retain the maximum historical
  allocated size and do not shrink on upgrade. When comparing bytecode, fetch the ProgramData
  account, slice exactly `data[45..45+<local .so size>]` (offset 45 skips the BPF upgradeable
  metadata header), and compare its hash to `md5sum` or `sha256sum program/target/deploy/clock_lend.so`.
  Any bytes beyond `45 + local_elf_size` are historical zero-padding).
- Admin PDA `9vX4JBN…Zn7Hq` contains `CLK_ADMN` with
  `admin = 5avuk58D…Qv29` (deployer) and `oracle_authority = HtiDpTk…JWVzJ` (keeper).
- Global SOL feed PDA `A4hjbxYH…oBXu` (same PDA address on mainnet) is fresh.
- Treasury ATA for `EPjFWdd5` owned by treasury PDA `6yY4P4x2…L4dq` exists.
- The canonical Pyth SOL account `7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE` exists and
  is receiver-owned with `verification_level = Full` (this is the app's Hermes-less
  fallback for SOL borrows).
- The SkrYieldVault PDA (CLK_SYLD, 121 bytes, `is_initialized = 1`, `authority` =
  deployer key) and its vault token account exist and are owned by the program / token
  program respectively.
- Note: `program/target/deploy/clock_lend-keypair.json` is a build artifact with a RANDOM
  key — it is not the program-id keypair. Keep the real `HAjGxuih…` keypair (and its
  backup) separate from the build tree, and commit `program/Cargo.lock` so the hash check
  builds the same ELF everywhere.

## 4. Risks to accept (documented design decisions)

| Risk | Mitigation |
|---|---|
| Single admin/oracle key can move all global prices | Keep the key offline; keeper uses a **separate** `oracle_authority` (rotate via `InitializeAdmin` with the upgrade-authority key) |
| Price manipulation / stale SOL price | Keeper cadence < 1 h; bounded price (≤ $1M/token) and decimals (1–18); pool-scoped feeds exist for pools that want their own pricing |
| P2P funders must verify the offer's on-chain `liquidity_mint` | Show mint in the UI before funding (mobile TODO) |
| Pool authority keys are irrevocable (no rotation instruction) | Deploy desks with durable keys |
| No LP shares — only the pool authority can deposit/withdraw | Document as single-owner desks (product decision) |
| Upgrade authority = deployer key | Store offline; rotate via `set-upgrade-authority` if needed — `InitializeAdmin` stays valid (ProgramData-derived) |

## 5. Upgrading the program later

```bash
cd program && cargo build-sbf
solana program write-buffer --url mainnet-beta --keypair $DEPLOYER_KEY target/deploy/clock_lend.so
solana program deploy --url mainnet-beta --keypair $DEPLOYER_KEY \
  --program-id HAjGxuih14imCMaWvCnJQ3nSdWmS8PQKzp74gyAgjsH3 --buffer <BUFFER>
```

Note the known CLI quirk: `solana program deploy <file>` can appear to no-op; the
write-buffer + `--buffer` flow is the reliable path.
