use borsh::BorshDeserialize;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};
#[allow(deprecated)]
use solana_program::system_instruction;

use crate::{
    error::ClockLendError,
    instruction::ClockLendInstruction,
    state::{
        LendingPool, LoanOrder, LoanStatus, OfferStatus, P2POffer, PoolType, UserProfile,
        ESCROW_SEED, LOAN_SEED, P2P_SEED, POOL_SEED, PROFILE_SEED, VAULT_SEED,
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
        } => process_initialize_pool(
            program_id,
            accounts,
            pool_id,
            pool_type,
            interest_rate_bps,
            max_ltv_bps,
            min_duration,
            max_duration,
            name,
        ),
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
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let authority = next_account_info(account_info_iter)?;
    let pool_account = next_account_info(account_info_iter)?;
    let liquidity_mint = next_account_info(account_info_iter)?;
    let vault_account = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;
    let _rent_sysvar = next_account_info(account_info_iter)?;

    assert_signer(authority)?;

    if max_ltv_bps > 10000 || max_duration <= min_duration || min_duration <= 0 {
        return Err(ClockLendError::InvalidInstruction.into());
    }

    let pool_id_bytes = pool_id.to_le_bytes();
    let (expected_pool_pda, pool_bump) = Pubkey::find_program_address(
        &[POOL_SEED, authority.key.as_ref(), &pool_id_bytes],
        program_id,
    );
    if expected_pool_pda != *pool_account.key {
        return Err(ClockLendError::InvalidSeeds.into());
    }

    let (expected_vault_pda, _vault_bump) =
        Pubkey::find_program_address(&[VAULT_SEED, pool_account.key.as_ref()], program_id);
    if expected_vault_pda != *vault_account.key {
        return Err(ClockLendError::InvalidSeeds.into());
    }

    // Create pool account if not already created
    if pool_account.lamports() == 0 {
        let rent = Rent::get()?;
        let required_lamports = rent.minimum_balance(LendingPool::LEN);
        invoke_signed(
            &system_instruction::create_account(
                authority.key,
                pool_account.key,
                required_lamports,
                LendingPool::LEN as u64,
                program_id,
            ),
            &[authority.clone(), pool_account.clone(), system_program.clone()],
            &[&[POOL_SEED, authority.key.as_ref(), &pool_id_bytes, &[pool_bump]]],
        )?;
    } else {
        assert_owned_by(pool_account, program_id)?;
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
    };

    pool.pack_into_slice(&mut pool_account.try_borrow_mut_data()?)?;
    msg!("ClockLend: Lending Pool #{} initialized successfully (type: {:?})", pool_id, pool_type);
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
    let pool_account_opt = next_account_info(account_info_iter).ok();
    let user_skr_account = next_account_info(account_info_iter)?;
    let skr_escrow_account = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;

    assert_signer(user)?;

    if amount == 0 {
        return Err(ClockLendError::InvalidInstruction.into());
    }

    let (expected_profile_pda, profile_bump) =
        Pubkey::find_program_address(&[PROFILE_SEED, user.key.as_ref()], program_id);
    if expected_profile_pda != *user_profile_account.key {
        return Err(ClockLendError::InvalidSeeds.into());
    }

    // Initialize user profile if first time
    if user_profile_account.lamports() == 0 {
        let rent = Rent::get()?;
        let required_lamports = rent.minimum_balance(UserProfile::LEN);
        invoke_signed(
            &system_instruction::create_account(
                user.key,
                user_profile_account.key,
                required_lamports,
                UserProfile::LEN as u64,
                program_id,
            ),
            &[user.clone(), user_profile_account.clone(), system_program.clone()],
            &[&[PROFILE_SEED, user.key.as_ref(), &[profile_bump]]],
        )?;

        let initial_profile = UserProfile {
            is_initialized: true,
            user: *user.key,
            staked_skr: 0,
            total_loans_completed: 0,
            total_loans_defaulted: 0,
            reputation_score: 10000, // 100% starting reputation
        };
        initial_profile.pack_into_slice(&mut user_profile_account.try_borrow_mut_data()?)?;
    } else {
        assert_owned_by(user_profile_account, program_id)?;
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
    if let Some(pool_account) = pool_account_opt {
        if pool_account.owner == program_id && !pool_account.data_is_empty() {
            if let Ok(mut pool) = LendingPool::unpack_from_slice(&pool_account.try_borrow_data()?) {
                pool.staked_skr_amount = pool
                    .staked_skr_amount
                    .checked_add(amount)
                    .ok_or(ClockLendError::AmountOverflow)?;
                pool.pack_into_slice(&mut pool_account.try_borrow_mut_data()?)?;
            }
        }
    }

    msg!("ClockLend: Staked {} SKR tokens successfully", amount);
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

    if duration_seconds < pool.min_duration || duration_seconds > pool.max_duration {
        return Err(ClockLendError::InvalidInstruction.into());
    }

    // Security check: Collateral ratio check
    // Max borrow allowed = (collateral_amount * max_ltv_bps) / 10000
    let max_borrow_allowed = (collateral_amount as u128)
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
    let (expected_escrow_pda, _escrow_bump) =
        Pubkey::find_program_address(&[ESCROW_SEED, loan_order_account.key.as_ref()], program_id);
    if expected_escrow_pda != *collateral_escrow_account.key {
        return Err(ClockLendError::InvalidEscrowAccount.into());
    }

    // Create loan order PDA if new
    if loan_order_account.lamports() == 0 {
        let rent = Rent::get()?;
        let required_lamports = rent.minimum_balance(LoanOrder::LEN);
        invoke_signed(
            &system_instruction::create_account(
                borrower.key,
                loan_order_account.key,
                required_lamports,
                LoanOrder::LEN as u64,
                program_id,
            ),
            &[borrower.clone(), loan_order_account.clone(), system_program.clone()],
            &[&[
                LOAN_SEED,
                pool_account.key.as_ref(),
                borrower.key.as_ref(),
                &loan_id_bytes,
                &[loan_bump],
            ]],
        )?;
    } else {
        // If loan account already exists, ensure it is owned by program and not currently active
        assert_owned_by(loan_order_account, program_id)?;
        let existing = LoanOrder::unpack_from_slice(&loan_order_account.try_borrow_data()?)?;
        if existing.is_active {
            return Err(ClockLendError::LoanAlreadyActive.into());
        }
    }

    // Dual Collateral Branching: Native SOL vs SPL Token (SKR)
    let is_native_sol = *collateral_mint.key == Pubkey::default()
        || collateral_mint.key == &solana_program::system_program::ID
        || collateral_mint.key == &spl_token::native_mint::id();

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
    } else {
        // SPL Token (SKR): transfer tokens from borrower to collateral escrow PDA
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

    let _user_profile_opt = next_account_info(account_info_iter).ok();
    let treasury_account_opt = next_account_info(account_info_iter).ok();

    if let Some(treasury_account) = treasury_account_opt {
        if origination_fee > 0
            && treasury_account.key != borrower.key
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
    let due_time = now + duration_seconds;

    // Calculate interest: (borrow_amount * interest_rate_bps * duration) / (10000 * 31536000)
    let interest_due = ((borrow_amount as u128)
        * (pool.interest_rate_bps as u128)
        * (duration_seconds as u128)
        / (10000u128 * 31536000u128)) as u64;

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

    if requested_amount == 0 || collateral_amount == 0 || duration_seconds <= 0 {
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
    let (expected_escrow_pda, _escrow_bump) =
        Pubkey::find_program_address(&[ESCROW_SEED, p2p_offer_account.key.as_ref()], program_id);
    if expected_escrow_pda != *collateral_escrow_account.key {
        return Err(ClockLendError::InvalidEscrowAccount.into());
    }

    if p2p_offer_account.lamports() == 0 {
        let rent = Rent::get()?;
        let required_lamports = rent.minimum_balance(P2POffer::LEN);
        invoke_signed(
            &system_instruction::create_account(
                creator.key,
                p2p_offer_account.key,
                required_lamports,
                P2POffer::LEN as u64,
                program_id,
            ),
            &[creator.clone(), p2p_offer_account.clone(), system_program.clone()],
            &[&[P2P_SEED, creator.key.as_ref(), &offer_id_bytes, &[offer_bump]]],
        )?;
    } else {
        assert_owned_by(p2p_offer_account, program_id)?;
    }

    // Escrow collateral: Native SOL or SPL token (SKR)
    let is_native_sol = *collateral_mint.key == Pubkey::default()
        || collateral_mint.key == &solana_program::system_program::ID
        || collateral_mint.key == &spl_token::native_mint::id();

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
    } else {
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

    let mut offer = P2POffer::unpack_from_slice(&p2p_offer_account.try_borrow_data()?)?;
    if offer.status != OfferStatus::Open {
        return Err(ClockLendError::OfferNotOpen.into());
    }

    // Security check: Funder cannot be the creator
    if *funder.key == offer.creator {
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
    offer.due_time = now + offer.duration_seconds;
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

    while let Ok(acc) = next_account_info(account_info_iter) {
        if *acc.key == spl_token::id() {
            token_program_opt = Some(acc);
        } else if acc.owner == program_id && acc.data_len() == LendingPool::LEN {
            pool_account_opt = Some(acc);
        } else if acc.owner == program_id && acc.data_len() == UserProfile::LEN {
            user_profile_opt = Some(acc);
        } else if pool_account_opt.is_none() && acc.owner == program_id {
            pool_account_opt = Some(acc);
        } else {
            treasury_account_opt = Some(acc);
        }
    }

    // Check if this is a LoanOrder (Pool loan)
    if let Ok(mut loan) = LoanOrder::unpack_from_slice(&loan_account.try_borrow_data()?) {
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

        if repay_amount < total_due {
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
        loan.pack_into_slice(&mut loan_account.try_borrow_mut_data()?)?;

        // Update pool stats
        pool.total_liquidity = pool.total_liquidity.saturating_add(repay_amount);
        pool.total_borrowed = pool.total_borrowed.saturating_sub(loan.principal_amount);
        pool.loans_repaid = pool.loans_repaid.saturating_add(1);
        pool.pack_into_slice(&mut pool_account.try_borrow_mut_data()?)?;

        // Feature 7: 15% Interest Take-Rate to ClockLend Treasury
        let protocol_fee = ((loan.interest_due as u128 * 1500) / 10000) as u64; // 15% interest take-rate
        let lender_interest = loan.interest_due.saturating_sub(protocol_fee);
        let lender_repay = loan.principal_amount.saturating_add(lender_interest);

        if let Some(token_program) = token_program_opt {
            if let Some(treasury_account) = treasury_account_opt {
                if protocol_fee > 0
                    && treasury_account.key != repayment_destination_account.key
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
                let escrow_lamports = collateral_escrow_account.lamports();
                let refund_lamports = loan.collateral_amount.min(escrow_lamports);
                **collateral_escrow_account.try_borrow_mut_lamports()? =
                    escrow_lamports.saturating_sub(refund_lamports);
                **borrower_collateral_account.try_borrow_mut_lamports()? =
                    borrower_collateral_account.lamports().saturating_add(refund_lamports);
                msg!(
                    "ClockLend: Released {} lamports native SOL collateral to borrower",
                    refund_lamports
                );
            } else {
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
        }

        // Boost user credit profile if passed
        if let Some(profile_account) = user_profile_opt {
            if profile_account.owner == program_id && !profile_account.data_is_empty() {
                if let Ok(mut profile) = UserProfile::unpack_from_slice(&profile_account.try_borrow_data()?) {
                    if profile.user == *borrower.key {
                        profile.total_loans_completed = profile.total_loans_completed.saturating_add(1);
                        profile.reputation_score = profile.reputation_score.saturating_add(50).min(10000);
                        profile.pack_into_slice(&mut profile_account.try_borrow_mut_data()?)?;
                    }
                }
            }
        }

        // Feature 4: Dedicated LoanOrder PDA rent refund to borrower upon repayment
        let rent_lamports = loan_account.lamports();
        if rent_lamports > 0 {
            **borrower.try_borrow_mut_lamports()? = borrower
                .lamports()
                .checked_add(rent_lamports)
                .ok_or(ClockLendError::AmountOverflow)?;
            **loan_account.try_borrow_mut_lamports()? = 0;
            loan_account.try_borrow_mut_data()?.fill(0);
            msg!("ClockLend: Loan #{} PDA closed & rent refunded to borrower!", loan.loan_id);
        }

        msg!("ClockLend: LoanOrder repaid successfully! Collateral returned.");
        return Ok(());
    }

    // Otherwise check if this is a P2POffer (Circle Deck loan)
    if let Ok(mut offer) = P2POffer::unpack_from_slice(&loan_account.try_borrow_data()?) {
        // Security check: only the creator/borrower can repay
        if *borrower.key != offer.creator {
            return Err(ClockLendError::Unauthorized.into());
        }

        if offer.status != OfferStatus::Funded {
            return Err(ClockLendError::InvalidInstruction.into());
        }

        let total_due = offer
            .requested_amount
            .checked_add(offer.interest_offered)
            .ok_or(ClockLendError::AmountOverflow)?;

        if repay_amount < total_due {
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

        // Checks-Effects-Interactions: Update state BEFORE transfers
        offer.status = OfferStatus::Repaid;
        offer.pack_into_slice(&mut loan_account.try_borrow_mut_data()?)?;

        if let Some(token_program) = token_program_opt {
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
                let escrow_lamports = collateral_escrow_account.lamports();
                let refund_lamports = offer.collateral_amount.min(escrow_lamports);
                **collateral_escrow_account.try_borrow_mut_lamports()? =
                    escrow_lamports.saturating_sub(refund_lamports);
                **borrower_collateral_account.try_borrow_mut_lamports()? =
                    borrower_collateral_account.lamports().saturating_add(refund_lamports);
                msg!(
                    "ClockLend: P2P released {} lamports native SOL collateral to creator",
                    refund_lamports
                );
            } else {
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
        }

        // Refund rent deposit to creator upon repayment
        let rent_lamports = loan_account.lamports();
        if rent_lamports > 0 {
            **borrower.try_borrow_mut_lamports()? = borrower
                .lamports()
                .checked_add(rent_lamports)
                .ok_or(ClockLendError::AmountOverflow)?;
            **loan_account.try_borrow_mut_lamports()? = 0;
            loan_account.try_borrow_mut_data()?.fill(0);
            msg!("ClockLend: P2P Offer #{} PDA closed & rent refunded to creator!", offer.offer_id);
        }

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

    assert_signer(caller)?;
    assert_owned_by(loan_account, program_id)?;

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    // Check LoanOrder
    if let Ok(mut loan) = LoanOrder::unpack_from_slice(&loan_account.try_borrow_data()?) {
        if loan.status != LoanStatus::Active {
            return Err(ClockLendError::InvalidInstruction.into());
        }
        if now < loan.due_time {
            return Err(ClockLendError::LoanNotDue.into());
        }

        loan.status = LoanStatus::InGracePeriod;
        loan.grace_period_expires = now + 86400; // 24-hour social grace period
        loan.pack_into_slice(&mut loan_account.try_borrow_mut_data()?)?;

        msg!(
            "ClockLend: 24h Social Grace Period triggered for Loan #{}! Expires at {}",
            loan.loan_id,
            loan.grace_period_expires
        );
        return Ok(());
    }

    // Check P2POffer
    if let Ok(mut offer) = P2POffer::unpack_from_slice(&loan_account.try_borrow_data()?) {
        if offer.status != OfferStatus::Funded {
            return Err(ClockLendError::InvalidInstruction.into());
        }
        if now < offer.due_time {
            return Err(ClockLendError::LoanNotDue.into());
        }

        offer.status = OfferStatus::InGracePeriod;
        offer.grace_period_expires = now + 86400;
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
    let mut user_profile_opt: Option<&AccountInfo> = None;
    let mut treasury_collateral_opt: Option<&AccountInfo> = None;
    let mut token_program_opt: Option<&AccountInfo> = None;

    while let Ok(acc) = next_account_info(account_info_iter) {
        if *acc.key == spl_token::id() {
            token_program_opt = Some(acc);
        } else if acc.owner == program_id && acc.data_len() == UserProfile::LEN {
            user_profile_opt = Some(acc);
        } else {
            treasury_collateral_opt = Some(acc);
        }
    }

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    // Check LoanOrder default
    if let Ok(mut loan) = LoanOrder::unpack_from_slice(&loan_account.try_borrow_data()?) {
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

        loan.status = LoanStatus::Defaulted;
        loan.is_active = false;
        loan.pack_into_slice(&mut loan_account.try_borrow_mut_data()?)?;

        let is_native_sol = loan.collateral_mint == Pubkey::default()
            || loan.collateral_mint == solana_program::system_program::ID
            || loan.collateral_mint == spl_token::native_mint::id();

        // Feature 8: Monetization - Protocol Liquidation Margin (5% excess collateral to Treasury)
        let protocol_margin = ((loan.collateral_amount as u128 * 500) / 10000) as u64; // 5% liquidation margin
        let lender_collateral = loan.collateral_amount.saturating_sub(protocol_margin);

        if is_native_sol {
            let escrow_lamports = collateral_escrow_account.lamports();
            let total_transfer = loan.collateral_amount.min(escrow_lamports);

            if let Some(treasury_account) = treasury_collateral_opt {
                let margin_amt = protocol_margin.min(total_transfer);
                let rem_amt = total_transfer.saturating_sub(margin_amt);
                **collateral_escrow_account.try_borrow_mut_lamports()? =
                    escrow_lamports.saturating_sub(total_transfer);
                **destination_collateral_account.try_borrow_mut_lamports()? =
                    destination_collateral_account.lamports().saturating_add(rem_amt);
                **treasury_account.try_borrow_mut_lamports()? =
                    treasury_account.lamports().saturating_add(margin_amt);
                msg!(
                    "ClockLend: Liquidated {} SOL to lender, {} SOL margin to Treasury",
                    rem_amt,
                    margin_amt
                );
            } else {
                **collateral_escrow_account.try_borrow_mut_lamports()? =
                    escrow_lamports.saturating_sub(total_transfer);
                **destination_collateral_account.try_borrow_mut_lamports()? =
                    destination_collateral_account.lamports().saturating_add(total_transfer);
                msg!("ClockLend: Liquidated {} SOL to lender", total_transfer);
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

        // Penalize borrower credit profile with deterministic integer arithmetic
        if let Some(profile_account) = user_profile_opt {
            if profile_account.owner == program_id && !profile_account.data_is_empty() {
                if let Ok(mut profile) = UserProfile::unpack_from_slice(&profile_account.try_borrow_data()?) {
                    if profile.user == loan.borrower {
                        profile.total_loans_defaulted = profile.total_loans_defaulted.saturating_add(1);
                        profile.reputation_score = profile.reputation_score.saturating_sub(1000); // severe penalty
                        // Slash 20% of staked SKR (pure integer math, no float)
                        profile.staked_skr = profile.staked_skr.saturating_mul(80) / 100;
                        profile.pack_into_slice(&mut profile_account.try_borrow_mut_data()?)?;
                    }
                }
            }
        }

        msg!("ClockLend: Loan #{} defaulted! Collateral liquidated.", loan.loan_id);
        return Ok(());
    }

    // Check P2POffer default
    if let Ok(mut offer) = P2POffer::unpack_from_slice(&loan_account.try_borrow_data()?) {
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
            let escrow_lamports = collateral_escrow_account.lamports();
            let transfer_amt = offer.collateral_amount.min(escrow_lamports);
            **collateral_escrow_account.try_borrow_mut_lamports()? =
                escrow_lamports.saturating_sub(transfer_amt);
            **destination_collateral_account.try_borrow_mut_lamports()? =
                destination_collateral_account.lamports().saturating_add(transfer_amt);
        } else if let Some(token_program) = token_program_opt {
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
