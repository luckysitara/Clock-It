use clock_lend::{
    error::ClockLendError,
    instruction::ClockLendInstruction,
    processor::process_instruction,
    state::{
        LendingPool, LoanOrder, LoanStatus, PoolType, UserProfile, ESCROW_SEED, LOAN_SEED, POOL_SEED,
        PROFILE_SEED, SKR_MINT, TREASURY_SEED, VAULT_SEED,
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

    let liquidity_mint = Pubkey::new_unique();
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
    assert!(result.is_err(), "Attacker should NOT be able to initialize pool for non-signing authority!");
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
    assert!(result.is_err(), "StakeSKR with non-SKR mint MUST be rejected!");
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
    assert!(result.is_err(), "Borrow with unapproved fake collateral mint MUST be rejected!");
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
    assert!(result.is_err(), "Borrow MUST revert when treasury account is omitted and fee > 0!");
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

    let pool_state = LendingPool {
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
    assert!(result.is_err(), "Re-borrow on a Defaulted loan MUST be rejected!");
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
        is_initialized: true,
        user: borrower.pubkey(),
        staked_skr: 100_000_000, // 100 SKR staked
        total_loans_completed: 0,
        total_loans_defaulted: 0,
        reputation_score: 5000,
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
        is_initialized: true,
        user: borrower.pubkey(),
        staked_skr: 100_000_000, // 100 SKR
        total_loans_completed: 0,
        total_loans_defaulted: 0,
        reputation_score: 5000,
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
