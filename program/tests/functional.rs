use borsh::BorshDeserialize;
use clock_lend::{
    instruction::ClockLendInstruction,
    state::{
        LendingPool, LoanOrder, LoanStatus, OfferStatus, P2POffer, PoolType, UserProfile,
    },
};
use solana_program::pubkey::Pubkey;

#[test]
fn test_lending_pool_serialization() {
    let mut name = [0u8; 32];
    let name_bytes = b"Lisbon Hacker Desk";
    name[..name_bytes.len()].copy_from_slice(name_bytes);

    let pool = LendingPool {
        is_initialized: true,
        pool_type: PoolType::Circle,
        authority: Pubkey::new_unique(),
        liquidity_mint: Pubkey::new_unique(),
        vault_pda: Pubkey::new_unique(),
        total_liquidity: 5_000_000_000,
        total_borrowed: 1_200_000_000,
        staked_skr_amount: 10_000_000_000,
        interest_rate_bps: 400, // 4%
        max_ltv_bps: 9000,      // 90% LTV
        min_duration: 86400 * 3,
        max_duration: 86400 * 30,
        loans_originated: 12,
        loans_repaid: 11,
        name,
    };

    let mut buffer = [0u8; LendingPool::LEN];
    pool.pack_into_slice(&mut buffer).expect("Pack failed");

    let unpacked = LendingPool::unpack_from_slice(&buffer).expect("Unpack failed");
    assert_eq!(unpacked.is_initialized, true);
    assert_eq!(unpacked.pool_type, PoolType::Circle);
    assert_eq!(unpacked.total_liquidity, 5_000_000_000);
    assert_eq!(unpacked.interest_rate_bps, 400);
    assert_eq!(unpacked.max_ltv_bps, 9000);
    assert_eq!(unpacked.name, name);
}

#[test]
fn test_loan_order_serialization() {
    let loan = LoanOrder {
        is_active: true,
        loan_id: 101,
        borrower: Pubkey::new_unique(),
        pool: Pubkey::new_unique(),
        principal_amount: 100_000_000,
        collateral_mint: Pubkey::new_unique(),
        collateral_amount: 1_500_000_000,
        interest_due: 500_000,
        origination_time: 1700000000,
        due_time: 1700000000 + 86400 * 7,
        grace_period_expires: 0,
        status: LoanStatus::Active,
    };

    let mut buffer = [0u8; LoanOrder::LEN];
    loan.pack_into_slice(&mut buffer).expect("Pack failed");

    let unpacked = LoanOrder::unpack_from_slice(&buffer).expect("Unpack failed");
    assert_eq!(unpacked.loan_id, 101);
    assert_eq!(unpacked.principal_amount, 100_000_000);
    assert_eq!(unpacked.status, LoanStatus::Active);
}

#[test]
fn test_p2p_offer_serialization() {
    let offer = P2POffer {
        is_initialized: true,
        offer_id: 42,
        creator: Pubkey::new_unique(),
        funder: Pubkey::default(),
        collateral_mint: Pubkey::new_unique(),
        collateral_amount: 1, // 1 NFT
        requested_amount: 250_000_000,
        interest_offered: 15_000_000,
        duration_seconds: 86400 * 14,
        created_at: 1700000000,
        due_time: 0,
        grace_period_expires: 0,
        status: OfferStatus::Open,
    };

    let mut buffer = [0u8; P2POffer::LEN];
    offer.pack_into_slice(&mut buffer).expect("Pack failed");

    let unpacked = P2POffer::unpack_from_slice(&buffer).expect("Unpack failed");
    assert_eq!(unpacked.offer_id, 42);
    assert_eq!(unpacked.status, OfferStatus::Open);
    assert_eq!(unpacked.requested_amount, 250_000_000);
}

#[test]
fn test_user_profile_serialization() {
    let profile = UserProfile {
        is_initialized: true,
        user: Pubkey::new_unique(),
        staked_skr: 5_000_000_000,
        total_loans_completed: 18,
        total_loans_defaulted: 0,
        reputation_score: 9950, // 99.5% completion rating
    };

    let mut buffer = [0u8; UserProfile::LEN];
    profile.pack_into_slice(&mut buffer).expect("Pack failed");

    let unpacked = UserProfile::unpack_from_slice(&buffer).expect("Unpack failed");
    assert_eq!(unpacked.staked_skr, 5_000_000_000);
    assert_eq!(unpacked.total_loans_completed, 18);
    assert_eq!(unpacked.reputation_score, 9950);
}

#[test]
fn test_instruction_serialization() {
    let mut name = [0u8; 32];
    let name_bytes = b"Superteam Pool";
    name[..name_bytes.len()].copy_from_slice(name_bytes);

    let ix = ClockLendInstruction::InitializePool {
        pool_id: 1,
        pool_type: PoolType::Individual,
        interest_rate_bps: 800,
        max_ltv_bps: 7500,
        min_duration: 86400,
        max_duration: 86400 * 30,
        name,
    };

    let serialized = borsh::to_vec(&ix).expect("Serialization failed");
    let deserialized =
        ClockLendInstruction::try_from_slice(&serialized).expect("Deserialization failed");
    assert_eq!(ix, deserialized);
}

#[test]
fn test_withdraw_liquidity_instruction_serialization() {
    let ix = ClockLendInstruction::WithdrawLiquidity {
        amount: 50_000_000_000,
    };

    let serialized = borsh::to_vec(&ix).expect("Serialization failed");
    let deserialized =
        ClockLendInstruction::try_from_slice(&serialized).expect("Deserialization failed");
    assert_eq!(ix, deserialized);
}

#[test]
fn test_institutional_pool_type_serialization() {
    let mut name = [0u8; 32];
    let name_bytes = b"Tokyo Whale Institutional Desk";
    name[..name_bytes.len()].copy_from_slice(name_bytes);

    let pool = LendingPool {
        is_initialized: true,
        pool_type: PoolType::Institutional,
        authority: Pubkey::new_unique(),
        liquidity_mint: Pubkey::new_unique(),
        vault_pda: Pubkey::new_unique(),
        total_liquidity: 500_000_000_000, // 500k USDC
        total_borrowed: 120_000_000_000,
        staked_skr_amount: 50_000_000_000,
        interest_rate_bps: 350, // 3.5% institutional APR
        max_ltv_bps: 8500,
        min_duration: 86400 * 7,
        max_duration: 86400 * 90,
        loans_originated: 45,
        loans_repaid: 45,
        name,
    };

    let mut buffer = [0u8; LendingPool::LEN];
    pool.pack_into_slice(&mut buffer).expect("Pack failed");

    let unpacked = LendingPool::unpack_from_slice(&buffer).expect("Unpack failed");
    assert_eq!(unpacked.pool_type, PoolType::Institutional);
    assert_eq!(unpacked.total_liquidity, 500_000_000_000);
    assert_eq!(unpacked.interest_rate_bps, 350);
}

#[test]
fn test_cancel_p2p_offer_instruction_serialization() {
    let ix = ClockLendInstruction::CancelP2POffer;
    let serialized = borsh::to_vec(&ix).expect("Serialization failed");
    let deserialized =
        ClockLendInstruction::try_from_slice(&serialized).expect("Deserialization failed");
    assert_eq!(ix, deserialized);
}

#[test]
fn test_unstake_skr_instruction_serialization() {
    let ix = ClockLendInstruction::UnstakeSKR { amount: 50_000_000 };
    let serialized = borsh::to_vec(&ix).expect("Serialization failed");
    let deserialized =
        ClockLendInstruction::try_from_slice(&serialized).expect("Deserialization failed");
    assert_eq!(ix, deserialized);
}

