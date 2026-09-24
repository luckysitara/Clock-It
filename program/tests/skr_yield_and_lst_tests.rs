// SKR yield vault tests (round-9 hardened ABI):
//   InitializeSkrYieldVault (tag 15): admin-gated, reinit-rejected, mint allowlisted
//   DepositSkrYield (tag 16): authority-only, unallocated-rewards folding
//   ClaimSkrYield (tag 17): escrow-authoritative stake, 1h cooldown
// Note: there is no LST in the program — "lst" in the filename is legacy.
//
// Determinism: every submission advances the bank one slot via warp_to_slot
// (which fills ticks and records a new blockhash). Identical
// (blockhash, message) pairs are memoized by solana-program-test and would
// silently skip execution — that was the round-9 flaky-test mechanism.
use clock_lend::{
    error::ClockLendError,
    instruction::ClockLendInstruction,
    processor::process_instruction,
    state::{
        AdminConfig, SkrYieldVault, UserYieldPosition,
        ADMIN_SEED, DISCRIMINATOR_ADMIN, DISCRIMINATOR_SKR_YIELD,
        DISCRIMINATOR_USER_YIELD, SKR_MINT,
        SKR_YIELD_VAULT_SEED, SKR_YIELD_TOKEN_SEED, USER_YIELD_SEED,
        USDC_DEVNET_MINT,
    },
};
use solana_program::{
    clock::Clock,
    instruction::{AccountMeta, Instruction, InstructionError},
    program_pack::Pack,
    pubkey::Pubkey,
    system_instruction, sysvar,
};
use solana_program_test::*;
use solana_sdk::{
    account::Account,
    signature::{Keypair, Signer},
    transaction::{Transaction, TransactionError},
};

const SKR_DECIMALS: u64 = 1_000_000;

fn create_mint_data(decimals: u8) -> Vec<u8> {
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

fn token_data(mint: Pubkey, owner: Pubkey, amount: u64) -> Vec<u8> {
    let mut data = vec![0u8; spl_token::state::Account::LEN];
    spl_token::state::Account {
        mint,
        owner,
        amount,
        delegate: solana_program::program_option::COption::None,
        state: spl_token::state::AccountState::Initialized,
        is_native: solana_program::program_option::COption::None,
        delegated_amount: 0,
        close_authority: solana_program::program_option::COption::None,
    }
    .pack_into_slice(&mut data);
    data
}

fn add_admin(program_test: &mut ProgramTest, program_id: Pubkey, admin: Pubkey) -> Pubkey {
    let (admin_pda, _) = Pubkey::find_program_address(&[ADMIN_SEED], &program_id);
    let config = AdminConfig {
        discriminator: DISCRIMINATOR_ADMIN,
        is_initialized: true,
        admin,
        oracle_authority: admin,
    };
    let mut data = vec![0u8; AdminConfig::LEN];
    config.pack_into_slice(&mut data);
    program_test.add_account(admin_pda, Account {
        lamports: 10_000_000, data, owner: program_id, executable: false, rent_epoch: 0,
    });
    admin_pda
}

/// Genesis-pack the user's SKR escrow token account (owner = escrow PDA).
fn add_skr_escrow(program_test: &mut ProgramTest, program_id: Pubkey, user: Pubkey, staked: u64) -> Pubkey {
    let (escrow_pda, _) = Pubkey::find_program_address(&[b"skr_escrow", user.as_ref()], &program_id);
    program_test.add_account(escrow_pda, Account {
        lamports: 10_000_000,
        data: token_data(SKR_MINT, escrow_pda, staked),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    escrow_pda
}

/// Genesis-pack a pre-existing yield position (stake already synced).
fn add_position(program_test: &mut ProgramTest, program_id: Pubkey, user: Pubkey, reward_mint: Pubkey,
                staked: u64, last_interaction: i64) -> Pubkey {
    let (pda, _) = Pubkey::find_program_address(&[USER_YIELD_SEED, user.as_ref(), reward_mint.as_ref()], &program_id);
    let pos = UserYieldPosition {
        discriminator: DISCRIMINATOR_USER_YIELD,
        is_initialized: true,
        user,
        reward_mint,
        staked_skr: staked,
        reward_debt: 0,
        accrued_rewards: 0,
        total_claimed: 0,
        last_interaction_time: last_interaction,
    };
    let mut data = vec![0u8; UserYieldPosition::LEN];
    pos.pack_into_slice(&mut data);
    program_test.add_account(pda, Account {
        lamports: 10_000_000, data, owner: program_id, executable: false, rent_epoch: 0,
    });
    pda
}

/// Genesis-pack a pre-initialized vault.
fn add_vault(program_test: &mut ProgramTest, program_id: Pubkey, reward_mint: Pubkey,
             authority: Pubkey, total_staked: u64) -> Pubkey {
    let (pda, _) = Pubkey::find_program_address(&[SKR_YIELD_VAULT_SEED, reward_mint.as_ref()], &program_id);
    let vault = SkrYieldVault {
        discriminator: DISCRIMINATOR_SKR_YIELD,
        is_initialized: true,
        authority,
        reward_mint,
        total_staked_skr: total_staked,
        acc_reward_per_share: 0,
        total_rewards_distributed: 0,
        pending_rewards: 0,
        unallocated_rewards: 0,
    };
    let mut data = vec![0u8; SkrYieldVault::LEN];
    vault.pack_into_slice(&mut data);
    program_test.add_account(pda, Account {
        lamports: 10_000_000, data, owner: program_id, executable: false, rent_epoch: 0,
    });
    pda
}

fn vault_ix(program_id: Pubkey, authority: &Keypair, reward_mint: Pubkey, vault_pda: Pubkey,
            token_pda: Pubkey, admin_pda: Pubkey) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(authority.pubkey(), true),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(reward_mint, false),
            AccountMeta::new(token_pda, false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(sysvar::rent::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(admin_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::InitializeSkrYieldVault).unwrap(),
    }
}

fn claim_ix(program_id: Pubkey, user: &Keypair, vault_pda: Pubkey, user_yield_pda: Pubkey,
            vault_token_pda: Pubkey, user_reward_tok: Pubkey, escrow_pda: Pubkey) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(user.pubkey(), true),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(user_yield_pda, false),
            AccountMeta::new(vault_token_pda, false),
            AccountMeta::new(user_reward_tok, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            AccountMeta::new_readonly(escrow_pda, false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::ClaimSkrYield).unwrap(),
    }
}

fn deposit_ix(program_id: Pubkey, depositor: &Keypair, vault_pda: Pubkey, depositor_tok: Pubkey,
              vault_token_pda: Pubkey, amount: u64) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(depositor.pubkey(), true),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(depositor_tok, false),
            AccountMeta::new(vault_token_pda, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: borsh::to_vec(&ClockLendInstruction::DepositSkrYield { amount }).unwrap(),
    }
}

/// Advance one slot (forcing a fresh blockhash) and submit — defeats the
/// identical-signature memoization that made the old tests flaky.
async fn submit(ctx: &mut ProgramTestContext, ixs: &[Instruction], signers: &[&Keypair],
                payer: &Keypair) -> Result<(), BanksClientError> {
    let clock: Clock = ctx.banks_client.get_sysvar().await.unwrap();
    ctx.warp_to_slot(clock.slot + 1).expect("warp");
    let bh = ctx.banks_client.get_latest_blockhash().await.unwrap();
    let tx = Transaction::new_signed_with_payer(ixs, Some(&payer.pubkey()), signers, bh);
    ctx.banks_client.process_transaction(tx).await
}

fn expect_custom(res: &Result<(), BanksClientError>, code: u32, ctx: &str) {
    match res.as_ref().err() {
        Some(BanksClientError::TransactionError(TransactionError::InstructionError(_, InstructionError::Custom(c)))) => {
            assert_eq!(*c, code, "{ctx}: expected Custom({code}), got Custom({c})");
        }
        err => panic!("{ctx}: expected Custom({code}), got {err:?}"),
    }
}

async fn read_vault(ctx: &mut ProgramTestContext, pda: Pubkey) -> SkrYieldVault {
    SkrYieldVault::unpack_from_slice(&ctx.banks_client.get_account(pda).await.unwrap().unwrap().data).unwrap()
}

async fn read_position(ctx: &mut ProgramTestContext, pda: Pubkey) -> UserYieldPosition {
    UserYieldPosition::unpack_from_slice(&ctx.banks_client.get_account(pda).await.unwrap().unwrap().data).unwrap()
}

async fn read_token_amount(ctx: &mut ProgramTestContext, pda: Pubkey) -> u64 {
    spl_token::state::Account::unpack(&ctx.banks_client.get_account(pda).await.unwrap().unwrap().data).unwrap().amount
}

#[tokio::test]
async fn test_skr_yield_vault_initialization() {
    let program_id = Pubkey::new_unique();
    let authority = Keypair::new();
    let reward_mint = USDC_DEVNET_MINT;

    let mut program_test = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));
    program_test.add_account(reward_mint, Account {
        lamports: 10_000_000, data: create_mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    let admin_pda = add_admin(&mut program_test, program_id, authority.pubkey());
    let (yield_vault_pda, _) =
        Pubkey::find_program_address(&[SKR_YIELD_VAULT_SEED, reward_mint.as_ref()], &program_id);
    let (vault_token_pda, _) =
        Pubkey::find_program_address(&[SKR_YIELD_TOKEN_SEED, reward_mint.as_ref()], &program_id);

    let mut ctx = program_test.start_with_context().await;
    let payer = ctx.payer.insecure_clone();
    submit(&mut ctx, &[system_instruction::transfer(&payer.pubkey(), &authority.pubkey(), 1_000_000_000)], &[&payer], &payer).await.unwrap();

    // 1. Non-admin caller is rejected.
    let intruder = Keypair::new();
    let res = submit(&mut ctx,
        &[vault_ix(program_id, &intruder, reward_mint, yield_vault_pda, vault_token_pda, admin_pda)],
        &[&payer, &intruder], &payer).await;
    expect_custom(&res, ClockLendError::Unauthorized as u32, "non-admin init");

    // 2. Admin initializes.
    submit(&mut ctx,
        &[vault_ix(program_id, &authority, reward_mint, yield_vault_pda, vault_token_pda, admin_pda)],
        &[&authority], &authority).await.unwrap();

    let vault = read_vault(&mut ctx, yield_vault_pda).await;
    assert!(vault.is_initialized);
    assert_eq!(vault.authority, authority.pubkey());
    assert_eq!(vault.reward_mint, reward_mint);
    assert_eq!(vault.total_staked_skr, 0);
    assert_eq!(vault.acc_reward_per_share, 0);
    assert_eq!(vault.pending_rewards, 0);
    assert_eq!(vault.unallocated_rewards, 0);

    // 3. C-2 regression: re-initialization is rejected.
    let res = submit(&mut ctx,
        &[vault_ix(program_id, &authority, reward_mint, yield_vault_pda, vault_token_pda, admin_pda)],
        &[&authority], &authority).await;
    expect_custom(&res, ClockLendError::PoolAlreadyInitialized as u32, "reinit");
}

#[tokio::test]
async fn test_skr_yield_deposit_claim_and_cooldown() {
    let program_id = Pubkey::new_unique();
    let authority = Keypair::new();
    let user = Keypair::new();
    let reward_mint = USDC_DEVNET_MINT;

    let mut program_test = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));
    program_test.add_account(reward_mint, Account {
        lamports: 10_000_000, data: create_mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    program_test.add_account(SKR_MINT, Account {
        lamports: 10_000_000, data: create_mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    let admin_pda = add_admin(&mut program_test, program_id, authority.pubkey());
    let (yield_vault_pda, _) =
        Pubkey::find_program_address(&[SKR_YIELD_VAULT_SEED, reward_mint.as_ref()], &program_id);
    let (vault_token_pda, _) =
        Pubkey::find_program_address(&[SKR_YIELD_TOKEN_SEED, reward_mint.as_ref()], &program_id);
    let (user_yield_pda, _) =
        Pubkey::find_program_address(&[USER_YIELD_SEED, user.pubkey().as_ref(), reward_mint.as_ref()], &program_id);
    let escrow_pda = add_skr_escrow(&mut program_test, program_id, user.pubkey(), 100_000 * SKR_DECIMALS);

    let depositor_token = Keypair::new();
    program_test.add_account(depositor_token.pubkey(), Account {
        lamports: 10_000_000,
        data: token_data(reward_mint, authority.pubkey(), 1_000 * 1_000_000),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    let user_reward_token = Keypair::new();
    program_test.add_account(user_reward_token.pubkey(), Account {
        lamports: 10_000_000,
        data: token_data(reward_mint, user.pubkey(), 0),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });

    let mut ctx = program_test.start_with_context().await;
    let payer = ctx.payer.insecure_clone();
    submit(&mut ctx, &[
        system_instruction::transfer(&payer.pubkey(), &authority.pubkey(), 1_000_000_000),
        system_instruction::transfer(&payer.pubkey(), &user.pubkey(), 1_000_000_000),
    ], &[&payer], &payer).await.unwrap();

    // 1. Init vault.
    submit(&mut ctx,
        &[vault_ix(program_id, &authority, reward_mint, yield_vault_pda, vault_token_pda, admin_pda)],
        &[&authority], &authority).await.unwrap();

    // 2. H-7 regression: deposit BEFORE any staker syncs -> parked.
    let first_deposit = 100 * 1_000_000;
    submit(&mut ctx,
        &[deposit_ix(program_id, &authority, yield_vault_pda, depositor_token.pubkey(), vault_token_pda, first_deposit)],
        &[&authority], &authority).await.unwrap();
    let vault = read_vault(&mut ctx, yield_vault_pda).await;
    assert_eq!(vault.unallocated_rewards, first_deposit, "pre-stake deposit must be parked");
    assert_eq!(vault.acc_reward_per_share, 0);

    // 3. H-3 regression: non-authority deposit is rejected.
    let res = submit(&mut ctx,
        &[deposit_ix(program_id, &user, yield_vault_pda, user_reward_token.pubkey(), vault_token_pda, 1_000_000)],
        &[&payer, &user], &payer).await;
    expect_custom(&res, ClockLendError::Unauthorized as u32, "non-authority deposit");

    // 4. Fresh position sync: registers shares from the escrow balance.
    submit(&mut ctx,
        &[claim_ix(program_id, &user, yield_vault_pda, user_yield_pda, vault_token_pda, user_reward_token.pubkey(), escrow_pda)],
        &[&user], &user).await.unwrap();
    let vault = read_vault(&mut ctx, yield_vault_pda).await;
    assert_eq!(vault.total_staked_skr, 100_000 * SKR_DECIMALS, "sync must register the escrow stake");

    // 5. Second deposit folds the unallocated backlog into acc.
    let second_deposit = 100 * 1_000_000;
    submit(&mut ctx,
        &[deposit_ix(program_id, &authority, yield_vault_pda, depositor_token.pubkey(), vault_token_pda, second_deposit)],
        &[&authority], &authority).await.unwrap();
    let vault = read_vault(&mut ctx, yield_vault_pda).await;
    assert_eq!(vault.unallocated_rewards, 0, "backlog must fold on the next deposit");
    assert_eq!(vault.acc_reward_per_share, 2_000_000_000, "200 USDC over 100k SKR");

    // 6. H-2 regression: the fresh stake is inside its 1h cooldown.
    let res = submit(&mut ctx,
        &[claim_ix(program_id, &user, yield_vault_pda, user_yield_pda, vault_token_pda, user_reward_token.pubkey(), escrow_pda)],
        &[&user], &user).await;
    expect_custom(&res, ClockLendError::YieldCooldown as u32, "claim inside cooldown");
    // The rejected claim must not have moved anything.
    assert_eq!(read_token_amount(&mut ctx, user_reward_token.pubkey()).await, 0);
}

#[tokio::test]
async fn test_skr_yield_claim_pays_after_cooldown_and_rearms() {
    let program_id = Pubkey::new_unique();
    let authority = Keypair::new();
    let user = Keypair::new();
    let reward_mint = USDC_DEVNET_MINT;

    let mut program_test = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));
    program_test.add_account(reward_mint, Account {
        lamports: 10_000_000, data: create_mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    program_test.add_account(SKR_MINT, Account {
        lamports: 10_000_000, data: create_mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    // Position pre-packed with an ANCIENT last_interaction (cooldown already
    // served) and its stake synced; vault pre-seeded with matching total.
    let user_yield_pda = add_position(&mut program_test, program_id, user.pubkey(), reward_mint,
        100_000 * SKR_DECIMALS, 1);
    let yield_vault_pda = add_vault(&mut program_test, program_id, reward_mint, authority.pubkey(),
        100_000 * SKR_DECIMALS);
    let (vault_token_pda, _) =
        Pubkey::find_program_address(&[SKR_YIELD_TOKEN_SEED, reward_mint.as_ref()], &program_id);
    program_test.add_account(vault_token_pda, Account {
        lamports: 10_000_000,
        data: token_data(reward_mint, yield_vault_pda, 1_000 * 1_000_000),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    let escrow_pda = add_skr_escrow(&mut program_test, program_id, user.pubkey(), 100_000 * SKR_DECIMALS);
    let depositor_token = Keypair::new();
    program_test.add_account(depositor_token.pubkey(), Account {
        lamports: 10_000_000,
        data: token_data(reward_mint, authority.pubkey(), 1_000 * 1_000_000),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    let user_reward_token = Keypair::new();
    program_test.add_account(user_reward_token.pubkey(), Account {
        lamports: 10_000_000,
        data: token_data(reward_mint, user.pubkey(), 0),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });

    let mut ctx = program_test.start_with_context().await;
    let payer = ctx.payer.insecure_clone();
    submit(&mut ctx, &[
        system_instruction::transfer(&payer.pubkey(), &authority.pubkey(), 1_000_000_000),
        system_instruction::transfer(&payer.pubkey(), &user.pubkey(), 1_000_000_000),
    ], &[&payer], &payer).await.unwrap();

    // Deposit $100 against 100k SKR -> acc = 1e9.
    let dep = 100 * 1_000_000;
    submit(&mut ctx,
        &[deposit_ix(program_id, &authority, yield_vault_pda, depositor_token.pubkey(), vault_token_pda, dep)],
        &[&authority], &authority).await.unwrap();

    // Claim (cooldown already served) pays the full $100.
    submit(&mut ctx,
        &[claim_ix(program_id, &user, yield_vault_pda, user_yield_pda, vault_token_pda, user_reward_token.pubkey(), escrow_pda)],
        &[&user], &user).await.unwrap();
    assert_eq!(read_token_amount(&mut ctx, user_reward_token.pubkey()).await, dep, "full dividend must be paid");
    let pos = read_position(&mut ctx, user_yield_pda).await;
    assert_eq!(pos.total_claimed, dep);
    assert_eq!(pos.accrued_rewards, 0);
    let vault = read_vault(&mut ctx, yield_vault_pda).await;
    assert_eq!(vault.pending_rewards, 0);

    // A second dividend accrues...
    submit(&mut ctx,
        &[deposit_ix(program_id, &authority, yield_vault_pda, depositor_token.pubkey(), vault_token_pda, dep)],
        &[&authority], &authority).await.unwrap();
    // ...but the payout re-armed the cooldown, so an immediate second claim is
    // rejected (this is what blocks the claim-in-slot-N+1 / unstake-in-N+2 snipe).
    let res = submit(&mut ctx,
        &[claim_ix(program_id, &user, yield_vault_pda, user_yield_pda, vault_token_pda, user_reward_token.pubkey(), escrow_pda)],
        &[&user], &user).await;
    expect_custom(&res, ClockLendError::YieldCooldown as u32, "claim re-arms the cooldown");
}

#[tokio::test]
async fn test_skr_yield_claim_is_bounded_by_real_escrow() {
    // C-1 regression: the claim path bounds accrual by the SKR escrow token
    // account, so a position with a forged cached stake (or a stake that was
    // unstaked) cannot claim ghost shares, and total_staked_skr syncs DOWN.
    let program_id = Pubkey::new_unique();
    let authority = Keypair::new();
    let user = Keypair::new();
    let reward_mint = USDC_DEVNET_MINT;

    let mut program_test = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));
    program_test.add_account(reward_mint, Account {
        lamports: 10_000_000, data: create_mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    program_test.add_account(SKR_MINT, Account {
        lamports: 10_000_000, data: create_mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    let (yield_vault_pda, _) =
        Pubkey::find_program_address(&[SKR_YIELD_VAULT_SEED, reward_mint.as_ref()], &program_id);
    let (vault_token_pda, _) =
        Pubkey::find_program_address(&[SKR_YIELD_TOKEN_SEED, reward_mint.as_ref()], &program_id);
    let escrow_pda = add_skr_escrow(&mut program_test, program_id, user.pubkey(), 100_000 * SKR_DECIMALS);

    // Position FALSELY claims 1,000,000 SKR (pre-fix cached accounting),
    // ancient cooldown.
    let user_yield_pda = add_position(&mut program_test, program_id, user.pubkey(), reward_mint,
        1_000_000 * SKR_DECIMALS, 1);
    // Vault pre-seeded with accrued dividend and the inflated total.
    let (seeded_vault_pda, _) =
        Pubkey::find_program_address(&[SKR_YIELD_VAULT_SEED, reward_mint.as_ref()], &program_id);
    let seeded = SkrYieldVault {
        discriminator: DISCRIMINATOR_SKR_YIELD,
        is_initialized: true,
        authority: authority.pubkey(),
        reward_mint,
        total_staked_skr: 1_000_000 * SKR_DECIMALS,
        acc_reward_per_share: 1_000_000_000_000, // 1 USDC per SKR
        total_rewards_distributed: 0,
        pending_rewards: 0,
        unallocated_rewards: 0,
    };
    let mut vault_data = vec![0u8; SkrYieldVault::LEN];
    seeded.pack_into_slice(&mut vault_data);
    program_test.add_account(seeded_vault_pda, Account {
        lamports: 10_000_000, data: vault_data,
        owner: program_id, executable: false, rent_epoch: 0,
    });
    program_test.add_account(vault_token_pda, Account {
        lamports: 10_000_000,
        data: token_data(reward_mint, seeded_vault_pda, 1_000_000 * 1_000_000),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    let user_reward_token = Keypair::new();
    program_test.add_account(user_reward_token.pubkey(), Account {
        lamports: 10_000_000,
        data: token_data(reward_mint, user.pubkey(), 0),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });

    let mut ctx = program_test.start_with_context().await;
    let payer = ctx.payer.insecure_clone();
    submit(&mut ctx, &[system_instruction::transfer(&payer.pubkey(), &user.pubkey(), 1_000_000_000)], &[&payer], &payer).await.unwrap();

    submit(&mut ctx,
        &[claim_ix(program_id, &user, seeded_vault_pda, user_yield_pda, vault_token_pda, user_reward_token.pubkey(), escrow_pda)],
        &[&user], &user).await.unwrap();

    // The claim must sync the position DOWN to the real escrow balance and
    // down-adjust total_staked_skr — the forged 1M SKR cannot earn.
    let pos = read_position(&mut ctx, user_yield_pda).await;
    assert_eq!(pos.staked_skr, 100_000 * SKR_DECIMALS, "stake must sync to the real escrow balance");
    let vault = read_vault(&mut ctx, seeded_vault_pda).await;
    assert_eq!(vault.total_staked_skr, 100_000 * SKR_DECIMALS, "total shares must follow the real escrow");
    // The payout is bounded by the REAL escrow share: 100k SKR * 1 USDC/SKR.
    assert_eq!(read_token_amount(&mut ctx, user_reward_token.pubkey()).await, 100_000 * 1_000_000);
}

#[tokio::test]
async fn test_skr_yield_multi_user_proportional_dividend_payout() {
    let program_id = Pubkey::new_unique();
    let authority = Keypair::new();
    let user_a = Keypair::new();
    let user_b = Keypair::new();
    let reward_mint = USDC_DEVNET_MINT;

    let mut program_test = ProgramTest::new("clock_lend", program_id, processor!(process_instruction));
    program_test.add_account(reward_mint, Account {
        lamports: 10_000_000, data: create_mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    program_test.add_account(SKR_MINT, Account {
        lamports: 10_000_000, data: create_mint_data(6),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    // Both positions pre-synced with ancient cooldowns; vault total matches.
    let pos_a = add_position(&mut program_test, program_id, user_a.pubkey(), reward_mint, 300_000 * SKR_DECIMALS, 1);
    let pos_b = add_position(&mut program_test, program_id, user_b.pubkey(), reward_mint, 100_000 * SKR_DECIMALS, 1);
    let yield_vault_pda = add_vault(&mut program_test, program_id, reward_mint, authority.pubkey(), 400_000 * SKR_DECIMALS);
    let (vault_token_pda, _) =
        Pubkey::find_program_address(&[SKR_YIELD_TOKEN_SEED, reward_mint.as_ref()], &program_id);
    program_test.add_account(vault_token_pda, Account {
        lamports: 10_000_000,
        data: token_data(reward_mint, yield_vault_pda, 1_000 * 1_000_000),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    let escrow_a = add_skr_escrow(&mut program_test, program_id, user_a.pubkey(), 300_000 * SKR_DECIMALS);
    let escrow_b = add_skr_escrow(&mut program_test, program_id, user_b.pubkey(), 100_000 * SKR_DECIMALS);

    let depositor_token = Keypair::new();
    program_test.add_account(depositor_token.pubkey(), Account {
        lamports: 10_000_000,
        data: token_data(reward_mint, authority.pubkey(), 1_000 * 1_000_000),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    let token_a = Keypair::new();
    program_test.add_account(token_a.pubkey(), Account {
        lamports: 10_000_000, data: token_data(reward_mint, user_a.pubkey(), 0),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });
    let token_b = Keypair::new();
    program_test.add_account(token_b.pubkey(), Account {
        lamports: 10_000_000, data: token_data(reward_mint, user_b.pubkey(), 0),
        owner: spl_token::id(), executable: false, rent_epoch: 0,
    });

    let mut ctx = program_test.start_with_context().await;
    let payer = ctx.payer.insecure_clone();
    submit(&mut ctx, &[
        system_instruction::transfer(&payer.pubkey(), &authority.pubkey(), 1_000_000_000),
        system_instruction::transfer(&payer.pubkey(), &user_a.pubkey(), 1_000_000_000),
        system_instruction::transfer(&payer.pubkey(), &user_b.pubkey(), 1_000_000_000),
    ], &[&payer], &payer).await.unwrap();

    // Deposit $400 against 400k SKR -> acc = 1e9.
    let dep_amount = 400 * 1_000_000;
    submit(&mut ctx,
        &[deposit_ix(program_id, &authority, yield_vault_pda, depositor_token.pubkey(), vault_token_pda, dep_amount)],
        &[&authority], &authority).await.unwrap();

    // A claims 75% = $300, B claims 25% = $100.
    submit(&mut ctx,
        &[claim_ix(program_id, &user_a, yield_vault_pda, pos_a, vault_token_pda, token_a.pubkey(), escrow_a)],
        &[&user_a], &user_a).await.unwrap();
    assert_eq!(read_token_amount(&mut ctx, token_a.pubkey()).await, 300 * 1_000_000, "A must receive exactly $300");

    submit(&mut ctx,
        &[claim_ix(program_id, &user_b, yield_vault_pda, pos_b, vault_token_pda, token_b.pubkey(), escrow_b)],
        &[&user_b], &user_b).await.unwrap();
    assert_eq!(read_token_amount(&mut ctx, token_b.pubkey()).await, 100 * 1_000_000, "B must receive exactly $100");

    let vault = read_vault(&mut ctx, yield_vault_pda).await;
    assert_eq!(vault.pending_rewards, 0);
}
