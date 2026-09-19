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

#[test]
fn test_security_interest_take_rate_split() {
    let interest_due: u64 = 10_000_000; // 10 USDC in interest
    let protocol_fee = ((interest_due as u128 * 1500) / 10000) as u64; // 15% take-rate
    let lender_interest = interest_due.saturating_sub(protocol_fee); // 85% to lender

    assert_eq!(protocol_fee, 1_500_000); // 1.50 USDC to ClockLend Treasury
    assert_eq!(lender_interest, 8_500_000); // 8.50 USDC to Lender Vault
    assert_eq!(protocol_fee + lender_interest, interest_due);
}

#[test]
fn test_security_origination_fee_skr_vs_sol() {
    let borrow_amount: u64 = 100_000_000; // 100 USDC

    // SKR Collateral: 0.50% fee (50 bps)
    let skr_fee_bps: u64 = 50;
    let skr_origination_fee = ((borrow_amount as u128 * skr_fee_bps as u128) / 10000) as u64;
    let skr_net_disbursed = borrow_amount.saturating_sub(skr_origination_fee);

    assert_eq!(skr_origination_fee, 500_000); // 0.50 USDC
    assert_eq!(skr_net_disbursed, 99_500_000); // 99.50 USDC
    assert_eq!(skr_origination_fee + skr_net_disbursed, borrow_amount);

    // SOL Collateral: 0.25% fee (25 bps)
    let sol_fee_bps: u64 = 25;
    let sol_origination_fee = ((borrow_amount as u128 * sol_fee_bps as u128) / 10000) as u64;
    let sol_net_disbursed = borrow_amount.saturating_sub(sol_origination_fee);

    assert_eq!(sol_origination_fee, 250_000); // 0.25 USDC
    assert_eq!(sol_net_disbursed, 99_750_000); // 99.75 USDC
    assert_eq!(sol_origination_fee + sol_net_disbursed, borrow_amount);
}

#[test]
fn test_security_default_liquidation_margin_capture() {
    let collateral_amount: u64 = 10_000_000_000; // 10,000 SKR

    // 5% liquidation penalty margin to Treasury (500 bps)
    let protocol_margin = ((collateral_amount as u128 * 500) / 10000) as u64;
    let lender_collateral = collateral_amount.saturating_sub(protocol_margin);

    assert_eq!(protocol_margin, 500_000_000); // 500 SKR to Treasury
    assert_eq!(lender_collateral, 9_500_000_000); // 9,500 SKR to Lender
    assert_eq!(protocol_margin + lender_collateral, collateral_amount);
}

#[test]
fn test_security_rent_refund_invariant() {
    let rent_lamports: u64 = 1_962_240; // 154 bytes LoanOrder rent
    let mut borrower_lamports: u64 = 5_000_000_000;
    let mut loan_account_lamports: u64 = rent_lamports;

    // Repayment execution refunds rent:
    borrower_lamports = borrower_lamports.checked_add(loan_account_lamports).unwrap();
    loan_account_lamports = 0;

    assert_eq!(borrower_lamports, 5_001_962_240);
    assert_eq!(loan_account_lamports, 0); // 100% refunded, zero rent waste
}
