// Sequence fuzzer: drives randomized instruction sequences against the program in
// the SVM and asserts cross-account invariants after every step.
//
// Invariants checked after each step:
//   1. vault token balance          >= pool.total_liquidity      (pool solvency)
//   2. profile.staked_skr           == skr_escrow balance        (bond accounting)
//   3. profile.locked_skr           <= profile.staked_skr        (lock <= stake)
//   4. profile.locked_skr           == sum of live loan locks    (lock bookkeeping)
//   5. loan.is_active               == status in {Active, InGracePeriod}
//
// The default run is short so it stays cheap in CI. For a real campaign:
//   FUZZ_STEPS=700 FUZZ_SEED=<n> cargo test --offline --test fuzz_invariants -- --nocapture
// Every failure prints the step and operation; rerun with the same FUZZ_SEED to replay.
//
// Drives randomized instruction sequences against the real program in the SVM and
// asserts cross-account invariants after every step. Reproducible: the seed is
// printed and can be replayed with FUZZ_SEED=<n>.
//
//   FUZZ_SEED=1234 FUZZ_STEPS=500 cargo test --offline --test zz_fuzz_tmp -- --nocapture
//
use clock_lend::{
    instruction::ClockLendInstruction,
    processor::process_instruction,
    state::{LendingPool, LoanOrder, LoanStatus, P2POffer, PoolType, UserProfile, ADMIN_SEED,
            DISCRIMINATOR_LOAN, DISCRIMINATOR_POOL, DISCRIMINATOR_PROFILE,
            ESCROW_SEED, LOAN_SEED, ORACLE_SEED, P2P_SEED, POOL_SEED, PROFILE_SEED, SKR_MINT,
            TREASURY_SEED, USDC_DEVNET_MINT, VAULT_SEED},
};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    program_pack::Pack,
    pubkey::Pubkey,
    sysvar,
};
use solana_program_test::*;
use solana_sdk::{
    account::Account,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};

const USDC: u64 = 1_000_000;
const NATIVE_MINT: Pubkey = spl_token::native_mint::ID;
const SYS: Pubkey = solana_program::system_program::ID;
const N_USERS: usize = 3;

// ---------------------------------------------------------------- rng
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self { Rng(if seed == 0 { 0x9E3779B97F4A7C15 } else { seed }) }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12; x ^= x << 25; x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: u64) -> u64 { if n == 0 { 0 } else { self.next_u64() % n } }
    fn range(&mut self, lo: u64, hi: u64) -> u64 { lo + self.below(hi.saturating_sub(lo) + 1) }
}

// ---------------------------------------------------------------- helpers
fn tok(mint: Pubkey, owner: Pubkey, amount: u64) -> Vec<u8> {
    let mut a = spl_token::state::Account::unpack_unchecked(&[0u8; 165]).unwrap();
    a.mint = mint; a.owner = owner; a.amount = amount;
    a.state = spl_token::state::AccountState::Initialized;
    let mut b = vec![0u8; 165];
    spl_token::state::Account::pack(a, &mut b).unwrap();
    b
}
fn mint_data(d: u8) -> Vec<u8> {
    let mut m = spl_token::state::Mint::unpack_unchecked(&[0u8; 82]).unwrap();
    m.mint_authority = spl_token::solana_program::program_option::COption::None;
    m.supply = u64::MAX; m.decimals = d; m.is_initialized = true;
    let mut b = vec![0u8; 82];
    spl_token::state::Mint::pack(m, &mut b).unwrap();
    b
}

// Mint with a live authority and zero supply, for harnesses that mint tokens
// for real (mint_to overflows on the u64::MAX supply that mint_data packs).
fn mint_data_with_authority(d: u8, auth: Pubkey) -> Vec<u8> {
    let mut m = spl_token::state::Mint::unpack_unchecked(&[0u8; 82]).unwrap();
    m.mint_authority = spl_token::solana_program::program_option::COption::Some(auth);
    m.supply = 0; m.decimals = d; m.is_initialized = true;
    let mut b = vec![0u8; 82];
    spl_token::state::Mint::pack(m, &mut b).unwrap();
    b
}
fn program_data_bytes(auth: &Pubkey) -> Vec<u8> {
    let mut d = vec![0u8; 45];
    d[0..4].copy_from_slice(&3u32.to_le_bytes());
    d[12] = 1;
    d[13..45].copy_from_slice(auth.as_ref());
    d
}
async fn send(bc: &mut BanksClient, ixs: &[Instruction], sg: &[&Keypair],
    pay: &Keypair, bh: solana_program::hash::Hash) -> Result<(), BanksClientError> {
    let mut tx = Transaction::new_with_payer(ixs, Some(&pay.pubkey()));
    let n = tx.message.header.num_required_signatures as usize;
    let need: Vec<&Keypair> = sg.iter().copied()
        .filter(|k| tx.message.account_keys[..n].contains(&k.pubkey())).collect();
    tx.partial_sign(&need, bh);
    bc.process_transaction(tx).await
}
async fn token_amount(bc: &mut BanksClient, pk: Pubkey) -> u64 {
    match bc.get_account(pk).await.unwrap() {
        Some(a) if a.owner == spl_token::id() && a.data.len() >= 165 =>
            spl_token::state::Account::unpack(&a.data).map(|t| t.amount).unwrap_or(0),
        _ => 0,
    }
}

struct User {
    kp: Keypair,
    usdc: Pubkey,
    skr: Pubkey,
    profile: Pubkey,
    skr_escrow: Pubkey,
}

struct Ctx {
    bc: BanksClient, payer: Keypair, bh: solana_program::hash::Hash, pid: Pubkey,
    pool: Pubkey, vault: Pubkey, treasury_tok: Pubkey, sol_oracle: Pubkey,
    lp: Keypair, lp_usdc: Pubkey,      // H-7: only the pool authority may deposit
    users: Vec<User>,
    loans: Vec<(usize, u64, Pubkey, Pubkey)>,   // (user_idx, loan_id, loan_pda, escrow_pda)
    next_loan_id: u64,
    offers: Vec<(usize, u64, Pubkey, Pubkey)>,  // (creator_idx, offer_id, offer_pda, escrow_pda)
    next_offer_id: u64,
    interest_paid: u64,                          // total interest returned to the vault by repayments
}

// ---------------------------------------------------------------- invariant checks
async fn check_invariants(ctx: &mut Ctx, step: usize, op: &str) {
    let pid = ctx.pid;

    // 1. Pool solvency: the vault must hold at least the free liquidity the pool claims.
    let pool = match ctx.bc.get_account(ctx.pool).await.unwrap() {
        Some(a) => LendingPool::unpack_from_slice(&a.data).expect("pool unpack"),
        None => panic!("step {step} ({op}): pool account vanished"),
    };
    let vault_bal = token_amount(&mut ctx.bc, ctx.vault).await;
    assert!(
        vault_bal >= pool.total_liquidity,
        "INVARIANT 1 VIOLATED at step {step} ({op}): vault {vault_bal} < pool.total_liquidity {} \
         (shortfall {})", pool.total_liquidity, pool.total_liquidity - vault_bal
    );
    // The vault's only inflows beyond deposits are repayments' interest
    // (principal replaces what was borrowed out). Track the interest and bound
    // the vault on BOTH sides: an understated vault is as much a solvency bug
    // as an overstated total_liquidity.
    assert!(
        vault_bal <= pool.total_liquidity + ctx.interest_paid,
        "INVARIANT 1b VIOLATED at step {step} ({op}): vault {vault_bal} > total_liquidity {} + \
         interest_paid {}", pool.total_liquidity, ctx.interest_paid
    );

    for (i, u) in ctx.users.iter().enumerate() {
        let profile = match ctx.bc.get_account(u.profile).await.unwrap() {
            Some(a) if a.owner == pid && a.data.len() >= UserProfile::LEN =>
                UserProfile::unpack_from_slice(&a.data).ok(),
            _ => None,
        };
        if let Some(p) = profile {
            let escrow = token_amount(&mut ctx.bc, u.skr_escrow).await;

            // 2. Bond accounting: the profile's stake must equal what is actually escrowed.
            assert_eq!(
                p.staked_skr, escrow,
                "INVARIANT 2 VIOLATED at step {step} ({op}): user {i} staked_skr {} != skr_escrow {}",
                p.staked_skr, escrow
            );

            // 3. A lock can never exceed the stake backing it.
            assert!(
                p.locked_skr <= p.staked_skr,
                "INVARIANT 3 VIOLATED at step {step} ({op}): user {i} locked_skr {} > staked_skr {}",
                p.locked_skr, p.staked_skr
            );

            // 4. The profile lock must equal the sum of locks on that user's live loans.
            let mut loan_locks: u64 = 0;
            for (ui, _, lp, _) in ctx.loans.iter() {
                if *ui != i { continue; }
                if let Some(a) = ctx.bc.get_account(*lp).await.unwrap() {
                    if a.owner == pid && a.data.len() >= LoanOrder::LEN {
                        if let Ok(l) = LoanOrder::unpack_from_slice(&a.data) {
                            if l.is_active { loan_locks = loan_locks.saturating_add(l.locked_skr); }
                        }
                    }
                }
            }
            assert_eq!(
                p.locked_skr, loan_locks,
                "INVARIANT 4 VIOLATED at step {step} ({op}): user {i} profile.locked_skr {} != \
                 sum of live loan locks {}", p.locked_skr, loan_locks
            );
        }
    }

    // 5. Loan status flags stay consistent.
    for (ui, lid, lp, _) in ctx.loans.iter() {
        if let Some(a) = ctx.bc.get_account(*lp).await.unwrap() {
            if a.owner == pid && a.data.len() >= LoanOrder::LEN {
                if let Ok(l) = LoanOrder::unpack_from_slice(&a.data) {
                    let live = matches!(l.status, LoanStatus::Active | LoanStatus::InGracePeriod);
                    assert_eq!(
                        l.is_active, live,
                        "INVARIANT 5 VIOLATED at step {step} ({op}): loan {lid} (user {ui}) \
                         is_active={} but status={:?}", l.is_active, l.status
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------- ops
async fn op_deposit(ctx: &mut Ctx, rng: &mut Rng, _ui: usize) {
    let amount = rng.range(1, 50) * USDC;
    let lp_pk = ctx.lp.pubkey();
    let ix = Instruction { program_id: ctx.pid, accounts: vec![
        AccountMeta::new(lp_pk, true), AccountMeta::new(ctx.pool, false),
        AccountMeta::new(ctx.lp_usdc, false), AccountMeta::new(ctx.vault, false),
        AccountMeta::new_readonly(spl_token::id(), false),
    ], data: borsh::to_vec(&ClockLendInstruction::DepositLiquidity { amount }).unwrap() };
    let _ = send(&mut ctx.bc, &[ix], &[&ctx.payer, &ctx.lp], &ctx.payer, ctx.bh).await;
}

async fn op_withdraw(ctx: &mut Ctx, rng: &mut Rng) {
    // Only the pool authority may withdraw; never more than the free liquidity.
    let pool = match ctx.bc.get_account(ctx.pool).await.unwrap() {
        Some(a) => LendingPool::unpack_from_slice(&a.data).expect("pool unpack"),
        None => return,
    };
    if pool.total_liquidity == 0 { return; }
    let amount = (rng.range(1, 20) as u64 * USDC).min(pool.total_liquidity);
    let lp_pk = ctx.lp.pubkey();
    let ix = Instruction { program_id: ctx.pid, accounts: vec![
        AccountMeta::new(lp_pk, true), AccountMeta::new(ctx.pool, false),
        AccountMeta::new(ctx.lp_usdc, false), AccountMeta::new(ctx.vault, false),
        AccountMeta::new_readonly(spl_token::id(), false),
    ], data: borsh::to_vec(&ClockLendInstruction::WithdrawLiquidity { amount }).unwrap() };
    let _ = send(&mut ctx.bc, &[ix], &[&ctx.payer, &ctx.lp], &ctx.payer, ctx.bh).await;
}

async fn op_stake(ctx: &mut Ctx, rng: &mut Rng, ui: usize) {
    let amount = rng.range(10, 2_000) * USDC;
    let (user_pk, profile, skr, escrow) = {
        let u = &ctx.users[ui];
        (u.kp.pubkey(), u.profile, u.skr, u.skr_escrow)
    };
    let ix = Instruction { program_id: ctx.pid, accounts: vec![
        AccountMeta::new(user_pk, true), AccountMeta::new(profile, false),
        AccountMeta::new(skr, false), AccountMeta::new(escrow, false),
        AccountMeta::new_readonly(SYS, false), AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(SKR_MINT, false),
    ], data: borsh::to_vec(&ClockLendInstruction::StakeSKR { amount }).unwrap() };
    let _ = send(&mut ctx.bc, &[ix], &[&ctx.payer, &ctx.users[ui].kp], &ctx.payer, ctx.bh).await;
}

async fn op_unstake(ctx: &mut Ctx, rng: &mut Rng, ui: usize) {
    let (user_pk, profile, skr, escrow) = {
        let u = &ctx.users[ui];
        (u.kp.pubkey(), u.profile, u.skr, u.skr_escrow)
    };
    // deliberately overshoot sometimes, to exercise the lock guard
    let amount = rng.range(1, 2_100) * USDC;
    let ix = Instruction { program_id: ctx.pid, accounts: vec![
        AccountMeta::new(user_pk, true), AccountMeta::new(profile, false),
        AccountMeta::new(skr, false), AccountMeta::new(escrow, false),
        AccountMeta::new_readonly(spl_token::id(), false),
    ], data: borsh::to_vec(&ClockLendInstruction::UnstakeSKR { amount }).unwrap() };
    let _ = send(&mut ctx.bc, &[ix], &[&ctx.payer, &ctx.users[ui].kp], &ctx.payer, ctx.bh).await;
}

async fn op_borrow(ctx: &mut Ctx, rng: &mut Rng, ui: usize, step: usize) {
    let loan_id = ctx.next_loan_id;
    let (user_pk, usdc, profile) = { let u = &ctx.users[ui]; (u.kp.pubkey(), u.usdc, u.profile) };
    let (loan, _) = Pubkey::find_program_address(
        &[LOAN_SEED, ctx.pool.as_ref(), user_pk.as_ref(), &loan_id.to_le_bytes()], &ctx.pid);
    let (escrow, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan.as_ref()], &ctx.pid);

    // SOL collateral in 0.05..1.5 SOL; principal at 10..70% LTV against a $150 SOL price
    let lamports = rng.range(50, 1_500) * 1_000_000;
    let value_usd = (lamports as u128 * 150_000_000u128) / 1_000_000_000u128;   // micro-USDC
    let ltv = rng.range(1_000, 7_000) as u128;
    let borrow_amount = ((value_usd * ltv) / 10_000) as u64;
    if borrow_amount == 0 { return; }
    let duration = (rng.range(1, 30) * 86_400) as i64;

    let ix = Instruction { program_id: ctx.pid, accounts: vec![
        AccountMeta::new(user_pk, true), AccountMeta::new(ctx.pool, false),
        AccountMeta::new(loan, false), AccountMeta::new(ctx.vault, false),
        AccountMeta::new(usdc, false), AccountMeta::new(user_pk, false),
        AccountMeta::new(escrow, false), AccountMeta::new_readonly(SYS, false),
        AccountMeta::new_readonly(spl_token::id(), false), AccountMeta::new_readonly(SYS, false),
        AccountMeta::new(profile, false), AccountMeta::new(ctx.treasury_tok, false),
        AccountMeta::new_readonly(ctx.sol_oracle, false),
    ], data: borsh::to_vec(&ClockLendInstruction::BorrowFromPool {
        loan_id, borrow_amount, collateral_amount: lamports, duration_seconds: duration }).unwrap() };

    if send(&mut ctx.bc, &[ix], &[&ctx.payer, &ctx.users[ui].kp], &ctx.payer, ctx.bh).await.is_ok() {
        ctx.loans.push((ui, loan_id, loan, escrow));
        println!("      step {step}: user {ui} borrowed {borrow_amount} against {lamports} lamports");
    }
    ctx.next_loan_id += 1;
}

async fn op_repay(ctx: &mut Ctx, rng: &mut Rng) {
    let live: Vec<usize> = ctx.loans.iter().enumerate()
        .filter_map(|(i, (_, _, lp, _))| {
            // cheap synchronous filter; full read happens below
            Some((i, *lp))
        }).map(|(i, _)| i).collect();
    if live.is_empty() { return; }
    let pick = live[rng.below(live.len() as u64) as usize];
    let (ui, loan_id, loan, escrow) = ctx.loans[pick];
    let (user_pk, usdc, profile) = { let u = &ctx.users[ui]; (u.kp.pubkey(), u.usdc, u.profile) };

    let l = match ctx.bc.get_account(loan).await.unwrap() {
        Some(a) if a.owner == ctx.pid => match LoanOrder::unpack_from_slice(&a.data) { Ok(l) => l, Err(_) => return },
        _ => return,
    };
    if !l.is_active { return; }
    let total_due = l.principal_amount.saturating_add(l.interest_due);

    let ix = Instruction { program_id: ctx.pid, accounts: vec![
        AccountMeta::new(user_pk, true), AccountMeta::new(loan, false),
        AccountMeta::new(usdc, false), AccountMeta::new(ctx.vault, false),
        AccountMeta::new(escrow, false), AccountMeta::new(user_pk, false),
        AccountMeta::new(ctx.pool, false), AccountMeta::new(profile, false),
        AccountMeta::new_readonly(spl_token::id(), false), AccountMeta::new_readonly(SYS, false),
        AccountMeta::new(ctx.treasury_tok, false),
    ], data: borsh::to_vec(&ClockLendInstruction::RepayLoan { repay_amount: total_due }).unwrap() };

    let r = send(&mut ctx.bc, &[ix], &[&ctx.payer, &ctx.users[ui].kp], &ctx.payer, ctx.bh).await;
    if r.is_ok() {
        println!("      step: user {ui} repaid loan {loan_id} ({total_due})");
        ctx.interest_paid = ctx.interest_paid.saturating_add(l.interest_due);
        ctx.loans.retain(|(_, lid, _, _)| *lid != loan_id);
    }
}

async fn op_p2p_create(ctx: &mut Ctx, rng: &mut Rng, ui: usize) {
    let offer_id = ctx.next_offer_id;
    let (user_pk, profile) = { let u = &ctx.users[ui]; (u.kp.pubkey(), u.profile) };
    let _ = profile;
    let (offer, _) = Pubkey::find_program_address(
        &[P2P_SEED, user_pk.as_ref(), &offer_id.to_le_bytes()], &ctx.pid);
    let (escrow, _) = Pubkey::find_program_address(&[ESCROW_SEED, offer.as_ref()], &ctx.pid);
    let requested = rng.range(1, 60) * USDC;
    let collateral_lamports = rng.range(10, 800) * 1_000_000;
    let interest = rng.range(1, 10) * USDC / 10;
    let duration = (rng.range(1, 30) * 86_400) as i64;

    let ix = Instruction { program_id: ctx.pid, accounts: vec![
        AccountMeta::new(user_pk, true), AccountMeta::new(offer, false),
        AccountMeta::new(user_pk, false), AccountMeta::new(escrow, false),
        AccountMeta::new_readonly(SYS, false), AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(SYS, false),
    ], data: borsh::to_vec(&ClockLendInstruction::CreateP2POffer {
        offer_id, requested_amount: requested, collateral_amount: collateral_lamports,
        interest_offered: interest, duration_seconds: duration }).unwrap() };

    if send(&mut ctx.bc, &[ix], &[&ctx.payer, &ctx.users[ui].kp], &ctx.payer, ctx.bh).await.is_ok() {
        ctx.offers.push((ui, offer_id, offer, escrow));
    }
    ctx.next_offer_id += 1;
}

async fn op_p2p_fund(ctx: &mut Ctx, rng: &mut Rng) {
    if ctx.offers.is_empty() { return; }
    let pick = rng.below(ctx.offers.len() as u64) as usize;
    let (creator_idx, offer_id, offer, _) = ctx.offers[pick];
    let funder_idx = (creator_idx + 1 + rng.below((N_USERS - 1) as u64) as usize) % N_USERS;
    let (funder_pk, funder_usdc) = { let u = &ctx.users[funder_idx]; (u.kp.pubkey(), u.usdc) };
    let creator_usdc = ctx.users[creator_idx].usdc;

    let ix = Instruction { program_id: ctx.pid, accounts: vec![
        AccountMeta::new(funder_pk, true), AccountMeta::new(offer, false),
        AccountMeta::new(funder_usdc, false), AccountMeta::new(creator_usdc, false),
        AccountMeta::new_readonly(spl_token::id(), false),
    ], data: borsh_to_vec_fund() };
    if send(&mut ctx.bc, &[ix], &[&ctx.payer, &ctx.users[funder_idx].kp], &ctx.payer, ctx.bh).await.is_ok() {
        println!("      step: user {funder_idx} funded P2P offer {offer_id} by user {creator_idx}");
    }
}
fn borsh_to_vec_fund() -> Vec<u8> {
    borsh::to_vec(&ClockLendInstruction::FundP2POffer).unwrap()
}

async fn op_p2p_cancel(ctx: &mut Ctx, rng: &mut Rng) {
    if ctx.offers.is_empty() { return; }
    let pick = rng.below(ctx.offers.len() as u64) as usize;
    let (creator_idx, offer_id, offer, escrow) = ctx.offers[pick];
    let (creator_pk, _) = { let u = &ctx.users[creator_idx]; (u.kp.pubkey(), u.usdc) };

    let ix = Instruction { program_id: ctx.pid, accounts: vec![
        AccountMeta::new(creator_pk, true), AccountMeta::new(offer, false),
        AccountMeta::new(escrow, false), AccountMeta::new(creator_pk, false),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(SYS, false),
    ], data: borsh::to_vec(&ClockLendInstruction::CancelP2POffer).unwrap() };
    if send(&mut ctx.bc, &[ix], &[&ctx.payer, &ctx.users[creator_idx].kp], &ctx.payer, ctx.bh).await.is_ok() {
        println!("      step: user {creator_idx} cancelled P2P offer {offer_id}");
        ctx.offers.retain(|(_, oid, _, _)| *oid != offer_id);
    }
}

// ---------------------------------------------------------------- the fuzzer
#[tokio::test]
async fn fuzz_sequence_invariants() {
    let seed: u64 = std::env::var("FUZZ_SEED").ok().and_then(|s| s.parse().ok())
        .unwrap_or(0xC10C_1E17_5EED_0001);
    let steps: usize = std::env::var("FUZZ_STEPS").ok().and_then(|s| s.parse().ok())
        .unwrap_or(150);
    let mut rng = Rng::new(seed);
    println!("\n=== sequence fuzzer === seed={seed} steps={steps}");

    // ---------- boot ----------
    let pid = clock_lend::id();
    // The program allowlists liquidity mints, so the pool's USDC mint must be
    // the real devnet mint address (created here in genesis, like SKR_MINT).
    let usdc = USDC_DEVNET_MINT;
    let deployer = Keypair::new();
    let (pool, _) = Pubkey::find_program_address(
        &[POOL_SEED, deployer.pubkey().as_ref(), &1u64.to_le_bytes()], &pid);
    let (vault, _) = Pubkey::find_program_address(&[VAULT_SEED, pool.as_ref()], &pid);
    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &pid);
    let (admin_pda, _) = Pubkey::find_program_address(&[ADMIN_SEED], &pid);
    let (sol_oracle, _) = Pubkey::find_program_address(&[ORACLE_SEED, NATIVE_MINT.as_ref()], &pid);
    let (pd, _) = Pubkey::find_program_address(&[pid.as_ref()], &solana_program::bpf_loader_upgradeable::id());

    let mut pt = ProgramTest::new("clock_lend", pid, processor!(process_instruction));
    pt.add_account(pd, Account { lamports: 100_000_000_000, data: program_data_bytes(&deployer.pubkey()),
        owner: solana_program::bpf_loader_upgradeable::id(), executable: false, rent_epoch: 0 });
    pt.add_account(SKR_MINT, Account { lamports: 100_000_000_000, data: mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0 });
    pt.add_account(USDC_DEVNET_MINT, Account { lamports: 100_000_000_000, data: mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0 });
    let treasury_tok = Pubkey::new_unique();
    pt.add_account(treasury_tok, Account { lamports: 100_000_000_000,
        data: tok(usdc, treasury_pda, 0), owner: spl_token::id(), executable: false, rent_epoch: 0 });

    let users: Vec<User> = (0..N_USERS).map(|_| {
        let kp = Keypair::new();
        let (profile, _) = Pubkey::find_program_address(&[PROFILE_SEED, kp.pubkey().as_ref()], &pid);
        let (skr_escrow, _) = Pubkey::find_program_address(&[b"skr_escrow", kp.pubkey().as_ref()], &pid);
        User { usdc: Pubkey::new_unique(), skr: Pubkey::new_unique(), profile, skr_escrow, kp }
    }).collect();
    let lp_usdc = Pubkey::new_unique();
    pt.add_account(lp_usdc, Account { lamports: 100_000_000_000,
        data: tok(usdc, deployer.pubkey(), 500_000 * USDC), owner: spl_token::id(),
        executable: false, rent_epoch: 0 });
    for u in &users {
        pt.add_account(u.usdc, Account { lamports: 100_000_000_000,
            data: tok(usdc, u.kp.pubkey(), 100_000 * USDC), owner: spl_token::id(),
            executable: false, rent_epoch: 0 });
        pt.add_account(u.skr, Account { lamports: 100_000_000_000,
            data: tok(SKR_MINT, u.kp.pubkey(), 100_000 * USDC), owner: spl_token::id(),
            executable: false, rent_epoch: 0 });
    }

    let (mut bc, payer, bh) = pt.start().await;
    send(&mut bc, &[
        system_instruction::transfer(&payer.pubkey(), &deployer.pubkey(), 30_000_000_000),
    ], &[&payer], &payer, bh).await.unwrap();
    for u in &users {
        send(&mut bc, &[system_instruction::transfer(&payer.pubkey(), &u.kp.pubkey(), 30_000_000_000)],
             &[&payer], &payer, bh).await.unwrap();
    }

    let admin_ix = Instruction { program_id: pid, accounts: vec![
        AccountMeta::new(deployer.pubkey(), true), AccountMeta::new(admin_pda, false),
        AccountMeta::new_readonly(SYS, false), AccountMeta::new_readonly(pd, false),
    ], data: borsh::to_vec(&ClockLendInstruction::InitializeAdmin).unwrap() };
    send(&mut bc, &[admin_ix], &[&payer, &deployer], &payer, bh).await.expect("admin");
    let feed_ix = Instruction { program_id: pid, accounts: vec![
        AccountMeta::new(deployer.pubkey(), true), AccountMeta::new(sol_oracle, false),
        AccountMeta::new_readonly(NATIVE_MINT, false), AccountMeta::new_readonly(SYS, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false), AccountMeta::new(admin_pda, false),
    ], data: borsh::to_vec(&ClockLendInstruction::SetPriceFeed {
        price_micro_usd: 150_000_000, decimals: 9 }).unwrap() };
    send(&mut bc, &[feed_ix], &[&payer, &deployer], &payer, bh).await.expect("feed");
    let init_ix = Instruction { program_id: pid, accounts: vec![
        AccountMeta::new(deployer.pubkey(), true), AccountMeta::new(pool, false),
        AccountMeta::new_readonly(usdc, false), AccountMeta::new(vault, false),
        AccountMeta::new_readonly(SYS, false), AccountMeta::new_readonly(sysvar::rent::id(), false),
        AccountMeta::new_readonly(spl_token::id(), false),
    ], data: borsh::to_vec(&ClockLendInstruction::InitializePool {
        pool_id: 1, pool_type: PoolType::Circle, interest_rate_bps: 800, max_ltv_bps: 6500,
        min_duration: 86_400, max_duration: 86_400 * 30, name: [1u8; 32],
        is_oracle_free: false,
    }).unwrap() };
    send(&mut bc, &[init_ix], &[&payer, &deployer], &payer, bh).await.expect("pool");

    let mut ctx = Ctx { bc, payer, bh, pid, pool, vault, treasury_tok, sol_oracle,
        lp: deployer,   // moved in after the setup transactions above
        lp_usdc, users,
        loans: Vec::new(), next_loan_id: 1, offers: Vec::new(), next_offer_id: 1,
        interest_paid: 0 };

    // ---------- run ----------
    for step in 0..steps {
        // ProgramTest's blockhash queue evicts the start blockhash after enough
        // transactions; without refreshing, banks_server panics on the lookup.
        ctx.bh = ctx.bc.get_latest_blockhash().await.expect("blockhash");
        let ui = rng.below(N_USERS as u64) as usize;
        let op = rng.below(100);
        let name = match op {
            0..=18 => { op_deposit(&mut ctx, &mut rng, ui).await; "DepositLiquidity" }
            19..=32 => { op_stake(&mut ctx, &mut rng, ui).await; "StakeSKR" }
            33..=46 => { op_unstake(&mut ctx, &mut rng, ui).await; "UnstakeSKR" }
            47..=70 => { op_borrow(&mut ctx, &mut rng, ui, step).await; "BorrowFromPool" }
            71..=82 => { op_repay(&mut ctx, &mut rng).await; "RepayLoan" }
            83..=88 => { op_p2p_create(&mut ctx, &mut rng, ui).await; "CreateP2POffer" }
            89..=91 => { op_p2p_fund(&mut ctx, &mut rng).await; "FundP2POffer" }
            92..=93 => { op_p2p_cancel(&mut ctx, &mut rng).await; "CancelP2POffer" }
            _ => { op_withdraw(&mut ctx, &mut rng).await; "WithdrawLiquidity" }
        };
        check_invariants(&mut ctx, step, name).await;
    }

    // ---------- report ----------
    let pool_acc = ctx.bc.get_account(ctx.pool).await.unwrap().unwrap();
    let p = LendingPool::unpack_from_slice(&pool_acc.data).unwrap();
    let vbal = token_amount(&mut ctx.bc, ctx.vault).await;
    println!("\n=== completed {steps} steps (seed {seed}) ===");
    println!("  pool.total_liquidity = {}", p.total_liquidity);
    println!("  vault balance        = {}", vbal);
    println!("  loans originated     = {}  repaid = {}", p.loans_originated, p.loans_repaid);
    println!("  live loans tracked   = {}", ctx.loans.len());
    println!("  live p2p offers      = {}", ctx.offers.len());
    for (i, u) in ctx.users.iter().enumerate() {
        let skr = token_amount(&mut ctx.bc, u.skr_escrow).await;
        let prof = ctx.bc.get_account(u.profile).await.unwrap()
            .map(|a| UserProfile::unpack_from_slice(&a.data).ok()).flatten();
        match prof {
            Some(pr) => println!("  user {i}: staked={} locked={} escrow={}", pr.staked_skr, pr.locked_skr, skr),
            None => println!("  user {i}: no profile (never staked)"),
        }
    }
    println!("  all invariants held");
    let _ = P2POffer::LEN;
}

// ============================================================================
// Regression: is_oracle_free must be driven by the typed instruction field,
// NOT by the pool's free-text name. A pool named "ORACLE_FREE ..." with
// is_oracle_free = false must still require a live oracle.
// ============================================================================
#[tokio::test]
async fn is_oracle_free_is_authoritative_not_name_derived() {
    let pid = clock_lend::id();
    // Program allowlists liquidity mints — use the real devnet USDC address.
    let usdc = USDC_DEVNET_MINT;

    for (label, pool_name, flag, expect_borrow_ok) in [
        ("name says ORACLE_FREE, flag false", b"ORACLE_FREE Desk".to_vec(), false, false),
        ("plain name,         flag true ", b"Plain Desk".to_vec(),         true,  true),
    ] {
        let authority = Keypair::new();
        let borrower = Keypair::new();
        let (pool, _) = Pubkey::find_program_address(
            &[POOL_SEED, authority.pubkey().as_ref(), &1u64.to_le_bytes()], &pid);
        let (vault, _) = Pubkey::find_program_address(&[VAULT_SEED, pool.as_ref()], &pid);
        let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &pid);

        let mut pt = ProgramTest::new("clock_lend", pid, processor!(process_instruction));
        pt.add_account(USDC_DEVNET_MINT, Account { lamports: 100_000_000_000, data: mint_data(6),
            owner: spl_token::id(), executable: false, rent_epoch: 0 });
        let treasury_tok = Pubkey::new_unique();
        pt.add_account(treasury_tok, Account { lamports: 100_000_000_000,
            data: tok(usdc, treasury_pda, 0), owner: spl_token::id(), executable: false, rent_epoch: 0 });
        let at_pk = Pubkey::new_unique();
        pt.add_account(at_pk, Account { lamports: 100_000_000_000,
            data: tok(usdc, authority.pubkey(), 1_000 * USDC), owner: spl_token::id(),
            executable: false, rent_epoch: 0 });
        let b_usdc = Pubkey::new_unique();
        pt.add_account(b_usdc, Account { lamports: 100_000_000_000,
            data: tok(usdc, borrower.pubkey(), 1_000 * USDC), owner: spl_token::id(),
            executable: false, rent_epoch: 0 });
        let (mut bc, payer, bh) = pt.start().await;
        send(&mut bc, &[
            system_instruction::transfer(&payer.pubkey(), &authority.pubkey(), 30_000_000_000),
        ], &[&payer], &payer, bh).await.unwrap();

        let mut name = [0u8; 32];
        name[..pool_name.len()].copy_from_slice(&pool_name);
        let ix = Instruction { program_id: pid, accounts: vec![
            AccountMeta::new(authority.pubkey(), true), AccountMeta::new(pool, false),
            AccountMeta::new_readonly(usdc, false), AccountMeta::new(vault, false),
            AccountMeta::new_readonly(SYS, false), AccountMeta::new_readonly(sysvar::rent::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ], data: borsh::to_vec(&ClockLendInstruction::InitializePool {
            pool_id: 1, pool_type: PoolType::Individual, interest_rate_bps: 800, max_ltv_bps: 6500,
            min_duration: 86_400, max_duration: 86_400 * 30, name, is_oracle_free: flag }).unwrap() };
        send(&mut bc, &[ix], &[&payer, &authority], &payer, bh).await.expect("init pool");

        let dep = Instruction { program_id: pid, accounts: vec![
            AccountMeta::new(authority.pubkey(), true), AccountMeta::new(pool, false),
            AccountMeta::new(at_pk, false), AccountMeta::new(vault, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ], data: borsh::to_vec(&ClockLendInstruction::DepositLiquidity { amount: 1_000 * USDC }).unwrap() };
        send(&mut bc, &[dep], &[&payer, &authority], &payer, bh).await.expect("deposit");

        let (loan, _) = Pubkey::find_program_address(
            &[LOAN_SEED, pool.as_ref(), borrower.pubkey().as_ref(), &1u64.to_le_bytes()], &pid);
        let (escrow, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan.as_ref()], &pid);
        let (profile, _) = Pubkey::find_program_address(&[PROFILE_SEED, borrower.pubkey().as_ref()], &pid);
        send(&mut bc, &[
            system_instruction::transfer(&payer.pubkey(), &borrower.pubkey(), 20_000_000_000),
        ], &[&payer], &payer, bh).await.unwrap();

        // borrow WITHOUT any oracle account appended
        let ix = Instruction { program_id: pid, accounts: vec![
            AccountMeta::new(borrower.pubkey(), true), AccountMeta::new(pool, false),
            AccountMeta::new(loan, false), AccountMeta::new(vault, false),
            AccountMeta::new(b_usdc, false), AccountMeta::new(borrower.pubkey(), false),
            AccountMeta::new(escrow, false), AccountMeta::new_readonly(SYS, false),
            AccountMeta::new_readonly(spl_token::id(), false), AccountMeta::new_readonly(SYS, false),
            AccountMeta::new(profile, false), AccountMeta::new(treasury_tok, false),
        ], data: borsh::to_vec(&ClockLendInstruction::BorrowFromPool {
            loan_id: 1, borrow_amount: 90 * USDC, collateral_amount: 1_000_000_000,
            duration_seconds: 86_400 * 7 }).unwrap() };
        let r = send(&mut bc, &[ix], &[&payer, &borrower], &payer, bh).await;
        println!("   [{label}] flag={flag} -> borrow w/o oracle: {}",
                 if r.is_ok() { "OK".into() } else { format!("{:?}", r.as_ref().err()) });
        assert_eq!(r.is_ok(), expect_borrow_ok,
            "name-derived behaviour leaked back in for: {label}");
    }
    println!("   -> CONFIRMED: the typed field is authoritative; the pool name is inert.");
}

// ============================================================================
// Liquidation fuzzer.
//
// The clock does not advance in ProgramTest, so due_time can never be reached
// by driving the lifecycle. Instead we pre-seed loans already in the
// InGracePeriod state with grace_period_expires = 0 (i.e. expired) and fuzz
// ClaimDefault against them with randomized callers and randomized
// optional-account sets. (TriggerGracePeriod is NOT driven here — the pool-
// loan branch is covered deterministically in bank_integration, the P2P
// branch by the repay-in-grace tests.)
//
// Invariants after every step:
//   L1. profile.staked_skr      == skr_escrow balance      (bond accounting)
//   L2. profile.locked_skr      <= profile.staked_skr
//   L3. loan Defaulted          => collateral escrow is empty
//   L4. loan.is_active          == status in {Active, InGracePeriod}
//   L5. slash tokens conserved  (escrow delta == destination delta)
// ============================================================================
#[tokio::test]
async fn fuzz_liquidation_path() {
    let seed: u64 = std::env::var("FUZZ_SEED").ok().and_then(|s| s.parse().ok())
        .unwrap_or(0x11D_0F00D_5EED_0007);
    let steps: usize = std::env::var("FUZZ_STEPS").ok().and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let mut rng = Rng::new(seed ^ 0xA5A5_A5A5);
    println!("\n=== liquidation fuzzer === seed={seed} steps={steps}");

    let pid = clock_lend::id();
    let usdc = Keypair::new();
    let authority = Keypair::new();
    let (pool, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &1u64.to_le_bytes()], &pid);
    let (vault, _) = Pubkey::find_program_address(&[VAULT_SEED, pool.as_ref()], &pid);
    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &pid);

    const N_LOANS: usize = 6;
    struct L { borrower: Keypair, loan: Pubkey, escrow: Pubkey, profile: Pubkey,
                skr_escrow: Pubkey, native: bool, staked: u64, locked: u64, dead: bool }
    let mut loans: Vec<L> = Vec::new();

    let mut pt = ProgramTest::new("clock_lend", pid, processor!(process_instruction));
    pt.add_account(SKR_MINT, Account { lamports: 100_000_000_000, data: mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0 });
    let treasury_tok = Pubkey::new_unique();
    pt.add_account(treasury_tok, Account { lamports: 100_000_000_000,
        data: tok(usdc.pubkey(), treasury_pda, 0), owner: spl_token::id(), executable: false, rent_epoch: 0 });
    // For SPL-collateral liquidation the 5% margin is paid in kind, so the
    // treasury account must be denominated in the COLLATERAL mint (SKR) - not
    // the pool's liquidity mint as borrow/repay require.
    let treasury_skr = Pubkey::new_unique();
    pt.add_account(treasury_skr, Account { lamports: 100_000_000_000,
        data: tok(SKR_MINT, treasury_pda, 0), owner: spl_token::id(), executable: false, rent_epoch: 0 });
    // a valid SKR slash destination owned by the pool authority
    let slash_dest = Pubkey::new_unique();
    pt.add_account(slash_dest, Account { lamports: 100_000_000_000,
        data: tok(SKR_MINT, authority.pubkey(), 0), owner: spl_token::id(), executable: false, rent_epoch: 0 });
    // Separate SKR token account for the seized collateral, so the slash flow and
    // the collateral flow land in different accounts and L5 can isolate the slash.
    let collat_dest = Pubkey::new_unique();
    pt.add_account(collat_dest, Account { lamports: 100_000_000_000,
        data: tok(SKR_MINT, authority.pubkey(), 0), owner: spl_token::id(), executable: false, rent_epoch: 0 });

    for i in 0..N_LOANS {
        let borrower = Keypair::new();
        let native = i % 2 == 0;
        let loan_id = (i as u64) + 1;
        let (loan, _) = Pubkey::find_program_address(
            &[LOAN_SEED, pool.as_ref(), borrower.pubkey().as_ref(), &loan_id.to_le_bytes()], &pid);
        let (escrow, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan.as_ref()], &pid);
        let (profile, _) = Pubkey::find_program_address(&[PROFILE_SEED, borrower.pubkey().as_ref()], &pid);
        let (skr_escrow, _) = Pubkey::find_program_address(&[b"skr_escrow", borrower.pubkey().as_ref()], &pid);

        // stake varies so both slash branches (locked vs 20% baseline) are hit
        let staked = if i % 3 == 0 { 100_000_000u64 } else { 1_000_000_000u64 };
        let locked = if i % 2 == 0 { staked } else { staked / 2 };

        pt.add_account(loan, Account { lamports: 10_000_000, executable: false, rent_epoch: 0, owner: pid,
            data: borsh::to_vec(&LoanOrder {
                discriminator: DISCRIMINATOR_LOAN,
                is_active: true, loan_id, borrower: borrower.pubkey(), pool,
                principal_amount: 100_000_000,
                collateral_mint: if native { Pubkey::default() } else { SKR_MINT },
                collateral_amount: if native { 1_000_000_000 } else { 5_000_000_000 },
                interest_due: 1_000_000, origination_time: 1000, due_time: 2000,
                grace_period_expires: 0,               // already expired
                status: LoanStatus::InGracePeriod,
                locked_skr: locked,
            }).unwrap() });
        pt.add_account(escrow, if native {
            Account { lamports: 1_000_000_000, data: vec![], owner: pid, executable: false, rent_epoch: 0 }
        } else {
            Account { lamports: 10_000_000, data: tok(SKR_MINT, escrow, 5_000_000_000),
                      owner: spl_token::id(), executable: false, rent_epoch: 0 }
        });
        pt.add_account(profile, Account { lamports: 10_000_000, executable: false, rent_epoch: 0, owner: pid,
            data: borsh::to_vec(&UserProfile { discriminator: DISCRIMINATOR_PROFILE,
                is_initialized: true, user: borrower.pubkey(),
                staked_skr: staked, total_loans_completed: 0, total_loans_defaulted: 0,
                reputation_score: 5000, locked_skr: locked }).unwrap() });
        pt.add_account(skr_escrow, Account { lamports: 10_000_000, executable: false, rent_epoch: 0,
            owner: spl_token::id(), data: tok(SKR_MINT, skr_escrow, staked) });

        loans.push(L { borrower, loan, escrow, profile, skr_escrow, native, staked, locked, dead: false });
    }
    pt.add_account(pool, Account { lamports: 10_000_000, executable: false, rent_epoch: 0, owner: pid,
        data: borsh::to_vec(&LendingPool {
            discriminator: DISCRIMINATOR_POOL, pool_id: 1,
            is_initialized: true, pool_type: PoolType::Individual, authority: authority.pubkey(),
            name: [0u8; 32], liquidity_mint: usdc.pubkey(), vault_pda: vault,
            total_liquidity: 0, total_borrowed: 0, staked_skr_amount: 0,
            interest_rate_bps: 800, max_ltv_bps: 6500,
            min_duration: 86_400, max_duration: 86_400 * 30,
            loans_originated: N_LOANS as u32, loans_repaid: 0,
            is_oracle_free: true, has_custom_oracle: false }).unwrap() });

    let (mut bc, payer, bh) = pt.start().await;
    send(&mut bc, &[system_instruction::transfer(&payer.pubkey(), &authority.pubkey(), 30_000_000_000)],
         &[&payer], &payer, bh).await.unwrap();
    let stranger = Keypair::new();
    send(&mut bc, &[system_instruction::transfer(&payer.pubkey(), &stranger.pubkey(), 30_000_000_000)],
         &[&payer], &payer, bh).await.unwrap();
    let mut bh = bc.get_latest_blockhash().await.unwrap();

    let mut accepted = 0usize; let mut rejected = 0usize;
    for step in 0..steps {
        bh = bc.get_latest_blockhash().await.unwrap();
        let idx = rng.below(N_LOANS as u64) as usize;
        let (l_loan, l_escrow, l_profile, l_skr_escrow, l_native) = {
            let l = &loans[idx];
            (l.loan, l.escrow, l.profile, l.skr_escrow, l.native)
        };

        // Mostly-valid calls with a minority of invalid variants, so the happy
        // path actually executes and the guards still get exercised.
        let caller_is_authority = rng.below(100) < 75;
        let caller = if caller_is_authority { authority.insecure_clone() } else { stranger.insecure_clone() };

        // treasury is mandatory (protocol_margin > 0): the PDA itself for a SOL
        // loan, a treasury-owned token account for an SKR loan.
        let include_treasury = rng.below(100) < 90;
        let include_slash = rng.below(100) < 80;

        // destination must be a token account owned by the authority / vault for
        // SKR loans, or the authority wallet / vault PDA for SOL loans.
        let dest = if l_native {
            if rng.below(100) < 60 { authority.pubkey() } else { vault }
        } else {
            collat_dest
        };

        let mut accounts = vec![
            AccountMeta::new(caller.pubkey(), true),
            AccountMeta::new(l_loan, false),
            AccountMeta::new(l_escrow, false),
            AccountMeta::new(dest, false),
            AccountMeta::new(pool, false),
            AccountMeta::new(l_profile, false),
            AccountMeta::new(l_skr_escrow, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(SYS, false),
        ];
        if include_slash { accounts.push(AccountMeta::new(slash_dest, false)); }
        if include_treasury {
            accounts.push(AccountMeta::new(if l_native { treasury_pda } else { treasury_skr }, false));
        }

        let ix = Instruction { program_id: pid, accounts,
            data: borsh::to_vec(&ClockLendInstruction::ClaimDefault).unwrap() };

        // L5 conservation: total SKR across every account that can hold it must be
        // unchanged. This is destination-agnostic, so it stays valid no matter which
        // candidate the resolver picks for the slash (slash_dest, the treasury SKR
        // account, or the collateral destination).


        let mut before_total = token_amount(&mut bc, l_skr_escrow).await
            + token_amount(&mut bc, slash_dest).await
            + token_amount(&mut bc, collat_dest).await
            + token_amount(&mut bc, treasury_skr).await;
        if !l_native { before_total += token_amount(&mut bc, l_escrow).await; }
        let r = send(&mut bc, &[ix], &[&payer, &caller], &payer, bh).await;
        let ok = r.is_ok();
        if ok { accepted += 1; } else { rejected += 1; }

        // L1/L2: bond accounting must hold regardless of outcome
        if let Some(a) = bc.get_account(l_profile).await.unwrap() {
            if a.owner == pid && a.data.len() >= UserProfile::LEN {
                let p = UserProfile::unpack_from_slice(&a.data).unwrap();
                let esc = token_amount(&mut bc, l_skr_escrow).await;
                assert_eq!(p.staked_skr, esc,
                    "L1 VIOLATED at step {step} (loan {idx}, ok={ok}): staked_skr {} != skr_escrow {}", 
                    p.staked_skr, esc);
                assert!(p.locked_skr <= p.staked_skr,
                    "L2 VIOLATED at step {step} (loan {idx}): locked {} > staked {}",
                    p.locked_skr, p.staked_skr);
            }
        }

        // L4/L3: loan state
        if let Some(a) = bc.get_account(l_loan).await.unwrap() {
            if a.owner == pid && a.data.len() >= LoanOrder::LEN {
                let lo = LoanOrder::unpack_from_slice(&a.data).unwrap();
                let live = matches!(lo.status, LoanStatus::Active | LoanStatus::InGracePeriod);
                assert_eq!(lo.is_active, live,
                    "L4 VIOLATED at step {step} (loan {idx}): is_active={} status={:?}",
                    lo.is_active, lo.status);
                if lo.status == LoanStatus::Defaulted {
                    let esc_lamports = bc.get_account(l_escrow).await.unwrap().map(|x| x.lamports).unwrap_or(0);
                    if l_native {
                        assert_eq!(esc_lamports, 0,
                            "L3 VIOLATED at step {step}: loan {idx} Defaulted but SOL escrow still holds {esc_lamports}");
                    }
                    loans[idx].dead = true;
                }
            }
        }

        if ok {
            let mut after = token_amount(&mut bc, l_skr_escrow).await
                + token_amount(&mut bc, slash_dest).await
                + token_amount(&mut bc, collat_dest).await
                + token_amount(&mut bc, treasury_skr).await;
            if !l_native { after += token_amount(&mut bc, l_escrow).await; }
            assert_eq!(before_total, after,
                "L5 VIOLATED at step {step} (loan {idx}): SKR not conserved - before {before_total}, after {after}");
        }
    }

    let dead = loans.iter().filter(|l| l.dead).count();
    println!("  accepted={accepted} rejected={rejected}");
    println!("  loans defaulted during fuzz = {dead}/{N_LOANS}");
    for (i, l) in loans.iter().enumerate() {
        println!("  loan {i} ({}): staked={} locked={} defaulted={}",
                 if l.native { "SOL" } else { "SKR" }, l.staked, l.locked, l.dead);
    }
    println!("  all liquidation invariants held (seed {seed})");
}
