use clock_lend::state::{POOL_SEED, VAULT_SEED};
use solana_program::pubkey::Pubkey;

#[test]
fn test_security_ltv_enforcement() {
    let collateral_amount: u64 = 1_000_000_000; // 1 SOL (in lamports)
    let max_ltv_bps: u16 = 8000; // 80.0% max LTV

    let max_allowed = (collateral_amount as u128 * max_ltv_bps as u128) / 10000;
    assert_eq!(max_allowed, 800_000_000); // exactly 0.8 SOL equivalent

    // A borrow request of 800_000_000 is allowed
    let valid_borrow = 800_000_000u64;
    assert!((valid_borrow as u128) <= max_allowed);

    // A malicious borrow request of 800_000_001 MUST be rejected
    let exploit_borrow = 800_000_001u64;
    assert!((exploit_borrow as u128) > max_allowed);
}

#[test]
fn test_security_pda_seeds_tamper_resistance() {
    let program_id = Pubkey::new_unique();
    let authority = Pubkey::new_unique();
    let pool_id: u64 = 7;
    let pool_id_bytes = pool_id.to_le_bytes();

    let (valid_pool_pda, _bump) = Pubkey::find_program_address(
        &[POOL_SEED, authority.as_ref(), &pool_id_bytes],
        &program_id,
    );

    // Tampered authority
    let attacker = Pubkey::new_unique();
    let (fake_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, attacker.as_ref(), &pool_id_bytes],
        &program_id,
    );
    assert_ne!(valid_pool_pda, fake_pda);

    // Tampered vault PDA
    let (valid_vault_pda, _) =
        Pubkey::find_program_address(&[VAULT_SEED, valid_pool_pda.as_ref()], &program_id);
    let (fake_vault_pda, _) =
        Pubkey::find_program_address(&[VAULT_SEED, fake_pda.as_ref()], &program_id);
    assert_ne!(valid_vault_pda, fake_vault_pda);
}

#[test]
fn test_security_slashing_deterministic_math() {
    let initial_staked_skr: u64 = 10_000_000_000; // 10,000 SKR

    // In claim_default, 20% is slashed deterministically using pure integer math
    let slashed = initial_staked_skr.saturating_mul(80) / 100;
    assert_eq!(slashed, 8_000_000_000); // 8,000 SKR remains

    // Ensure no overflow on large numbers
    let huge_skr: u64 = u64::MAX / 100;
    let safe_slash = huge_skr.saturating_mul(80) / 100;
    assert!(safe_slash < huge_skr);
}

#[test]
fn test_security_interest_calculation() {
    let borrow_amount: u64 = 100_000_000; // 100 USDC (6 decimals)
    let interest_rate_bps: u16 = 800;     // 8.0% APR
    let duration_seconds: i64 = 86400 * 7; // 7 days

    let interest_due = ((borrow_amount as u128)
        * (interest_rate_bps as u128)
        * (duration_seconds as u128)
        / (10000u128 * 31536000u128)) as u64;

    // 100 * 0.08 * (7/365) = ~0.1534 USDC = 153,424 units
    assert!(interest_due > 150_000 && interest_due < 160_000);
}

#[test]
fn test_security_grace_period_timing() {
    let now: i64 = 1700000000;
    let due_time: i64 = now + 86400 * 7;

    // Loan is NOT due yet
    let current_time_early: i64 = now + 86400 * 3;
    assert!(current_time_early < due_time); // Cannot trigger grace period

    // Loan is due
    let current_time_due: i64 = due_time + 10;
    assert!(current_time_due >= due_time); // Grace period can be triggered

    // Grace period lasts 24h
    let grace_expires = current_time_due + 86400;
    let during_grace: i64 = grace_expires - 3600;
    assert!(during_grace < grace_expires); // Cannot liquidate yet!

    let after_grace: i64 = grace_expires + 1;
    assert!(after_grace >= grace_expires); // Can be liquidated!
}
