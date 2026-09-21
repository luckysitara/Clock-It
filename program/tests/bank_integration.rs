use clock_lend::{
    error::ClockLendError,
    instruction::ClockLendInstruction,
    processor::process_instruction,
    state::{
        AdminConfig, LendingPool, LoanOrder, LoanStatus, PoolType, PriceFeed, UserProfile, ADMIN_SEED, ESCROW_SEED, LOAN_SEED,
        ORACLE_SEED, P2P_SEED, POOL_SEED, PROFILE_SEED, SKR_MINT, TREASURY_SEED, VAULT_SEED,
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
        is_oracle_free: true,
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
        is_oracle_free: true,
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
        is_oracle_free: true,
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
        is_oracle_free: true,
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
                is_initialized: true,
                admin: oracle_authority.pubkey(),
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
        is_initialized: true,
        mint: SKR_MINT,
        price_micro_usd: 50_000,
        decimals: 6,
        last_updated_at: 1, // Ancient timestamp -> STALE
        authority: oracle_authority.pubkey(),
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
    assert!(res.is_err(), "Borrow MUST fail when oracle feed is stale (> 24h)!");
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
        is_initialized: true,
        mint: SKR_MINT,
        price_micro_usd: 20_000,
        decimals: 6,
        last_updated_at: 1720000000,
        authority: original_authority.pubkey(),
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
    assert!(res.is_err(), "Attacker MUST NOT be able to overwrite oracle price feed!");
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
    assert!(res.is_err(), "Borrower MUST NOT be able to unstake SKR while loan is active!");
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
    assert!(res.is_err());

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
        }).unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[init_sol_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer], recent_blockhash);
    let res = banks_client.process_transaction(tx).await;
    assert!(res.is_err(), "Pool initialization with raw native SOL liquidity mint MUST fail!");
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

    let program_test = ProgramTest::new(
        "clock_lend",
        program_id,
        processor!(process_instruction),
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
    assert!(res.is_err(), "P2P offer with 1 lamport collateral asking for 1M USDC MUST be rejected!");
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

    let mut program_test = program_test;
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
    assert!(res.is_err(), "Attacker MUST NOT be able to claim uninitialized oracle feed without admin authorization!");
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
    assert!(res_attacker2.is_err(), "Attacker MUST NOT initialize feed using AdminConfig!");
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
    assert!(res_overwrite.is_err(), "Attacker MUST NOT overwrite existing price feed!");
    match res_overwrite.unwrap_err() {
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
                is_initialized: true,
                admin: admin.pubkey(),
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
    assert!(res_exploit1.is_err(), "Omitting oracle on dynamic pool MUST fail with InvalidOracleAccount!");
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
    assert!(res_exploit2.is_err(), "Passing uninitialized oracle on dynamic pool MUST fail with InvalidOracleAccount!");
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
    assert!(res_fail_ltv.is_err(), "100 USDC borrow against 1 SOL at $50 MUST fail InvalidCollateralRatio!");
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
    assert!(res_attacker.is_err(), "Non-pool authority MUST NOT initialize pool-specific oracle!");
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



