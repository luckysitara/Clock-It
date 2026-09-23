# ClockLend — Mainnet Runbook

Status: program v7 (F1–F5 + hardening batch + F7 typed `is_oracle_free` + fuzz invariant
suite + round-8: Full-only Pyth verification, ±staleness bounds, mint allowlist,
admin-rotation fix). **Devnet runs the pre-round-8 build** (hash `600fba4a`) and must be
re-upgraded from the current tree before devnet testing reflects the oracle hardening.
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
- [ ] **Fund the deployer wallet with ~2.0 SOL on mainnet-beta.** Breakdown: program rent
      **1.5246 SOL** (299,952-byte ProgramData — permanent; the staging buffer's lamports
      are transferred into the ProgramData by `deploy --buffer`, there is NO refund),
      PDAs + feeds + first desk ~0.02 SOL, fees ~0.005. `solana balance -u m` must show
      ≥ 2.0 (the script enforces a 1.7 SOL floor).
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
- [ ] `cd program && cargo build-sbf && cargo test` (88/88 incl. fuzz invariants).

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

## 2. Pricing (Pyth pull oracles — keeper dormant)

SOL and SKR are priced by **Pyth pull oracles** (feed ids hardcoded in the program:
SOL/USD `0xef0d8b6f…` and SKR/USD `0x38846ec4…`). The program accepts **Full guardian
verification only** and bounds staleness on BOTH sides: freshness 120s (SOL) / 300s
(SKR), plus a +60s future-skew bound (`OraclePriceFromFuture`).

The app fetches updates from Hermes v2 — which now **requires an API key**: set
`EXPO_PUBLIC_HERMES_API_KEY` in `mobile/.env`. Before attaching, the client checks the
VAA's signature count against the on-chain guardian-set quorum and refuses to post
anything that would be stored as `Partial` (which the program rejects); it also refuses
payloads that exceed the transaction size limit.

Fallbacks, in order: **canonical cranked SOL account** (`7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE`,
Full-verified, re-verified by the program — SOL borrows stay Pyth-priced even when
Hermes is down); then the **admin PriceFeed** (emergency path). **SKR has no canonical
account**: SKR-collateral loans fall back to the admin feed, which must stay fresh
(3600s window) — run the keeper below if SKR lending matters and the Pyth path is
unavailable.

Manual admin-feed fallback (only when the Pyth/Hermes path is unavailable):
```bash
KEEPER_KEY=~/.config/solana/mainnet-keeper.json node mobile/scripts/keeper.mjs --network mainnet-beta
```
(`keeper.mjs` reads `KEEPER_KEY`/`ORACLE_KEY` as **keypair file paths**, not base64.)

If the admin feed goes stale for >1 h, Pyth-priced borrows are unaffected; admin-feed
borrows revert with `StaleOraclePrice` (fail-closed, no loss).

## 3. Verify (after deploy)

- `solana program show --url m HAjGxuih14imCMaWvCnJQ3nSdWmS8PQKzp74gyAgjsH3`
  (Data Length may show a larger zero-padded allocation — verify the hash instead):
  fetch the ProgramData account, hash `data[45..45+<local .so size>]` and compare to
  `md5sum program/target/deploy/clock_lend.so`.
- Admin PDA `9vX4JBN…Zn7Hq` contains `CLK_ADMN` with
  `admin = 5avuk58D…Qv29` (deployer) and `oracle_authority = HtiDpTk…JWVzJ` (keeper).
- Global SOL feed PDA `A4hjbxYH…oBXu` (same PDA address on mainnet) is fresh.
- Treasury ATA for `EPjFWdd5` owned by treasury PDA `6yY4P4x2…L4dq` exists.
- The canonical Pyth SOL account `7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE` exists and
  is receiver-owned with `verification_level = Full` (this is the app's Hermes-less
  fallback for SOL borrows).
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
