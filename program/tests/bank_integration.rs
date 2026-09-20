use clock_lend::{
    instruction::ClockLendInstruction,
    processor::process_instruction,
    state::{LendingPool, PoolType, POOL_SEED, VAULT_SEED},
};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    sysvar,
};
use solana_program_test::*;
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
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
