use borsh::BorshDeserialize;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::{self, Sysvar},
};
#[allow(deprecated)]
use solana_program::system_instruction;

use crate::{
    error::ClockLendError,
    instruction::ClockLendInstruction,
    state::{
        AdminConfig, LendingPool, LoanOrder, LoanStatus, OfferStatus, P2POffer, PoolType, PriceFeed, UserProfile,
        ADMIN_SEED, ESCROW_SEED, LOAN_SEED, ORACLE_SEED, P2P_SEED, POOL_SEED, PROFILE_SEED, TREASURY_SEED, VAULT_SEED,
        SKR_MINT,
    },
};

// Security helper: verify account owner
#[inline(always)]
fn assert_owned_by(account: &AccountInfo, owner: &Pubkey) -> ProgramResult {
    if account.owner != owner {
        return Err(ClockLendError::InvalidAccountOwner.into());
    }
    Ok(())
}

// Security helper: verify signer
#[inline(always)]
fn assert_signer(account: &AccountInfo) -> ProgramResult {
    if !account.is_signer {
        return Err(ClockLendError::Unauthorized.into());
    }
    Ok(())
}

// Security helper: verify SPL Token program ID
#[inline(always)]
fn assert_token_program(account: &AccountInfo) -> ProgramResult {
    if account.key != &spl_token::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    Ok(())
}

// Security helper: verify System program ID
#[inline(always)]
fn assert_system_program(account: &AccountInfo) -> ProgramResult {
    if account.key != &solana_program::system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    Ok(())
}

// Security helper: safe PDA account creation / initialization with front-run lamport injection protection
fn create_or_allocate_pda<'a>(
    program_id: &Pubkey,
    payer: &AccountInfo<'a>,
    pda_account: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    space: usize,
    signers_seeds: &[&[u8]],
) -> ProgramResult {
    if pda_account.owner == &solana_program::system_program::id() {
        let rent = Rent::get()?;
        let required_lamports = rent.minimum_balance(space);
        if pda_account.lamports() == 0 {
            invoke_signed(
                &system_instruction::create_account(
                    payer.key,
                    pda_account.key,
                    required_lamports,
                    space as u64,
                    program_id,
                ),
                &[payer.clone(), pda_account.clone(), system_program.clone()],
                &[signers_seeds],
            )?;
        } else {
            let lamports_diff = required_lamports.saturating_sub(pda_account.lamports());
            if lamports_diff > 0 {
                invoke(
                    &system_instruction::transfer(payer.key, pda_account.key, lamports_diff),
                    &[payer.clone(), pda_account.clone(), system_program.clone()],
                )?;
            }
            invoke_signed(
                &system_instruction::allocate(pda_account.key, space as u64),
                &[pda_account.clone(), system_program.clone()],
                &[signers_seeds],
            )?;
            invoke_signed(
                &system_instruction::assign(pda_account.key, program_id),
                &[pda_account.clone(), system_program.clone()],
                &[signers_seeds],
            )?;
        }
        Ok(())
    } else {
        assert_owned_by(pda_account, program_id)?;
        Ok(())
    }
}

// Security helper: safe PDA SPL Token Account creation / initialization via CPI
fn create_or_allocate_token_pda<'a>(
    payer: &AccountInfo<'a>,
    pda_account: &AccountInfo<'a>,
    mint_account: &AccountInfo<'a>,
    owner_authority: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    token_program: &AccountInfo<'a>,
    rent_sysvar_opt: Option<&AccountInfo<'a>>,
    signers_seeds: &[&[u8]],
) -> ProgramResult {
    if pda_account.owner == &solana_program::system_program::id() {
        let rent = Rent::get()?;
        let space = spl_token::state::Account::LEN;
        let required_lamports = rent.minimum_balance(space);
        if pda_account.lamports() == 0 {
            invoke_signed(
                &system_instruction::create_account(
                    payer.key,
                    pda_account.key,
                    required_lamports,
                    space as u64,
                    token_program.key,
                ),
                &[payer.clone(), pda_account.clone(), system_program.clone()],
                &[signers_seeds],
            )?;
        } else {
            let diff = required_lamports.saturating_sub(pda_account.lamports());
            if diff > 0 {
                invoke(
                    &system_instruction::transfer(payer.key, pda_account.key, diff),
                    &[payer.clone(), pda_account.clone(), system_program.clone()],
                )?;
            }
            invoke_signed(
                &system_instruction::allocate(pda_account.key, space as u64),
                &[pda_account.clone(), system_program.clone()],
                &[signers_seeds],
            )?;
            invoke_signed(
                &system_instruction::assign(pda_account.key, token_program.key),
                &[pda_account.clone(), system_program.clone()],
                &[signers_seeds],
            )?;
        }

        if let Some(rent_sysvar) = rent_sysvar_opt {
            invoke(
                &spl_token::instruction::initialize_account(
                    token_program.key,
                    pda_account.key,
                    mint_account.key,
                    owner_authority.key,
                )?,
                &[
                    pda_account.clone(),
                    mint_account.clone(),
                    owner_authority.clone(),
                    rent_sysvar.clone(),
                    token_program.clone(),
                ],
            )?;
        } else {
            invoke(
                &spl_token::instruction::initialize_account3(
                    token_program.key,
                    pda_account.key,
                    mint_account.key,
                    owner_authority.key,
                )?,
                &[
                    pda_account.clone(),
                    mint_account.clone(),
                    token_program.clone(),
                ],
            )?;
        }
        Ok(())
    } else {
        if pda_account.owner != token_program.key {
            return Err(ClockLendError::InvalidAccountOwner.into());
        }
        Ok(())
    }
}

// Security helper: safely transfer native SOL from an escrow PDA to a destination account
fn transfer_native_sol_from_escrow<'a>(
    escrow: &AccountInfo<'a>,
    dest: &AccountInfo<'a>,
    system_program_opt: Option<&AccountInfo<'a>>,
    amount: u64,
    signers_seeds: &[&[u8]],
) -> ProgramResult {
    let escrow_lamports = escrow.lamports();
    // F-08: Ensure escrow has sufficient lamports; do not silently clamp
    if escrow_lamports < amount {
        return Err(ClockLendError::InsufficientCollateral.into());
    }
    if amount == 0 {
        return Ok(());
    }

    if escrow.owner == &solana_program::system_program::id() {
        if let Some(sys_prog) = system_program_opt {
            invoke_signed(
                &system_instruction::transfer(escrow.key, dest.key, amount),
                &[escrow.clone(), dest.clone(), sys_prog.clone()],
                &[signers_seeds],
            )?;
            return Ok(());
        }
    }

    **escrow.try_borrow_mut_lamports()? = escrow_lamports.saturating_sub(amount);
    **dest.try_borrow_mut_lamports()? = dest.lamports().saturating_add(amount);
    Ok(())
}

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = ClockLendInstruction::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;

    match instruction {
        ClockLendInstruction::InitializePool {
            pool_id,
            pool_type,
            interest_rate_bps,
            max_ltv_bps,
            min_duration,
            max_duration,
            name,
        } => {
            let is_oracle_free = name.starts_with(b"ORACLE_FREE")
                || name.starts_with(b"oracle_free")
                || (instruction_data.len() > 62 && instruction_data[62] != 0);
            process_initialize_pool(
                program_id,
                accounts,
                pool_id,
                pool_type,
                interest_rate_bps,
                max_ltv_bps,
                min_duration,
                max_duration,
                name,
                is_oracle_free,
            )
        }
        ClockLendInstruction::DepositLiquidity { amount } => {
            process_deposit_liquidity(program_id, accounts, amount)
        }
        ClockLendInstruction::StakeSKR { amount } => {
            process_stake_skr(program_id, accounts, amount)
        }
        ClockLendInstruction::BorrowFromPool {
            loan_id,
            borrow_amount,
            collateral_amount,
            duration_seconds,
        } => process_borrow_from_pool(
            program_id,
            accounts,
            loan_id,
            borrow_amount,
            collateral_amount,
            duration_seconds,
        ),
        ClockLendInstruction::CreateP2POffer {
            offer_id,
            requested_amount,
            collateral_amount,
            interest_offered,
            duration_seconds,
        } => process_create_p2p_offer(
            program_id,
            accounts,
            offer_id,
            requested_amount,
            collateral_amount,
            interest_offered,
            duration_seconds,
        ),
        ClockLendInstruction::FundP2POffer => process_fund_p2p_offer(program_id, accounts),
        ClockLendInstruction::RepayLoan { repay_amount } => {
            process_repay_loan(program_id, accounts, repay_amount)
        }
        ClockLendInstruction::TriggerGracePeriod => {
            process_trigger_grace_period(program_id, accounts)
        }
        ClockLendInstruction::ClaimDefault => process_claim_default(program_id, accounts),
        ClockLendInstruction::WithdrawLiquidity { amount } => {
            process_withdraw_liquidity(program_id, accounts, amount)
        }
        ClockLendInstruction::CancelP2POffer => {
            process_cancel_p2p_offer(program_id, accounts)
        }
        ClockLendInstruction::UnstakeSKR { amount } => {
            process_unstake_skr(program_id, accounts, amount)
        }
        ClockLendInstruction::SetPriceFeed {
            price_micro_usd,
            decimals,
        } => process_set_price_feed(program_id, accounts, price_micro_usd, decimals),
        ClockLendInstruction::InitializeAdmin => process_initialize_admin(program_id, accounts),
    }
}

pub fn process_initialize_pool(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    pool_id: u64,
    pool_type: PoolType,
    interest_rate_bps: u16,
    max_ltv_bps: u16,
    min_duration: i64,
    max_duration: i64,
    name: [u8; 32],
    is_oracle_free: bool,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let authority = next_account_info(account_info_iter)?;
    let pool_account = next_account_info(account_info_iter)?;
    let liquidity_mint = next_account_info(account_info_iter)?;
    let vault_account = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;
    let _rent_sysvar = next_account_info(account_info_iter)?;

    assert_signer(authority)?;
    assert_system_program(system_program)?;

    let pool_id_bytes = pool_id.to_le_bytes();
    let (expected_pool_pda, pool_bump) = Pubkey::find_program_address(
        &[POOL_SEED, authority.key.as_ref(), &pool_id_bytes],
        program_id,
    );
    if expected_pool_pda != *pool_account.key {
        return Err(ClockLendError::InvalidSeeds.into());
    }

    let (expected_vault_pda, vault_bump) =
        Pubkey::find_program_address(&[VAULT_SEED, pool_account.key.as_ref()], program_id);
    if expected_vault_pda != *vault_account.key {
        return Err(ClockLendError::InvalidSeeds.into());
    }

    let token_program_opt = next_account_info(account_info_iter).ok();

    // Security check: Reject re-initialization if pool already active
    if pool_account.owner == program_id && !pool_account.data_is_empty() {
        if let Ok(existing_pool) = LendingPool::unpack_from_slice(&pool_account.try_borrow_data()?) {
            if existing_pool.is_initialized {
                return Err(ClockLendError::PoolAlreadyInitialized.into());
            }
        }
    }

    // Safe PDA creation immune to front-running lamport injection
    create_or_allocate_pda(
        program_id,
        authority,
        pool_account,
        system_program,
        LendingPool::LEN,
        &[POOL_SEED, authority.key.as_ref(), &pool_id_bytes, &[pool_bump]],
    )?;

    // Informational 2: Liquidity pools must be SPL token based. Raw native SOL is rejected (Wrapped SOL must be used instead).
    if *liquidity_mint.key == Pubkey::default() || liquidity_mint.key == &solana_program::system_program::ID {
        return Err(ClockLendError::UnsupportedCollateralMint.into());
    }

    // F-02: If SPL token/WSOL liquidity mint and vault account is uninitialized, create/initialize vault token PDA
    if vault_account.owner == &solana_program::system_program::id() {
        if let Some(token_program) = token_program_opt {
            create_or_allocate_token_pda(
                authority,
                vault_account,
                liquidity_mint,
                vault_account,
                system_program,
                token_program,
                None,
                &[VAULT_SEED, pool_account.key.as_ref(), &[vault_bump]],
            )?;
        }
    }

    let pool = LendingPool {
        is_initialized: true,
        pool_type,
        authority: *authority.key,
        liquidity_mint: *liquidity_mint.key,
        vault_pda: *vault_account.key,
        total_liquidity: 0,
        total_borrowed: 0,
        staked_skr_amount: 0,
        interest_rate_bps,
        max_ltv_bps,
        min_duration,
        max_duration,
        loans_originated: 0,
        loans_repaid: 0,
        name,
        is_oracle_free,
    };

    pool.pack_into_slice(&mut pool_account.try_borrow_mut_data()?)?;
    msg!("ClockLend: Lending Pool #{} initialized successfully (type: {:?}, oracle_free: {})", pool_id, pool_type, is_oracle_free);
    Ok(())
}

pub fn process_deposit_liquidity(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let depositor = next_account_info(account_info_iter)?;
    let pool_account = next_account_info(account_info_iter)?;
    let depositor_token_account = next_account_info(account_info_iter)?;
    let vault_account = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;

    assert_signer(depositor)?;
    assert_owned_by(pool_account, program_id)?;
    assert_token_program(token_program)?;

    if amount == 0 {
        return Err(ClockLendError::InvalidInstruction.into());
    }

    let mut pool = LendingPool::unpack_from_slice(&pool_account.try_borrow_data()?)?;
    if !pool.is_initialized {
        return Err(ClockLendError::PoolInactive.into());
    }

    // Security check: ensure vault account is the registered pool vault
    if *vault_account.key != pool.vault_pda {
        return Err(ClockLendError::InvalidVaultAccount.into());
    }

    // F-11: Validate token mints match pool's declared liquidity_mint
    let depositor_token = spl_token::state::Account::unpack(&depositor_token_account.try_borrow_data()?)?;
    if depositor_token.mint != pool.liquidity_mint {
        return Err(ClockLendError::InvalidMint.into());
    }
    let vault_token = spl_token::state::Account::unpack(&vault_account.try_borrow_data()?)?;
    if vault_token.mint != pool.liquidity_mint {
        return Err(ClockLendError::InvalidMint.into());
    }

    // Transfer liquidity tokens from depositor to pool vault
    invoke(
        &spl_token::instruction::transfer(
            token_program.key,
            depositor_token_account.key,
            vault_account.key,
            depositor.key,
            &[],
            amount,
        )?,
        &[
            depositor_token_account.clone(),
            vault_account.clone(),
            depositor.clone(),
            token_program.clone(),
        ],
    )?;

    pool.total_liquidity = pool
        .total_liquidity
        .checked_add(amount)
        .ok_or(ClockLendError::AmountOverflow)?;
    pool.pack_into_slice(&mut pool_account.try_borrow_mut_data()?)?;

    msg!("ClockLend: Deposited {} liquidity into pool", amount);
    Ok(())
}

pub fn process_stake_skr(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let user = next_account_info(account_info_iter)?;
    let user_profile_account = next_account_info(account_info_iter)?;

    // Dynamically distinguish if optional pool account is present
    let next_acc = next_account_info(account_info_iter)?;
    let (pool_account_opt, user_skr_account) = if next_acc.owner == program_id && (next_acc.data_len() == LendingPool::LEN || next_acc.data_len() == 182) {
        (Some(next_acc), next_account_info(account_info_iter)?)
    } else {
        (None, next_acc)
    };
    let skr_escrow_account = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;

    assert_signer(user)?;
    assert_token_program(token_program)?;
    assert_system_program(system_program)?;

    if amount == 0 {
        return Err(ClockLendError::InvalidInstruction.into());
    }

    let (expected_profile_pda, profile_bump) =
        Pubkey::find_program_address(&[PROFILE_SEED, user.key.as_ref()], program_id);
    if expected_profile_pda != *user_profile_account.key {
        return Err(ClockLendError::InvalidSeeds.into());
    }

    let (expected_skr_escrow_pda, escrow_bump) =
        Pubkey::find_program_address(&[b"skr_escrow", user.key.as_ref()], program_id);
    if expected_skr_escrow_pda != *skr_escrow_account.key {
        return Err(ClockLendError::InvalidEscrowAccount.into());
    }

    let skr_mint_opt = next_account_info(account_info_iter).ok();

    // Security check: validate user_skr_account is owned by caller and mint is canonical SKR
    let user_token = spl_token::state::Account::unpack(&user_skr_account.try_borrow_data()?)?;
    if user_token.owner != *user.key {
        return Err(ClockLendError::Unauthorized.into());
    }
    if user_token.mint != SKR_MINT {
        return Err(ClockLendError::InvalidMint.into());
    }

    // F-02: Create/initialize skr_escrow_account token PDA if uninitialized
    if skr_escrow_account.owner == &solana_program::system_program::id() {
        if let Some(skr_mint) = skr_mint_opt {
            if *skr_mint.key != SKR_MINT {
                return Err(ClockLendError::InvalidMint.into());
            }
            create_or_allocate_token_pda(
                user,
                skr_escrow_account,
                skr_mint,
                skr_escrow_account,
                system_program,
                token_program,
                None,
                &[b"skr_escrow", user.key.as_ref(), &[escrow_bump]],
            )?;
        }
    }

    let is_new_profile = user_profile_account.owner == &solana_program::system_program::id();
    create_or_allocate_pda(
        program_id,
        user,
        user_profile_account,
        system_program,
        UserProfile::LEN,
        &[PROFILE_SEED, user.key.as_ref(), &[profile_bump]],
    )?;

    if is_new_profile {
        let initial_profile = UserProfile {
            is_initialized: true,
            user: *user.key,
            staked_skr: 0,
            total_loans_completed: 0,
            total_loans_defaulted: 0,
            reputation_score: 10000, // 100% starting reputation
            locked_skr: 0,
        };
        initial_profile.pack_into_slice(&mut user_profile_account.try_borrow_mut_data()?)?;
    }

    // Transfer SKR tokens to escrow
    invoke(
        &spl_token::instruction::transfer(
            token_program.key,
            user_skr_account.key,
            skr_escrow_account.key,
            user.key,
            &[],
            amount,
        )?,
        &[
            user_skr_account.clone(),
            skr_escrow_account.clone(),
            user.clone(),
            token_program.clone(),
        ],
    )?;

    // Update user profile
    let mut profile = UserProfile::unpack_from_slice(&user_profile_account.try_borrow_data()?)?;
    profile.staked_skr = profile
        .staked_skr
        .checked_add(amount)
        .ok_or(ClockLendError::AmountOverflow)?;
    profile.pack_into_slice(&mut user_profile_account.try_borrow_mut_data()?)?;

    // If pool is also provided, update pool's staked_skr (for verified merchant status)
    // Security check: Only the pool authority can stake SKR to back their own pool
    if let Some(pool_account) = pool_account_opt {
        if pool_account.owner == program_id && !pool_account.data_is_empty() {
            let mut pool = LendingPool::unpack_from_slice(&pool_account.try_borrow_data()?)?;
            if pool.authority != *user.key {
                return Err(ClockLendError::Unauthorized.into());
            }
            pool.staked_skr_amount = pool
                .staked_skr_amount
                .checked_add(amount)
                .ok_or(ClockLendError::AmountOverflow)?;
            pool.pack_into_slice(&mut pool_account.try_borrow_mut_data()?)?;
        }
    }

    msg!("ClockLend: Staked {} SKR tokens successfully", amount);
    Ok(())
}

pub fn process_unstake_skr(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let user = next_account_info(account_info_iter)?;
    let user_profile_account = next_account_info(account_info_iter)?;
    let user_skr_account = next_account_info(account_info_iter)?;
    let skr_escrow_account = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;
    let pool_account_opt = next_account_info(account_info_iter).ok();

    assert_signer(user)?;
    assert_token_program(token_program)?;
    assert_owned_by(user_profile_account, program_id)?;

    if amount == 0 {
        return Err(ClockLendError::InvalidInstruction.into());
    }

    let (expected_profile_pda, _) =
        Pubkey::find_program_address(&[PROFILE_SEED, user.key.as_ref()], program_id);
    if expected_profile_pda != *user_profile_account.key {
        return Err(ClockLendError::InvalidSeeds.into());
    }

    let mut profile = UserProfile::unpack_from_slice(&user_profile_account.try_borrow_data()?)?;
    if profile.user != *user.key {
        return Err(ClockLendError::Unauthorized.into());
    }

    if profile.staked_skr < amount {
        return Err(ClockLendError::ExpectedAmountMismatch.into());
    }

    // Security check: Staked SKR cannot be withdrawn while locked by active loans
    let available_skr = profile.staked_skr.saturating_sub(profile.locked_skr);
    if available_skr < amount {
        return Err(ClockLendError::StakeLocked.into());
    }

    let (expected_skr_escrow_pda, escrow_bump) =
        Pubkey::find_program_address(&[b"skr_escrow", user.key.as_ref()], program_id);
    if expected_skr_escrow_pda != *skr_escrow_account.key {
        return Err(ClockLendError::InvalidEscrowAccount.into());
    }

    let user_skr_token = spl_token::state::Account::unpack(&user_skr_account.try_borrow_data()?)?;
    if user_skr_token.owner != *user.key {
        return Err(ClockLendError::Unauthorized.into());
    }

    // Transfer SKR tokens from escrow PDA back to user wallet
    invoke_signed(
        &spl_token::instruction::transfer(
            token_program.key,
            skr_escrow_account.key,
            user_skr_account.key,
            skr_escrow_account.key,
            &[],
            amount,
        )?,
        &[
            skr_escrow_account.clone(),
            user_skr_account.clone(),
            token_program.clone(),
        ],
        &[&[b"skr_escrow", user.key.as_ref(), &[escrow_bump]]],
    )?;

    profile.staked_skr = profile
        .staked_skr
        .checked_sub(amount)
        .ok_or(ClockLendError::AmountOverflow)?;
    profile.pack_into_slice(&mut user_profile_account.try_borrow_mut_data()?)?;

    // Informational 1: If pool is also provided, update pool's staked_skr_amount (to keep in sync)
    if let Some(pool_account) = pool_account_opt {
        if pool_account.owner == program_id && !pool_account.data_is_empty() {
            let mut pool = LendingPool::unpack_from_slice(&pool_account.try_borrow_data()?)?;
            if pool.authority == *user.key {
                pool.staked_skr_amount = pool.staked_skr_amount.saturating_sub(amount);
                pool.pack_into_slice(&mut pool_account.try_borrow_mut_data()?)?;
            }
        }
    }

    msg!("ClockLend: Unstaked {} SKR tokens successfully", amount);
    Ok(())
}

pub fn process_initialize_admin(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let authority = next_account_info(account_info_iter)?;
    let admin_account = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;

    assert_signer(authority)?;
    assert_system_program(system_program)?;

    let (expected_admin_pda, bump) = Pubkey::find_program_address(&[ADMIN_SEED], program_id);
    if expected_admin_pda != *admin_account.key {
        return Err(ClockLendError::InvalidSeeds.into());
    }

    if admin_account.owner == program_id && !admin_account.data_is_empty() {
        if let Ok(existing) = AdminConfig::unpack_from_slice(&admin_account.try_borrow_data()?) {
            if existing.is_initialized {
                return Err(ClockLendError::Unauthorized.into());
            }
        }
    }

    create_or_allocate_pda(
        program_id,
        authority,
        admin_account,
        system_program,
        AdminConfig::LEN,
        &[ADMIN_SEED, &[bump]],
    )?;

    let config = AdminConfig {
        is_initialized: true,
        admin: *authority.key,
    };
    config.pack_into_slice(&mut admin_account.try_borrow_mut_data()?)?;

    msg!("ClockLend: AdminConfig initialized with admin: {}", authority.key);
    Ok(())
}

pub fn process_set_price_feed(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    price_micro_usd: u64,
    decimals: u8,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let authority = next_account_info(account_info_iter)?;
    let oracle_account = next_account_info(account_info_iter)?;
    let mint_account = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;

    assert_signer(authority)?;
    assert_system_program(system_program)?;

    if price_micro_usd == 0 {
        return Err(ClockLendError::InvalidInstruction.into());
    }

    let (expected_admin_pda, _) = Pubkey::find_program_address(&[ADMIN_SEED], program_id);
    let (expected_global_oracle_pda, global_bump) =
        Pubkey::find_program_address(&[ORACLE_SEED, mint_account.key.as_ref()], program_id);

    let mut clock_sysvar_opt: Option<&AccountInfo> = None;
    let mut admin_account_opt: Option<&AccountInfo> = None;
    let mut pool_account_opt: Option<&AccountInfo> = None;

    while let Ok(acc) = next_account_info(account_info_iter) {
        if *acc.key == sysvar::clock::id() {
            clock_sysvar_opt = Some(acc);
        } else if *acc.key == expected_admin_pda {
            admin_account_opt = Some(acc);
        } else if acc.owner == program_id && (acc.data_len() == LendingPool::LEN || acc.data_len() == 182) {
            pool_account_opt = Some(acc);
        }
    }

    let (is_pool_oracle, _bump, pda_seeds): (bool, u8, Vec<Vec<u8>>) = if let Some(pool_acc) = pool_account_opt {
        let (expected_pool_oracle, pool_oracle_bump) = Pubkey::find_program_address(
            &[ORACLE_SEED, pool_acc.key.as_ref(), mint_account.key.as_ref()],
            program_id,
        );
        if expected_pool_oracle == *oracle_account.key {
            (true, pool_oracle_bump, vec![
                ORACLE_SEED.to_vec(),
                pool_acc.key.as_ref().to_vec(),
                mint_account.key.as_ref().to_vec(),
                vec![pool_oracle_bump],
            ])
        } else if expected_global_oracle_pda == *oracle_account.key {
            (false, global_bump, vec![
                ORACLE_SEED.to_vec(),
                mint_account.key.as_ref().to_vec(),
                vec![global_bump],
            ])
        } else {
            return Err(ClockLendError::InvalidSeeds.into());
        }
    } else {
        if expected_global_oracle_pda != *oracle_account.key {
            return Err(ClockLendError::InvalidSeeds.into());
        }
        (false, global_bump, vec![
            ORACLE_SEED.to_vec(),
            mint_account.key.as_ref().to_vec(),
            vec![global_bump],
        ])
    };

    let mut feed = if !oracle_account.data_is_empty() && oracle_account.owner == program_id {
        if let Ok(existing) = PriceFeed::unpack_from_slice(&oracle_account.try_borrow_data()?) {
            if existing.is_initialized {
                // If feed already initialized, signer MUST be existing.authority
                if existing.authority != *authority.key {
                    return Err(ClockLendError::Unauthorized.into());
                }
                existing
            } else {
                return Err(ClockLendError::InvalidOracleAccount.into());
            }
        } else {
            return Err(ClockLendError::InvalidOracleAccount.into());
        }
    } else {
        // Feed does not exist yet: First-time initialization must be authorized!
        if is_pool_oracle {
            let pool_acc = pool_account_opt.ok_or(ClockLendError::Unauthorized)?;
            let pool = LendingPool::unpack_from_slice(&pool_acc.try_borrow_data()?)?;
            if pool.authority != *authority.key {
                return Err(ClockLendError::Unauthorized.into());
            }
        } else {
            // Global oracle feed: caller MUST be AdminConfig.admin!
            let admin_acc = admin_account_opt.ok_or(ClockLendError::Unauthorized)?;
            if admin_acc.owner != program_id || admin_acc.data_is_empty() {
                return Err(ClockLendError::Unauthorized.into());
            }
            let admin_config = AdminConfig::unpack_from_slice(&admin_acc.try_borrow_data()?)?;
            if !admin_config.is_initialized || admin_config.admin != *authority.key {
                return Err(ClockLendError::Unauthorized.into());
            }
        }

        let signers_seeds: Vec<&[u8]> = pda_seeds.iter().map(|s| s.as_slice()).collect();
        create_or_allocate_pda(
            program_id,
            authority,
            oracle_account,
            system_program,
            PriceFeed::LEN,
            &signers_seeds,
        )?;

        PriceFeed {
            is_initialized: true,
            mint: *mint_account.key,
            price_micro_usd: 0,
            decimals: 0,
            last_updated_at: 0,
            authority: *authority.key,
        }
    };

    let unix_timestamp = if let Some(clock_acc) = clock_sysvar_opt {
        if let Ok(clock) = solana_program::sysvar::clock::Clock::from_account_info(clock_acc) {
            clock.unix_timestamp
        } else {
            Clock::get()?.unix_timestamp
        }
    } else {
        Clock::get()?.unix_timestamp
    };

    feed.is_initialized = true;
    feed.mint = *mint_account.key;
    feed.price_micro_usd = price_micro_usd;
    feed.decimals = decimals;
    feed.last_updated_at = unix_timestamp;
    feed.authority = *authority.key;

    feed.pack_into_slice(&mut oracle_account.try_borrow_mut_data()?)?;

    msg!(
        "ClockLend: Price feed set for mint {} to {} micro-USD (decimals: {}, timestamp: {})",
        mint_account.key,
        price_micro_usd,
        decimals,
        unix_timestamp
    );

    Ok(())
}

pub fn process_borrow_from_pool(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    loan_id: u64,
    borrow_amount: u64,
    collateral_amount: u64,
    duration_seconds: i64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let borrower = next_account_info(account_info_iter)?;
    let pool_account = next_account_info(account_info_iter)?;
    let loan_order_account = next_account_info(account_info_iter)?;
    let vault_account = next_account_info(account_info_iter)?;
    let borrower_liquidity_account = next_account_info(account_info_iter)?;
    let borrower_collateral_account = next_account_info(account_info_iter)?;
    let collateral_escrow_account = next_account_info(account_info_iter)?;
    let collateral_mint = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;

    assert_signer(borrower)?;
    assert_owned_by(pool_account, program_id)?;
    assert_system_program(system_program)?;

    if borrow_amount == 0 || collateral_amount == 0 {
        return Err(ClockLendError::InvalidInstruction.into());
    }

    let mut pool = LendingPool::unpack_from_slice(&pool_account.try_borrow_data()?)?;
    if !pool.is_initialized {
        return Err(ClockLendError::PoolInactive.into());
    }

    // Security check: verify vault account
    if *vault_account.key != pool.vault_pda {
        return Err(ClockLendError::InvalidVaultAccount.into());
    }

    if pool.total_liquidity < borrow_amount {
        return Err(ClockLendError::InsufficientLiquidity.into());
    }

    if duration_seconds < pool.min_duration || duration_seconds > pool.max_duration || duration_seconds > 365 * 86400 {
        return Err(ClockLendError::InvalidInstruction.into());
    }

    // Dual Collateral Branching: Native SOL vs SPL Token (SKR)
    let is_native_sol = *collateral_mint.key == Pubkey::default()
        || collateral_mint.key == &solana_program::system_program::ID
        || collateral_mint.key == &spl_token::native_mint::id();
    let is_pool_native_sol = pool.liquidity_mint == Pubkey::default()
        || pool.liquidity_mint == solana_program::system_program::ID
        || pool.liquidity_mint == spl_token::native_mint::id();

    // F-11: Validate liquidity mint matches pool definition
    if !is_pool_native_sol {
        let vault_token = spl_token::state::Account::unpack(&vault_account.try_borrow_data()?)?;
        if vault_token.mint != pool.liquidity_mint {
            return Err(ClockLendError::InvalidMint.into());
        }
        let borrower_liq = spl_token::state::Account::unpack(&borrower_liquidity_account.try_borrow_data()?)?;
        if borrower_liq.mint != pool.liquidity_mint {
            return Err(ClockLendError::InvalidMint.into());
        }
    }

    // F-03: Collateral Allowlist - Collateral must be Native SOL or canonical SKR
    let is_skr = *collateral_mint.key == SKR_MINT;
    if !is_native_sol && !is_skr {
        return Err(ClockLendError::InvalidMint.into());
    }

    // Canonical mint pubkeys for oracle PDA derivation
    let canonical_collateral_mint = if is_native_sol {
        spl_token::native_mint::id()
    } else {
        *collateral_mint.key
    };
    let canonical_pool_mint = if is_pool_native_sol {
        spl_token::native_mint::id()
    } else {
        pool.liquidity_mint
    };

    let (expected_profile_pda, _) =
        Pubkey::find_program_address(&[PROFILE_SEED, borrower.key.as_ref()], program_id);
    let (expected_treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], program_id);

    let (expected_pool_collateral_oracle1, _) =
        Pubkey::find_program_address(&[ORACLE_SEED, pool_account.key.as_ref(), collateral_mint.key.as_ref()], program_id);
    let (expected_pool_collateral_oracle2, _) =
        Pubkey::find_program_address(&[ORACLE_SEED, pool_account.key.as_ref(), canonical_collateral_mint.as_ref()], program_id);

    let (expected_global_collateral_oracle1, _) =
        Pubkey::find_program_address(&[ORACLE_SEED, collateral_mint.key.as_ref()], program_id);
    let (expected_global_collateral_oracle2, _) =
        Pubkey::find_program_address(&[ORACLE_SEED, canonical_collateral_mint.as_ref()], program_id);

    let (expected_pool_liq_oracle1, _) =
        Pubkey::find_program_address(&[ORACLE_SEED, pool_account.key.as_ref(), pool.liquidity_mint.as_ref()], program_id);
    let (expected_pool_liq_oracle2, _) =
        Pubkey::find_program_address(&[ORACLE_SEED, pool_account.key.as_ref(), canonical_pool_mint.as_ref()], program_id);

    let (expected_global_pool_oracle1, _) =
        Pubkey::find_program_address(&[ORACLE_SEED, pool.liquidity_mint.as_ref()], program_id);
    let (expected_global_pool_oracle2, _) =
        Pubkey::find_program_address(&[ORACLE_SEED, canonical_pool_mint.as_ref()], program_id);

    let mut user_profile_opt: Option<&AccountInfo> = None;
    let mut treasury_account_opt: Option<&AccountInfo> = None;
    let mut collateral_oracle_opt: Option<&AccountInfo> = None;
    let mut pool_oracle_opt: Option<&AccountInfo> = None;

    // Scan trailing optional accounts
    while let Ok(acc) = next_account_info(account_info_iter) {
        if *acc.key == expected_profile_pda {
            user_profile_opt = Some(acc);
        } else if *acc.key == expected_treasury_pda {
            treasury_account_opt = Some(acc);
        } else if *acc.key == expected_pool_collateral_oracle1 || *acc.key == expected_pool_collateral_oracle2 {
            collateral_oracle_opt = Some(acc);
        } else if *acc.key == expected_global_collateral_oracle1 || *acc.key == expected_global_collateral_oracle2 {
            if collateral_oracle_opt.is_none() {
                collateral_oracle_opt = Some(acc);
            }
        } else if *acc.key == expected_pool_liq_oracle1 || *acc.key == expected_pool_liq_oracle2 {
            pool_oracle_opt = Some(acc);
        } else if *acc.key == expected_global_pool_oracle1 || *acc.key == expected_global_pool_oracle2 {
            if pool_oracle_opt.is_none() {
                pool_oracle_opt = Some(acc);
            }
        } else if acc.owner == token_program.key {
            if let Ok(tok) = spl_token::state::Account::unpack(&acc.try_borrow_data()?) {
                if tok.owner == expected_treasury_pda && tok.mint == pool.liquidity_mint {
                    treasury_account_opt = Some(acc);
                }
            }
        }
    }

    const MAX_ORACLE_STALENESS_SECONDS: i64 = 86400; // 24 hours
    let current_time = Clock::get()?.unix_timestamp;

    // Resolve dynamic collateral price & decimals (mandatory unless pool.is_oracle_free)
    let (collateral_price_micro_usd, collateral_decimals): (u64, u8) = if let Some(oracle_acc) = collateral_oracle_opt {
        if oracle_acc.owner == program_id && !oracle_acc.data_is_empty() {
            let feed = PriceFeed::unpack_from_slice(&oracle_acc.try_borrow_data()?)?;
            if !feed.is_initialized || feed.price_micro_usd == 0 {
                return Err(ClockLendError::InvalidOracleAccount.into());
            }
            if feed.mint != *collateral_mint.key && feed.mint != canonical_collateral_mint {
                return Err(ClockLendError::InvalidOracleAccount.into());
            }
            if feed.last_updated_at <= 0 || current_time.saturating_sub(feed.last_updated_at) > MAX_ORACLE_STALENESS_SECONDS {
                return Err(ClockLendError::StaleOraclePrice.into());
            }
            (feed.price_micro_usd, feed.decimals)
        } else if oracle_acc.owner == &solana_program::system_program::id() && oracle_acc.data_is_empty() {
            // Unprovisioned PDA: ONLY permitted if pool is explicitly oracle-free!
            if pool.is_oracle_free {
                if is_native_sol {
                    (150_000_000, 9)
                } else {
                    (20_000, 6)
                }
            } else {
                return Err(ClockLendError::InvalidOracleAccount.into());
            }
        } else {
            return Err(ClockLendError::InvalidOracleAccount.into());
        }
    } else {
        // Oracle omitted: ONLY permitted if pool is explicitly oracle-free!
        if pool.is_oracle_free {
            if is_native_sol {
                (150_000_000, 9) // Baseline $150.00 / SOL (9 decimals)
            } else {
                (20_000, 6)      // Baseline $0.02 / SKR (6 decimals)
            }
        } else {
            return Err(ClockLendError::InvalidOracleAccount.into());
        }
    };

    // Resolve dynamic pool liquidity price & decimals (mandatory for non-USD unless pool.is_oracle_free)
    let (pool_price_micro_usd, pool_decimals): (u64, u8) = if let Some(oracle_acc) = pool_oracle_opt {
        if oracle_acc.owner == program_id && !oracle_acc.data_is_empty() {
            let feed = PriceFeed::unpack_from_slice(&oracle_acc.try_borrow_data()?)?;
            if !feed.is_initialized || feed.price_micro_usd == 0 {
                return Err(ClockLendError::InvalidOracleAccount.into());
            }
            if feed.mint != pool.liquidity_mint && feed.mint != canonical_pool_mint {
                return Err(ClockLendError::InvalidOracleAccount.into());
            }
            if feed.last_updated_at <= 0 || current_time.saturating_sub(feed.last_updated_at) > MAX_ORACLE_STALENESS_SECONDS {
                return Err(ClockLendError::StaleOraclePrice.into());
            }
            (feed.price_micro_usd, feed.decimals)
        } else if oracle_acc.owner == &solana_program::system_program::id() && oracle_acc.data_is_empty() {
            if pool.is_oracle_free || !is_pool_native_sol {
                if is_pool_native_sol {
                    (150_000_000, 9)
                } else {
                    (1_000_000, 6)
                }
            } else {
                return Err(ClockLendError::InvalidOracleAccount.into());
            }
        } else {
            return Err(ClockLendError::InvalidOracleAccount.into());
        }
    } else {
        if pool.is_oracle_free || !is_pool_native_sol {
            if is_pool_native_sol {
                (150_000_000, 9) // Baseline $150.00 / SOL (9 decimals)
            } else {
                (1_000_000, 6)   // Baseline $1.00 / USDC (6 decimals)
            }
        } else {
            return Err(ClockLendError::InvalidOracleAccount.into());
        }
    };

    // Dynamic valuation normalized to pool liquidity denomination
    let collateral_value = if is_native_sol && is_pool_native_sol {
        collateral_amount as u128
    } else if !is_pool_native_sol {
        // Pool is USDC (or other 6-decimal USD pegged pool)
        (collateral_amount as u128)
            .checked_mul(collateral_price_micro_usd as u128)
            .ok_or(ClockLendError::AmountOverflow)?
            / (10u128.pow(collateral_decimals as u32))
    } else {
        // Pool is Native SOL, collateral is SKR (or other token)
        let num = (collateral_amount as u128)
            .checked_mul(collateral_price_micro_usd as u128)
            .ok_or(ClockLendError::AmountOverflow)?
            .checked_mul(10u128.pow(pool_decimals as u32))
            .ok_or(ClockLendError::AmountOverflow)?;
        let den = (10u128.pow(collateral_decimals as u32))
            .checked_mul(pool_price_micro_usd as u128)
            .ok_or(ClockLendError::AmountOverflow)?;
        num / den
    };

    let max_borrow_allowed = collateral_value
        .checked_mul(pool.max_ltv_bps as u128)
        .ok_or(ClockLendError::AmountOverflow)?
        / 10000u128;
    if (borrow_amount as u128) > max_borrow_allowed {
        return Err(ClockLendError::InvalidCollateralRatio.into());
    }

    // Verify loan order PDA
    let loan_id_bytes = loan_id.to_le_bytes();
    let (expected_loan_pda, loan_bump) = Pubkey::find_program_address(
        &[
            LOAN_SEED,
            pool_account.key.as_ref(),
            borrower.key.as_ref(),
            &loan_id_bytes,
        ],
        program_id,
    );
    if expected_loan_pda != *loan_order_account.key {
        return Err(ClockLendError::InvalidSeeds.into());
    }

    // Security check: verify collateral escrow PDA
    let (expected_escrow_pda, escrow_bump) =
        Pubkey::find_program_address(&[ESCROW_SEED, loan_order_account.key.as_ref()], program_id);
    if expected_escrow_pda != *collateral_escrow_account.key {
        return Err(ClockLendError::InvalidEscrowAccount.into());
    }

    // Create loan order PDA (safe against front-run lamport injection)
    create_or_allocate_pda(
        program_id,
        borrower,
        loan_order_account,
        system_program,
        LoanOrder::LEN,
        &[
            LOAN_SEED,
            pool_account.key.as_ref(),
            borrower.key.as_ref(),
            &loan_id_bytes,
            &[loan_bump],
        ],
    )?;

    // F-12: Reject re-borrow on active, defaulted, or repaid loan orders
    if !loan_order_account.data_is_empty() {
        if let Ok(existing) = LoanOrder::unpack_from_slice(&loan_order_account.try_borrow_data()?) {
            if existing.is_active {
                return Err(ClockLendError::LoanAlreadyActive.into());
            }
            if existing.status == LoanStatus::Defaulted {
                return Err(ClockLendError::LoanInDefault.into());
            }
            if existing.status == LoanStatus::Repaid {
                return Err(ClockLendError::LoanAlreadyRepaid.into());
            }
        }
    }

    if is_native_sol {
        // Native SOL: transfer lamports directly from borrower to collateral escrow PDA
        invoke(
            &system_instruction::transfer(
                borrower.key,
                collateral_escrow_account.key,
                collateral_amount,
            ),
            &[
                borrower.clone(),
                collateral_escrow_account.clone(),
                system_program.clone(),
            ],
        )?;
        // Assign escrow to program_id so ClockLend owns the escrow PDA
        if collateral_escrow_account.owner == &solana_program::system_program::id() {
            invoke_signed(
                &system_instruction::assign(collateral_escrow_account.key, program_id),
                &[collateral_escrow_account.clone(), system_program.clone()],
                &[&[ESCROW_SEED, loan_order_account.key.as_ref(), &[escrow_bump]]],
            )?;
        }
    } else {
        // SPL Token: verify mint matches escrow and transfer tokens to collateral escrow PDA
        assert_token_program(token_program)?;

        // F-02: Create/initialize collateral escrow token PDA if uninitialized
        if collateral_escrow_account.owner == &solana_program::system_program::id() {
            create_or_allocate_token_pda(
                borrower,
                collateral_escrow_account,
                collateral_mint,
                collateral_escrow_account,
                system_program,
                token_program,
                None,
                &[ESCROW_SEED, loan_order_account.key.as_ref(), &[escrow_bump]],
            )?;
        }

        let escrow_token_acc = spl_token::state::Account::unpack(&collateral_escrow_account.try_borrow_data()?)?;
        if escrow_token_acc.mint != *collateral_mint.key {
            return Err(ClockLendError::InvalidMint.into());
        }
        invoke(
            &spl_token::instruction::transfer(
                token_program.key,
                borrower_collateral_account.key,
                collateral_escrow_account.key,
                borrower.key,
                &[],
                collateral_amount,
            )?,
            &[
                borrower_collateral_account.clone(),
                collateral_escrow_account.clone(),
                borrower.clone(),
                token_program.clone(),
            ],
        )?;
    }

    // Feature 8: Monetization - Origination fee (0.50% for SKR, 0.25% for SOL)
    let origination_fee_bps: u64 = if is_native_sol { 25 } else { 50 };
    let origination_fee = ((borrow_amount as u128 * origination_fee_bps as u128) / 10000) as u64;
    let net_disbursement = borrow_amount.saturating_sub(origination_fee);

    // Disburse liquidity from pool vault PDA to borrower
    let (expected_vault_pda, vault_bump) =
        Pubkey::find_program_address(&[VAULT_SEED, pool_account.key.as_ref()], program_id);
    if expected_vault_pda != *vault_account.key {
        return Err(ClockLendError::InvalidSeeds.into());
    }

    assert_token_program(token_program)?;

    // F-04: Mandatory protocol fees - Treasury account cannot be omitted if fee > 0
    if origination_fee > 0 {
        let treasury_account = treasury_account_opt.ok_or(ClockLendError::InvalidTreasuryAccount)?;
        let treasury_token_acc = spl_token::state::Account::unpack(&treasury_account.try_borrow_data()?)?;
        if treasury_token_acc.owner != expected_treasury_pda || treasury_token_acc.mint != pool.liquidity_mint {
            return Err(ClockLendError::InvalidTreasuryAccount.into());
        }

        if treasury_account.key != borrower.key
            && treasury_account.key != borrower_liquidity_account.key
        {
            // Disburse net loan amount to borrower
            invoke_signed(
                &spl_token::instruction::transfer(
                    token_program.key,
                    vault_account.key,
                    borrower_liquidity_account.key,
                    vault_account.key,
                    &[],
                    net_disbursement,
                )?,
                &[
                    vault_account.clone(),
                    borrower_liquidity_account.clone(),
                    token_program.clone(),
                ],
                &[&[VAULT_SEED, pool_account.key.as_ref(), &[vault_bump]]],
            )?;

            // Route upfront origination fee directly to ClockLend Treasury
            invoke_signed(
                &spl_token::instruction::transfer(
                    token_program.key,
                    vault_account.key,
                    treasury_account.key,
                    vault_account.key,
                    &[],
                    origination_fee,
                )?,
                &[
                    vault_account.clone(),
                    treasury_account.clone(),
                    token_program.clone(),
                ],
                &[&[VAULT_SEED, pool_account.key.as_ref(), &[vault_bump]]],
            )?;

            msg!(
                "ClockLend: Origination fee {} ({} bps) routed to Treasury. Disbursed: {}",
                origination_fee,
                origination_fee_bps,
                net_disbursement
            );
        } else {
            return Err(ClockLendError::InvalidTreasuryAccount.into());
        }
    } else {
        invoke_signed(
            &spl_token::instruction::transfer(
                token_program.key,
                vault_account.key,
                borrower_liquidity_account.key,
                vault_account.key,
                &[],
                borrow_amount,
            )?,
            &[
                vault_account.clone(),
                borrower_liquidity_account.clone(),
                token_program.clone(),
            ],
            &[&[VAULT_SEED, pool_account.key.as_ref(), &[vault_bump]]],
        )?;
    }

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;
    let due_time = now.checked_add(duration_seconds).ok_or(ClockLendError::AmountOverflow)?;

    let mut effective_interest_rate_bps = pool.interest_rate_bps;
    let mut bond_to_lock: u64 = 0;
    if let Some(profile_acc) = user_profile_opt {
        if profile_acc.owner == program_id && (profile_acc.data_len() == UserProfile::LEN || profile_acc.data_len() == 51) {
            if profile_acc.data_len() < UserProfile::LEN {
                let rent = Rent::get()?;
                let required_lamports = rent.minimum_balance(UserProfile::LEN);
                if profile_acc.lamports() < required_lamports {
                    let diff = required_lamports.saturating_sub(profile_acc.lamports());
                    invoke(
                        &system_instruction::transfer(borrower.key, profile_acc.key, diff),
                        &[borrower.clone(), profile_acc.clone(), system_program.clone()],
                    )?;
                }
                #[allow(deprecated)]
                profile_acc.realloc(UserProfile::LEN, false)?;
            }
            let maybe_profile: Option<UserProfile> = {
                let data = profile_acc.try_borrow_data()?;
                UserProfile::unpack_from_slice(&data).ok()
            };
            if let Some(mut profile) = maybe_profile {
                if profile.user == *borrower.key {
                    let available_skr = profile.staked_skr.saturating_sub(profile.locked_skr);
                    if available_skr >= 1_000_000_000 {
                        // Tier 2: >= 1,000 SKR gives 50% discount and locks 1,000 SKR bond
                        let discount = effective_interest_rate_bps / 2;
                        effective_interest_rate_bps = effective_interest_rate_bps.saturating_sub(discount);
                        bond_to_lock = 1_000_000_000;
                    } else if available_skr >= 100_000_000 {
                        // Tier 1: >= 100 SKR gives 25% discount and locks 100 SKR bond
                        let discount = (effective_interest_rate_bps as u32 * 2500 / 10000) as u16;
                        effective_interest_rate_bps = effective_interest_rate_bps.saturating_sub(discount);
                        bond_to_lock = 100_000_000;
                    }
                    if bond_to_lock > 0 {
                        profile.locked_skr = profile.locked_skr.saturating_add(bond_to_lock);
                        profile.pack_into_slice(&mut profile_acc.try_borrow_mut_data()?)?;
                    }
                }
            }
        }
    }

    // Calculate interest: (borrow_amount * effective_interest_rate_bps * duration) / (10000 * 31536000)
    let interest_due_u128 = (borrow_amount as u128)
        .checked_mul(effective_interest_rate_bps as u128)
        .ok_or(ClockLendError::AmountOverflow)?
        .checked_mul(duration_seconds as u128)
        .ok_or(ClockLendError::AmountOverflow)?
        / (10000u128 * 31536000u128);
    let interest_due: u64 = interest_due_u128
        .try_into()
        .map_err(|_| ClockLendError::AmountOverflow)?;

    let loan_order = LoanOrder {
        is_active: true,
        loan_id,
        borrower: *borrower.key,
        pool: *pool_account.key,
        principal_amount: borrow_amount,
        collateral_mint: *collateral_mint.key,
        collateral_amount,
        interest_due,
        origination_time: now,
        due_time,
        grace_period_expires: 0,
        status: LoanStatus::Active,
        locked_skr: bond_to_lock,
    };

    loan_order.pack_into_slice(&mut loan_order_account.try_borrow_mut_data()?)?;

    pool.total_liquidity = pool.total_liquidity.saturating_sub(borrow_amount);
    pool.total_borrowed = pool.total_borrowed.saturating_add(borrow_amount);
    pool.loans_originated = pool.loans_originated.saturating_add(1);
    pool.pack_into_slice(&mut pool_account.try_borrow_mut_data()?)?;

    msg!(
        "ClockLend: Loan #{} created! Disbursed: {} | Due: {}",
        loan_id,
        borrow_amount,
        due_time
    );
    Ok(())
}

pub fn process_create_p2p_offer(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    offer_id: u64,
    requested_amount: u64,
    collateral_amount: u64,
    interest_offered: u64,
    duration_seconds: i64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let creator = next_account_info(account_info_iter)?;
    let p2p_offer_account = next_account_info(account_info_iter)?;
    let creator_collateral_account = next_account_info(account_info_iter)?;
    let collateral_escrow_account = next_account_info(account_info_iter)?;
    let collateral_mint = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;

    assert_signer(creator)?;
    assert_system_program(system_program)?;

    if requested_amount == 0 || collateral_amount == 0 || duration_seconds <= 0 || duration_seconds > 365 * 86400 {
        return Err(ClockLendError::InvalidInstruction.into());
    }

    let offer_id_bytes = offer_id.to_le_bytes();
    let (expected_offer_pda, offer_bump) = Pubkey::find_program_address(
        &[P2P_SEED, creator.key.as_ref(), &offer_id_bytes],
        program_id,
    );
    if expected_offer_pda != *p2p_offer_account.key {
        return Err(ClockLendError::InvalidSeeds.into());
    }

    // Security check: verify escrow PDA
    let (expected_escrow_pda, escrow_bump) =
        Pubkey::find_program_address(&[ESCROW_SEED, p2p_offer_account.key.as_ref()], program_id);
    if expected_escrow_pda != *collateral_escrow_account.key {
        return Err(ClockLendError::InvalidEscrowAccount.into());
    }

    // Escrow collateral: Native SOL or SPL token (SKR)
    let is_native_sol = *collateral_mint.key == Pubkey::default()
        || collateral_mint.key == &solana_program::system_program::ID
        || collateral_mint.key == &spl_token::native_mint::id();
    let is_skr = *collateral_mint.key == SKR_MINT;

    // F-03: Collateral Allowlist - Collateral must be Native SOL or canonical SKR
    if !is_native_sol && !is_skr {
        return Err(ClockLendError::InvalidMint.into());
    }

    // Informational 3: P2P LTV Sanity Check - Max 150% LTV of collateral value (prevents uncollateralized loan spam)
    let oracle_feed_opt = next_account_info(account_info_iter).ok();
    let (collateral_price_micro_usd, collateral_decimals): (u64, u8) = if let Some(oracle_acc) = oracle_feed_opt {
        if oracle_acc.owner == program_id && !oracle_acc.data_is_empty() {
            let feed = PriceFeed::unpack_from_slice(&oracle_acc.try_borrow_data()?)?;
            if feed.is_initialized && feed.price_micro_usd > 0 {
                (feed.price_micro_usd, feed.decimals)
            } else if is_native_sol {
                (150_000_000, 9)
            } else {
                (20_000, 6)
            }
        } else if is_native_sol {
            (150_000_000, 9)
        } else {
            (20_000, 6)
        }
    } else if is_native_sol {
        (150_000_000, 9) // Baseline $150.00 / SOL (9 decimals)
    } else {
        (20_000, 6)      // Baseline $0.02 / SKR (6 decimals)
    };

    let collateral_value_micro_usd = (collateral_amount as u128)
        .checked_mul(collateral_price_micro_usd as u128)
        .ok_or(ClockLendError::AmountOverflow)?
        / 10u128.pow(collateral_decimals as u32);

    let max_requested_amount = collateral_value_micro_usd
        .checked_mul(15000)
        .ok_or(ClockLendError::AmountOverflow)?
        / 10000;

    if (requested_amount as u128) > max_requested_amount {
        return Err(ClockLendError::InvalidCollateralRatio.into());
    }

    // Create P2P offer PDA (safe against front-run lamport injection)
    create_or_allocate_pda(
        program_id,
        creator,
        p2p_offer_account,
        system_program,
        P2POffer::LEN,
        &[P2P_SEED, creator.key.as_ref(), &offer_id_bytes, &[offer_bump]],
    )?;

    if !p2p_offer_account.data_is_empty() {
        if let Ok(existing) = P2POffer::unpack_from_slice(&p2p_offer_account.try_borrow_data()?) {
            if existing.is_initialized && existing.status == OfferStatus::Open {
                return Err(ClockLendError::InvalidInstruction.into());
            }
        }
    }

    if is_native_sol {
        invoke(
            &system_instruction::transfer(
                creator.key,
                collateral_escrow_account.key,
                collateral_amount,
            ),
            &[
                creator.clone(),
                collateral_escrow_account.clone(),
                system_program.clone(),
            ],
        )?;
        if collateral_escrow_account.owner == &solana_program::system_program::id() {
            invoke_signed(
                &system_instruction::assign(collateral_escrow_account.key, program_id),
                &[collateral_escrow_account.clone(), system_program.clone()],
                &[&[ESCROW_SEED, p2p_offer_account.key.as_ref(), &[escrow_bump]]],
            )?;
        }
    } else {
        assert_token_program(token_program)?;

        // F-02: Create/initialize collateral escrow token PDA if uninitialized
        if collateral_escrow_account.owner == &solana_program::system_program::id() {
            create_or_allocate_token_pda(
                creator,
                collateral_escrow_account,
                collateral_mint,
                collateral_escrow_account,
                system_program,
                token_program,
                None,
                &[ESCROW_SEED, p2p_offer_account.key.as_ref(), &[escrow_bump]],
            )?;
        }

        let escrow_token_acc = spl_token::state::Account::unpack(&collateral_escrow_account.try_borrow_data()?)?;
        if escrow_token_acc.mint != *collateral_mint.key {
            return Err(ClockLendError::InvalidMint.into());
        }
        invoke(
            &spl_token::instruction::transfer(
                token_program.key,
                creator_collateral_account.key,
                collateral_escrow_account.key,
                creator.key,
                &[],
                collateral_amount,
            )?,
            &[
                creator_collateral_account.clone(),
                collateral_escrow_account.clone(),
                creator.clone(),
                token_program.clone(),
            ],
        )?;
    }

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    let offer = P2POffer {
        is_initialized: true,
        offer_id,
        creator: *creator.key,
        funder: Pubkey::default(),
        collateral_mint: *collateral_mint.key,
        collateral_amount,
        requested_amount,
        interest_offered,
        duration_seconds,
        created_at: now,
        due_time: 0,
        grace_period_expires: 0,
        status: OfferStatus::Open,
    };

    offer.pack_into_slice(&mut p2p_offer_account.try_borrow_mut_data()?)?;
    msg!("ClockLend: P2P Pawn Offer #{} listed on Circle Deck!", offer_id);
    Ok(())
}

pub fn process_fund_p2p_offer(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let funder = next_account_info(account_info_iter)?;
    let p2p_offer_account = next_account_info(account_info_iter)?;
    let funder_liquidity_account = next_account_info(account_info_iter)?;
    let creator_liquidity_account = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;

    assert_signer(funder)?;
    assert_owned_by(p2p_offer_account, program_id)?;
    assert_token_program(token_program)?;

    let mut offer = P2POffer::unpack_from_slice(&p2p_offer_account.try_borrow_data()?)?;
    if !offer.is_initialized || offer.status != OfferStatus::Open {
        return Err(ClockLendError::OfferNotOpen.into());
    }

    // Security check: Funder cannot be the creator
    if *funder.key == offer.creator {
        return Err(ClockLendError::Unauthorized.into());
    }

    // Security check: Verify creator_liquidity_account is owned by offer.creator
    let creator_token_acc = spl_token::state::Account::unpack(&creator_liquidity_account.try_borrow_data()?)?;
    if creator_token_acc.owner != offer.creator {
        return Err(ClockLendError::Unauthorized.into());
    }

    // Transfer requested liquidity directly from funder to creator
    invoke(
        &spl_token::instruction::transfer(
            token_program.key,
            funder_liquidity_account.key,
            creator_liquidity_account.key,
            funder.key,
            &[],
            offer.requested_amount,
        )?,
        &[
            funder_liquidity_account.clone(),
            creator_liquidity_account.clone(),
            funder.clone(),
            token_program.clone(),
        ],
    )?;

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    offer.funder = *funder.key;
    offer.due_time = now.checked_add(offer.duration_seconds).ok_or(ClockLendError::AmountOverflow)?;
    offer.status = OfferStatus::Funded;
    offer.pack_into_slice(&mut p2p_offer_account.try_borrow_mut_data()?)?;

    msg!(
        "ClockLend: P2P Pawn Offer #{} successfully funded by peer {}!",
        offer.offer_id,
        funder.key
    );
    Ok(())
}

pub fn process_repay_loan(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    repay_amount: u64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let borrower = next_account_info(account_info_iter)?;
    let loan_account = next_account_info(account_info_iter)?;
    let borrower_liquidity_account = next_account_info(account_info_iter)?;
    let repayment_destination_account = next_account_info(account_info_iter)?;
    let collateral_escrow_account = next_account_info(account_info_iter)?;
    let borrower_collateral_account = next_account_info(account_info_iter)?;

    assert_signer(borrower)?;
    assert_owned_by(loan_account, program_id)?;

    // Dynamically scan remaining accounts for flexible invocation order
    let mut pool_account_opt: Option<&AccountInfo> = None;
    let mut user_profile_opt: Option<&AccountInfo> = None;
    let mut treasury_account_opt: Option<&AccountInfo> = None;
    let mut token_program_opt: Option<&AccountInfo> = None;
    let mut system_program_opt: Option<&AccountInfo> = None;

    while let Ok(acc) = next_account_info(account_info_iter) {
        if *acc.key == spl_token::id() {
            token_program_opt = Some(acc);
        } else if *acc.key == solana_program::system_program::id() {
            system_program_opt = Some(acc);
        } else if acc.owner == program_id && (acc.data_len() == LendingPool::LEN || acc.data_len() == 182) {
            pool_account_opt = Some(acc);
        } else if acc.owner == program_id && (acc.data_len() == UserProfile::LEN || acc.data_len() == 51) {
            user_profile_opt = Some(acc);
        } else if pool_account_opt.is_none() && acc.owner == program_id {
            pool_account_opt = Some(acc);
        } else {
            treasury_account_opt = Some(acc);
        }
    }

    // Check if this is a LoanOrder (Pool loan)
    let maybe_loan: Option<LoanOrder> = {
        let data = loan_account.try_borrow_data()?;
        LoanOrder::unpack_from_slice(&data).ok()
    };
    if let Some(mut loan) = maybe_loan {
        // Security check: only the borrower can repay
        if *borrower.key != loan.borrower {
            return Err(ClockLendError::Unauthorized.into());
        }

        if !loan.is_active || loan.status == LoanStatus::Repaid {
            return Err(ClockLendError::LoanAlreadyRepaid.into());
        }

        let total_due = loan
            .principal_amount
            .checked_add(loan.interest_due)
            .ok_or(ClockLendError::AmountOverflow)?;

        // L-2 check: must match total_due exactly to prevent liquidity inflation
        if repay_amount != total_due {
            return Err(ClockLendError::ExpectedAmountMismatch.into());
        }

        // Security check: Verify pool and destination account
        let pool_account = pool_account_opt.ok_or(ClockLendError::InvalidInstruction)?;
        assert_owned_by(pool_account, program_id)?;
        if *pool_account.key != loan.pool {
            return Err(ClockLendError::InvalidInstruction.into());
        }
        let mut pool = LendingPool::unpack_from_slice(&pool_account.try_borrow_data()?)?;
        if *repayment_destination_account.key != pool.vault_pda {
            return Err(ClockLendError::InvalidRepaymentDestination.into());
        }

        // Security check: Verify Escrow PDA
        let (expected_escrow_pda, escrow_bump) = Pubkey::find_program_address(
            &[ESCROW_SEED, loan_account.key.as_ref()],
            program_id,
        );
        if expected_escrow_pda != *collateral_escrow_account.key {
            return Err(ClockLendError::InvalidEscrowAccount.into());
        }

        // Checks-Effects-Interactions: Update state BEFORE transfers
        loan.is_active = false;
        loan.status = LoanStatus::Repaid;
        let locked_to_release = loan.locked_skr;
        loan.locked_skr = 0;
        loan.pack_into_slice(&mut loan_account.try_borrow_mut_data()?)?;

        // Feature 7: 15% Interest Take-Rate to ClockLend Treasury
        let protocol_fee = ((loan.interest_due as u128 * 1500) / 10000) as u64; // 15% interest take-rate
        let lender_interest = loan.interest_due.saturating_sub(protocol_fee);
        let lender_repay = loan.principal_amount.saturating_add(lender_interest);

        // F-10: Update pool liquidity with amount actually received by the vault (lender_repay)
        pool.total_liquidity = pool.total_liquidity.saturating_add(lender_repay);
        pool.total_borrowed = pool.total_borrowed.saturating_sub(loan.principal_amount);
        pool.loans_repaid = pool.loans_repaid.saturating_add(1);
        pool.pack_into_slice(&mut pool_account.try_borrow_mut_data()?)?;

        let token_program = token_program_opt.ok_or(ClockLendError::InvalidInstruction)?;
        assert_token_program(token_program)?;

        let (expected_treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], program_id);

        // F-04: Mandatory protocol fee - Treasury account required when fee > 0
        if protocol_fee > 0 {
            let treasury_account = treasury_account_opt.ok_or(ClockLendError::InvalidTreasuryAccount)?;
            let treasury_token_acc = spl_token::state::Account::unpack(&treasury_account.try_borrow_data()?)?;
            if treasury_token_acc.owner != expected_treasury_pda || treasury_token_acc.mint != pool.liquidity_mint {
                return Err(ClockLendError::InvalidTreasuryAccount.into());
            }

            if treasury_account.key != repayment_destination_account.key
                && treasury_account.key != borrower.key
            {
                // Transfer lender portion (principal + 85% interest) to Pool Vault
                invoke(
                    &spl_token::instruction::transfer(
                        token_program.key,
                        borrower_liquidity_account.key,
                        repayment_destination_account.key,
                        borrower.key,
                        &[],
                        lender_repay,
                    )?,
                    &[
                        borrower_liquidity_account.clone(),
                        repayment_destination_account.clone(),
                        borrower.clone(),
                        token_program.clone(),
                    ],
                )?;

                // Transfer 15% interest take-rate directly to ClockLend Treasury
                invoke(
                    &spl_token::instruction::transfer(
                        token_program.key,
                        borrower_liquidity_account.key,
                        treasury_account.key,
                        borrower.key,
                        &[],
                        protocol_fee,
                    )?,
                    &[
                        borrower_liquidity_account.clone(),
                        treasury_account.clone(),
                        borrower.clone(),
                        token_program.clone(),
                    ],
                )?;

                msg!(
                    "ClockLend: Repaid {} to Vault | 15% Take-Rate ({}) routed to Treasury",
                    lender_repay,
                    protocol_fee
                );
            } else {
                return Err(ClockLendError::InvalidTreasuryAccount.into());
            }
        } else {
            invoke(
                &spl_token::instruction::transfer(
                    token_program.key,
                    borrower_liquidity_account.key,
                    repayment_destination_account.key,
                    borrower.key,
                    &[],
                    repay_amount,
                )?,
                &[
                    borrower_liquidity_account.clone(),
                    repayment_destination_account.clone(),
                    borrower.clone(),
                    token_program.clone(),
                ],
            )?;
        }

        // Dual Collateral Return: Native SOL vs SPL Token (SKR)
        let is_native_sol = loan.collateral_mint == Pubkey::default()
            || loan.collateral_mint == solana_program::system_program::ID
            || loan.collateral_mint == spl_token::native_mint::id();

        if is_native_sol {
            transfer_native_sol_from_escrow(
                collateral_escrow_account,
                borrower_collateral_account,
                system_program_opt,
                loan.collateral_amount,
                &[ESCROW_SEED, loan_account.key.as_ref(), &[escrow_bump]],
            )?;
            msg!(
                "ClockLend: Released {} lamports native SOL collateral to borrower",
                loan.collateral_amount
            );
        } else {
            // Security check: Verify borrower collateral token account is owned by borrower
            let borrower_token_acc = spl_token::state::Account::unpack(&borrower_collateral_account.try_borrow_data()?)?;
            if borrower_token_acc.owner != loan.borrower {
                return Err(ClockLendError::Unauthorized.into());
            }

            invoke_signed(
                &spl_token::instruction::transfer(
                    token_program.key,
                    collateral_escrow_account.key,
                    borrower_collateral_account.key,
                    collateral_escrow_account.key,
                    &[],
                    loan.collateral_amount,
                )?,
                &[
                    collateral_escrow_account.clone(),
                    borrower_collateral_account.clone(),
                    token_program.clone(),
                ],
                &[&[ESCROW_SEED, loan_account.key.as_ref(), &[escrow_bump]]],
            )?;
            msg!(
                "ClockLend: Released {} SKR tokens to borrower",
                loan.collateral_amount
            );
        }

        // Boost user credit profile if passed and release locked SKR bond
        if let Some(profile_account) = user_profile_opt {
            if profile_account.owner == program_id && !profile_account.data_is_empty() {
                let maybe_profile: Option<UserProfile> = {
                    let data = profile_account.try_borrow_data()?;
                    UserProfile::unpack_from_slice(&data).ok()
                };
                if let Some(mut profile) = maybe_profile {
                    if profile.user == *borrower.key {
                        profile.total_loans_completed = profile.total_loans_completed.saturating_add(1);
                        profile.reputation_score = profile.reputation_score.saturating_add(50).min(10000);
                        if locked_to_release > 0 {
                            profile.locked_skr = profile.locked_skr.saturating_sub(locked_to_release);
                        }
                        profile.pack_into_slice(&mut profile_account.try_borrow_mut_data()?)?;
                    }
                }
            }
        }

        // F-09: Retain rent-exempt balance and loan history (status = Repaid) instead of zeroing account
        msg!("ClockLend: LoanOrder repaid successfully! Collateral returned.");
        return Ok(());
    }

    // Otherwise check if this is a P2POffer (Circle Deck loan)
    let maybe_offer: Option<P2POffer> = {
        let data = loan_account.try_borrow_data()?;
        P2POffer::unpack_from_slice(&data).ok()
    };
    if let Some(mut offer) = maybe_offer {
        // Security check: only the creator/borrower can repay
        if *borrower.key != offer.creator {
            return Err(ClockLendError::Unauthorized.into());
        }

        if !offer.is_initialized || offer.status != OfferStatus::Funded {
            return Err(ClockLendError::InvalidInstruction.into());
        }

        let total_due = offer
            .requested_amount
            .checked_add(offer.interest_offered)
            .ok_or(ClockLendError::AmountOverflow)?;

        if repay_amount != total_due {
            return Err(ClockLendError::ExpectedAmountMismatch.into());
        }

        // Security check: Verify Escrow PDA
        let (expected_escrow_pda, escrow_bump) = Pubkey::find_program_address(
            &[ESCROW_SEED, loan_account.key.as_ref()],
            program_id,
        );
        if expected_escrow_pda != *collateral_escrow_account.key {
            return Err(ClockLendError::InvalidEscrowAccount.into());
        }

        let token_program = token_program_opt.ok_or(ClockLendError::InvalidInstruction)?;
        assert_token_program(token_program)?;

        // Security check: Verify repayment destination is an SPL token account owned by offer.funder
        let funder_token_acc = spl_token::state::Account::unpack(&repayment_destination_account.try_borrow_data()?)?;
        if funder_token_acc.owner != offer.funder {
            return Err(ClockLendError::InvalidRepaymentDestination.into());
        }

        // Checks-Effects-Interactions: Update state BEFORE transfers
        offer.status = OfferStatus::Repaid;
        offer.pack_into_slice(&mut loan_account.try_borrow_mut_data()?)?;

        // Pay back funder directly
        invoke(
            &spl_token::instruction::transfer(
                token_program.key,
                borrower_liquidity_account.key,
                repayment_destination_account.key,
                borrower.key,
                &[],
                repay_amount,
            )?,
            &[
                borrower_liquidity_account.clone(),
                repayment_destination_account.clone(),
                borrower.clone(),
                token_program.clone(),
            ],
        )?;

        // Return collateral: Native SOL vs SPL Token (SKR)
        let is_native_sol = offer.collateral_mint == Pubkey::default()
            || offer.collateral_mint == solana_program::system_program::ID
            || offer.collateral_mint == spl_token::native_mint::id();

        if is_native_sol {
            transfer_native_sol_from_escrow(
                collateral_escrow_account,
                borrower_collateral_account,
                system_program_opt,
                offer.collateral_amount,
                &[ESCROW_SEED, loan_account.key.as_ref(), &[escrow_bump]],
            )?;
            msg!(
                "ClockLend: P2P released {} lamports native SOL collateral to creator",
                offer.collateral_amount
            );
        } else {
            // Security check: Verify borrower collateral token account is owned by offer.creator
            let creator_token_acc = spl_token::state::Account::unpack(&borrower_collateral_account.try_borrow_data()?)?;
            if creator_token_acc.owner != offer.creator {
                return Err(ClockLendError::Unauthorized.into());
            }

            invoke_signed(
                &spl_token::instruction::transfer(
                    token_program.key,
                    collateral_escrow_account.key,
                    borrower_collateral_account.key,
                    collateral_escrow_account.key,
                    &[],
                    offer.collateral_amount,
                )?,
                &[
                    collateral_escrow_account.clone(),
                    borrower_collateral_account.clone(),
                    token_program.clone(),
                ],
                &[&[ESCROW_SEED, loan_account.key.as_ref(), &[escrow_bump]]],
            )?;
            msg!(
                "ClockLend: P2P released {} SKR tokens to creator",
                offer.collateral_amount
            );
        }

        // F-09: Retain rent-exempt balance and offer history (status = Repaid) instead of zeroing account

        msg!("ClockLend: P2P Offer #{} repaid! Collateral returned to creator.", offer.offer_id);
        return Ok(());
    }

    Err(ClockLendError::InvalidInstruction.into())
}

pub fn process_trigger_grace_period(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let caller = next_account_info(account_info_iter)?;
    let loan_account = next_account_info(account_info_iter)?;

    let pool_account_opt = next_account_info(account_info_iter).ok();

    assert_signer(caller)?;
    assert_owned_by(loan_account, program_id)?;

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    // Check LoanOrder
    let maybe_loan: Option<LoanOrder> = {
        let data = loan_account.try_borrow_data()?;
        LoanOrder::unpack_from_slice(&data).ok()
    };
    if let Some(mut loan) = maybe_loan {
        let is_authorized = if *caller.key == loan.borrower {
            true
        } else if let Some(pool_acc) = pool_account_opt {
            if pool_acc.owner == program_id && *pool_acc.key == loan.pool {
                if let Ok(pool) = LendingPool::unpack_from_slice(&pool_acc.try_borrow_data()?) {
                    pool.authority == *caller.key
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        if !is_authorized {
            return Err(ClockLendError::UnauthorizedCaller.into());
        }

        if !loan.is_active || loan.status != LoanStatus::Active {
            return Err(ClockLendError::InvalidInstruction.into());
        }
        if now < loan.due_time {
            return Err(ClockLendError::LoanNotDue.into());
        }

        loan.status = LoanStatus::InGracePeriod;
        loan.grace_period_expires = now.checked_add(86400).ok_or(ClockLendError::AmountOverflow)?;
        loan.pack_into_slice(&mut loan_account.try_borrow_mut_data()?)?;

        msg!(
            "ClockLend: 24h Social Grace Period triggered for Loan #{}! Expires at {}",
            loan.loan_id,
            loan.grace_period_expires
        );
        return Ok(());
    }

    // Check P2POffer
    let maybe_offer: Option<P2POffer> = {
        let data = loan_account.try_borrow_data()?;
        P2POffer::unpack_from_slice(&data).ok()
    };
    if let Some(mut offer) = maybe_offer {
        if !offer.is_initialized || offer.status != OfferStatus::Funded {
            return Err(ClockLendError::InvalidInstruction.into());
        }

        // Security check: Only creator or funder can trigger grace period
        if *caller.key != offer.creator && *caller.key != offer.funder {
            return Err(ClockLendError::UnauthorizedCaller.into());
        }

        if now < offer.due_time {
            return Err(ClockLendError::LoanNotDue.into());
        }

        offer.status = OfferStatus::InGracePeriod;
        offer.grace_period_expires = now.checked_add(86400).ok_or(ClockLendError::AmountOverflow)?;
        offer.pack_into_slice(&mut loan_account.try_borrow_mut_data()?)?;

        msg!(
            "ClockLend: 24h Social Grace Period triggered for P2P Offer #{}!",
            offer.offer_id
        );
        return Ok(());
    }

    Err(ClockLendError::InvalidInstruction.into())
}

pub fn process_claim_default(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let caller = next_account_info(account_info_iter)?;
    let loan_account = next_account_info(account_info_iter)?;
    let collateral_escrow_account = next_account_info(account_info_iter)?;
    let destination_collateral_account = next_account_info(account_info_iter)?;

    assert_signer(caller)?;
    assert_owned_by(loan_account, program_id)?;

    // Dynamically collect optional accounts
    let mut pool_account_opt: Option<&AccountInfo> = None;
    let mut user_profile_opt: Option<&AccountInfo> = None;
    let mut treasury_collateral_opt: Option<&AccountInfo> = None;
    let mut skr_escrow_opt: Option<&AccountInfo> = None;
    let mut skr_slash_dest_opt: Option<&AccountInfo> = None;
    let mut token_program_opt: Option<&AccountInfo> = None;
    let mut system_program_opt: Option<&AccountInfo> = None;
    let mut spl_token_accounts: Vec<&AccountInfo> = Vec::new();

    let (expected_treasury_pda, _) = Pubkey::find_program_address(&[TREASURY_SEED], program_id);

    while let Ok(acc) = next_account_info(account_info_iter) {
        if *acc.key == spl_token::id() {
            token_program_opt = Some(acc);
        } else if *acc.key == solana_program::system_program::id() {
            system_program_opt = Some(acc);
        } else if acc.owner == program_id && (acc.data_len() == LendingPool::LEN || acc.data_len() == 182) {
            pool_account_opt = Some(acc);
        } else if acc.owner == program_id && (acc.data_len() == UserProfile::LEN || acc.data_len() == 51) {
            user_profile_opt = Some(acc);
        } else if *acc.key == expected_treasury_pda {
            treasury_collateral_opt = Some(acc);
        } else if acc.owner == &spl_token::id() {
            spl_token_accounts.push(acc);
        } else {
            treasury_collateral_opt = Some(acc);
        }
    }

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    // Check LoanOrder default
    let maybe_loan: Option<LoanOrder> = {
        let data = loan_account.try_borrow_data()?;
        LoanOrder::unpack_from_slice(&data).ok()
    };
    if let Some(mut loan) = maybe_loan {
        // C-1 Security Check: Caller must be the pool authority
        let pool_account = pool_account_opt.ok_or(ClockLendError::PoolInactive)?;
        if *pool_account.key != loan.pool {
            return Err(ClockLendError::InvalidVaultAccount.into());
        }
        let pool = LendingPool::unpack_from_slice(&pool_account.try_borrow_data()?)?;
        if *caller.key != pool.authority {
            return Err(ClockLendError::UnauthorizedCaller.into());
        }

        let (expected_borrower_skr_escrow, skr_bump) =
            Pubkey::find_program_address(&[b"skr_escrow", loan.borrower.as_ref()], program_id);

        for acc in spl_token_accounts {
            if *acc.key == expected_borrower_skr_escrow {
                skr_escrow_opt = Some(acc);
            } else if let Ok(tok) = spl_token::state::Account::unpack(&acc.try_borrow_data()?) {
                if tok.mint == SKR_MINT
                    && (tok.owner == pool.authority || tok.owner == pool.vault_pda || tok.owner == expected_treasury_pda)
                {
                    skr_slash_dest_opt = Some(acc);
                } else if tok.owner == expected_treasury_pda {
                    treasury_collateral_opt = Some(acc);
                }
            }
        }

        let slash_destination_account: Option<&AccountInfo> = if let Some(dest) = skr_slash_dest_opt {
            Some(dest)
        } else if destination_collateral_account.owner == &spl_token::id() {
            if let Ok(tok) = spl_token::state::Account::unpack(&destination_collateral_account.try_borrow_data()?) {
                if tok.mint == SKR_MINT
                    && (tok.owner == pool.authority || tok.owner == pool.vault_pda || tok.owner == expected_treasury_pda)
                {
                    Some(destination_collateral_account)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        if loan.status != LoanStatus::InGracePeriod {
            return Err(ClockLendError::GracePeriodActive.into());
        }
        if now < loan.grace_period_expires {
            return Err(ClockLendError::GracePeriodActive.into());
        }

        // Liquidate collateral to destination
        let (expected_escrow_pda, escrow_bump) = Pubkey::find_program_address(
            &[ESCROW_SEED, loan_account.key.as_ref()],
            program_id,
        );
        if expected_escrow_pda != *collateral_escrow_account.key {
            return Err(ClockLendError::InvalidEscrowAccount.into());
        }

        let is_native_sol = loan.collateral_mint == Pubkey::default()
            || loan.collateral_mint == solana_program::system_program::ID
            || loan.collateral_mint == spl_token::native_mint::id();

        // C-1 Security Check: Destination collateral account must belong to pool authority or pool vault
        if is_native_sol {
            if *destination_collateral_account.key != pool.vault_pda && *destination_collateral_account.key != pool.authority {
                return Err(ClockLendError::Unauthorized.into());
            }
        } else {
            let dest_token_acc = spl_token::state::Account::unpack(&destination_collateral_account.try_borrow_data()?)?;
            if dest_token_acc.owner != pool.authority && dest_token_acc.owner != pool.vault_pda {
                return Err(ClockLendError::Unauthorized.into());
            }
        }

        loan.status = LoanStatus::Defaulted;
        loan.is_active = false;
        let loan_locked_skr = loan.locked_skr;
        loan.locked_skr = 0;
        loan.pack_into_slice(&mut loan_account.try_borrow_mut_data()?)?;

        // Feature 8: Monetization - Protocol Liquidation Margin (5% excess collateral to Treasury)
        let protocol_margin = ((loan.collateral_amount as u128 * 500) / 10000) as u64; // 5% liquidation margin
        let lender_collateral = loan.collateral_amount.saturating_sub(protocol_margin);

        // F-04: Mandatory protocol fee - Treasury account required when margin > 0
        if protocol_margin > 0 {
            let treasury_account = treasury_collateral_opt.ok_or(ClockLendError::InvalidTreasuryAccount)?;
            if is_native_sol {
                if *treasury_account.key != expected_treasury_pda {
                    return Err(ClockLendError::InvalidTreasuryAccount.into());
                }
            } else {
                let treasury_token = spl_token::state::Account::unpack(&treasury_account.try_borrow_data()?)?;
                if treasury_token.owner != expected_treasury_pda {
                    return Err(ClockLendError::InvalidTreasuryAccount.into());
                }
            }
        }

        if is_native_sol {
            if let Some(treasury_account) = treasury_collateral_opt {
                let margin_amt = protocol_margin.min(loan.collateral_amount);
                let rem_amt = loan.collateral_amount.saturating_sub(margin_amt);
                transfer_native_sol_from_escrow(
                    collateral_escrow_account,
                    destination_collateral_account,
                    system_program_opt,
                    rem_amt,
                    &[ESCROW_SEED, loan_account.key.as_ref(), &[escrow_bump]],
                )?;
                transfer_native_sol_from_escrow(
                    collateral_escrow_account,
                    treasury_account,
                    system_program_opt,
                    margin_amt,
                    &[ESCROW_SEED, loan_account.key.as_ref(), &[escrow_bump]],
                )?;
                msg!(
                    "ClockLend: Liquidated {} SOL to lender, {} SOL margin to Treasury",
                    rem_amt,
                    margin_amt
                );
            } else {
                transfer_native_sol_from_escrow(
                    collateral_escrow_account,
                    destination_collateral_account,
                    system_program_opt,
                    loan.collateral_amount,
                    &[ESCROW_SEED, loan_account.key.as_ref(), &[escrow_bump]],
                )?;
                msg!("ClockLend: Liquidated {} SOL to lender", loan.collateral_amount);
            }
        } else if let Some(token_program) = token_program_opt {
            if let Some(treasury_account) = treasury_collateral_opt {
                if protocol_margin > 0 && treasury_account.key != destination_collateral_account.key {
                    // Transfer lender collateral
                    invoke_signed(
                        &spl_token::instruction::transfer(
                            token_program.key,
                            collateral_escrow_account.key,
                            destination_collateral_account.key,
                            collateral_escrow_account.key,
                            &[],
                            lender_collateral,
                        )?,
                        &[
                            collateral_escrow_account.clone(),
                            destination_collateral_account.clone(),
                            token_program.clone(),
                        ],
                        &[&[ESCROW_SEED, loan_account.key.as_ref(), &[escrow_bump]]],
                    )?;

                    // Transfer 5% liquidation margin to ClockLend Treasury
                    invoke_signed(
                        &spl_token::instruction::transfer(
                            token_program.key,
                            collateral_escrow_account.key,
                            treasury_account.key,
                            collateral_escrow_account.key,
                            &[],
                            protocol_margin,
                        )?,
                        &[
                            collateral_escrow_account.clone(),
                            treasury_account.clone(),
                            token_program.clone(),
                        ],
                        &[&[ESCROW_SEED, loan_account.key.as_ref(), &[escrow_bump]]],
                    )?;

                    msg!(
                        "ClockLend: Default liquidation! {} SKR to lender, {} SKR (5%) to Treasury",
                        lender_collateral,
                        protocol_margin
                    );
                } else {
                    invoke_signed(
                        &spl_token::instruction::transfer(
                            token_program.key,
                            collateral_escrow_account.key,
                            destination_collateral_account.key,
                            collateral_escrow_account.key,
                            &[],
                            loan.collateral_amount,
                        )?,
                        &[
                            collateral_escrow_account.clone(),
                            destination_collateral_account.clone(),
                            token_program.clone(),
                        ],
                        &[&[ESCROW_SEED, loan_account.key.as_ref(), &[escrow_bump]]],
                    )?;
                }
            } else {
                invoke_signed(
                    &spl_token::instruction::transfer(
                        token_program.key,
                        collateral_escrow_account.key,
                        destination_collateral_account.key,
                        collateral_escrow_account.key,
                        &[],
                        loan.collateral_amount,
                    )?,
                    &[
                        collateral_escrow_account.clone(),
                        destination_collateral_account.clone(),
                        token_program.clone(),
                    ],
                    &[&[ESCROW_SEED, loan_account.key.as_ref(), &[escrow_bump]]],
                )?;
            }
        }

        // F-06: Penalize borrower credit profile and transfer slashed SKR
        if let Some(profile_account) = user_profile_opt {
            if profile_account.owner == program_id && !profile_account.data_is_empty() {
                let maybe_profile: Option<UserProfile> = {
                    let data = profile_account.try_borrow_data()?;
                    UserProfile::unpack_from_slice(&data).ok()
                };
                if let Some(mut profile) = maybe_profile {
                    if profile.user == loan.borrower {
                        profile.total_loans_defaulted = profile.total_loans_defaulted.saturating_add(1);
                        profile.reputation_score = profile.reputation_score.saturating_sub(1000); // severe penalty

                        // F-06: Slash locked SKR bond (or 20% of staked SKR, whichever is greater)
                        let base_slash = profile.staked_skr.saturating_mul(20) / 100;
                        let slash_amount = loan_locked_skr.max(base_slash).min(profile.staked_skr);
                        if slash_amount > 0 {
                            if let (Some(skr_escrow), Some(token_program)) = (skr_escrow_opt, token_program_opt) {
                                if *skr_escrow.key == expected_borrower_skr_escrow {
                                    let slash_dest = slash_destination_account
                                        .ok_or(ClockLendError::InvalidInstruction)?;
                                    let dest_tok = spl_token::state::Account::unpack(&slash_dest.try_borrow_data()?)?;
                                    if dest_tok.mint != SKR_MINT {
                                        return Err(ClockLendError::UnsupportedCollateralMint.into());
                                    }
                                    if dest_tok.owner != pool.authority
                                        && dest_tok.owner != pool.vault_pda
                                        && dest_tok.owner != expected_treasury_pda
                                    {
                                        return Err(ClockLendError::Unauthorized.into());
                                    }

                                    invoke_signed(
                                        &spl_token::instruction::transfer(
                                            token_program.key,
                                            skr_escrow.key,
                                            slash_dest.key,
                                            skr_escrow.key,
                                            &[],
                                            slash_amount,
                                        )?,
                                        &[
                                            skr_escrow.clone(),
                                            slash_dest.clone(),
                                            token_program.clone(),
                                        ],
                                        &[&[b"skr_escrow", loan.borrower.as_ref(), &[skr_bump]]],
                                    )?;

                                    // Only debit profile.staked_skr AFTER transfer completes successfully
                                    profile.staked_skr = profile.staked_skr.saturating_sub(slash_amount);
                                    if loan_locked_skr > 0 {
                                        profile.locked_skr = profile.locked_skr.saturating_sub(loan_locked_skr);
                                    }
                                    msg!("ClockLend: Slashed & transferred {} SKR to lender ({})", slash_amount, slash_dest.key);
                                }
                            }
                        } else if loan_locked_skr > 0 {
                            profile.locked_skr = profile.locked_skr.saturating_sub(loan_locked_skr);
                        }
                        profile.pack_into_slice(&mut profile_account.try_borrow_mut_data()?)?;
                    }
                }
            }
        }

        msg!("ClockLend: Loan #{} defaulted! Collateral liquidated.", loan.loan_id);
        return Ok(());
    }

    // Check P2POffer default
    let maybe_offer: Option<P2POffer> = {
        let data = loan_account.try_borrow_data()?;
        P2POffer::unpack_from_slice(&data).ok()
    };
    if let Some(mut offer) = maybe_offer {
        if !offer.is_initialized {
            return Err(ClockLendError::InvalidInstruction.into());
        }

        // Security check: Only the funder can claim the default
        if *caller.key != offer.funder {
            return Err(ClockLendError::UnauthorizedCaller.into());
        }

        if offer.status != OfferStatus::InGracePeriod {
            return Err(ClockLendError::GracePeriodActive.into());
        }
        if now < offer.grace_period_expires {
            return Err(ClockLendError::GracePeriodActive.into());
        }

        // Verify escrow PDA
        let (expected_escrow_pda, escrow_bump) = Pubkey::find_program_address(
            &[ESCROW_SEED, loan_account.key.as_ref()],
            program_id,
        );
        if expected_escrow_pda != *collateral_escrow_account.key {
            return Err(ClockLendError::InvalidEscrowAccount.into());
        }

        offer.status = OfferStatus::Defaulted;
        offer.pack_into_slice(&mut loan_account.try_borrow_mut_data()?)?;

        let is_native_sol = offer.collateral_mint == Pubkey::default()
            || offer.collateral_mint == solana_program::system_program::ID
            || offer.collateral_mint == spl_token::native_mint::id();

        if is_native_sol {
            // Security check: Verify destination is funder
            if *destination_collateral_account.key != offer.funder {
                return Err(ClockLendError::Unauthorized.into());
            }
            transfer_native_sol_from_escrow(
                collateral_escrow_account,
                destination_collateral_account,
                system_program_opt,
                offer.collateral_amount,
                &[ESCROW_SEED, loan_account.key.as_ref(), &[escrow_bump]],
            )?;
        } else {
            let token_program = token_program_opt.ok_or(ClockLendError::InvalidInstruction)?;
            assert_token_program(token_program)?;

            // Security check: Verify destination token account is owned by funder
            let funder_token_acc = spl_token::state::Account::unpack(&destination_collateral_account.try_borrow_data()?)?;
            if funder_token_acc.owner != offer.funder {
                return Err(ClockLendError::Unauthorized.into());
            }

            // Transfer collateral to funder
            invoke_signed(
                &spl_token::instruction::transfer(
                    token_program.key,
                    collateral_escrow_account.key,
                    destination_collateral_account.key,
                    collateral_escrow_account.key,
                    &[],
                    offer.collateral_amount,
                )?,
                &[
                    collateral_escrow_account.clone(),
                    destination_collateral_account.clone(),
                    token_program.clone(),
                ],
                &[&[ESCROW_SEED, loan_account.key.as_ref(), &[escrow_bump]]],
            )?;
        }

        msg!("ClockLend: P2P Offer #{} defaulted! Collateral claimed by funder.", offer.offer_id);
        return Ok(());
    }

    Err(ClockLendError::InvalidInstruction.into())
}

pub fn process_withdraw_liquidity(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let authority = next_account_info(account_info_iter)?;
    let pool_account = next_account_info(account_info_iter)?;
    let vault_account = next_account_info(account_info_iter)?;
    let authority_token_account = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;

    assert_signer(authority)?;
    assert_owned_by(pool_account, program_id)?;
    assert_token_program(token_program)?;

    if amount == 0 {
        return Err(ClockLendError::InvalidInstruction.into());
    }

    let mut pool = LendingPool::unpack_from_slice(&pool_account.try_borrow_data()?)?;
    if !pool.is_initialized {
        return Err(ClockLendError::PoolInactive.into());
    }

    if *authority.key != pool.authority {
        return Err(ClockLendError::Unauthorized.into());
    }

    if *vault_account.key != pool.vault_pda {
        return Err(ClockLendError::InvalidVaultAccount.into());
    }

    if pool.total_liquidity < amount {
        return Err(ClockLendError::InsufficientLiquidity.into());
    }

    // Verify vault seeds
    let (expected_vault_pda, vault_bump) =
        Pubkey::find_program_address(&[VAULT_SEED, pool_account.key.as_ref()], program_id);
    if expected_vault_pda != *vault_account.key {
        return Err(ClockLendError::InvalidSeeds.into());
    }

    // Update pool state before transfer (Checks-Effects-Interactions)
    pool.total_liquidity = pool.total_liquidity.saturating_sub(amount);
    pool.pack_into_slice(&mut pool_account.try_borrow_mut_data()?)?;

    // Transfer liquidity from vault to authority token account
    invoke_signed(
        &spl_token::instruction::transfer(
            token_program.key,
            vault_account.key,
            authority_token_account.key,
            vault_account.key,
            &[],
            amount,
        )?,
        &[
            vault_account.clone(),
            authority_token_account.clone(),
            token_program.clone(),
        ],
        &[&[VAULT_SEED, pool_account.key.as_ref(), &[vault_bump]]],
    )?;

    msg!("ClockLend: Authority withdrew {} liquidity from pool", amount);
    Ok(())
}

pub fn process_cancel_p2p_offer(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let creator = next_account_info(account_info_iter)?;
    let p2p_offer_account = next_account_info(account_info_iter)?;
    let collateral_escrow_account = next_account_info(account_info_iter)?;
    let creator_collateral_account = next_account_info(account_info_iter)?;

    assert_signer(creator)?;
    assert_owned_by(p2p_offer_account, program_id)?;

    let mut token_program_opt: Option<&AccountInfo> = None;
    let mut system_program_opt: Option<&AccountInfo> = None;
    while let Ok(acc) = next_account_info(account_info_iter) {
        if *acc.key == spl_token::id() {
            token_program_opt = Some(acc);
        } else if *acc.key == solana_program::system_program::id() {
            system_program_opt = Some(acc);
        }
    }

    let offer = P2POffer::unpack_from_slice(&p2p_offer_account.try_borrow_data()?)?;
    if offer.creator != *creator.key {
        return Err(ClockLendError::Unauthorized.into());
    }

    if !offer.is_initialized || offer.status != OfferStatus::Open {
        return Err(ClockLendError::OfferNotOpen.into());
    }

    // Security check: verify escrow PDA
    let (expected_escrow_pda, escrow_bump) =
        Pubkey::find_program_address(&[ESCROW_SEED, p2p_offer_account.key.as_ref()], program_id);
    if expected_escrow_pda != *collateral_escrow_account.key {
        return Err(ClockLendError::InvalidEscrowAccount.into());
    }

    let is_native_sol = offer.collateral_mint == Pubkey::default()
        || offer.collateral_mint == solana_program::system_program::ID
        || offer.collateral_mint == spl_token::native_mint::id();

    if is_native_sol {
        let escrow_lamports = collateral_escrow_account.lamports();
        transfer_native_sol_from_escrow(
            collateral_escrow_account,
            creator_collateral_account,
            system_program_opt,
            escrow_lamports,
            &[ESCROW_SEED, p2p_offer_account.key.as_ref(), &[escrow_bump]],
        )?;
        msg!(
            "ClockLend: P2P Offer #{} cancelled, refunded {} lamports to creator",
            offer.offer_id,
            escrow_lamports
        );
    } else {
        let token_program = token_program_opt.ok_or(ClockLendError::InvalidInstruction)?;
        assert_token_program(token_program)?;

        let creator_token = spl_token::state::Account::unpack(&creator_collateral_account.try_borrow_data()?)?;
        if creator_token.owner != *creator.key {
            return Err(ClockLendError::Unauthorized.into());
        }

        invoke_signed(
            &spl_token::instruction::transfer(
                token_program.key,
                collateral_escrow_account.key,
                creator_collateral_account.key,
                collateral_escrow_account.key,
                &[],
                offer.collateral_amount,
            )?,
            &[
                collateral_escrow_account.clone(),
                creator_collateral_account.clone(),
                token_program.clone(),
            ],
            &[&[ESCROW_SEED, p2p_offer_account.key.as_ref(), &[escrow_bump]]],
        )?;

        // Close SPL token escrow account to return rent to creator safely
        invoke_signed(
            &spl_token::instruction::close_account(
                token_program.key,
                collateral_escrow_account.key,
                creator.key,
                collateral_escrow_account.key,
                &[],
            )?,
            &[
                collateral_escrow_account.clone(),
                creator.clone(),
                token_program.clone(),
            ],
            &[&[ESCROW_SEED, p2p_offer_account.key.as_ref(), &[escrow_bump]]],
        )?;

        msg!(
            "ClockLend: P2P Offer #{} cancelled, refunded {} SKR tokens to creator",
            offer.offer_id,
            offer.collateral_amount
        );
    }

    // Close offer PDA and refund rent lamports to creator
    let offer_lamports = p2p_offer_account.lamports();
    if offer_lamports > 0 {
        **creator.try_borrow_mut_lamports()? =
            creator.lamports().saturating_add(offer_lamports);
        **p2p_offer_account.try_borrow_mut_lamports()? = 0;
        p2p_offer_account.try_borrow_mut_data()?.fill(0);
    }

    msg!("ClockLend: P2P Offer #{} account closed & rent refunded to creator", offer.offer_id);
    Ok(())
}
