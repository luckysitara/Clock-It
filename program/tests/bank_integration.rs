use clock_lend::{
    error::ClockLendError,
    instruction::ClockLendInstruction,
    processor::process_instruction,
    state::{
        AdminConfig, LendingPool, LoanOrder, LoanStatus, PoolType, PriceFeed, SkrYieldVault, UserProfile, UserYieldPosition,
        ADMIN_SEED, DISCRIMINATOR_SKR_YIELD, DISCRIMINATOR_USER_YIELD, ESCROW_SEED, LOAN_SEED,
        ORACLE_SEED, P2P_SEED, POOL_SEED, PROFILE_SEED, SKR_MINT, SKR_YIELD_VAULT_SEED, TREASURY_SEED,
        USER_YIELD_SEED, USDC_DEVNET_MINT, VAULT_SEED,
    },
};
use solana_program::{
    instruction::{AccountMeta, Instruction, InstructionError},
    program_pack::Pack,
    pubkey::Pubkey,
    sysvar,
};
use solana_program_test::*;
use solana_sdk::{
    account::Account,
    signature::{Keypair, Signer},
    transaction::{Transaction, TransactionError},
};

/// Assert a transaction failed with a specific program error code.
///
/// A bare `assert!(res.is_err())` cannot tell "rejected for the right reason"
/// from "rejected for any reason" - which is how four Pyth fixtures silently
/// encoded a missing-staleness-bound bug and kept reporting green.
#[track_caller]
fn expect_custom_error(res: &Result<(), BanksClientError>, code: u32, ctx: &str) {
    let err = match res.as_ref().err() {
        Some(e) => format!("{e:?}"),
        None => panic!("{ctx}: expected failure, but the transaction SUCCEEDED"),
    };
    if !err.contains(&format!("Custom({code})")) {
        panic!("{ctx}: expected Custom({code}), got {err}");
    }
}

#[tokio::test]
async fn test_bank_initialize_pool_success() {
    let program_id = Pubkey::new_unique();
    let program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let pool_id: u64 = 1;
    let pool_id_bytes = pool_id.to_le_bytes();
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, payer.pubkey().as_ref(), &pool_id_bytes],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(
        &[VAULT_SEED, pool_pda.as_ref()],
        &program_id,
    );

    // The program allowlists liquidity mints (USDC devnet/mainnet or wrapped
    // SOL) — use the real devnet USDC mint rather than a random one.
    let liquidity_mint = clock_lend::state::USDC_DEVNET_MINT;
    let mut name = [0u8; 32];
    let name_bytes = b"Seeker Genesis Pool";
    name[..name_bytes.len()].copy_from_slice(name_bytes);

    let init_ix_data = borsh::to_vec(&ClockLendInstruction::InitializePool {
        pool_id,
        pool_type: PoolType::Individual,
        interest_rate_bps: 600, // 6%
        max_ltv_bps: 8500,      // 85%
        min_duration: 86400,
        max_duration: 86400 * 30,
        name,
        is_oracle_free: false,
    })
    .expect("Serialization failed");

    let accounts = vec![
        AccountMeta::new(payer.pubkey(), true),
        AccountMeta::new(pool_pda, false),
        AccountMeta::new_readonly(liquidity_mint, false),
        AccountMeta::new(vault_pda, false),
        AccountMeta::new_readonly(solana_program::system_program::id(), false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data: init_ix_data,
    };

    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
    transaction.sign(&[&payer], recent_blockhash);

    let result = banks_client.process_transaction(transaction).await;
    assert!(result.is_ok(), "InitializePool transaction failed on SVM bank!");

    // Verify pool account state on bank
    let pool_account = banks_client
        .get_account(pool_pda)
        .await
        .expect("Failed to get pool account")
        .expect("Pool account not found on bank");

    let pool = LendingPool::unpack_from_slice(&pool_account.data).expect("Failed to unpack pool data");
    assert_eq!(pool.is_initialized, true);
    assert_eq!(pool.interest_rate_bps, 600);
    assert_eq!(pool.max_ltv_bps, 8500);
    assert_eq!(pool.authority, payer.pubkey());
    assert_eq!(pool.name, name);
}

#[tokio::test]
async fn test_bank_initialize_pool_rejects_unauthorized_signer() {
    let program_id = Pubkey::new_unique();
    let program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let victim_authority = Keypair::new(); // Did NOT sign!
    let pool_id: u64 = 99;
    let pool_id_bytes = pool_id.to_le_bytes();
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, victim_authority.pubkey().as_ref(), &pool_id_bytes],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(
        &[VAULT_SEED, pool_pda.as_ref()],
        &program_id,
    );

    let liquidity_mint = Pubkey::new_unique();
    let name = [0u8; 32];

    let init_ix_data = borsh::to_vec(&ClockLendInstruction::InitializePool {
        pool_id,
        pool_type: PoolType::Individual,
        interest_rate_bps: 800,
        max_ltv_bps: 8000,
        min_duration: 86400,
        max_duration: 86400 * 30,
        name,
        is_oracle_free: false,
    })
    .expect("Serialization failed");

    // Attacker passes victim_authority as non-signer
    let accounts = vec![
        AccountMeta::new_readonly(victim_authority.pubkey(), false), // is_signer = false!
        AccountMeta::new(pool_pda, false),
        AccountMeta::new_readonly(liquidity_mint, false),
        AccountMeta::new(vault_pda, false),
        AccountMeta::new_readonly(solana_program::system_program::id(), false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data: init_ix_data,
    };

    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
    transaction.sign(&[&payer], recent_blockhash);

    let result = banks_client.process_transaction(transaction).await;
    expect_custom_error(&result, 5, "Attacker should NOT be able to initialize pool for non-signing authority!");
}

fn token_acct_data(mint: Pubkey, owner: Pubkey, amount: u64) -> Vec<u8> {
    let mut data = vec![0u8; spl_token::state::Account::LEN];
    use solana_program::program_pack::Pack;
    let acct = spl_token::state::Account {
        mint,
        owner,
        amount,
        delegate: solana_program::program_option::COption::None,
        state: spl_token::state::AccountState::Initialized,
        is_native: solana_program::program_option::COption::None,
        delegated_amount: 0,
        close_authority: solana_program::program_option::COption::None,
    };
    acct.pack_into_slice(&mut data);
    data
}

fn mint_data(decimals: u8) -> Vec<u8> {
    let mut data = vec![0u8; spl_token::state::Mint::LEN];
    let mint = spl_token::state::Mint {
        mint_authority: solana_program::program_option::COption::None,
        supply: 1_000_000_000_000_000,
        decimals,
        is_initialized: true,
        freeze_authority: solana_program::program_option::COption::None,
    };
    mint.pack_into_slice(&mut data);
    data
}

#[tokio::test]
async fn test_bank_stake_skr_rejects_unauthorized_mint() {
    // F-05: Attacker tries to stake an arbitrary token mint to get SKR discount
    let program_id = Pubkey::new_unique();
    let fake_skr_mint = Pubkey::new_unique();
    let user = Keypair::new();

    let (profile_pda, _) = Pubkey::find_program_address(
        &[PROFILE_SEED, user.pubkey().as_ref()],
        &program_id,
    );
    let (skr_escrow_pda, _) = Pubkey::find_program_address(
        &[b"skr_escrow", user.pubkey().as_ref()],
        &program_id,
    );

    let user_token_pubkey = Pubkey::new_unique();

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    program_test.add_account(
        user_token_pubkey,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(fake_skr_mint, user.pubkey(), 1_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let init_ix_data = borsh::to_vec(&ClockLendInstruction::StakeSKR {
        amount: 1_000_000_000,
    })
    .unwrap();

    let accounts = vec![
        AccountMeta::new(user.pubkey(), true),
        AccountMeta::new(profile_pda, false),
        AccountMeta::new(user_token_pubkey, false),
        AccountMeta::new(skr_escrow_pda, false),
        AccountMeta::new_readonly(solana_program::system_program::id(), false),
        AccountMeta::new_readonly(spl_token::id(), false),
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data: init_ix_data,
    };

    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
    transaction.sign(&[&payer, &user], recent_blockhash);

    let result = banks_client.process_transaction(transaction).await;
    expect_custom_error(&result, 17, "StakeSKR with non-SKR mint MUST be rejected!");
}

#[tokio::test]
async fn test_bank_borrow_rejects_unauthorized_collateral_mint() {
    // F-03: Attacker creates a pool and tries to borrow using self-minted worthless collateral
    let program_id = Pubkey::new_unique();
    let usdc_mint = Pubkey::new_unique();
    let fake_collateral_mint = Pubkey::new_unique(); // NOT SOL and NOT SKR!
    let authority = Keypair::new();
    let borrower = Keypair::new();

    let pool_id: u64 = 1;
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);

    let loan_id: u64 = 42;
    let (loan_pda, _) = Pubkey::find_program_address(
        &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan_pda.as_ref()], &program_id);
    let (profile_pda, _) = Pubkey::find_program_address(&[PROFILE_SEED, borrower.pubkey().as_ref()], &program_id);

    let borrower_usdc = Pubkey::new_unique();
    let borrower_collateral = Pubkey::new_unique();

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    // Setup pool fixture
    let pool_state = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        liquidity_mint: usdc_mint,
        vault_pda,
        total_liquidity: 10_000_000_000,
        total_borrowed: 0,
        staked_skr_amount: 0,
        interest_rate_bps: 800,
        max_ltv_bps: 6500,
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 0,
        loans_repaid: 0,
        name: [0u8; 32],
        is_oracle_free: true,
        pool_id,
        has_custom_oracle: false,
    };
    program_test.add_account(
        pool_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&pool_state).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        vault_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, vault_pda, 10_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        borrower_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, borrower.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        borrower_collateral,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(fake_collateral_mint, borrower.pubkey(), 1_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let borrow_ix_data = borsh::to_vec(&ClockLendInstruction::BorrowFromPool {
        loan_id,
        borrow_amount: 650_000_000,
        collateral_amount: 1_000_000_000,
        duration_seconds: 86400 * 7,
    })
    .unwrap();

    let accounts = vec![
        AccountMeta::new(borrower.pubkey(), true),
        AccountMeta::new(pool_pda, false),
        AccountMeta::new(loan_pda, false),
        AccountMeta::new(vault_pda, false),
        AccountMeta::new(borrower_usdc, false),
        AccountMeta::new(borrower_collateral, false),
        AccountMeta::new(escrow_pda, false),
        AccountMeta::new_readonly(fake_collateral_mint, false),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(solana_program::system_program::id(), false),
        AccountMeta::new(profile_pda, false),
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data: borrow_ix_data,
    };

    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
    transaction.sign(&[&payer, &borrower], recent_blockhash);

    let result = banks_client.process_transaction(transaction).await;
    expect_custom_error(&result, 17, "Borrow with unapproved fake collateral mint MUST be rejected!");
}

#[tokio::test]
async fn test_bank_borrow_requires_treasury_when_origination_fee_positive() {
    // F-04: Attacker tries to bypass the origination fee by omitting the treasury account
    let program_id = Pubkey::new_unique();
    let usdc_mint = Pubkey::new_unique();
    let authority = Keypair::new();
    let borrower = Keypair::new();

    let pool_id: u64 = 1;
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);

    let loan_id: u64 = 43;
    let (loan_pda, _) = Pubkey::find_program_address(
        &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan_pda.as_ref()], &program_id);
    let (profile_pda, _) = Pubkey::find_program_address(&[PROFILE_SEED, borrower.pubkey().as_ref()], &program_id);

    let borrower_usdc = Pubkey::new_unique();
    let borrower_collateral = Pubkey::new_unique();

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let pool_state = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        liquidity_mint: usdc_mint,
        vault_pda,
        total_liquidity: 10_000_000_000,
        total_borrowed: 0,
        staked_skr_amount: 0,
        interest_rate_bps: 800,
        max_ltv_bps: 6500,
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 0,
        loans_repaid: 0,
        name: [0u8; 32],
        is_oracle_free: true,
        pool_id,
        has_custom_oracle: false,
    };
    program_test.add_account(
        pool_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&pool_state).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        vault_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, vault_pda, 10_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        borrower_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, borrower.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        borrower_collateral,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, borrower.pubkey(), 100_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let borrow_ix_data = borsh::to_vec(&ClockLendInstruction::BorrowFromPool {
        loan_id,
        borrow_amount: 10_000_000, // 10 USDC (fee is 50 bps = 50,000 micro-USDC > 0)
        collateral_amount: 1_000_000_000, // 1000 SKR
        duration_seconds: 86400 * 7,
    })
    .unwrap();

    // Accounts OMITTING treasury account
    let accounts = vec![
        AccountMeta::new(borrower.pubkey(), true),
        AccountMeta::new(pool_pda, false),
        AccountMeta::new(loan_pda, false),
        AccountMeta::new(vault_pda, false),
        AccountMeta::new(borrower_usdc, false),
        AccountMeta::new(borrower_collateral, false),
        AccountMeta::new(escrow_pda, false),
        AccountMeta::new_readonly(SKR_MINT, false),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(solana_program::system_program::id(), false),
        AccountMeta::new(profile_pda, false),
        // No treasury account!
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data: borrow_ix_data,
    };

    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
    transaction.sign(&[&payer, &borrower], recent_blockhash);

    let result = banks_client.process_transaction(transaction).await;
    expect_custom_error(&result, 1, "Borrow MUST revert when treasury account is omitted and fee > 0!");
}

#[tokio::test]
async fn test_bank_borrow_rejects_overwriting_defaulted_loan() {
    // F-12: Attacker tries to reuse a loan_id from a defaulted loan to erase credit history
    let program_id = Pubkey::new_unique();
    let usdc_mint = Pubkey::new_unique();
    let authority = Keypair::new();
    let borrower = Keypair::new();

    let pool_id: u64 = 1;
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);

    let loan_id: u64 = 99;
    let (loan_pda, _) = Pubkey::find_program_address(
        &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan_pda.as_ref()], &program_id);

    let borrower_usdc = Pubkey::new_unique();
    let borrower_collateral = Pubkey::new_unique();

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    // The F-11 vault-mint check runs BEFORE the F-12 reuse guard, so the vault
    // and the borrower's liquidity account must be real token accounts or the
    // instruction fails decoding them long before the guard is reached - which
    // is exactly how this test used to "pass" while never exercising F-12.
    program_test.add_account(
        vault_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, vault_pda, 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        borrower_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, borrower.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let pool_state = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        liquidity_mint: usdc_mint,
        vault_pda,
        total_liquidity: 10_000_000_000,
        total_borrowed: 0,
        staked_skr_amount: 0,
        interest_rate_bps: 800,
        max_ltv_bps: 6500,
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 1,
        loans_repaid: 0,
        name: [0u8; 32],
        is_oracle_free: true,
        pool_id,
        has_custom_oracle: false,
    };
    program_test.add_account(
        pool_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&pool_state).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    // Existing loan is DEFAULTED
    let defaulted_loan = LoanOrder {
        discriminator: LoanOrder::DISCRIMINATOR,
        is_active: false,
        loan_id,
        borrower: borrower.pubkey(),
        pool: pool_pda,
        principal_amount: 100_000_000,
        collateral_mint: SKR_MINT,
        collateral_amount: 10_000_000_000,
        interest_due: 1_000_000,
        origination_time: 1000,
        due_time: 2000,
        grace_period_expires: 3000,
        status: LoanStatus::Defaulted,
        locked_skr: 0,
    };
    program_test.add_account(
        loan_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&defaulted_loan).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let borrow_ix_data = borsh::to_vec(&ClockLendInstruction::BorrowFromPool {
        loan_id,
        borrow_amount: 10_000_000,
        collateral_amount: 1_000_000_000,
        duration_seconds: 86400 * 7,
    })
    .unwrap();

    let accounts = vec![
        AccountMeta::new(borrower.pubkey(), true),
        AccountMeta::new(pool_pda, false),
        AccountMeta::new(loan_pda, false),
        AccountMeta::new(vault_pda, false),
        AccountMeta::new(borrower_usdc, false),
        AccountMeta::new(borrower_collateral, false),
        AccountMeta::new(escrow_pda, false),
        AccountMeta::new_readonly(SKR_MINT, false),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(solana_program::system_program::id(), false),
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data: borrow_ix_data,
    };

    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
    transaction.sign(&[&payer, &borrower], recent_blockhash);

    let result = banks_client.process_transaction(transaction).await;
    expect_custom_error(&result, 16, "Re-borrow on a Defaulted loan MUST be rejected!");
}

#[tokio::test]
async fn test_bank_claim_default_sol_loan_requires_skr_slash_destination() {
    let program_id = Pubkey::new_unique();
    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let authority = Keypair::new();
    let borrower = Keypair::new();
    let pool_id: u64 = 1;
    let pool_id_bytes = pool_id.to_le_bytes();

    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id_bytes],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);

    let loan_id: u64 = 100;
    let loan_id_bytes = loan_id.to_le_bytes();
    let (loan_pda, _) = Pubkey::find_program_address(
        &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id_bytes],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan_pda.as_ref()], &program_id);

    let (profile_pda, _) = Pubkey::find_program_address(&[PROFILE_SEED, borrower.pubkey().as_ref()], &program_id);
    let (skr_escrow_pda, _) = Pubkey::find_program_address(&[b"skr_escrow", borrower.pubkey().as_ref()], &program_id);

    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &program_id);
    program_test.add_account(
        treasury_pda,
        Account {
            lamports: 10_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    // Pre-populate pool account
    let pool = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        name: [0u8; 32],
        liquidity_mint: Pubkey::new_unique(),
        vault_pda,
        total_liquidity: 10_000_000_000,
        total_borrowed: 100_000_000,
        staked_skr_amount: 0,
        interest_rate_bps: 600,
        max_ltv_bps: 8500,
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 1,
        loans_repaid: 0,
        is_oracle_free: true,
        pool_id,
        has_custom_oracle: false,
    };
    program_test.add_account(
        pool_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&pool).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    // Pre-populate loan in grace period (grace period expired)
    let loan = LoanOrder {
        discriminator: LoanOrder::DISCRIMINATOR,
        is_active: true,
        loan_id,
        borrower: borrower.pubkey(),
        pool: pool_pda,
        principal_amount: 100_000_000,
        collateral_mint: Pubkey::default(), // Native SOL
        collateral_amount: 1_000_000_000,
        interest_due: 1_000_000,
        origination_time: 1000,
        due_time: 2000,
        grace_period_expires: 0, // already expired
        status: LoanStatus::InGracePeriod,
        locked_skr: 0,
    };
    program_test.add_account(
        loan_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&loan).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    // Pre-populate escrow with 1 SOL
    program_test.add_account(
        escrow_pda,
        Account {
            lamports: 1_000_000_000,
            data: vec![],
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    // Pre-populate borrower profile with staked SKR
    let profile = UserProfile {
        discriminator: UserProfile::DISCRIMINATOR,
        is_initialized: true,
        user: borrower.pubkey(),
        staked_skr: 100_000_000, // 100 SKR staked
        total_loans_completed: 0,
        total_loans_defaulted: 0,
        reputation_score: 5000,
        locked_skr: 0,
    };
    program_test.add_account(
        profile_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&profile).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    // Pre-populate borrower skr_escrow account with 100 SKR
    program_test.add_account(
        skr_escrow_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, skr_escrow_pda, 100_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let claim_ix_data = borsh::to_vec(&ClockLendInstruction::ClaimDefault).unwrap();

    // Destination collateral account is authority.pubkey() - a Native SOL wallet!
    // Treasury account is supplied so the F-04 check passes.
    // No independent SKR token account is passed for slashing.
    let accounts = vec![
        AccountMeta::new(authority.pubkey(), true),
        AccountMeta::new(loan_pda, false),
        AccountMeta::new(escrow_pda, false),
        AccountMeta::new(authority.pubkey(), false),
        AccountMeta::new(pool_pda, false),
        AccountMeta::new(profile_pda, false),
        AccountMeta::new(treasury_pda, false),
        AccountMeta::new(skr_escrow_pda, false),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(solana_program::system_program::id(), false),
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data: claim_ix_data,
    };

    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
    transaction.sign(&[&payer, &authority], recent_blockhash);

    // F-06: Because destination_collateral_account is a wallet and cannot receive SPL tokens,
    // and no independent SKR token account was provided, the transaction MUST fail specifically
    // with InvalidInstruction (Custom(0)) at the slash check, rather than on the treasury check or swallowing the error!
    let result = banks_client.process_transaction(transaction).await;
    let err = result.expect_err("ClaimDefault on SOL loan with staked SKR MUST fail when no valid SKR destination is provided!");
    match err {
        BanksClientError::TransactionError(TransactionError::InstructionError(0, InstructionError::Custom(code))) => {
            assert_eq!(
                code,
                ClockLendError::InvalidInstruction as u32,
                "Expected InvalidInstruction (Custom(0)) due to missing SKR slash destination, got Custom({})",
                code
            );
        }
        other => panic!("Expected TransactionError::InstructionError::Custom(InvalidInstruction), got {:?}", other),
    }
}

#[tokio::test]
async fn test_bank_claim_default_sol_loan_with_skr_slash_success() {
    let program_id = Pubkey::new_unique();
    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let authority = Keypair::new();
    let borrower = Keypair::new();
    let pool_id: u64 = 1;
    let pool_id_bytes = pool_id.to_le_bytes();

    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id_bytes],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);

    let loan_id: u64 = 100;
    let loan_id_bytes = loan_id.to_le_bytes();
    let (loan_pda, _) = Pubkey::find_program_address(
        &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id_bytes],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan_pda.as_ref()], &program_id);

    let (profile_pda, _) = Pubkey::find_program_address(&[PROFILE_SEED, borrower.pubkey().as_ref()], &program_id);
    let (skr_escrow_pda, _) = Pubkey::find_program_address(&[b"skr_escrow", borrower.pubkey().as_ref()], &program_id);
    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &program_id);

    program_test.add_account(
        treasury_pda,
        Account {
            lamports: 10_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let pool = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        name: [0u8; 32],
        liquidity_mint: Pubkey::new_unique(),
        vault_pda,
        total_liquidity: 10_000_000_000,
        total_borrowed: 100_000_000,
        staked_skr_amount: 0,
        interest_rate_bps: 600,
        max_ltv_bps: 8500,
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 1,
        loans_repaid: 0,
        is_oracle_free: true,
        pool_id,
        has_custom_oracle: false,
    };
    program_test.add_account(
        pool_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&pool).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    let loan = LoanOrder {
        discriminator: LoanOrder::DISCRIMINATOR,
        is_active: true,
        loan_id,
        borrower: borrower.pubkey(),
        pool: pool_pda,
        principal_amount: 100_000_000,
        collateral_mint: Pubkey::default(), // Native SOL
        collateral_amount: 1_000_000_000,   // 1 SOL
        interest_due: 1_000_000,
        origination_time: 1000,
        due_time: 2000,
        grace_period_expires: 0, // expired
        status: LoanStatus::InGracePeriod,
        locked_skr: 0,
    };
    program_test.add_account(
        loan_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&loan).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    program_test.add_account(
        escrow_pda,
        Account {
            lamports: 1_000_000_000,
            data: vec![],
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    let profile = UserProfile {
        discriminator: UserProfile::DISCRIMINATOR,
        is_initialized: true,
        user: borrower.pubkey(),
        staked_skr: 100_000_000, // 100 SKR
        total_loans_completed: 0,
        total_loans_defaulted: 0,
        reputation_score: 5000,
        locked_skr: 0,
    };
    program_test.add_account(
        profile_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&profile).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    // Borrower's skr_escrow with 100 SKR
    program_test.add_account(
        skr_escrow_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, skr_escrow_pda, 100_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    // Authority's dedicated SKR slash destination token account with 0 SKR
    let authority_skr_token = Keypair::new();
    program_test.add_account(
        authority_skr_token.pubkey(),
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, authority.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let claim_ix_data = borsh::to_vec(&ClockLendInstruction::ClaimDefault).unwrap();

    let accounts = vec![
        AccountMeta::new(authority.pubkey(), true),
        AccountMeta::new(loan_pda, false),
        AccountMeta::new(escrow_pda, false),
        AccountMeta::new(authority.pubkey(), false), // Native SOL destination
        AccountMeta::new(pool_pda, false),
        AccountMeta::new(profile_pda, false),
        AccountMeta::new(treasury_pda, false),
        AccountMeta::new(skr_escrow_pda, false),
        AccountMeta::new(authority_skr_token.pubkey(), false), // Dedicated SKR slash destination
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(solana_program::system_program::id(), false),
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data: claim_ix_data,
    };

    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
    transaction.sign(&[&payer, &authority], recent_blockhash);

    let result = banks_client.process_transaction(transaction).await;
    assert!(result.is_ok(), "ClaimDefault MUST succeed when dedicated SKR slash destination is provided! Result: {:?}", result);

    // Verify by execution:
    // 1. Escrow balance slashed from 100_000_000 to 80_000_000 (20% slash)
    let updated_skr_escrow = banks_client.get_account(skr_escrow_pda).await.unwrap().unwrap();
    let skr_escrow_tok = spl_token::state::Account::unpack(&updated_skr_escrow.data).unwrap();
    assert_eq!(skr_escrow_tok.amount, 80_000_000, "SKR escrow balance must be 80,000,000");

    // 2. Authority SKR slash destination credited with 20_000_000
    let updated_slash_dest = banks_client.get_account(authority_skr_token.pubkey()).await.unwrap().unwrap();
    let slash_dest_tok = spl_token::state::Account::unpack(&updated_slash_dest.data).unwrap();
    assert_eq!(slash_dest_tok.amount, 20_000_000, "Slash destination must receive 20,000,000 SKR");

    // 3. UserProfile staked_skr debited to 80_000_000 matching escrow exactly (no desync)
    let updated_profile_acc = banks_client.get_account(profile_pda).await.unwrap().unwrap();
    let updated_profile = UserProfile::unpack_from_slice(&updated_profile_acc.data).unwrap();
    assert_eq!(updated_profile.staked_skr, 80_000_000, "UserProfile staked_skr must match escrow balance");
    assert_eq!(updated_profile.total_loans_defaulted, 1, "Default count must increment");

    // 4. Loan marked Defaulted
    let updated_loan_acc = banks_client.get_account(loan_pda).await.unwrap().unwrap();
    let updated_loan = LoanOrder::unpack_from_slice(&updated_loan_acc.data).unwrap();
    assert_eq!(updated_loan.status, LoanStatus::Defaulted);
    assert_eq!(updated_loan.is_active, false);
}

#[tokio::test]
async fn test_bank_claim_default_syncs_borrower_yield_position() {
    let program_id = Pubkey::new_unique();
    let authority = Keypair::new();
    let borrower = Keypair::new();
    let native_mint = solana_program::system_program::id();
    let reward_mint = USDC_DEVNET_MINT;

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    program_test.add_account(
        SKR_MINT,
        Account {
            lamports: 10_000_000,
            data: mint_data(6),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let pool_id: u64 = 55;
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(
        &[VAULT_SEED, pool_pda.as_ref()],
        &program_id,
    );

    let pool = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        name: [0u8; 32],
        liquidity_mint: native_mint,
        vault_pda,
        total_liquidity: 10_000_000_000,
        total_borrowed: 100_000_000,
        staked_skr_amount: 0,
        interest_rate_bps: 600,
        max_ltv_bps: 8500,
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 1,
        loans_repaid: 0,
        is_oracle_free: true,
        pool_id,
        has_custom_oracle: false,
    };
    program_test.add_account(
        pool_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&pool).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    let (loan_pda, _) = Pubkey::find_program_address(
        &[LOAN_SEED, borrower.pubkey().as_ref(), &1u64.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(
        &[ESCROW_SEED, loan_pda.as_ref()],
        &program_id,
    );

    let loan = LoanOrder {
        discriminator: LoanOrder::DISCRIMINATOR,
        is_active: true,
        loan_id: 1,
        borrower: borrower.pubkey(),
        pool: pool_pda,
        principal_amount: 100_000_000,
        collateral_mint: Pubkey::default(), // Native SOL
        collateral_amount: 1_000_000_000,
        interest_due: 1_000_000,
        origination_time: 1000,
        due_time: 2000,
        grace_period_expires: 0, // expired
        status: LoanStatus::InGracePeriod,
        locked_skr: 0,
    };
    program_test.add_account(
        loan_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&loan).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    // Escrow with native SOL collateral
    program_test.add_account(
        escrow_pda,
        Account {
            lamports: 2_000_000_000,
            data: vec![],
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &program_id);
    program_test.add_account(
        treasury_pda,
        Account {
            lamports: 10_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (profile_pda, _) = Pubkey::find_program_address(
        &[PROFILE_SEED, borrower.pubkey().as_ref()],
        &program_id,
    );
    let (skr_escrow_pda, _) = Pubkey::find_program_address(
        &[b"skr_escrow", borrower.pubkey().as_ref()],
        &program_id,
    );

    let profile = UserProfile {
        discriminator: clock_lend::state::DISCRIMINATOR_PROFILE,
        is_initialized: true,
        user: borrower.pubkey(),
        staked_skr: 100_000_000,
        total_loans_completed: 0,
        total_loans_defaulted: 0,
        reputation_score: 10000,
        locked_skr: 0,
    };
    program_test.add_account(
        profile_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&profile).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    program_test.add_account(
        skr_escrow_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, skr_escrow_pda, 100_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let authority_skr_token = Keypair::new();
    program_test.add_account(
        authority_skr_token.pubkey(),
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, authority.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    // Yield vault with 100_000_000 total staked SKR
    let (yield_vault_pda, _) = Pubkey::find_program_address(
        &[SKR_YIELD_VAULT_SEED, reward_mint.as_ref()],
        &program_id,
    );
    let vault = SkrYieldVault {
        discriminator: DISCRIMINATOR_SKR_YIELD,
        is_initialized: true,
        authority: authority.pubkey(),
        reward_mint,
        total_staked_skr: 100_000_000,
        acc_reward_per_share: 0,
        total_rewards_distributed: 0,
        pending_rewards: 0,
        unallocated_rewards: 0,
    };
    let mut vault_data = vec![0u8; SkrYieldVault::LEN];
    vault.pack_into_slice(&mut vault_data).unwrap();
    program_test.add_account(
        yield_vault_pda,
        Account {
            lamports: 10_000_000,
            data: vault_data,
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    // Borrower's UserYieldPosition with 100_000_000 staked SKR
    let (borrower_yield_pda, _) = Pubkey::find_program_address(
        &[USER_YIELD_SEED, borrower.pubkey().as_ref(), reward_mint.as_ref()],
        &program_id,
    );
    let pos = UserYieldPosition {
        discriminator: DISCRIMINATOR_USER_YIELD,
        is_initialized: true,
        user: borrower.pubkey(),
        reward_mint,
        staked_skr: 100_000_000,
        reward_debt: 0,
        accrued_rewards: 0,
        total_claimed: 0,
        last_interaction_time: 1000,
    };
    let mut pos_data = vec![0u8; UserYieldPosition::LEN];
    pos.pack_into_slice(&mut pos_data).unwrap();
    program_test.add_account(
        borrower_yield_pda,
        Account {
            lamports: 10_000_000,
            data: pos_data,
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let claim_ix_data = borsh::to_vec(&ClockLendInstruction::ClaimDefault).unwrap();

    let accounts = vec![
        AccountMeta::new(authority.pubkey(), true),
        AccountMeta::new(loan_pda, false),
        AccountMeta::new(escrow_pda, false),
        AccountMeta::new(authority.pubkey(), false),
        AccountMeta::new(pool_pda, false),
        AccountMeta::new(profile_pda, false),
        AccountMeta::new(treasury_pda, false),
        AccountMeta::new(skr_escrow_pda, false),
        AccountMeta::new(authority_skr_token.pubkey(), false),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(solana_program::system_program::id(), false),
        // Yield vault & borrower position appended
        AccountMeta::new(yield_vault_pda, false),
        AccountMeta::new(borrower_yield_pda, false),
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data: claim_ix_data,
    };

    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
    transaction.sign(&[&payer, &authority], recent_blockhash);

    let result = banks_client.process_transaction(transaction).await;
    assert!(result.is_ok(), "ClaimDefault with yield sync MUST succeed! Result: {:?}", result);

    // Verify 20% slash (20_000_000 SKR) synced across all accounts:
    // 1. Escrow debited
    let updated_skr_escrow = banks_client.get_account(skr_escrow_pda).await.unwrap().unwrap();
    let skr_escrow_tok = spl_token::state::Account::unpack(&updated_skr_escrow.data).unwrap();
    assert_eq!(skr_escrow_tok.amount, 80_000_000);

    // 2. Profile debited
    let updated_profile_acc = banks_client.get_account(profile_pda).await.unwrap().unwrap();
    let updated_profile = UserProfile::unpack_from_slice(&updated_profile_acc.data).unwrap();
    assert_eq!(updated_profile.staked_skr, 80_000_000);

    // 3. High-2 fix: UserYieldPosition staked_skr debited to 80_000_000
    let updated_pos_acc = banks_client.get_account(borrower_yield_pda).await.unwrap().unwrap();
    let updated_pos = UserYieldPosition::unpack_from_slice(&updated_pos_acc.data).unwrap();
    assert_eq!(updated_pos.staked_skr, 80_000_000, "Borrower yield position must be synced down to 80,000,000");

    // 4. High-2 fix: SkrYieldVault total_staked_skr debited to 80_000_000 (no denominator inflation)
    let updated_vault_acc = banks_client.get_account(yield_vault_pda).await.unwrap().unwrap();
    let updated_vault = SkrYieldVault::unpack_from_slice(&updated_vault_acc.data).unwrap();
    assert_eq!(updated_vault.total_staked_skr, 80_000_000, "Vault total_staked_skr must be synced down to 80,000,000");
}

#[tokio::test]
async fn test_bank_claim_default_skr_collateral_success() {
    // Regression test for F1: SKR-collateral pool loans MUST be liquidatable.
    // The treasury-owned SKR account serves as the 5% margin treasury while the
    // authority-owned SKR account receives the lender's share — the treasury is
    // classified by role (owner == treasury PDA, mint == collateral mint), not
    // stolen by the slash-destination classifier.
    let program_id = Pubkey::new_unique();
    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let authority = Keypair::new();
    let borrower = Keypair::new();
    let pool_id: u64 = 1;
    let pool_id_bytes = pool_id.to_le_bytes();

    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id_bytes],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);

    let loan_id: u64 = 200;
    let loan_id_bytes = loan_id.to_le_bytes();
    let (loan_pda, _) = Pubkey::find_program_address(
        &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id_bytes],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan_pda.as_ref()], &program_id);
    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &program_id);

    let pool = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        name: [0u8; 32],
        liquidity_mint: Pubkey::new_unique(),
        vault_pda,
        total_liquidity: 10_000_000_000,
        total_borrowed: 100_000_000,
        staked_skr_amount: 0,
        interest_rate_bps: 600,
        max_ltv_bps: 8500,
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 1,
        loans_repaid: 0,
        is_oracle_free: true,
        pool_id,
        has_custom_oracle: false,
    };
    program_test.add_account(
        pool_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&pool).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    let loan = LoanOrder {
        discriminator: LoanOrder::DISCRIMINATOR,
        is_active: true,
        loan_id,
        borrower: borrower.pubkey(),
        pool: pool_pda,
        principal_amount: 100_000_000,
        collateral_mint: SKR_MINT, // SPL (SKR) collateral
        collateral_amount: 1_000_000_000, // 1,000 SKR
        interest_due: 1_000_000,
        origination_time: 1000,
        due_time: 2000,
        grace_period_expires: 0, // expired
        status: LoanStatus::InGracePeriod,
        locked_skr: 0,
    };
    program_test.add_account(
        loan_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&loan).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    // Collateral escrow: SPL token account (SKR mint, authority = escrow PDA) holding 1,000 SKR
    program_test.add_account(
        escrow_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, escrow_pda, 1_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    // Lender destination: authority-owned SKR token account (receives 95%)
    let authority_skr_token = Keypair::new();
    program_test.add_account(
        authority_skr_token.pubkey(),
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, authority.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    // Margin treasury: treasury-owned SKR token account (receives 5%)
    let treasury_skr_token = Keypair::new();
    program_test.add_account(
        treasury_skr_token.pubkey(),
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, treasury_pda, 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let claim_ix_data = borsh::to_vec(&ClockLendInstruction::ClaimDefault).unwrap();

    let accounts = vec![
        AccountMeta::new(authority.pubkey(), true),
        AccountMeta::new(loan_pda, false),
        AccountMeta::new(escrow_pda, false),
        AccountMeta::new(authority_skr_token.pubkey(), false), // SPL lender destination
        AccountMeta::new(pool_pda, false),
        AccountMeta::new(treasury_skr_token.pubkey(), false), // treasury-owned SKR margin account
        AccountMeta::new_readonly(spl_token::id(), false),
    ];

    let instruction = Instruction {
        program_id,
        accounts,
        data: claim_ix_data,
    };

    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
    transaction.sign(&[&payer, &authority], recent_blockhash);

    let result = banks_client.process_transaction(transaction).await;
    assert!(result.is_ok(), "ClaimDefault MUST succeed for SKR collateral! Result: {:?}", result);

    // 1. Escrow fully drained
    let updated_escrow = banks_client.get_account(escrow_pda).await.unwrap().unwrap();
    let escrow_tok = spl_token::state::Account::unpack(&updated_escrow.data).unwrap();
    assert_eq!(escrow_tok.amount, 0, "Escrow must be fully drained");

    // 2. Lender received 95% (1,000 - 50 = 950 SKR)
    let updated_dest = banks_client.get_account(authority_skr_token.pubkey()).await.unwrap().unwrap();
    let dest_tok = spl_token::state::Account::unpack(&updated_dest.data).unwrap();
    assert_eq!(dest_tok.amount, 950_000_000, "Lender destination must receive 950,000,000 SKR");

    // 3. Treasury received the 5% margin (50 SKR)
    let updated_treasury = banks_client.get_account(treasury_skr_token.pubkey()).await.unwrap().unwrap();
    let treasury_tok = spl_token::state::Account::unpack(&updated_treasury.data).unwrap();
    assert_eq!(treasury_tok.amount, 50_000_000, "Treasury must receive the 50,000,000 SKR margin");

    // 4. Loan marked Defaulted and total_borrowed unwound
    let updated_loan_acc = banks_client.get_account(loan_pda).await.unwrap().unwrap();
    let updated_loan = LoanOrder::unpack_from_slice(&updated_loan_acc.data).unwrap();
    assert_eq!(updated_loan.status, LoanStatus::Defaulted);
    assert_eq!(updated_loan.is_active, false);

    let updated_pool_acc = banks_client.get_account(pool_pda).await.unwrap().unwrap();
    let updated_pool = LendingPool::unpack_from_slice(&updated_pool_acc.data).unwrap();
    assert_eq!(updated_pool.total_borrowed, 0, "total_borrowed must be decremented on default");
}

#[tokio::test]
async fn test_bank_set_price_feed_and_borrow_dynamic_oracle_success() {
    let program_id = Pubkey::new_unique();
    let usdc_mint = Pubkey::new_unique();
    let authority = Keypair::new();
    let borrower = Keypair::new();
    let oracle_authority = Keypair::new();

    let pool_id: u64 = 77;
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);

    let loan_id: u64 = 101;
    let (loan_pda, _) = Pubkey::find_program_address(
        &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan_pda.as_ref()], &program_id);
    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &program_id);
    let treasury_usdc = Pubkey::new_unique();
    let borrower_usdc = Pubkey::new_unique();
    let borrower_collateral = Pubkey::new_unique();

    let (oracle_pda, _) = Pubkey::find_program_address(&[ORACLE_SEED, SKR_MINT.as_ref()], &program_id);

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let pool_state = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        liquidity_mint: usdc_mint,
        vault_pda,
        total_liquidity: 10_000_000_000,
        total_borrowed: 0,
        staked_skr_amount: 0,
        interest_rate_bps: 800,
        max_ltv_bps: 8000, // 80% LTV
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 0,
        loans_repaid: 0,
        name: [0u8; 32],
        is_oracle_free: true,
        pool_id,
        has_custom_oracle: false,
    };

    program_test.add_account(
        pool_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&pool_state).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        vault_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, vault_pda, 10_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        treasury_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, treasury_pda, 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        borrower_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, borrower.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        borrower_collateral,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, borrower.pubkey(), 100_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        oracle_authority.pubkey(),
        Account {
            lamports: 1_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        borrower.pubkey(),
        Account {
            lamports: 1_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        SKR_MINT,
        Account {
            lamports: 10_000_000,
            data: mint_data(6),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (admin_pda, _) = Pubkey::find_program_address(&[ADMIN_SEED], &program_id);
    program_test.add_account(
        admin_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&AdminConfig {
                discriminator: AdminConfig::DISCRIMINATOR,
                is_initialized: true,
                admin: oracle_authority.pubkey(),
                oracle_authority: oracle_authority.pubkey(),
            })
            .unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // 1. Set Oracle price feed for SKR to $0.05 (50,000 micro-USD) instead of baseline $0.02
    let set_price_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(oracle_authority.pubkey(), true),
            AccountMeta::new(oracle_pda, false),
            AccountMeta::new_readonly(SKR_MINT, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(sysvar::clock::id(), false),
            AccountMeta::new_readonly(admin_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::SetPriceFeed {
            price_micro_usd: 50_000, // $0.05
            decimals: 6,
        })
        .unwrap(),
    };

    let mut tx1 = Transaction::new_with_payer(&[set_price_ix], Some(&payer.pubkey()));
    tx1.sign(&[&payer, &oracle_authority], recent_blockhash);
    let res1 = banks_client.process_transaction(tx1).await;
    assert!(res1.is_ok(), "SetPriceFeed transaction MUST succeed! Result: {:?}", res1);

    // Verify on-chain PriceFeed state
    let oracle_account_data = banks_client.get_account(oracle_pda).await.unwrap().unwrap();
    let feed = PriceFeed::unpack_from_slice(&oracle_account_data.data).unwrap();
    assert_eq!(feed.is_initialized, true);
    assert_eq!(feed.price_micro_usd, 50_000);
    assert_eq!(feed.decimals, 6);
    assert_eq!(feed.mint, SKR_MINT);
    assert_eq!(feed.authority, oracle_authority.pubkey());

    // 2. Borrower borrows $35 USDC against 1,000 SKR collateral
    // Under baseline ($0.02), 1,000 SKR = $20 -> max borrow at 80% LTV was $16.
    // Under dynamic oracle ($0.05), 1,000 SKR = $50 -> max borrow at 80% LTV is $40.
    // So $35 USDC borrow is valid only thanks to the dynamic price feed!
    let borrow_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(loan_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(borrower_usdc, false),
            AccountMeta::new(borrower_collateral, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new_readonly(SKR_MINT, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new(treasury_usdc, false),
            AccountMeta::new_readonly(oracle_pda, false), // Trailing dynamic oracle account
        ],
        data: borsh::to_vec(&ClockLendInstruction::BorrowFromPool {
            loan_id,
            borrow_amount: 35_000_000, // $35 USDC
            collateral_amount: 1_000_000_000, // 1000 SKR
            duration_seconds: 86400 * 7,
        })
        .unwrap(),
    };

    let mut tx2 = Transaction::new_with_payer(&[borrow_ix], Some(&payer.pubkey()));
    tx2.sign(&[&payer, &borrower], recent_blockhash);
    let res2 = banks_client.process_transaction(tx2).await;
    assert!(res2.is_ok(), "Borrow with dynamic price feed MUST succeed! Result: {:?}", res2);

    // Verify loan order is active on-chain
    let loan_acc = banks_client.get_account(loan_pda).await.unwrap().unwrap();
    let loan = LoanOrder::unpack_from_slice(&loan_acc.data).unwrap();
    assert_eq!(loan.is_active, true);
    assert_eq!(loan.principal_amount, 35_000_000);
}

#[tokio::test]
async fn test_bank_borrow_rejects_stale_oracle_price() {
    let program_id = Pubkey::new_unique();
    let usdc_mint = Pubkey::new_unique();
    let authority = Keypair::new();
    let borrower = Keypair::new();
    let oracle_authority = Keypair::new();

    let pool_id: u64 = 88;
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);

    let loan_id: u64 = 202;
    let (loan_pda, _) = Pubkey::find_program_address(
        &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan_pda.as_ref()], &program_id);
    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &program_id);
    let treasury_usdc = Pubkey::new_unique();
    let borrower_usdc = Pubkey::new_unique();
    let borrower_collateral = Pubkey::new_unique();

    let (oracle_pda, _) = Pubkey::find_program_address(&[ORACLE_SEED, SKR_MINT.as_ref()], &program_id);

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let pool_state = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        liquidity_mint: usdc_mint,
        vault_pda,
        total_liquidity: 10_000_000_000,
        total_borrowed: 0,
        staked_skr_amount: 0,
        interest_rate_bps: 800,
        max_ltv_bps: 8000,
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 0,
        loans_repaid: 0,
        name: [0u8; 32],
        is_oracle_free: false,
        pool_id,
        has_custom_oracle: false,
    };

    program_test.add_account(
        pool_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&pool_state).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        vault_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, vault_pda, 10_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        treasury_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, treasury_pda, 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        borrower_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, borrower.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        borrower_collateral,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, borrower.pubkey(), 100_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    // Add stale oracle account: updated at timestamp 1 (more than 24h old)
    let stale_feed = PriceFeed {
        discriminator: PriceFeed::DISCRIMINATOR,
        is_initialized: true,
        mint: SKR_MINT,
        price_micro_usd: 50_000,
        decimals: 6,
        last_updated_at: 1, // Ancient timestamp -> STALE
        authority: oracle_authority.pubkey(),
        max_staleness_seconds: 86400,
    };
    program_test.add_account(
        oracle_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&stale_feed).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let borrow_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(loan_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(borrower_usdc, false),
            AccountMeta::new(borrower_collateral, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new_readonly(SKR_MINT, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new(treasury_usdc, false),
            AccountMeta::new_readonly(oracle_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::BorrowFromPool {
            loan_id,
            borrow_amount: 10_000_000,
            collateral_amount: 1_000_000_000,
            duration_seconds: 86400 * 7,
        })
        .unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[borrow_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &borrower], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 29, "Borrow MUST fail when oracle feed is stale (> 24h)!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::StaleOraclePrice as u32, "Error must be StaleOraclePrice");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }
}

#[tokio::test]
async fn test_bank_set_price_feed_rejects_unauthorized_signer() {
    let program_id = Pubkey::new_unique();
    let original_authority = Keypair::new();
    let attacker = Keypair::new();

    let (oracle_pda, _) = Pubkey::find_program_address(&[ORACLE_SEED, SKR_MINT.as_ref()], &program_id);

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let existing_feed = PriceFeed {
        discriminator: PriceFeed::DISCRIMINATOR,
        is_initialized: true,
        mint: SKR_MINT,
        price_micro_usd: 20_000,
        decimals: 6,
        last_updated_at: 1720000000,
        authority: original_authority.pubkey(),
        max_staleness_seconds: 86400,
    };
    program_test.add_account(
        oracle_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&existing_feed).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // Attacker attempts to update the price feed
    let malicious_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(attacker.pubkey(), true), // Malicious attacker signs!
            AccountMeta::new(oracle_pda, false),
            AccountMeta::new_readonly(SKR_MINT, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(sysvar::clock::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::SetPriceFeed {
            price_micro_usd: 999_999_000, // Attacker tries to artificially pump collateral price
            decimals: 6,
        })
        .unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[malicious_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &attacker], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 5, "Attacker MUST NOT be able to overwrite oracle price feed!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::Unauthorized as u32, "Error must be Unauthorized");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }
}

#[tokio::test]
async fn test_bank_skr_bond_cannot_be_withdrawn_while_loan_is_active() {
    let program_id = Pubkey::new_unique();
    let usdc_mint = Pubkey::new_unique();
    let authority = Keypair::new();
    let borrower = Keypair::new();

    let pool_id: u64 = 1;
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);

    let loan_id: u64 = 777;
    let (loan_pda, _) = Pubkey::find_program_address(
        &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan_pda.as_ref()], &program_id);
    let (profile_pda, _) = Pubkey::find_program_address(&[PROFILE_SEED, borrower.pubkey().as_ref()], &program_id);
    let (skr_escrow_pda, _) = Pubkey::find_program_address(&[b"skr_escrow", borrower.pubkey().as_ref()], &program_id);
    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &program_id);

    let borrower_usdc = Keypair::new();
    let borrower_skr = Keypair::new();
    let treasury_usdc = Keypair::new();

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let pool = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        liquidity_mint: usdc_mint,
        vault_pda,
        total_liquidity: 10_000_000_000,
        total_borrowed: 0,
        staked_skr_amount: 0,
        interest_rate_bps: 1000, // 10%
        max_ltv_bps: 8000,
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 0,
        loans_repaid: 0,
        name: [0u8; 32],
        is_oracle_free: true,
        pool_id,
        has_custom_oracle: false,
    };

    program_test.add_account(
        pool_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&pool).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    program_test.add_account(
        vault_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, vault_pda, 10_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        borrower.pubkey(),
        Account {
            lamports: 10_000_000_000, // 10 SOL to fund collateral and rent
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    program_test.add_account(
        borrower_usdc.pubkey(),
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, borrower.pubkey(), 200_000_000), // pre-funded with USDC
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    program_test.add_account(
        borrower_skr.pubkey(),
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, borrower.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    program_test.add_account(
        skr_escrow_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, skr_escrow_pda, 1_000_000_000), // 1000 SKR staked
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let profile = UserProfile {
        discriminator: UserProfile::DISCRIMINATOR,
        is_initialized: true,
        user: borrower.pubkey(),
        staked_skr: 1_000_000_000, // 1000 SKR
        total_loans_completed: 0,
        total_loans_defaulted: 0,
        reputation_score: 10000,
        locked_skr: 0,
    };
    program_test.add_account(
        profile_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&profile).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    program_test.add_account(
        treasury_usdc.pubkey(),
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, treasury_pda, 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // 1. Borrower borrows from pool using SOL collateral and provides user profile for 50% discount
    let borrow_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(loan_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(borrower_usdc.pubkey(), false),
            AccountMeta::new(borrower.pubkey(), false),          // borrower_collateral_account
            AccountMeta::new(escrow_pda, false),                 // collateral_escrow_account
            AccountMeta::new_readonly(Pubkey::default(), false), // collateral_mint (Native SOL)
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new(profile_pda, false),
            AccountMeta::new(treasury_usdc.pubkey(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::BorrowFromPool {
            loan_id,
            borrow_amount: 100_000_000, // 100 USDC
            collateral_amount: 1_000_000_000, // 1 SOL
            duration_seconds: 86400 * 7,
        }).unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[borrow_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &borrower], recent_blockhash);
    banks_client.process_transaction(tx).await.unwrap();

    // Verify loan was created with 1,000 SKR locked bond
    let loan_acc = banks_client.get_account(loan_pda).await.unwrap().unwrap();
    let loan = LoanOrder::unpack_from_slice(&loan_acc.data).unwrap();
    assert_eq!(loan.locked_skr, 1_000_000_000, "Loan order must have 1000 SKR locked");
    assert!(loan.is_active);

    // Verify profile has locked_skr == 1_000_000_000
    let profile_acc = banks_client.get_account(profile_pda).await.unwrap().unwrap();
    let prof = UserProfile::unpack_from_slice(&profile_acc.data).unwrap();
    assert_eq!(prof.locked_skr, 1_000_000_000, "User profile locked_skr must be 1000 SKR");

    // 2. Borrower attempts to unstake SKR while loan is active -> MUST FAIL with StakeLocked!
    let unstake_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(profile_pda, false),
            AccountMeta::new(borrower_skr.pubkey(), false),
            AccountMeta::new(skr_escrow_pda, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::UnstakeSKR {
            amount: 1_000_000_000, // Try to withdraw full bond
        }).unwrap(),
    };

    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[unstake_ix.clone()], Some(&payer.pubkey()));
    tx.sign(&[&payer, &borrower], blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 30, "Borrower MUST NOT be able to unstake SKR while loan is active!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::StakeLocked as u32, "Error must be StakeLocked");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }

    // Even attempting to unstake 1 token must fail
    let unstake_1_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(profile_pda, false),
            AccountMeta::new(borrower_skr.pubkey(), false),
            AccountMeta::new(skr_escrow_pda, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::UnstakeSKR {
            amount: 1,
        }).unwrap(),
    };
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[unstake_1_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &borrower], blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 30, "unpinned assertion");

    // 3. Borrower repays loan -> releases locked SKR bond
    let total_due = loan.principal_amount + loan.interest_due;
    let repay_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(loan_pda, false),
            AccountMeta::new(borrower_usdc.pubkey(), false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new(borrower.pubkey(), false),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(profile_pda, false),
            AccountMeta::new(treasury_usdc.pubkey(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::RepayLoan {
            repay_amount: total_due,
        }).unwrap(),
    };

    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[repay_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &borrower], blockhash);
    banks_client.process_transaction(tx).await.unwrap();

    // Verify profile locked_skr is released to 0
    let profile_acc = banks_client.get_account(profile_pda).await.unwrap().unwrap();
    let prof = UserProfile::unpack_from_slice(&profile_acc.data).unwrap();
    assert_eq!(prof.locked_skr, 0, "User profile locked_skr must be 0 after repayment");

    // 4. Now unstake succeeds!
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[unstake_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &borrower], blockhash);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_ok(), "Unstake must succeed once loan is repaid! Result: {:?}", res);

    // Verify tokens were transferred to borrower wallet
    let borrower_skr_acc = banks_client.get_account(borrower_skr.pubkey()).await.unwrap().unwrap();
    let tok = spl_token::state::Account::unpack(&borrower_skr_acc.data).unwrap();
    assert_eq!(tok.amount, 1_000_000_000, "Borrower must have received unstaked tokens");
}

#[tokio::test]
async fn test_bank_claim_default_slashes_locked_bond() {
    let program_id = Pubkey::new_unique();
    let authority = Keypair::new();
    let borrower = Keypair::new();
    let pool_id: u64 = 1;
    let pool_id_bytes = pool_id.to_le_bytes();

    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id_bytes],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);

    let loan_id: u64 = 888;
    let loan_id_bytes = loan_id.to_le_bytes();
    let (loan_pda, _) = Pubkey::find_program_address(
        &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id_bytes],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan_pda.as_ref()], &program_id);
    let (profile_pda, _) = Pubkey::find_program_address(&[PROFILE_SEED, borrower.pubkey().as_ref()], &program_id);
    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &program_id);
    let (skr_escrow_pda, _) = Pubkey::find_program_address(&[b"skr_escrow", borrower.pubkey().as_ref()], &program_id);

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let pool = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        liquidity_mint: Pubkey::new_unique(),
        vault_pda,
        total_liquidity: 10_000_000_000,
        total_borrowed: 100_000_000,
        staked_skr_amount: 0,
        interest_rate_bps: 1000,
        max_ltv_bps: 8000,
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 1,
        loans_repaid: 0,
        name: [0u8; 32],
        is_oracle_free: true,
        pool_id,
        has_custom_oracle: false,
    };
    program_test.add_account(
        pool_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&pool).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    // Loan had 1,000 SKR locked bond (50% discount tier) and expired in grace period
    let loan = LoanOrder {
        discriminator: LoanOrder::DISCRIMINATOR,
        is_active: true,
        loan_id,
        borrower: borrower.pubkey(),
        pool: pool_pda,
        principal_amount: 100_000_000,
        collateral_mint: Pubkey::default(), // Native SOL
        collateral_amount: 1_000_000_000,
        interest_due: 1_000_000,
        origination_time: 1000,
        due_time: 2000,
        grace_period_expires: 0, // expired
        status: LoanStatus::InGracePeriod,
        locked_skr: 1_000_000_000, // 1,000 SKR locked bond
    };
    program_test.add_account(
        loan_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&loan).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    program_test.add_account(
        escrow_pda,
        Account {
            lamports: 1_000_000_000,
            data: vec![],
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    let profile = UserProfile {
        discriminator: UserProfile::DISCRIMINATOR,
        is_initialized: true,
        user: borrower.pubkey(),
        staked_skr: 1_000_000_000, // 1000 SKR staked
        total_loans_completed: 0,
        total_loans_defaulted: 0,
        reputation_score: 5000,
        locked_skr: 1_000_000_000, // 1000 SKR locked
    };
    program_test.add_account(
        profile_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&profile).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    // Borrower's skr_escrow with 1,000 SKR
    program_test.add_account(
        skr_escrow_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, skr_escrow_pda, 1_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    // Authority's dedicated SKR slash destination token account with 0 SKR
    let authority_skr_token = Keypair::new();
    program_test.add_account(
        authority_skr_token.pubkey(),
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, authority.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let claim_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(authority.pubkey(), true),
            AccountMeta::new(loan_pda, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new(authority.pubkey(), false), // Native SOL destination
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(profile_pda, false),
            AccountMeta::new(treasury_pda, false),
            AccountMeta::new(skr_escrow_pda, false),
            AccountMeta::new(authority_skr_token.pubkey(), false), // Dedicated SKR slash destination
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::ClaimDefault).unwrap(),
    };

    let mut transaction = Transaction::new_with_payer(&[claim_ix], Some(&payer.pubkey()));
    transaction.sign(&[&payer, &authority], recent_blockhash);
    let result = banks_client.process_transaction(transaction).await;
    assert!(result.is_ok(), "ClaimDefault with locked SKR bond must succeed! Result: {:?}", result);

    // 1. Borrower escrow was slashed for full locked bond (1,000 SKR -> 0)
    let updated_skr_escrow = banks_client.get_account(skr_escrow_pda).await.unwrap().unwrap();
    let skr_escrow_tok = spl_token::state::Account::unpack(&updated_skr_escrow.data).unwrap();
    assert_eq!(skr_escrow_tok.amount, 0, "SKR escrow balance must be 0 after full locked bond slashed");

    // 2. Authority SKR slash destination credited with 1,000 SKR
    let updated_slash_dest = banks_client.get_account(authority_skr_token.pubkey()).await.unwrap().unwrap();
    let slash_dest_tok = spl_token::state::Account::unpack(&updated_slash_dest.data).unwrap();
    assert_eq!(slash_dest_tok.amount, 1_000_000_000, "Slash destination must receive full 1,000 SKR bond");

    // 3. UserProfile staked_skr and locked_skr debited
    let updated_profile_acc = banks_client.get_account(profile_pda).await.unwrap().unwrap();
    let updated_profile = UserProfile::unpack_from_slice(&updated_profile_acc.data).unwrap();
    assert_eq!(updated_profile.staked_skr, 0);
    assert_eq!(updated_profile.locked_skr, 0);
    assert_eq!(updated_profile.total_loans_defaulted, 1);
}

#[tokio::test]
async fn test_bank_initialize_pool_rejects_raw_native_sol_liquidity_mint() {
    let program_id = Pubkey::new_unique();
    let pool_id: u64 = 99;

    let program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, payer.pubkey().as_ref(), &pool_id.to_le_bytes()],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);

    // Attempt to initialize pool with raw system_program::ID as liquidity mint
    let init_sol_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false), // Raw SOL mint!
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::InitializePool {
            pool_id,
            pool_type: PoolType::Individual,
            interest_rate_bps: 500,
            max_ltv_bps: 7000,
            min_duration: 86400,
            max_duration: 86400 * 30,
            name: [0u8; 32],
            is_oracle_free: false,
        }).unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[init_sol_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 27, "Pool initialization with raw native SOL liquidity mint MUST fail!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::UnsupportedCollateralMint as u32, "Error must be UnsupportedCollateralMint");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }

    // Now initialize with Wrapped SOL (spl_token::native_mint::id()) -> SUCCEEDS!
    let pool_id_wsol: u64 = 100;
    let (pool_pda_wsol, _) = Pubkey::find_program_address(
        &[POOL_SEED, payer.pubkey().as_ref(), &pool_id_wsol.to_le_bytes()],
        &program_id,
    );
    let (vault_pda_wsol, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda_wsol.as_ref()], &program_id);

    let init_wsol_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(pool_pda_wsol, false),
            AccountMeta::new_readonly(spl_token::native_mint::id(), false), // WSOL SPL token mint
            AccountMeta::new(vault_pda_wsol, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::InitializePool {
            pool_id: pool_id_wsol,
            pool_type: PoolType::Individual,
            interest_rate_bps: 500,
            max_ltv_bps: 7000,
            min_duration: 86400,
            max_duration: 86400 * 30,
            name: [0u8; 32],
            is_oracle_free: false,
        }).unwrap(),
    };

    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx_wsol = Transaction::new_with_payer(&[init_wsol_ix], Some(&payer.pubkey()));
    tx_wsol.sign(&[&payer], blockhash);
    let res_wsol = banks_client.process_transaction(tx_wsol).await;
    assert!(res_wsol.is_ok(), "Pool initialization with Wrapped SOL mint MUST succeed!");
}

#[tokio::test]
async fn test_bank_create_p2p_offer_rejects_unreasonable_ltv() {
    let program_id = Pubkey::new_unique();

    let (sol_oracle_pda, _) = Pubkey::find_program_address(
        &[ORACLE_SEED, spl_token::native_mint::id().as_ref()],
        &program_id,
    );
    let sol_feed = PriceFeed {
        discriminator: PriceFeed::DISCRIMINATOR,
        is_initialized: true,
        mint: spl_token::native_mint::id(),
        price_micro_usd: 150_000_000, // $150.00 / SOL
        decimals: 9,
        last_updated_at: now_secs(),
        max_staleness_seconds: 86400,
        authority: Pubkey::default(),
    };

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );
    program_test.add_account(
        sol_oracle_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&sol_feed).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let offer_id: u64 = 1;
    let (offer_pda, _) = Pubkey::find_program_address(
        &[P2P_SEED, payer.pubkey().as_ref(), &offer_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, offer_pda.as_ref()], &program_id);

    // Attacker tries to offer 1 lamport of SOL collateral and request 1,000,000 USDC ($1M)
    let spam_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(offer_pda, false),
            AccountMeta::new(payer.pubkey(), false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false), // Native SOL
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(sol_oracle_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::CreateP2POffer {
            offer_id,
            requested_amount: 1_000_000_000_000, // 1M USDC
            collateral_amount: 1, // 1 lamport
            interest_offered: 10_000_000,
            duration_seconds: 86400 * 7,
        }).unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[spam_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 10, "P2P offer with 1 lamport collateral asking for 1M USDC MUST be rejected!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::InvalidCollateralRatio as u32, "Error must be InvalidCollateralRatio");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }

    // Reasonable offer: 1 SOL collateral ($150) asking for 100 USDC -> SUCCEEDS!
    let offer_id_valid: u64 = 2;
    let (offer_pda_valid, _) = Pubkey::find_program_address(
        &[P2P_SEED, payer.pubkey().as_ref(), &offer_id_valid.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda_valid, _) = Pubkey::find_program_address(&[ESCROW_SEED, offer_pda_valid.as_ref()], &program_id);

    let valid_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(offer_pda_valid, false),
            AccountMeta::new(payer.pubkey(), false),
            AccountMeta::new(escrow_pda_valid, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false), // Native SOL
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(sol_oracle_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::CreateP2POffer {
            offer_id: offer_id_valid,
            requested_amount: 100_000_000, // 100 USDC (well within 150% LTV of 1 SOL = $150)
            collateral_amount: 1_000_000_000, // 1 SOL
            interest_offered: 10_000_000,
            duration_seconds: 86400 * 7,
        }).unwrap(),
    };

    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx_valid = Transaction::new_with_payer(&[valid_ix], Some(&payer.pubkey()));
    tx_valid.sign(&[&payer], blockhash);
    let res_valid = banks_client.process_transaction(tx_valid).await;
    assert!(res_valid.is_ok(), "P2P offer with reasonable LTV MUST succeed!");
}

#[tokio::test]
async fn test_bank_create_p2p_offer_rejects_unallowlisted_liquidity_mint() {
    // F5 regression: the loan asset (liquidity_mint) is allowlisted to
    // 6-decimal USD-pegged mints (devnet/mainnet USDC). A creator cannot
    // declare an arbitrary mint and exploit the base-unit vs micro-USD
    // comparison (e.g. a 2-decimal token denominated as if it were USDC).
    let program_id = Pubkey::new_unique();

    let (sol_oracle_pda, _) = Pubkey::find_program_address(
        &[ORACLE_SEED, spl_token::native_mint::id().as_ref()],
        &program_id,
    );
    let sol_feed = PriceFeed {
        discriminator: PriceFeed::DISCRIMINATOR,
        is_initialized: true,
        mint: spl_token::native_mint::id(),
        price_micro_usd: 150_000_000, // $150.00 / SOL
        decimals: 9,
        last_updated_at: now_secs(),
        max_staleness_seconds: 86400,
        authority: Pubkey::default(),
    };

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );
    program_test.add_account(
        sol_oracle_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&sol_feed).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    // A fake 2-decimal token mint, owned by the token program (82 bytes)
    let fake_mint = Keypair::new();
    program_test.add_account(
        fake_mint.pubkey(),
        Account {
            lamports: 10_000_000,
            data: mint_data(2),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let offer_id: u64 = 7;
    let (offer_pda, _) = Pubkey::find_program_address(
        &[P2P_SEED, payer.pubkey().as_ref(), &offer_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, offer_pda.as_ref()], &program_id);

    // 1 SOL collateral, 100 USDC requested — reasonable under the real feed,
    // but the creator declares the fake 2-decimal mint as the loan asset.
    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(offer_pda, false),
            AccountMeta::new(payer.pubkey(), false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false), // Native SOL collateral
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(sol_oracle_pda, false),
            AccountMeta::new_readonly(fake_mint.pubkey(), false), // Unallowlisted loan asset
        ],
        data: borsh::to_vec(&ClockLendInstruction::CreateP2POffer {
            offer_id,
            requested_amount: 100_000_000, // 100 units of the fake mint
            collateral_amount: 1_000_000_000, // 1 SOL
            interest_offered: 10_000_000,
            duration_seconds: 86400 * 7,
        }).unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[ix], Some(&payer.pubkey()));
    tx.sign(&[&payer], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 17, "An unallowlisted loan-asset mint MUST be rejected!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::InvalidMint as u32, "Error must be InvalidMint");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }
}

#[tokio::test]
async fn test_bank_oracle_permissionless_claim_rejected_v2() {
    let program_id = Pubkey::new_unique();
    let admin_authority = Keypair::new();
    let attacker = Keypair::new();
    let sol_mint = spl_token::native_mint::id();

    let (admin_pda, _) = Pubkey::find_program_address(&[ADMIN_SEED], &program_id);
    let (oracle_pda, _) = Pubkey::find_program_address(&[ORACLE_SEED, sol_mint.as_ref()], &program_id);

    let program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let (program_data_pda, _) = Pubkey::find_program_address(
        &[program_id.as_ref()],
        &solana_program::bpf_loader_upgradeable::id(),
    );
    let mut program_data_bytes = vec![0u8; 45];
    program_data_bytes[0] = 3;
    program_data_bytes[12] = 1;
    program_data_bytes[13..45].copy_from_slice(admin_authority.pubkey().as_ref());

    let mut program_test = program_test;
    program_test.add_account(
        program_data_pda,
        Account {
            lamports: 10_000_000,
            data: program_data_bytes,
            owner: solana_program::bpf_loader_upgradeable::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        admin_authority.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        attacker.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // 1. Attacker attempts to claim uninitialized global oracle feed without AdminConfig
    let attacker_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(attacker.pubkey(), true),
            AccountMeta::new(oracle_pda, false),
            AccountMeta::new_readonly(sol_mint, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::SetPriceFeed {
            price_micro_usd: 1_000_000_000_000, // Attacker tries to set SOL = $1,000,000!
            decimals: 9,
        })
        .unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[attacker_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &attacker], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 5, "Attacker MUST NOT be able to claim uninitialized oracle feed without admin authorization!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::Unauthorized as u32, "Error must be Unauthorized");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }

    // 2. Legitimate admin initializes AdminConfig via InitializeAdmin
    let init_admin_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(admin_authority.pubkey(), true),
            AccountMeta::new(admin_pda, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(program_data_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::InitializeAdmin).unwrap(),
    };
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx_admin = Transaction::new_with_payer(&[init_admin_ix], Some(&payer.pubkey()));
    tx_admin.sign(&[&payer, &admin_authority], blockhash);
    let res_admin = banks_client.process_transaction(tx_admin).await;
    assert!(res_admin.is_ok(), "AdminConfig initialization MUST succeed!");

    // 3. Attacker tries to call SetPriceFeed supplying AdminConfig PDA but signing as attacker
    let attacker_ix_with_admin = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(attacker.pubkey(), true),
            AccountMeta::new(oracle_pda, false),
            AccountMeta::new_readonly(sol_mint, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(admin_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::SetPriceFeed {
            price_micro_usd: 1_000_000_000_000,
            decimals: 9,
        })
        .unwrap(),
    };
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx_attacker2 = Transaction::new_with_payer(&[attacker_ix_with_admin], Some(&payer.pubkey()));
    tx_attacker2.sign(&[&payer, &attacker], blockhash);
    let res_attacker2 = banks_client.process_transaction(tx_attacker2).await;
    expect_custom_error(&res_attacker2, 5, "Attacker MUST NOT initialize feed using AdminConfig!");
    match res_attacker2.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::Unauthorized as u32, "Error must be Unauthorized");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }

    // 4. Authorized admin initializes the price feed
    let valid_feed_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(admin_authority.pubkey(), true),
            AccountMeta::new(oracle_pda, false),
            AccountMeta::new_readonly(sol_mint, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(admin_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::SetPriceFeed {
            price_micro_usd: 150_000_000, // $150.00 / SOL
            decimals: 9,
        })
        .unwrap(),
    };
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx_valid = Transaction::new_with_payer(&[valid_feed_ix], Some(&payer.pubkey()));
    tx_valid.sign(&[&payer, &admin_authority], blockhash);
    let res_valid = banks_client.process_transaction(tx_valid).await;
    assert!(res_valid.is_ok(), "Admin initializing price feed MUST succeed!");

    // 5. Attacker tries to overwrite initialized price feed
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx_overwrite = Transaction::new_with_payer(&[
        Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new(attacker.pubkey(), true),
                AccountMeta::new(oracle_pda, false),
                AccountMeta::new_readonly(sol_mint, false),
                AccountMeta::new_readonly(solana_program::system_program::id(), false),
            ],
            data: borsh::to_vec(&ClockLendInstruction::SetPriceFeed {
                price_micro_usd: 1_000_000_000_000,
                decimals: 9,
            })
            .unwrap(),
        }
    ], Some(&payer.pubkey()));
    tx_overwrite.sign(&[&payer, &attacker], blockhash);
    let res_overwrite = banks_client.process_transaction(tx_overwrite).await;
    expect_custom_error(&res_overwrite, 5, "Attacker MUST NOT overwrite existing price feed!");
    match res_overwrite.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::Unauthorized as u32, "Error must be Unauthorized");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }
}

#[tokio::test]
async fn test_bank_initialize_admin_rejects_non_upgrade_authority() {
    // F3/F4 regression: no hardcoded key — the ONLY valid caller is the key
    // recorded in the ProgramData account's upgrade-authority field.
    let program_id = Pubkey::new_unique();
    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let real_authority = Keypair::new();
    let attacker = Keypair::new();

    let (admin_pda, _) = Pubkey::find_program_address(&[ADMIN_SEED], &program_id);
    let (program_data_pda, _) = Pubkey::find_program_address(
        &[program_id.as_ref()],
        &solana_program::bpf_loader_upgradeable::id(),
    );

    let mut program_data_bytes = vec![0u8; 45];
    program_data_bytes[0] = 3;
    program_data_bytes[12] = 1;
    program_data_bytes[13..45].copy_from_slice(real_authority.pubkey().as_ref());
    program_test.add_account(
        program_data_pda,
        Account {
            lamports: 10_000_000,
            data: program_data_bytes,
            owner: solana_program::bpf_loader_upgradeable::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        attacker.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let attacker_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(attacker.pubkey(), true),
            AccountMeta::new(admin_pda, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(program_data_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::InitializeAdmin).unwrap(),
    };
    let mut tx = Transaction::new_with_payer(&[attacker_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &attacker], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 5, "A non-upgrade-authority signer MUST be rejected");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::Unauthorized as u32, "Error must be Unauthorized");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }
}

#[tokio::test]
async fn test_bank_initialize_admin_requires_programdata() {
    // F3/F4 regression: omitting the ProgramData account must fail closed.
    let program_id = Pubkey::new_unique();
    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let caller = Keypair::new();
    let (admin_pda, _) = Pubkey::find_program_address(&[ADMIN_SEED], &program_id);
    program_test.add_account(
        caller.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(caller.pubkey(), true),
            AccountMeta::new(admin_pda, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::InitializeAdmin).unwrap(),
    };
    let mut tx = Transaction::new_with_payer(&[ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &caller], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 5, "Omitting the ProgramData account MUST be rejected");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::Unauthorized as u32, "Error must be Unauthorized");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }
}

#[tokio::test]
async fn test_bank_oracle_optionality_exploit_rejected_v3() {
    let program_id = Pubkey::new_unique();
    let usdc_mint = Pubkey::new_unique();
    let authority = Keypair::new();
    let borrower = Keypair::new();
    let admin = Keypair::new();
    let sol_mint = spl_token::native_mint::id();

    let pool_id: u64 = 901;
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);
    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &program_id);
    let (admin_pda, _) = Pubkey::find_program_address(&[ADMIN_SEED], &program_id);
    let (oracle_pda, _) = Pubkey::find_program_address(&[ORACLE_SEED, sol_mint.as_ref()], &program_id);

    let treasury_usdc = Pubkey::new_unique();
    let borrower_usdc = Pubkey::new_unique();

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    // Dynamic pool: is_oracle_free is FALSE, max LTV is 75%
    let pool_state = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        liquidity_mint: usdc_mint,
        vault_pda,
        total_liquidity: 10_000_000_000,
        total_borrowed: 0,
        staked_skr_amount: 0,
        interest_rate_bps: 800,
        max_ltv_bps: 7500, // 75% max LTV
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 0,
        loans_repaid: 0,
        name: [0u8; 32],
        is_oracle_free: false,
        pool_id,
        has_custom_oracle: false,
    };

    program_test.add_account(
        pool_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&pool_state).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        vault_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, vault_pda, 10_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        treasury_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, treasury_pda, 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        borrower_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, borrower.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        admin_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&AdminConfig {
                discriminator: AdminConfig::DISCRIMINATOR,
                is_initialized: true,
                admin: admin.pubkey(),
                oracle_authority: admin.pubkey(),
            })
            .unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        admin.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        borrower.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // Admin provisions live oracle for SOL at $50.00 / SOL (50,000,000 micro-USD)
    let set_price_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(admin.pubkey(), true),
            AccountMeta::new(oracle_pda, false),
            AccountMeta::new_readonly(sol_mint, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(sysvar::clock::id(), false),
            AccountMeta::new_readonly(admin_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::SetPriceFeed {
            price_micro_usd: 50_000_000, // $50.00 / SOL
            decimals: 9,
        })
        .unwrap(),
    };
    let mut tx_set = Transaction::new_with_payer(&[set_price_ix], Some(&payer.pubkey()));
    tx_set.sign(&[&payer, &admin], recent_blockhash);
    banks_client.process_transaction(tx_set).await.unwrap();

    // Borrower wants 100 USDC against 1 SOL collateral.
    // Baseline ($150) would allow: 1 SOL * 75% = $112.50 >= $100.
    // Live price ($50) allows: 1 SOL * 75% = $37.50 < $100 (REJECT).

    // Exploit Attempt A: Borrower omits the oracle account to force baseline fallback
    let loan_id_1: u64 = 101;
    let (loan_pda_1, _) = Pubkey::find_program_address(
        &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id_1.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda_1, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan_pda_1.as_ref()], &program_id);

    let exploit_ix_omit_oracle = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(loan_pda_1, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(borrower_usdc, false),
            AccountMeta::new(borrower.pubkey(), true), // native SOL collateral source
            AccountMeta::new(escrow_pda_1, false),
            AccountMeta::new_readonly(sol_mint, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new(treasury_usdc, false),
            // Oracle account intentionally omitted!
        ],
        data: borsh::to_vec(&ClockLendInstruction::BorrowFromPool {
            loan_id: loan_id_1,
            borrow_amount: 100_000_000, // 100 USDC
            collateral_amount: 1_000_000_000, // 1 SOL
            duration_seconds: 86400 * 7,
        })
        .unwrap(),
    };

    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx_exploit1 = Transaction::new_with_payer(&[exploit_ix_omit_oracle], Some(&payer.pubkey()));
    tx_exploit1.sign(&[&payer, &borrower], blockhash);
    let res_exploit1 = banks_client.process_transaction(tx_exploit1).await;
    expect_custom_error(&res_exploit1, 28, "Omitting oracle on dynamic pool MUST fail with InvalidOracleAccount!");
    match res_exploit1.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::InvalidOracleAccount as u32, "Error must be InvalidOracleAccount");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }

    // Exploit Attempt B: Borrower supplies an uninitialized dummy account as oracle
    let dummy_oracle = Keypair::new();
    let exploit_ix_uninit_oracle = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(loan_pda_1, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(borrower_usdc, false),
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(escrow_pda_1, false),
            AccountMeta::new_readonly(sol_mint, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new(treasury_usdc, false),
            AccountMeta::new_readonly(dummy_oracle.pubkey(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::BorrowFromPool {
            loan_id: loan_id_1,
            borrow_amount: 100_000_000,
            collateral_amount: 1_000_000_000,
            duration_seconds: 86400 * 7,
        })
        .unwrap(),
    };

    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx_exploit2 = Transaction::new_with_payer(&[exploit_ix_uninit_oracle], Some(&payer.pubkey()));
    tx_exploit2.sign(&[&payer, &borrower], blockhash);
    let res_exploit2 = banks_client.process_transaction(tx_exploit2).await;
    expect_custom_error(&res_exploit2, 28, "Passing uninitialized oracle on dynamic pool MUST fail with InvalidOracleAccount!");
    match res_exploit2.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::InvalidOracleAccount as u32, "Error must be InvalidOracleAccount");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }

    // Attempt C: Borrower supplies live $50 oracle feed with 100 USDC borrow (exceeds 75% LTV at $50/SOL)
    let borrow_ix_with_oracle = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(loan_pda_1, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(borrower_usdc, false),
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(escrow_pda_1, false),
            AccountMeta::new_readonly(sol_mint, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new(treasury_usdc, false),
            AccountMeta::new_readonly(oracle_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::BorrowFromPool {
            loan_id: loan_id_1,
            borrow_amount: 100_000_000, // 100 USDC
            collateral_amount: 1_000_000_000, // 1 SOL
            duration_seconds: 86400 * 7,
        })
        .unwrap(),
    };

    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx_fail_ltv = Transaction::new_with_payer(&[borrow_ix_with_oracle], Some(&payer.pubkey()));
    tx_fail_ltv.sign(&[&payer, &borrower], blockhash);
    let res_fail_ltv = banks_client.process_transaction(tx_fail_ltv).await;
    expect_custom_error(&res_fail_ltv, 10, "100 USDC borrow against 1 SOL at $50 MUST fail InvalidCollateralRatio!");
    match res_fail_ltv.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::InvalidCollateralRatio as u32, "Error must be InvalidCollateralRatio");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }

    // Legitimate Path: Borrower requests 30 USDC (<= 37.50 USDC allowed) with live $50 oracle feed
    let valid_borrow_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(loan_pda_1, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(borrower_usdc, false),
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(escrow_pda_1, false),
            AccountMeta::new_readonly(sol_mint, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new(treasury_usdc, false),
            AccountMeta::new_readonly(oracle_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::BorrowFromPool {
            loan_id: loan_id_1,
            borrow_amount: 30_000_000, // 30 USDC (well within 75% LTV of $50)
            collateral_amount: 1_000_000_000, // 1 SOL
            duration_seconds: 86400 * 7,
        })
        .unwrap(),
    };

    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx_valid = Transaction::new_with_payer(&[valid_borrow_ix], Some(&payer.pubkey()));
    tx_valid.sign(&[&payer, &borrower], blockhash);
    let res_valid = banks_client.process_transaction(tx_valid).await;
    assert!(res_valid.is_ok(), "Borrowing within live oracle valuation MUST succeed! Result: {:?}", res_valid);
}

#[tokio::test]
async fn test_bank_oracle_free_pool_baseline_success() {
    let program_id = Pubkey::new_unique();
    let usdc_mint = Pubkey::new_unique();
    let authority = Keypair::new();
    let borrower = Keypair::new();
    let sol_mint = spl_token::native_mint::id();

    let pool_id: u64 = 902;
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);
    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &program_id);

    let loan_id: u64 = 102;
    let (loan_pda, _) = Pubkey::find_program_address(
        &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan_pda.as_ref()], &program_id);

    let treasury_usdc = Pubkey::new_unique();
    let borrower_usdc = Pubkey::new_unique();

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    // Pool explicitly configured as oracle-free
    let pool_state = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        liquidity_mint: usdc_mint,
        vault_pda,
        total_liquidity: 10_000_000_000,
        total_borrowed: 0,
        staked_skr_amount: 0,
        interest_rate_bps: 800,
        max_ltv_bps: 7500, // 75% max LTV
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 0,
        loans_repaid: 0,
        name: [0u8; 32],
        is_oracle_free: true,
        pool_id,
        has_custom_oracle: false,
    };

    program_test.add_account(
        pool_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&pool_state).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        vault_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, vault_pda, 10_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        treasury_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, treasury_pda, 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        borrower_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, borrower.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        borrower.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // Borrow 100 USDC against 1 SOL collateral with NO oracle account provided
    // In oracle-free pool, baseline $150/SOL is used: 1 SOL * 75% = $112.50 >= 100 USDC -> OK!
    let borrow_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(loan_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(borrower_usdc, false),
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new_readonly(sol_mint, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new(treasury_usdc, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::BorrowFromPool {
            loan_id,
            borrow_amount: 100_000_000, // 100 USDC
            collateral_amount: 1_000_000_000, // 1 SOL
            duration_seconds: 86400 * 7,
        })
        .unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[borrow_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &borrower], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_ok(), "Borrow on oracle-free pool using baseline price MUST succeed! Result: {:?}", res);
}

#[tokio::test]
async fn test_bank_pool_specific_oracle_gating() {
    let program_id = Pubkey::new_unique();
    let pool_authority = Keypair::new();
    let attacker = Keypair::new();
    let sol_mint = spl_token::native_mint::id();

    let pool_id: u64 = 903;
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, pool_authority.pubkey().as_ref(), &pool_id.to_le_bytes()],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);
    let (pool_oracle_pda, _) = Pubkey::find_program_address(
        &[ORACLE_SEED, pool_pda.as_ref(), sol_mint.as_ref()],
        &program_id,
    );

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    let pool_state = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: pool_authority.pubkey(),
        liquidity_mint: Pubkey::new_unique(),
        vault_pda,
        total_liquidity: 1_000_000_000,
        total_borrowed: 0,
        staked_skr_amount: 0,
        interest_rate_bps: 800,
        max_ltv_bps: 7500,
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 0,
        loans_repaid: 0,
        name: [0u8; 32],
        is_oracle_free: false,
        pool_id,
        has_custom_oracle: false,
    };

    program_test.add_account(
        pool_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&pool_state).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        pool_authority.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        attacker.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // 1. Attacker attempts to initialize pool-specific oracle
    let attacker_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(attacker.pubkey(), true),
            AccountMeta::new(pool_oracle_pda, false),
            AccountMeta::new_readonly(sol_mint, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(pool_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::SetPriceFeed {
            price_micro_usd: 999_999_999,
            decimals: 9,
        })
        .unwrap(),
    };

    let mut tx_attacker = Transaction::new_with_payer(&[attacker_ix], Some(&payer.pubkey()));
    tx_attacker.sign(&[&payer, &attacker], recent_blockhash);
    let res_attacker = banks_client.process_transaction(tx_attacker).await;
    expect_custom_error(&res_attacker, 5, "Non-pool authority MUST NOT initialize pool-specific oracle!");
    match res_attacker.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::Unauthorized as u32, "Error must be Unauthorized");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }

    // 2. Legitimate pool authority initializes pool-specific oracle
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let auth_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(pool_authority.pubkey(), true),
            AccountMeta::new(pool_oracle_pda, false),
            AccountMeta::new_readonly(sol_mint, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(pool_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::SetPriceFeed {
            price_micro_usd: 140_000_000,
            decimals: 9,
        })
        .unwrap(),
    };

    let mut tx_auth = Transaction::new_with_payer(&[auth_ix], Some(&payer.pubkey()));
    tx_auth.sign(&[&payer, &pool_authority], blockhash);
    let res_auth = banks_client.process_transaction(tx_auth).await;
    assert!(res_auth.is_ok(), "Pool authority initializing pool-specific oracle MUST succeed!");
}

#[tokio::test]
async fn test_bank_withdraw_treasury_admin_auth_success_and_exploit_rejected() {
    let program_id = Pubkey::new_unique();
    let admin = Keypair::new();
    let attacker = Keypair::new();

    let (admin_pda, _) = Pubkey::find_program_address(&[ADMIN_SEED], &program_id);
    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &program_id);

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    // Pre-populate AdminConfig
    program_test.add_account(
        admin_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&AdminConfig {
                discriminator: AdminConfig::DISCRIMINATOR,
                is_initialized: true,
                admin: admin.pubkey(),
                oracle_authority: admin.pubkey(),
            })
            .unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    // Pre-populate Treasury with 5 SOL
    program_test.add_account(
        treasury_pda,
        Account {
            lamports: 5_000_000_000,
            data: vec![],
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    program_test.add_account(
        admin.pubkey(),
        Account {
            lamports: 1_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        attacker.pubkey(),
        Account {
            lamports: 1_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // 1. Attacker attempts to withdraw from Treasury
    let attacker_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(attacker.pubkey(), true),
            AccountMeta::new_readonly(admin_pda, false),
            AccountMeta::new(treasury_pda, false),
            AccountMeta::new(attacker.pubkey(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::WithdrawTreasury {
            amount: 1_000_000_000,
        })
        .unwrap(),
    };

    let mut tx_attacker = Transaction::new_with_payer(&[attacker_ix], Some(&payer.pubkey()));
    tx_attacker.sign(&[&payer, &attacker], recent_blockhash);
    let res_attacker = banks_client.process_transaction(tx_attacker).await;
    expect_custom_error(&res_attacker, 5, "Non-admin MUST NOT withdraw treasury funds!");
    match res_attacker.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::Unauthorized as u32);
        }
        err => panic!("Unexpected error: {:?}", err),
    }

    // 2. Legitimate admin withdraws 1 SOL from Treasury
    let admin_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(admin.pubkey(), true),
            AccountMeta::new_readonly(admin_pda, false),
            AccountMeta::new(treasury_pda, false),
            AccountMeta::new(admin.pubkey(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::WithdrawTreasury {
            amount: 1_000_000_000,
        })
        .unwrap(),
    };

    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx_admin = Transaction::new_with_payer(&[admin_ix], Some(&payer.pubkey()));
    tx_admin.sign(&[&payer, &admin], blockhash);
    let res_admin = banks_client.process_transaction(tx_admin).await;
    assert!(res_admin.is_ok(), "Admin withdrawing treasury funds MUST succeed! Result: {:?}", res_admin);

    let treasury_acc = banks_client.get_account(treasury_pda).await.unwrap().unwrap();
    assert_eq!(treasury_acc.lamports, 4_000_000_000);
}

#[tokio::test]
async fn test_bank_type_confusion_loan_as_p2p_offer_rejected() {
    // C-1 & H-1: Passing a LoanOrder account into FundP2POffer MUST fail closed
    let program_id = Pubkey::new_unique();
    let funder = Keypair::new();
    let borrower = Keypair::new();
    let authority = Keypair::new();

    let pool_id: u64 = 1;
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()],
        &program_id,
    );
    let loan_id: u64 = 999;
    let (loan_pda, _) = Pubkey::find_program_address(
        &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id.to_le_bytes()],
        &program_id,
    );

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    // Pre-populate legitimate LoanOrder (which has LoanOrder::DISCRIMINATOR)
    let loan_state = LoanOrder {
        discriminator: LoanOrder::DISCRIMINATOR,
        is_active: true,
        loan_id,
        borrower: borrower.pubkey(),
        pool: pool_pda,
        principal_amount: 100_000_000,
        collateral_mint: Pubkey::default(),
        collateral_amount: 1_000_000_000,
        interest_due: 1_000_000,
        origination_time: 1000,
        due_time: 2000,
        grace_period_expires: 0,
        status: LoanStatus::Active,
        locked_skr: 0,
    };
    program_test.add_account(
        loan_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&loan_state).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    program_test.add_account(
        funder.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // Attacker passes loan_pda as p2p_offer_account to FundP2POffer
    let fund_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(funder.pubkey(), true),
            AccountMeta::new(loan_pda, false), // <--- TYPE CONFUSION ATTEMPT
            AccountMeta::new(Pubkey::new_unique(), false),
            AccountMeta::new(Pubkey::new_unique(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(borrower.pubkey(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::FundP2POffer).unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[fund_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &funder], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 31, "Passing LoanOrder to FundP2POffer MUST fail closed!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert!(
                code == ClockLendError::InvalidAccountData as u32
                    || code == ClockLendError::InvalidSeeds as u32
            );
        }
        err => panic!("Unexpected error: {:?}", err),
    }
}

#[tokio::test]
async fn test_bank_p2p_offer_already_active_rejected() {
    // C-2: Overwriting an already active P2P offer PDA MUST be rejected
    let program_id = Pubkey::new_unique();
    let creator = Keypair::new();
    let offer_id: u64 = 42;

    let (offer_pda, _) = Pubkey::find_program_address(
        &[P2P_SEED, creator.pubkey().as_ref(), &offer_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(
        &[b"p2p_escrow", offer_pda.as_ref()],
        &program_id,
    );

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    // Pre-populate active P2POffer
    use clock_lend::state::{OfferStatus, P2POffer};
    let active_offer = P2POffer {
        discriminator: P2POffer::DISCRIMINATOR,
        is_initialized: true,
        offer_id,
        creator: creator.pubkey(),
        funder: Pubkey::default(),
        collateral_mint: Pubkey::default(),
        liquidity_mint: Pubkey::default(),
        collateral_amount: 1_000_000_000,
        requested_amount: 100_000_000,
        interest_offered: 5_000_000,
        duration_seconds: 86400 * 7,
        created_at: 1000,
        due_time: 0,
        grace_period_expires: 0,
        status: OfferStatus::Open,
    };
    program_test.add_account(
        offer_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&active_offer).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        creator.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // Creator or attacker attempts to call CreateP2POffer reusing active offer_pda
    let create_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(creator.pubkey(), true),
            AccountMeta::new(offer_pda, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new(creator.pubkey(), false),
            AccountMeta::new_readonly(Pubkey::default(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::CreateP2POffer {
            offer_id,
            collateral_amount: 1_000_000_000,
            requested_amount: 100_000_000,
            interest_offered: 5_000_000,
            duration_seconds: 86400 * 7,
        })
        .unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[create_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &creator], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 34, "Re-initializing active offer MUST fail!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::OfferAlreadyActive as u32, "Error must be OfferAlreadyActive");
        }
        err => panic!("Unexpected error: {:?}", err),
    }
}

#[tokio::test]
async fn test_bank_p2p_offer_lifecycle_create_fund_repay() {
    let program_id = Pubkey::new_unique();
    let usdc_mint = clock_lend::state::USDC_DEVNET_MINT;
    let creator = Keypair::new();
    let funder = Keypair::new();
    let offer_id: u64 = 55;

    let (offer_pda, _) = Pubkey::find_program_address(
        &[P2P_SEED, creator.pubkey().as_ref(), &offer_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(
        &[ESCROW_SEED, offer_pda.as_ref()],
        &program_id,
    );

    let (sol_oracle_pda, _) = Pubkey::find_program_address(
        &[ORACLE_SEED, spl_token::native_mint::id().as_ref()],
        &program_id,
    );
    let sol_feed = PriceFeed {
        discriminator: PriceFeed::DISCRIMINATOR,
        is_initialized: true,
        mint: spl_token::native_mint::id(),
        price_micro_usd: 150_000_000, // $150.00 / SOL
        decimals: 9,
        last_updated_at: now_secs(),
        max_staleness_seconds: 86400,
        authority: Pubkey::default(),
    };

    let creator_usdc = Pubkey::new_unique();
    let funder_usdc = Pubkey::new_unique();

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    program_test.add_account(
        sol_oracle_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&sol_feed).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    // Creator has 10 SOL
    program_test.add_account(
        creator.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    // Funder has 10 SOL
    program_test.add_account(
        funder.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    // Creator USDC account (starts with 200 USDC)
    program_test.add_account(
        creator_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, creator.pubkey(), 200_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    // Funder USDC account (starts with 500 USDC)
    program_test.add_account(
        funder_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, funder.pubkey(), 500_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // 1. Create P2P Offer: 1 SOL collateral for 100 USDC requested, 5 USDC interest, 7 days
    let create_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(creator.pubkey(), true),
            AccountMeta::new(offer_pda, false),
            AccountMeta::new(creator.pubkey(), true),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new_readonly(Pubkey::default(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(sol_oracle_pda, false),
            AccountMeta::new_readonly(usdc_mint, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::CreateP2POffer {
            offer_id,
            collateral_amount: 1_000_000_000,
            requested_amount: 100_000_000,
            interest_offered: 5_000_000,
            duration_seconds: 86400 * 7,
        })
        .unwrap(),
    };

    let mut tx_create = Transaction::new_with_payer(&[create_ix], Some(&payer.pubkey()));
    tx_create.sign(&[&payer, &creator], recent_blockhash);
    let res_create = banks_client.process_transaction(tx_create).await;
    assert!(res_create.is_ok(), "CreateP2POffer MUST succeed! Result: {:?}", res_create);

    // 2. Fund P2P Offer: Funder provides 100 USDC
    let fund_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(funder.pubkey(), true),
            AccountMeta::new(offer_pda, false),
            AccountMeta::new(funder_usdc, false),
            AccountMeta::new(creator_usdc, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(creator.pubkey(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::FundP2POffer).unwrap(),
    };

    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx_fund = Transaction::new_with_payer(&[fund_ix], Some(&payer.pubkey()));
    tx_fund.sign(&[&payer, &funder], blockhash);
    let res_fund = banks_client.process_transaction(tx_fund).await;
    assert!(res_fund.is_ok(), "FundP2POffer MUST succeed! Result: {:?}", res_fund);

    // 3. Repay P2P Loan: Creator repays 105 USDC (100 requested + 5 interest)
    let repay_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(creator.pubkey(), true),
            AccountMeta::new(offer_pda, false),
            AccountMeta::new(creator_usdc, false),
            AccountMeta::new(funder_usdc, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new(creator.pubkey(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::RepayLoan {
            repay_amount: 105_000_000,
        })
        .unwrap(),
    };

    let blockhash2 = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx_repay = Transaction::new_with_payer(&[repay_ix], Some(&payer.pubkey()));
    tx_repay.sign(&[&payer, &creator], blockhash2);
    let res_repay = banks_client.process_transaction(tx_repay).await;
    assert!(res_repay.is_ok(), "Repaying P2P offer MUST succeed! Result: {:?}", res_repay);

    // Verify offer status is Repaid
    let offer_acc = banks_client.get_account(offer_pda).await.unwrap().unwrap();
    use clock_lend::state::{OfferStatus, P2POffer};
    let offer_data = P2POffer::unpack_from_slice(&offer_acc.data).unwrap();
    assert_eq!(offer_data.status, OfferStatus::Repaid);
}

#[tokio::test]
async fn test_bank_p2p_fund_wrong_mint_rejected() {
    let program_id = Pubkey::new_unique();
    let usdc_mint = clock_lend::state::USDC_DEVNET_MINT;
    let fake_mint = Pubkey::new_unique();
    let creator = Keypair::new();
    let attacker = Keypair::new();
    let offer_id: u64 = 77;

    let (offer_pda, _) = Pubkey::find_program_address(
        &[P2P_SEED, creator.pubkey().as_ref(), &offer_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(
        &[ESCROW_SEED, offer_pda.as_ref()],
        &program_id,
    );

    let (sol_oracle_pda, _) = Pubkey::find_program_address(
        &[ORACLE_SEED, spl_token::native_mint::id().as_ref()],
        &program_id,
    );
    let sol_feed = PriceFeed {
        discriminator: PriceFeed::DISCRIMINATOR,
        is_initialized: true,
        mint: spl_token::native_mint::id(),
        price_micro_usd: 150_000_000,
        decimals: 9,
        last_updated_at: now_secs(),
        max_staleness_seconds: 86400,
        authority: Pubkey::default(),
    };

    let creator_usdc = Pubkey::new_unique();
    let attacker_fake_token = Pubkey::new_unique();

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    program_test.add_account(
        sol_oracle_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&sol_feed).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        creator.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        attacker.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        creator_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, creator.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        attacker_fake_token,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(fake_mint, attacker.pubkey(), 1_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // Create P2P offer asking for USDC
    let create_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(creator.pubkey(), true),
            AccountMeta::new(offer_pda, false),
            AccountMeta::new(creator.pubkey(), true),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new_readonly(Pubkey::default(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(sol_oracle_pda, false),
            AccountMeta::new_readonly(usdc_mint, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::CreateP2POffer {
            offer_id,
            collateral_amount: 1_000_000_000,
            requested_amount: 100_000_000,
            interest_offered: 5_000_000,
            duration_seconds: 86400 * 7,
        }).unwrap(),
    };

    let mut tx_create = Transaction::new_with_payer(&[create_ix], Some(&payer.pubkey()));
    tx_create.sign(&[&payer, &creator], recent_blockhash);
    banks_client.process_transaction(tx_create).await.unwrap();

    // Attacker tries to fund offer with fake_mint tokens
    let fund_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(attacker.pubkey(), true),
            AccountMeta::new(offer_pda, false),
            AccountMeta::new(attacker_fake_token, false),
            AccountMeta::new(creator_usdc, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(creator.pubkey(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::FundP2POffer).unwrap(),
    };

    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx_fund = Transaction::new_with_payer(&[fund_ix], Some(&payer.pubkey()));
    tx_fund.sign(&[&payer, &attacker], blockhash);
    let res = banks_client.process_transaction(tx_fund).await;
    expect_custom_error(&res, 17, "Funding with counterfeit token MUST fail with InvalidMint!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::InvalidMint as u32);
        }
        err => panic!("Unexpected error: {:?}", err),
    }
}

#[tokio::test]
async fn test_bank_p2p_repay_in_grace_period_success() {
    let program_id = Pubkey::new_unique();
    let usdc_mint = clock_lend::state::USDC_DEVNET_MINT;
    let creator = Keypair::new();
    let funder = Keypair::new();
    let offer_id: u64 = 88;

    let (offer_pda, _) = Pubkey::find_program_address(
        &[P2P_SEED, creator.pubkey().as_ref(), &offer_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(
        &[ESCROW_SEED, offer_pda.as_ref()],
        &program_id,
    );

    let creator_usdc = Pubkey::new_unique();
    let funder_usdc = Pubkey::new_unique();

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    use clock_lend::state::{OfferStatus, P2POffer};
    let grace_offer = P2POffer {
        discriminator: P2POffer::DISCRIMINATOR,
        is_initialized: true,
        offer_id,
        creator: creator.pubkey(),
        funder: funder.pubkey(),
        collateral_mint: Pubkey::default(), // Native SOL
        liquidity_mint: usdc_mint,
        collateral_amount: 1_000_000_000,
        requested_amount: 100_000_000,
        interest_offered: 5_000_000,
        duration_seconds: 86400 * 7,
        created_at: 1000,
        due_time: 1500,
        grace_period_expires: 2_000_000_000, // Future expiration
        status: OfferStatus::InGracePeriod,
    };

    program_test.add_account(
        offer_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&grace_offer).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );
    // Escrow account holding 1 SOL
    program_test.add_account(
        escrow_pda,
        Account {
            lamports: 1_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        creator.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        creator_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, creator.pubkey(), 200_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        funder_usdc,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, funder.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // Creator repays while in grace period
    let repay_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(creator.pubkey(), true),
            AccountMeta::new(offer_pda, false),
            AccountMeta::new(creator_usdc, false),
            AccountMeta::new(funder_usdc, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new(creator.pubkey(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::RepayLoan {
            repay_amount: 105_000_000,
        }).unwrap(),
    };

    let mut tx_repay = Transaction::new_with_payer(&[repay_ix], Some(&payer.pubkey()));
    tx_repay.sign(&[&payer, &creator], recent_blockhash);
    let res = banks_client.process_transaction(tx_repay).await;
    assert!(res.is_ok(), "Repaying in grace period MUST succeed! Result: {:?}", res);

    let offer_acc = banks_client.get_account(offer_pda).await.unwrap().unwrap();
    let offer_data = P2POffer::unpack_from_slice(&offer_acc.data).unwrap();
    assert_eq!(offer_data.status, OfferStatus::Repaid);
}

#[tokio::test]
async fn test_bank_p2p_create_without_oracle_rejected() {
    let program_id = Pubkey::new_unique();
    let creator = Keypair::new();
    let offer_id: u64 = 99;

    let (offer_pda, _) = Pubkey::find_program_address(
        &[P2P_SEED, creator.pubkey().as_ref(), &offer_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(
        &[ESCROW_SEED, offer_pda.as_ref()],
        &program_id,
    );

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );
    program_test.add_account(
        creator.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // Call CreateP2POffer WITHOUT providing oracle account
    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(creator.pubkey(), true),
            AccountMeta::new(offer_pda, false),
            AccountMeta::new(creator.pubkey(), true),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new_readonly(Pubkey::default(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::CreateP2POffer {
            offer_id,
            collateral_amount: 1_000_000_000,
            requested_amount: 100_000_000,
            interest_offered: 5_000_000,
            duration_seconds: 86400 * 7,
        }).unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &creator], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 28, "CreateP2POffer without oracle MUST fail closed!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::InvalidOracleAccount as u32);
        }
        err => panic!("Unexpected error: {:?}", err),
    }
}

#[tokio::test]
async fn test_bank_p2p_cancel_with_dust_succeeds() {
    let program_id = Pubkey::new_unique();
    let creator = Keypair::new();
    let offer_id: u64 = 101;

    let (offer_pda, _) = Pubkey::find_program_address(
        &[P2P_SEED, creator.pubkey().as_ref(), &offer_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(
        &[ESCROW_SEED, offer_pda.as_ref()],
        &program_id,
    );

    let creator_skr = Pubkey::new_unique();

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    use clock_lend::state::{OfferStatus, P2POffer, SKR_MINT};
    let open_offer = P2POffer {
        discriminator: P2POffer::DISCRIMINATOR,
        is_initialized: true,
        offer_id,
        creator: creator.pubkey(),
        funder: Pubkey::default(),
        collateral_mint: SKR_MINT,
        liquidity_mint: clock_lend::state::USDC_DEVNET_MINT,
        collateral_amount: 1_000_000_000, // 1000 SKR
        requested_amount: 20_000_000,
        interest_offered: 1_000_000,
        duration_seconds: 86400 * 7,
        created_at: 1000,
        due_time: 0,
        grace_period_expires: 0,
        status: OfferStatus::Open,
    };

    program_test.add_account(
        offer_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&open_offer).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );
    // Escrow token account has collateral + 1 unit of dust (donated by griefer!)
    program_test.add_account(
        escrow_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, escrow_pda, 1_000_000_001),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        creator.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        creator_skr,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, creator.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // Creator cancels offer despite dust
    let cancel_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(creator.pubkey(), true),
            AccountMeta::new(offer_pda, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new(creator_skr, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::CancelP2POffer).unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[cancel_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &creator], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_ok(), "Cancelling offer with dust in escrow MUST succeed and drain all tokens! Result: {:?}", res);

    // Escrow account is now closed (lamports 0 or None)
    let escrow_opt = banks_client.get_account(escrow_pda).await.unwrap();
    assert!(escrow_opt.is_none() || escrow_opt.unwrap().lamports == 0);
}

#[tokio::test]
async fn test_bank_claim_default_without_token_program_rejected() {
    let program_id = Pubkey::new_unique();
    let authority = Keypair::new();
    let borrower = Keypair::new();
    let pool_account = Pubkey::new_unique();
    let loan_account = Pubkey::new_unique();
    let (escrow_pda, _) = Pubkey::find_program_address(
        &[ESCROW_SEED, loan_account.as_ref()],
        &program_id,
    );
    let authority_skr = Pubkey::new_unique();

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );

    use clock_lend::state::{LendingPool, LoanOrder, LoanStatus, PoolType, SKR_MINT};
    let pool = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_id: 1,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        liquidity_mint: clock_lend::state::USDC_DEVNET_MINT,
        vault_pda: Pubkey::new_unique(),
        total_liquidity: 100_000_000,
        total_borrowed: 50_000_000,
        staked_skr_amount: 0,
        interest_rate_bps: 1000,
        max_ltv_bps: 7000,
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 1,
        loans_repaid: 0,
        name: [0u8; 32],
        is_oracle_free: false,
        has_custom_oracle: false,
    };

    let expired_loan = LoanOrder {
        discriminator: LoanOrder::DISCRIMINATOR,
        is_active: true,
        loan_id: 1,
        borrower: borrower.pubkey(),
        pool: pool_account,
        principal_amount: 50_000_000,
        collateral_mint: SKR_MINT,
        collateral_amount: 1_000_000_000,
        interest_due: 5_000_000,
        origination_time: 1000,
        due_time: 2000,
        grace_period_expires: 3000, // Expired in the past relative to current bank time
        status: LoanStatus::InGracePeriod,
        locked_skr: 0,
    };

    program_test.add_account(
        pool_account,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&pool).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        loan_account,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&expired_loan).unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        escrow_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, escrow_pda, 1_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        authority_skr,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, authority.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        authority.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // Caller calls ClaimDefault on SPL token loan WITHOUT providing spl_token program
    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(authority.pubkey(), true),
            AccountMeta::new(loan_account, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new(authority_skr, false),
            AccountMeta::new(pool_account, false),
            // Notice: spl_token program is omitted!
        ],
        data: borsh::to_vec(&ClockLendInstruction::ClaimDefault).unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &authority], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 24, "ClaimDefault on SPL token without token program MUST fail!");

    // Verify loan was NOT corrupted to Defaulted
    let loan_acc = banks_client.get_account(loan_account).await.unwrap().unwrap();
    let loan_state = LoanOrder::unpack_from_slice(&loan_acc.data).unwrap();
    assert_eq!(loan_state.status, LoanStatus::InGracePeriod, "Loan state must not be corrupted!");
}

#[tokio::test]
async fn test_bank_stake_skr_rejects_frontrun_escrow_authority() {
    // Front-run defense: a token account pre-created at the skr_escrow PDA
    // with an attacker-controlled authority must be rejected, otherwise the
    // attacker could drain every token staked into it.
    use solana_program::system_program;

    let program_id = Pubkey::new_unique();
    let mut program_test = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));

    let user = Keypair::new();
    let attacker = Keypair::new();

    let (profile_pda, _) = Pubkey::find_program_address(&[PROFILE_SEED, user.pubkey().as_ref()], &program_id);
    let (skr_escrow_pda, _) = Pubkey::find_program_address(&[b"skr_escrow", user.pubkey().as_ref()], &program_id);

    program_test.add_account(
        skr_escrow_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, attacker.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let user_skr = Keypair::new();
    program_test.add_account(
        user_skr.pubkey(),
        Account {
            lamports: 10_000_000,
            data: token_acct_data(SKR_MINT, user.pubkey(), 1_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        user.pubkey(),
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_program::system_program::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(user.pubkey(), true),
            AccountMeta::new(profile_pda, false),
            AccountMeta::new(user_skr.pubkey(), false),
            AccountMeta::new(skr_escrow_pda, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(SKR_MINT, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::StakeSKR { amount: 100_000_000 }).unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &user], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 18, "Front-run escrow with attacker authority MUST be rejected!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::InvalidAccountOwner as u32, "Error must be InvalidAccountOwner");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }
}

#[tokio::test]
async fn test_bank_create_p2p_offer_rejects_excessive_interest() {
    // V-A1: interest beyond the principal would make the offer permanently
    // unrepayable — reject at creation.
    let program_id = Pubkey::new_unique();
    let program_test = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let offer_id: u64 = 1;
    let (offer_pda, _) = Pubkey::find_program_address(
        &[P2P_SEED, payer.pubkey().as_ref(), &offer_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, offer_pda.as_ref()], &program_id);

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(offer_pda, false),
            AccountMeta::new(payer.pubkey(), false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::CreateP2POffer {
            offer_id,
            requested_amount: 100_000_000,
            collateral_amount: 1_000_000_000,
            interest_offered: 200_000_000, // > principal — must be rejected
            duration_seconds: 86400 * 7,
        }).unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[ix], Some(&payer.pubkey()));
    tx.sign(&[&payer], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 0, "Interest exceeding the principal MUST be rejected!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::InvalidInstruction as u32, "Error must be InvalidInstruction");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }
}

#[tokio::test]
async fn test_bank_claim_default_native_rejects_vault_destination() {
    // F6: raw lamports liquidated into the SPL vault token account would be
    // permanently stranded — native SOL must go to the pool authority wallet.
    let program_id = Pubkey::new_unique();
    let mut program_test = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));

    let authority = Keypair::new();
    let borrower = Keypair::new();
    let pool_id: u64 = 1;
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);
    let loan_id: u64 = 300;
    let (loan_pda, _) = Pubkey::find_program_address(
        &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan_pda.as_ref()], &program_id);
    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &program_id);

    let pool = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        name: [0u8; 32],
        liquidity_mint: Pubkey::new_unique(),
        vault_pda,
        total_liquidity: 0,
        total_borrowed: 100_000_000,
        staked_skr_amount: 0,
        interest_rate_bps: 600,
        max_ltv_bps: 8500,
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 1,
        loans_repaid: 0,
        is_oracle_free: true,
        pool_id,
        has_custom_oracle: false,
    };
    program_test.add_account(pool_pda, Account {
        lamports: 10_000_000,
        data: borsh::to_vec(&pool).unwrap(),
        owner: program_id,
        executable: false,
        rent_epoch: 0,
    });

    let loan = LoanOrder {
        discriminator: LoanOrder::DISCRIMINATOR,
        is_active: true,
        loan_id,
        borrower: borrower.pubkey(),
        pool: pool_pda,
        principal_amount: 100_000_000,
        collateral_mint: Pubkey::default(),
        collateral_amount: 1_000_000_000,
        interest_due: 0,
        origination_time: 1000,
        due_time: 2000,
        grace_period_expires: 0,
        status: LoanStatus::InGracePeriod,
        locked_skr: 0,
    };
    program_test.add_account(loan_pda, Account {
        lamports: 10_000_000,
        data: borsh::to_vec(&loan).unwrap(),
        owner: program_id,
        executable: false,
        rent_epoch: 0,
    });
    program_test.add_account(escrow_pda, Account {
        lamports: 1_000_000_000,
        data: vec![],
        owner: program_id,
        executable: false,
        rent_epoch: 0,
    });
    program_test.add_account(treasury_pda, Account {
        lamports: 10_000_000,
        data: vec![],
        owner: solana_program::system_program::id(),
        executable: false,
        rent_epoch: 0,
    });

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(authority.pubkey(), true),
            AccountMeta::new(loan_pda, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new(vault_pda, false), // WRONG: vault is an SPL token account
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(treasury_pda, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::ClaimDefault).unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &authority], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 5, "Native SOL liquidation into the vault MUST be rejected!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::Unauthorized as u32, "Error must be Unauthorized");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }
}


#[tokio::test]
async fn test_bank_p2p_grace_trigger_transitions_and_gates() {
    // P2P grace: only the creator or funder may trigger it, only after
    // due_time, and the transition Funded -> InGracePeriod must persist.
    use clock_lend::state::{OfferStatus, P2POffer};

    let program_id = Pubkey::new_unique();
    let mut program_test = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));

    let creator = Keypair::new();
    let funder = Keypair::new();
    let outsider = Keypair::new();

    let offer_id: u64 = 1;
    let (offer_pda, _) = Pubkey::find_program_address(
        &[P2P_SEED, creator.pubkey().as_ref(), &offer_id.to_le_bytes()],
        &program_id,
    );

    // Offer A: funded and past due
    let offer_a = P2POffer {
        discriminator: P2POffer::DISCRIMINATOR,
        is_initialized: true,
        offer_id,
        creator: creator.pubkey(),
        funder: funder.pubkey(),
        collateral_mint: Pubkey::default(), // native SOL
        liquidity_mint: clock_lend::state::USDC_DEVNET_MINT,
        collateral_amount: 1_000_000_000,
        requested_amount: 100_000_000,
        interest_offered: 10_000_000,
        duration_seconds: 60,
        created_at: 1000,
        due_time: 0, // already past due
        grace_period_expires: 0,
        status: OfferStatus::Funded,
    };
    program_test.add_account(offer_pda, Account {
        lamports: 10_000_000,
        data: borsh::to_vec(&offer_a).unwrap(),
        owner: program_id,
        executable: false,
        rent_epoch: 0,
    });
    program_test.add_account(funder.pubkey(), Account {
        lamports: 10_000_000_000,
        data: vec![],
        owner: solana_program::system_program::id(),
        executable: false,
        rent_epoch: 0,
    });
    program_test.add_account(outsider.pubkey(), Account {
        lamports: 10_000_000_000,
        data: vec![],
        owner: solana_program::system_program::id(),
        executable: false,
        rent_epoch: 0,
    });


    // 4. Offer B: funded but NOT yet due -> grace rejected with LoanNotDue
    let offer_id_b: u64 = 2;
    let (offer_b_pda, _) = Pubkey::find_program_address(
        &[P2P_SEED, creator.pubkey().as_ref(), &offer_id_b.to_le_bytes()],
        &program_id,
    );
    let offer_b = P2POffer {
        due_time: 4_000_000_000, // far future
        ..offer_a.clone()
    };
    program_test.add_account(offer_b_pda, Account {
        lamports: 10_000_000,
        data: borsh::to_vec(&offer_b).unwrap(),
        owner: program_id,
        executable: false,
        rent_epoch: 0,
    });

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // 1. Outsider cannot trigger grace
    let outsider_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(outsider.pubkey(), true),
            AccountMeta::new(offer_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::TriggerGracePeriod).unwrap(),
    };
    let mut tx = Transaction::new_with_payer(&[outsider_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &outsider], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 23, "A non-party MUST NOT trigger P2P grace!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::UnauthorizedCaller as u32, "Error must be UnauthorizedCaller");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }

    // 2. Funder triggers grace after due_time -> Funded -> InGracePeriod
    let grace_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(funder.pubkey(), true),
            AccountMeta::new(offer_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::TriggerGracePeriod).unwrap(),
    };
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[grace_ix.clone()], Some(&payer.pubkey()));
    tx.sign(&[&payer, &funder], blockhash);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_ok(), "Funder MUST be able to trigger grace past due_time! Result: {:?}", res);

    let offer_acc = banks_client.get_account(offer_pda).await.unwrap().unwrap();
    let offer = P2POffer::unpack_from_slice(&offer_acc.data).unwrap();
    assert_eq!(offer.status, OfferStatus::InGracePeriod, "Offer must transition to InGracePeriod");
    assert!(offer.grace_period_expires > 0, "Grace expiry must be set to now + 86400");

    let grace_b_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(funder.pubkey(), true),
            AccountMeta::new(offer_b_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::TriggerGracePeriod).unwrap(),
    };
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[grace_b_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &funder], blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 6, "Grace before due_time MUST be rejected!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::LoanNotDue as u32, "Error must be LoanNotDue");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }
}

#[tokio::test]
async fn test_bank_p2p_claim_default_after_grace() {
    // P2P liquidation: only the funder may claim after grace expiry; the
    // collateral moves to the funder and the offer becomes Defaulted.
    use clock_lend::state::{OfferStatus, P2POffer};

    let program_id = Pubkey::new_unique();
    let mut program_test = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));

    let creator = Keypair::new();
    let funder = Keypair::new();
    let outsider = Keypair::new();

    let offer_id: u64 = 7;
    let (offer_pda, _) = Pubkey::find_program_address(
        &[P2P_SEED, creator.pubkey().as_ref(), &offer_id.to_le_bytes()],
        &program_id,
    );
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, offer_pda.as_ref()], &program_id);

    let offer = P2POffer {
        discriminator: P2POffer::DISCRIMINATOR,
        is_initialized: true,
        offer_id,
        creator: creator.pubkey(),
        funder: funder.pubkey(),
        collateral_mint: Pubkey::default(), // native SOL
        liquidity_mint: clock_lend::state::USDC_DEVNET_MINT,
        collateral_amount: 1_000_000_000,
        requested_amount: 100_000_000,
        interest_offered: 10_000_000,
        duration_seconds: 60,
        created_at: 1000,
        due_time: 2000,
        grace_period_expires: 0, // expired
        status: OfferStatus::InGracePeriod,
    };
    program_test.add_account(offer_pda, Account {
        lamports: 10_000_000,
        data: borsh::to_vec(&offer).unwrap(),
        owner: program_id,
        executable: false,
        rent_epoch: 0,
    });
    program_test.add_account(escrow_pda, Account {
        lamports: 1_000_000_000,
        data: vec![],
        owner: program_id,
        executable: false,
        rent_epoch: 0,
    });
    program_test.add_account(funder.pubkey(), Account {
        lamports: 10_000_000_000,
        data: vec![],
        owner: solana_program::system_program::id(),
        executable: false,
        rent_epoch: 0,
    });
    program_test.add_account(outsider.pubkey(), Account {
        lamports: 10_000_000_000,
        data: vec![],
        owner: solana_program::system_program::id(),
        executable: false,
        rent_epoch: 0,
    });

    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // 1. Outsider cannot claim
    let outsider_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(outsider.pubkey(), true),
            AccountMeta::new(offer_pda, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new(outsider.pubkey(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::ClaimDefault).unwrap(),
    };
    let mut tx = Transaction::new_with_payer(&[outsider_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &outsider], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 23, "A non-funder MUST NOT claim P2P collateral!");
    match res.unwrap_err() {
        BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(code))) => {
            assert_eq!(code, ClockLendError::UnauthorizedCaller as u32, "Error must be UnauthorizedCaller");
        }
        err => panic!("Unexpected error variant: {:?}", err),
    }

    // 2. Funder claims after grace expiry
    let funder_before = banks_client.get_account(funder.pubkey()).await.unwrap().unwrap().lamports;
    let claim_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(funder.pubkey(), true),
            AccountMeta::new(offer_pda, false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new(funder.pubkey(), false), // native SOL destination == funder
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::ClaimDefault).unwrap(),
    };
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[claim_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &funder], blockhash);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_ok(), "Funder MUST be able to claim defaulted collateral! Result: {:?}", res);

    let escrow = banks_client.get_account(escrow_pda).await.unwrap();
    assert!(escrow.is_none(), "Escrow must be fully drained and purged by rent collection");

    let funder_after = banks_client.get_account(funder.pubkey()).await.unwrap().unwrap().lamports;
    assert_eq!(funder_after, funder_before + 1_000_000_000, "Funder must receive the 1 SOL collateral");

    let offer_acc = banks_client.get_account(offer_pda).await.unwrap().unwrap();
    let offer_state = P2POffer::unpack_from_slice(&offer_acc.data).unwrap();
    assert_eq!(offer_state.status, OfferStatus::Defaulted, "Offer must be Defaulted");
}

// ============================================================================
/// Wall-clock seconds. ProgramTest's bank clock tracks wall time, so this is
/// what a real `post_update_atomic` would produce. These fixtures previously
/// passed `i64::MAX / 2`, which only worked because the staleness check had no
/// upper bound - a future-dated price never expires.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_secs() as i64
}

async fn test_bank_withdraw_liquidity_authority_only_and_success() {
    let program_id = Pubkey::new_unique();
    let usdc_mint = Pubkey::new_unique();
    let authority = Keypair::new();
    let pool_id: u64 = 1;

    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()],
        &program_id,
    );
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);
    let authority_tok = Pubkey::new_unique();

    let mut program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
    );
    program_test.add_account(
        vault_pda,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, vault_pda, 1_000_000_000),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        authority_tok,
        Account {
            lamports: 10_000_000,
            data: token_acct_data(usdc_mint, authority.pubkey(), 0),
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    program_test.add_account(
        pool_pda,
        Account {
            lamports: 10_000_000,
            data: borsh::to_vec(&LendingPool {
                discriminator: LendingPool::DISCRIMINATOR,
                is_initialized: true,
                pool_type: PoolType::Individual,
                authority: authority.pubkey(),
                liquidity_mint: usdc_mint,
                vault_pda,
                total_liquidity: 1_000_000_000,
                total_borrowed: 0,
                staked_skr_amount: 0,
                interest_rate_bps: 800,
                max_ltv_bps: 6500,
                min_duration: 86400,
                max_duration: 86400 * 30,
                loans_originated: 0,
                loans_repaid: 0,
                name: [0u8; 32],
                is_oracle_free: true,
                pool_id,
                has_custom_oracle: false,
            })
            .unwrap(),
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    let (mut banks_client, payer, recent_blockhash) = program_test.start().await;
    let attacker = Keypair::new();
    let mut tx = Transaction::new_with_payer(
        &[solana_program::system_instruction::transfer(
            &payer.pubkey(), &attacker.pubkey(), 5_000_000_000)],
        Some(&payer.pubkey()),
    );
    tx.sign(&[&payer], recent_blockhash);
    banks_client.process_transaction(tx).await.unwrap();

    let mk_ix = |signer: Pubkey| Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(authority_tok, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::WithdrawLiquidity { amount: 250_000_000 }).unwrap(),
    };

    // (a) a non-authority signer must be rejected
    let mut tx = Transaction::new_with_payer(&[mk_ix(attacker.pubkey())], Some(&payer.pubkey()));
    tx.sign(&[&payer, &attacker], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 5, "Non-authority MUST NOT withdraw pool liquidity");

    // (b) the pool authority may withdraw
    let mut tx = Transaction::new_with_payer(&[mk_ix(authority.pubkey())], Some(&payer.pubkey()));
    tx.sign(&[&payer, &authority], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_ok(), "Authority withdraw MUST succeed: {:?}", res);

    let vault_after = banks_client
        .get_account(vault_pda)
        .await
        .unwrap()
        .expect("vault");
    let tok = spl_token::state::Account::unpack(&vault_after.data).unwrap();
    assert_eq!(tok.amount, 750_000_000, "vault must drop by the withdrawn amount");

    let pool_after = banks_client
        .get_account(pool_pda)
        .await
        .unwrap()
        .expect("pool");
    let pool = LendingPool::unpack_from_slice(&pool_after.data).unwrap();
    assert_eq!(
        pool.total_liquidity, 750_000_000,
        "total_liquidity must track the vault after a withdrawal"
    );

    let dest = banks_client.get_account(authority_tok).await.unwrap().expect("dest");
    let dest_tok = spl_token::state::Account::unpack(&dest.data).unwrap();
    assert_eq!(dest_tok.amount, 250_000_000, "authority must receive the funds");
}

// ============================================================================
// Round-8 test-fidelity additions: future-dated oracle bound, P2P Pyth path,
// admin rotation, InitializePool guards, DepositLiquidity effects.
// ============================================================================

async fn test_bank_admin_rotation_transfers_oracle_authority() {
    let program_id = Pubkey::new_unique();
    let admin_authority = Keypair::new();
    let keeper = Keypair::new();
    let new_admin = Keypair::new();

    let (admin_pda, _) = Pubkey::find_program_address(&[ADMIN_SEED], &program_id);
    let (program_data_pda, _) = Pubkey::find_program_address(
        &[program_id.as_ref()], &solana_program::bpf_loader_upgradeable::id());
    let (oracle_pda, _) = Pubkey::find_program_address(
        &[ORACLE_SEED, spl_token::native_mint::id().as_ref()], &program_id);

    let mut program_data_bytes = vec![0u8; 45];
    program_data_bytes[0] = 3;
    program_data_bytes[12] = 1;
    program_data_bytes[13..45].copy_from_slice(admin_authority.pubkey().as_ref());

    let mut pt = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));
    pt.add_account(program_data_pda, Account {
        lamports: 10_000_000, data: program_data_bytes,
        owner: solana_program::bpf_loader_upgradeable::id(), executable: false, rent_epoch: 0,
    });
    pt.add_account(admin_authority.pubkey(), Account {
        lamports: 10_000_000_000, data: vec![],
        owner: solana_program::system_program::id(), executable: false, rent_epoch: 0,
    });
    pt.add_account(keeper.pubkey(), Account {
        lamports: 10_000_000_000, data: vec![],
        owner: solana_program::system_program::id(), executable: false, rent_epoch: 0,
    });
    pt.add_account(new_admin.pubkey(), Account {
        lamports: 10_000_000_000, data: vec![],
        owner: solana_program::system_program::id(), executable: false, rent_epoch: 0,
    });
    let (banks_client, payer, bh) = pt.start().await;

    // 1. First init (4-account form).
    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(admin_authority.pubkey(), true),
            AccountMeta::new(admin_pda, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(program_data_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::InitializeAdmin).unwrap(),
    };
    let mut tx = Transaction::new_with_payer(&[ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &admin_authority], bh);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_ok(), "Initial InitializeAdmin MUST succeed: {:?}", res);

    // 2. Rotation (6-account form): new admin + new oracle authority.
    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(admin_authority.pubkey(), true),
            AccountMeta::new(admin_pda, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(program_data_pda, false),
            AccountMeta::new_readonly(new_admin.pubkey(), false),
            AccountMeta::new_readonly(keeper.pubkey(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::InitializeAdmin).unwrap(),
    };
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &admin_authority], blockhash);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_ok(), "Rotation MUST succeed: {:?}", res);

    let admin_acc = banks_client.get_account(admin_pda).await.unwrap().unwrap();
    let cfg = AdminConfig::unpack_from_slice(&admin_acc.data).unwrap();
    assert_eq!(cfg.admin, new_admin.pubkey(), "admin must be rotated");
    assert_eq!(cfg.oracle_authority, keeper.pubkey(), "oracle_authority must be rotated");

    // 3. The OLD admin key has lost SetPriceFeed (now owned by the keeper).
    let feed_ix = |signer: &Keypair| Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(signer.pubkey(), true),
            AccountMeta::new(oracle_pda, false),
            AccountMeta::new_readonly(spl_token::native_mint::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(admin_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::SetPriceFeed {
            price_micro_usd: 150_000_000, decimals: 9,
        }).unwrap(),
    };
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[feed_ix(&admin_authority)], Some(&payer.pubkey()));
    tx.sign(&[&payer, &admin_authority], blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 5, "Rotated-out admin MUST NOT publish feeds");

    // 4. The keeper key CAN publish.
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[feed_ix(&keeper)], Some(&payer.pubkey()));
    tx.sign(&[&payer, &keeper], blockhash);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_ok(), "Rotated-in keeper MUST be able to publish feeds: {:?}", res);
}

/// InitializePool parameter guards (InvalidDuration/InvalidInterestRate/
/// InvalidCollateralRatio) and PoolAlreadyInitialized had zero direct tests.
#[tokio::test]
async fn test_bank_initialize_pool_rejects_invalid_bounds() {
    let program_id = Pubkey::new_unique();
    let authority = Keypair::new();
    let mut pt = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));
    pt.add_account(authority.pubkey(), Account {
        lamports: 10_000_000_000, data: vec![],
        owner: solana_program::system_program::id(), executable: false, rent_epoch: 0,
    });
    // The pool init CPI (InitializeAccount3) requires the mint to exist and be
    // token-program owned.
    pt.add_account(clock_lend::state::USDC_DEVNET_MINT, Account {
        lamports: 10_000_000, data: mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    let (banks_client, payer, bh) = pt.start().await;

    let init = |pool_id: u64, interest: u16, ltv: u16, min_d: i64, max_d: i64| {
        let (pool_pda, _) = Pubkey::find_program_address(
            &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()], &program_id);
        let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);
        Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new(authority.pubkey(), true),
                AccountMeta::new(pool_pda, false),
                AccountMeta::new_readonly(clock_lend::state::USDC_DEVNET_MINT, false),
                AccountMeta::new(vault_pda, false),
                AccountMeta::new_readonly(solana_program::system_program::id(), false),
                AccountMeta::new_readonly(sysvar::rent::id(), false),
                AccountMeta::new_readonly(spl_token::id(), false),
            ],
            data: borsh::to_vec(&ClockLendInstruction::InitializePool {
                pool_id, pool_type: PoolType::Individual, interest_rate_bps: interest,
                max_ltv_bps: ltv, min_duration: min_d, max_duration: max_d,
                name: [0u8; 32], is_oracle_free: false,
            }).unwrap(),
        }
    };

    let cases: [(u64, u16, u16, i64, i64, u32, &str); 5] = [
        (811, 800, 8500, 0, 86400 * 30, 32, "min_duration 0"),
        (812, 800, 8500, 86400 * 30, 86400, 32, "max < min"),
        (813, 800, 0, 86400, 86400 * 30, 10, "ltv 0"),
        (814, 800, 9501, 86400, 86400 * 30, 10, "ltv > 9500"),
        (815, 10001, 8500, 86400, 86400 * 30, 33, "interest > 10000"),
    ];
    for (pool_id, interest, ltv, min_d, max_d, code, label) in cases {
        let blockhash = banks_client.get_latest_blockhash().await.unwrap();
        let mut tx = Transaction::new_with_payer(&[init(pool_id, interest, ltv, min_d, max_d)], Some(&payer.pubkey()));
        tx.sign(&[&payer, &authority], blockhash);
        let res = banks_client.process_transaction(tx).await;
        expect_custom_error(&res, code, &format!("{label} must fail with Custom({code})"));
    }

    // PoolAlreadyInitialized: same pool id twice (use different params to ensure distinct tx signature).
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[init(816, 800, 8500, 86400, 86400 * 30)], Some(&payer.pubkey()));
    tx.sign(&[&payer, &authority], blockhash);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_ok(), "first init of pool 816 MUST succeed: {:?}", res);
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[init(816, 801, 8500, 86400, 86400 * 30)], Some(&payer.pubkey()));
    tx.sign(&[&payer, &authority], blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 25, "re-initializing the same pool MUST fail");
}

/// DepositLiquidity had no test asserting its effects anywhere: this pins that
/// the vault balance and pool.total_liquidity both move by the deposit amount.
#[tokio::test]
async fn test_bank_deposit_liquidity_moves_tokens_and_updates_pool() {
    let program_id = Pubkey::new_unique();
    let authority = Keypair::new();
    let pool_id: u64 = 817;
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()], &program_id);
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);

    let lp_tok = Pubkey::new_unique();
    let mut pt = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));
    pt.add_account(authority.pubkey(), Account {
        lamports: 10_000_000_000, data: vec![],
        owner: solana_program::system_program::id(), executable: false, rent_epoch: 0,
    });
    pt.add_account(lp_tok, Account {
        lamports: 10_000_000,
        data: token_acct_data(clock_lend::state::USDC_DEVNET_MINT, authority.pubkey(), 1_000 * 1_000_000),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    // The pool init CPI (InitializeAccount3) requires the mint to exist and be
    // token-program owned.
    pt.add_account(clock_lend::state::USDC_DEVNET_MINT, Account {
        lamports: 10_000_000, data: mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    let (banks_client, payer, bh) = pt.start().await;

    let init_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(authority.pubkey(), true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new_readonly(clock_lend::state::USDC_DEVNET_MINT, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(sysvar::rent::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::InitializePool {
            pool_id, pool_type: PoolType::Individual, interest_rate_bps: 800, max_ltv_bps: 8500,
            min_duration: 86400, max_duration: 86400 * 30,
            name: [0u8; 32], is_oracle_free: false,
        }).unwrap(),
    };
    let mut tx = Transaction::new_with_payer(&[init_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &authority], bh);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_ok(), "pool init MUST succeed: {:?}", res);

    let dep_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(authority.pubkey(), true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(lp_tok, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::DepositLiquidity { amount: 1_000 * 1_000_000 }).unwrap(),
    };
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[dep_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &authority], blockhash);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_ok(), "deposit MUST succeed: {:?}", res);

    let vault_acc = banks_client.get_account(vault_pda).await.unwrap().unwrap();
    let vault_tok = spl_token::state::Account::unpack(&vault_acc.data).unwrap();
    assert_eq!(vault_tok.amount, 1_000 * 1_000_000, "vault must hold the full deposit");

    let pool_acc = banks_client.get_account(pool_pda).await.unwrap().unwrap();
    let pool = LendingPool::unpack_from_slice(&pool_acc.data).unwrap();
    assert_eq!(pool.total_liquidity, 1_000 * 1_000_000, "total_liquidity must reflect the deposit");
}

/// TriggerGracePeriod's pool-LoanOrder branch (processor.rs:2421-2464) was
/// never exercised: the only TriggerGrace coverage was the P2P branch. Loans
/// are genesis-packed because ProgramTest's clock does not advance to due_time.
#[tokio::test]
async fn test_bank_trigger_grace_pool_loan_branch() {
    let program_id = Pubkey::new_unique();
    let borrower = Keypair::new();
    let authority = Keypair::new();
    let stranger = Keypair::new();
    let pool_id: u64 = 818;
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()], &program_id);

    let now = now_secs();
    let make_loan = |loan_id: u64, due_time: i64| -> (Pubkey, LoanOrder) {
        let (loan_pda, _) = Pubkey::find_program_address(
            &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id.to_le_bytes()],
            &program_id);
        (loan_pda, LoanOrder {
            discriminator: LoanOrder::DISCRIMINATOR,
            is_active: true,
            loan_id,
            borrower: borrower.pubkey(),
            pool: pool_pda,
            principal_amount: 100_000_000,
            collateral_mint: spl_token::native_mint::id(),
            collateral_amount: 1_000_000_000,
            interest_due: 5_000_000,
            origination_time: now - 10 * 86400,
            due_time,
            grace_period_expires: 0,
            status: LoanStatus::Active,
            locked_skr: 0,
        })
    };
    let (overdue_loan_pda, overdue_loan) = make_loan(8181, now - 100);
    let (future_loan_pda, future_loan) = make_loan(8182, now + 3600);
    let (unauth_loan_pda, unauth_loan) = make_loan(8183, now - 100);

    let mut pt = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));
    pt.add_account(pool_pda, Account {
        lamports: 10_000_000,
        data: borsh::to_vec(&LendingPool {
            discriminator: LendingPool::DISCRIMINATOR,
            is_initialized: true,
            pool_type: PoolType::Individual,
            authority: authority.pubkey(),
            liquidity_mint: clock_lend::state::USDC_DEVNET_MINT,
            vault_pda: Pubkey::new_unique(),
            total_liquidity: 1_000_000_000,
            total_borrowed: 0,
            staked_skr_amount: 0,
            interest_rate_bps: 800,
            max_ltv_bps: 6500,
            min_duration: 86400,
            max_duration: 86400 * 30,
            loans_originated: 1,
            loans_repaid: 0,
            name: [0u8; 32],
            is_oracle_free: false,
            pool_id,
            has_custom_oracle: false,
        }).unwrap(),
        owner: program_id, executable: false, rent_epoch: 0,
    });
    for (pda, loan) in [(overdue_loan_pda, overdue_loan), (future_loan_pda, future_loan), (unauth_loan_pda, unauth_loan)] {
        pt.add_account(pda, Account {
            lamports: 10_000_000, data: borsh::to_vec(&loan).unwrap(),
            owner: program_id, executable: false, rent_epoch: 0,
        });
    }
    for kp in [&borrower, &authority, &stranger] {
        pt.add_account(kp.pubkey(), Account {
            lamports: 10_000_000_000, data: vec![],
            owner: solana_program::system_program::id(), executable: false, rent_epoch: 0,
        });
    }
    let (banks_client, payer, bh) = pt.start().await;

    let grace_ix = |caller: &Keypair, loan_pda: Pubkey, with_pool: bool| {
        let mut accounts = vec![
            AccountMeta::new(caller.pubkey(), true),
            AccountMeta::new(loan_pda, false),
        ];
        if with_pool { accounts.push(AccountMeta::new_readonly(pool_pda, false)); }
        Instruction {
            program_id,
            accounts,
            data: borsh::to_vec(&ClockLendInstruction::TriggerGracePeriod).unwrap(),
        }
    };

    // 1. Borrower triggers grace on an overdue loan.
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[grace_ix(&borrower, overdue_loan_pda, false)], Some(&payer.pubkey()));
    tx.sign(&[&payer, &borrower], blockhash);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_ok(), "borrower grace trigger on overdue pool loan MUST succeed: {:?}", res);
    let acc = banks_client.get_account(overdue_loan_pda).await.unwrap().unwrap();
    let loan = LoanOrder::unpack_from_slice(&acc.data).unwrap();
    assert_eq!(loan.status, LoanStatus::InGracePeriod, "status must move to InGracePeriod");
    assert!(loan.grace_period_expires > now_secs(), "grace expiry must be in the future");

    // 2. Loan not yet due: LoanNotDue.
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[grace_ix(&borrower, future_loan_pda, false)], Some(&payer.pubkey()));
    tx.sign(&[&payer, &borrower], blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 6, "grace trigger before due_time MUST fail with LoanNotDue");

    // 3. A stranger is neither borrower nor pool authority: UnauthorizedCaller.
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[grace_ix(&stranger, unauth_loan_pda, true)], Some(&payer.pubkey()));
    tx.sign(&[&payer, &stranger], blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 23, "stranger grace trigger MUST fail with UnauthorizedCaller");

    // 4. The pool authority IS authorized.
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[grace_ix(&authority, unauth_loan_pda, true)], Some(&payer.pubkey()));
    tx.sign(&[&payer, &authority], blockhash);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_ok(), "pool authority grace trigger MUST succeed: {:?}", res);
}

// ============================================================================
// Round-11 regressions (Pyth removed; admin-feed-only pricing + fixes)
// ============================================================================

/// Oracle-free pools are capped at 30% LTV (the hardcoded baselines have no
/// on-chain update path and SKR's baseline has been 3.2x off within months).
#[tokio::test]
async fn test_bank_initialize_pool_oracle_free_ltv_cap() {
    let program_id = Pubkey::new_unique();
    let authority = Keypair::new();
    let mut pt = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));
    pt.add_account(authority.pubkey(), Account {
        lamports: 10_000_000_000, data: vec![],
        owner: solana_program::system_program::id(), executable: false, rent_epoch: 0,
    });
    pt.add_account(clock_lend::state::USDC_DEVNET_MINT, Account {
        lamports: 10_000_000, data: mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    let (banks_client, payer, bh) = pt.start().await;

    let init = |pool_id: u64, ltv: u16| {
        let (pool_pda, _) = Pubkey::find_program_address(
            &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()], &program_id);
        let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);
        Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new(authority.pubkey(), true),
                AccountMeta::new(pool_pda, false),
                AccountMeta::new_readonly(clock_lend::state::USDC_DEVNET_MINT, false),
                AccountMeta::new(vault_pda, false),
                AccountMeta::new_readonly(solana_program::system_program::id(), false),
                AccountMeta::new_readonly(sysvar::rent::id(), false),
                AccountMeta::new_readonly(spl_token::id(), false),
            ],
            data: borsh::to_vec(&ClockLendInstruction::InitializePool {
                pool_id, pool_type: PoolType::Individual, interest_rate_bps: 800,
                max_ltv_bps: ltv, min_duration: 86400, max_duration: 86400 * 30,
                name: [0u8; 32], is_oracle_free: true,
            }).unwrap(),
        }
    };

    // 3001 bps on an oracle-free pool is rejected; 3000 is accepted.
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[init(8201, 3001)], Some(&payer.pubkey()));
    tx.sign(&[&payer, &authority], blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 10, "oracle-free pool above 30% LTV must be rejected");

    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[init(8202, 3000)], Some(&payer.pubkey()));
    tx.sign(&[&payer, &authority], blockhash);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_ok(), "oracle-free pool at exactly 30% LTV must succeed: {:?}", res);
}

/// Round-11: the pool-side feed decimals must match the liquidity mint — a
/// native-SOL pool feed published with 6 decimals re-values all collateral
/// by 10^3 and must be rejected.
#[tokio::test]
async fn test_bank_borrow_rejects_mismatched_pool_feed_decimals() {
    let program_id = Pubkey::new_unique();
    let authority = Keypair::new();
    let mut pt = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));
    pt.add_account(authority.pubkey(), Account {
        lamports: 10_000_000_000, data: vec![],
        owner: solana_program::system_program::id(), executable: false, rent_epoch: 0,
    });
    pt.add_account(clock_lend::state::USDC_DEVNET_MINT, Account {
        lamports: 10_000_000, data: mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    // Native-SOL pool with a pool-scoped feed that declares 6 decimals (wrong
    // for the native mint — should be 9).
    let pool_id: u64 = 8203;
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()], &program_id);
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);
    let (pool_oracle, _) = Pubkey::find_program_address(
        &[ORACLE_SEED, pool_pda.as_ref(), spl_token::native_mint::id().as_ref()], &program_id);
    let (global_sol_oracle, _) = Pubkey::find_program_address(
        &[ORACLE_SEED, spl_token::native_mint::id().as_ref()], &program_id);
    pt.add_account(pool_oracle, Account {
        lamports: 10_000_000,
        data: borsh::to_vec(&PriceFeed {
            discriminator: PriceFeed::DISCRIMINATOR,
            is_initialized: true,
            mint: spl_token::native_mint::id(),
            price_micro_usd: 150_000_000,
            decimals: 6, // WRONG for a native-SOL pool
            last_updated_at: now_secs(),
            authority: authority.pubkey(),
            max_staleness_seconds: 3600,
        }).unwrap(),
        owner: program_id, executable: false, rent_epoch: 0,
    });
    pt.add_account(global_sol_oracle, Account {
        lamports: 10_000_000,
        data: borsh::to_vec(&PriceFeed {
            discriminator: PriceFeed::DISCRIMINATOR,
            is_initialized: true,
            mint: spl_token::native_mint::id(),
            price_micro_usd: 150_000_000,
            decimals: 9,
            last_updated_at: now_secs(),
            authority: authority.pubkey(),
            max_staleness_seconds: 3600,
        }).unwrap(),
        owner: program_id, executable: false, rent_epoch: 0,
    });
    // Seed a pool with has_custom_oracle = true so the pool-scoped feed is used.
    let pool_state = LendingPool {
        discriminator: LendingPool::DISCRIMINATOR,
        is_initialized: true,
        pool_type: PoolType::Individual,
        authority: authority.pubkey(),
        liquidity_mint: spl_token::native_mint::id(),
        vault_pda,
        total_liquidity: 1_000_000_000,
        total_borrowed: 0,
        staked_skr_amount: 0,
        interest_rate_bps: 800,
        max_ltv_bps: 6500,
        min_duration: 86400,
        max_duration: 86400 * 30,
        loans_originated: 0,
        loans_repaid: 0,
        name: [0u8; 32],
        is_oracle_free: false,
        pool_id,
        has_custom_oracle: true,
    };
    pt.add_account(pool_pda, Account {
        lamports: 10_000_000, data: borsh::to_vec(&pool_state).unwrap(),
        owner: program_id, executable: false, rent_epoch: 0,
    });
    pt.add_account(vault_pda, Account {
        lamports: 10_000_000,
        data: token_acct_data(spl_token::native_mint::id(), vault_pda, 1_000_000_000),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    let (banks_client, payer, bh) = pt.start().await;

    let borrower = Keypair::new();
    let loan_id: u64 = 820301;
    let (loan_pda, _) = Pubkey::find_program_address(
        &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id.to_le_bytes()], &program_id);
    let (escrow_pda, _) = Pubkey::find_program_address(&[ESCROW_SEED, loan_pda.as_ref()], &program_id);
    let (profile_pda, _) = Pubkey::find_program_address(
        &[PROFILE_SEED, borrower.pubkey().as_ref()], &program_id);
    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &program_id);
    let treasury_tok = Keypair::new();
    let borrower_usdc = Keypair::new();

    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[
        solana_sdk::system_instruction::transfer(&payer.pubkey(), &borrower.pubkey(), 2_000_000_000),
        solana_sdk::system_instruction::create_account(&payer.pubkey(), &borrower_usdc.pubkey(),
            banks_client.get_rent().await.unwrap().minimum_balance(165), 165, &spl_token::id()),
        spl_token::instruction::initialize_account(&spl_token::id(), &borrower_usdc.pubkey(),
            &spl_token::native_mint::id(), &borrower.pubkey()).unwrap(),
        solana_sdk::system_instruction::create_account(&payer.pubkey(), &treasury_tok.pubkey(),
            banks_client.get_rent().await.unwrap().minimum_balance(165), 165, &spl_token::id()),
        spl_token::instruction::initialize_account(&spl_token::id(), &treasury_tok.pubkey(),
            &spl_token::native_mint::id(), &treasury_pda).unwrap(),
    ], Some(&payer.pubkey()));
    tx.sign(&[&payer, &borrower_usdc, &treasury_tok], blockhash);
    banks_client.process_transaction(tx).await.unwrap();

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(loan_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(borrower_usdc.pubkey(), false),
            AccountMeta::new(borrower.pubkey(), false),
            AccountMeta::new(escrow_pda, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new(profile_pda, false),
            AccountMeta::new(treasury_tok.pubkey(), false),
            AccountMeta::new_readonly(pool_oracle, false),
            AccountMeta::new_readonly(global_sol_oracle, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::BorrowFromPool {
            loan_id, borrow_amount: 100_000_000, collateral_amount: 1_000_000_000,
            duration_seconds: 86400 * 7,
        }).unwrap(),
    };
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &borrower], blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, 28, "mismatched pool-feed decimals must be rejected");
}

/// Round-11: repayment is closed once the grace period expires (mirror of the
/// P2P branch) — no perpetual redemption option front-running ClaimDefault.
#[tokio::test]
async fn test_bank_repay_blocked_after_grace_expiry() {
    let program_id = Pubkey::new_unique();
    let borrower = Keypair::new();
    let authority = Keypair::new();
    let pool_id: u64 = 8204;
    let now = now_secs();
    let (pool_pda, _) = Pubkey::find_program_address(
        &[POOL_SEED, authority.pubkey().as_ref(), &pool_id.to_le_bytes()], &program_id);
    let (vault_pda, _) = Pubkey::find_program_address(&[VAULT_SEED, pool_pda.as_ref()], &program_id);
    let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], &program_id);

    let make_loan = |loan_id: u64, expires: i64| {
        let (loan_pda, _) = Pubkey::find_program_address(
            &[LOAN_SEED, pool_pda.as_ref(), borrower.pubkey().as_ref(), &loan_id.to_le_bytes()],
            &program_id);
        (loan_pda, LoanOrder {
            discriminator: LoanOrder::DISCRIMINATOR,
            is_active: true,
            loan_id,
            borrower: borrower.pubkey(),
            pool: pool_pda,
            principal_amount: 100_000_000,
            collateral_mint: spl_token::native_mint::id(),
            collateral_amount: 1_000_000_000,
            interest_due: 5_000_000,
            origination_time: now - 10 * 86400,
            due_time: now - 86400,
            grace_period_expires: expires,
            status: LoanStatus::InGracePeriod,
            locked_skr: 0,
        })
    };
    let (expired_loan_pda, expired_loan) = make_loan(820401, now - 100);
    let (active_grace_pda, active_grace_loan) = make_loan(820402, now + 3600);

    let mut pt = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));
    for (pda, loan) in [(expired_loan_pda, expired_loan), (active_grace_pda, active_grace_loan)] {
        pt.add_account(pda, Account {
            lamports: 10_000_000, data: borsh::to_vec(&loan).unwrap(),
            owner: program_id, executable: false, rent_epoch: 0,
        });
    }
    pt.add_account(borrower.pubkey(), Account {
        lamports: 10_000_000_000, data: vec![],
        owner: solana_program::system_program::id(), executable: false, rent_epoch: 0,
    });
    let borrower_usdc = Pubkey::new_unique();
    let treasury_tok = Pubkey::new_unique();
    pt.add_account(borrower_usdc, Account {
        lamports: 10_000_000,
        data: token_acct_data(clock_lend::state::USDC_DEVNET_MINT, borrower.pubkey(), 1_000_000_000),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    pt.add_account(treasury_tok, Account {
        lamports: 10_000_000,
        data: token_acct_data(clock_lend::state::USDC_DEVNET_MINT, treasury_pda, 0),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    let (banks_client, payer, bh) = pt.start().await;

    let repay_ix = |loan_pda: Pubkey| Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(borrower.pubkey(), true),
            AccountMeta::new(loan_pda, false),
            AccountMeta::new(borrower_usdc, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(Pubkey::new_unique(), false), // escrow
            AccountMeta::new(borrower.pubkey(), false),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(Pubkey::new_unique(), false), // profile
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new(treasury_tok, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::RepayLoan { repay_amount: 105_000_000 }).unwrap(),
    };

    // Expired grace: repay is rejected with GracePeriodExpired.
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[repay_ix(expired_loan_pda)], Some(&payer.pubkey()));
    tx.sign(&[&payer, &borrower], blockhash);
    let res = banks_client.process_transaction(tx).await;
    expect_custom_error(&res, ClockLendError::GracePeriodExpired as u32, "repay after grace expiry must fail");

    // Grace still active: repay proceeds past the grace guard (the borrower's
    // escrow account is a placeholder so this fails later — the point is the
    // guard does NOT fire with a live grace window).
    let blockhash = banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[repay_ix(active_grace_pda)], Some(&payer.pubkey()));
    tx.sign(&[&payer, &borrower], blockhash);
    let res = banks_client.process_transaction(tx).await;
    match res.as_ref().err() {
        Some(BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(c)))) => {
            assert_ne!(*c, ClockLendError::GracePeriodExpired as u32, "live grace window must not be treated as expired");
        }
        _ => {} // any other failure (placeholder accounts) is fine for this probe
    }
}
