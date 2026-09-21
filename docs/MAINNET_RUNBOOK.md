# ClockLend — Mainnet Runbook

Status: program v5 (F1–F5 + hardening batch) is **live on devnet**; mainnet bootstrap is
prepared but **not executed** (deployer wallet has 0 mainnet SOL). Follow this checklist in order.

## 0. Pre-flight

- [ ] **Fund the deployer wallet with ~3.5 SOL on mainnet-beta** (program rent ≈ 2.9 SOL +
      buffers + tx fees). `solana balance -u m` must show ≥ 3.5.
- [ ] **Key hygiene decision.** The devnet upgrade authority key
      (`BEmX1nfeZT5i4VpSEeZmhiYxpZ9z4Y1LQLjAtPR9c3re`) is embedded in the mobile app source
      as a legacy pool-lookup fallback. For mainnet, deploy with a **fresh keypair** that has
      never lived in any repo:
      ```bash
      solana-keygen new --outfile ~/.config/solana/mainnet-deployer.json
      export DEPLOYER_KEY=~/.config/solana/mainnet-deployer.json
      ```
      The program derives the admin from the **on-chain upgrade authority** (no hardcoded
      key since F3), so a fresh key works cleanly.
- [ ] **SKR price decision.** The SKR mint exists on mainnet (6 decimals ✓). Decide the
      initial SKR feed price (or run SKR-collateral flows on devnet only).
- [ ] `cd program && cargo build-sbf && cargo test` (74/74).

## 1. Deploy

```bash
cd /home/rootkit/lend
export DEPLOYER_KEY=~/.config/solana/mainnet-deployer.json
node scripts/deploy-mainnet.mjs --skr-price 0.02 --create-pool
```

This, in order: deploys the program (same id `HAjGxuih…`, upgradeable), calls
`InitializeAdmin` with ProgramData proof, creates the treasury USDC ATA
(`EPjFWdd5…`, owner = treasury PDA), publishes the global SOL feed (live CoinGecko price),
optionally the SKR feed, and optionally the first desk.

## 2. Keeper (mandatory — staleness window is 3600 s)

```bash
crontab -e
# */15 * * * * cd /home/rootkit/lend && DEPLOYER_KEY=~/.config/solana/mainnet-deployer.json node scripts/keeper.mjs --network mainnet-beta --skr-price 0.02 >> keeper.log 2>&1
```

If the keeper stops for >1 h, all borrows revert with `StaleOraclePrice` (fail-closed, no
loss). A missing SKR feed blocks SKR-collateral borrows only.

## 3. Verify (after deploy)

- `solana program show --url m HAjGxuih14imCMaWvCnJQ3nSdWmS8PQKzp74gyAgjsH3`
  (Data Length may show a larger zero-padded allocation — verify the hash instead):
  fetch the ProgramData account, hash `data[45..45+<local .so size>]` and compare to
  `md5sum program/target/deploy/clock_lend.so`.
- Admin PDA `9vX4JBN…Zn7Hq` contains `CLK_ADMN` + your key.
- Global SOL feed PDA `A4hjbxYH…oBXu` (same PDA address on mainnet) is fresh.
- Treasury ATA for `EPjFWdd5` owned by treasury PDA `6yY4P4x2…L4dq` exists.

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
