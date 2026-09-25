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
        AccountKind, AdminConfig, LendingPool, LoanOrder, LoanStatus, OfferStatus, P2POffer, PoolType, PriceFeed, UserProfile,
        SkrYieldVault, UserYieldPosition,
        ADMIN_SEED, ESCROW_SEED, LOAN_SEED, ORACLE_SEED, P2P_SEED, POOL_SEED, PROFILE_SEED, TREASURY_SEED, VAULT_SEED,
        SKR_YIELD_VAULT_SEED, SKR_YIELD_TOKEN_SEED, USER_YIELD_SEED,
        SKR_MINT, USDC_DEVNET_MINT, USDC_MAINNET_MINT,
        DISCRIMINATOR_ADMIN, DISCRIMINATOR_FEED, DISCRIMINATOR_LOAN, DISCRIMINATOR_OFFER, DISCRIMINATOR_POOL, DISCRIMINATOR_PROFILE,
        DISCRIMINATOR_SKR_YIELD, DISCRIMINATOR_USER_YIELD,
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

#[inline(always)]
fn get_account_kind(account: &AccountInfo) -> AccountKind {
    account.try_borrow_data().map(|d| AccountKind::from_slice(&d)).unwrap_or(AccountKind::Unknown)
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
        // Front-run defense: if the token account already exists at the PDA
        // address, its authority MUST be the intended PDA authority —
        // otherwise whoever pre-created it could drain the funds deposited
        // into it, and the program (signing as the PDA) could never withdraw.
        let existing = spl_token::state::Account::unpack(&pda_account.try_borrow_data()?)?;
        if existing.owner != *owner_authority.key {
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
            is_oracle_free,
        } => {
            // F7: is_oracle_free is now an explicit typed field on the
            // instruction. The previous name-prefix convention (and the dead
            // trailing-byte channel before it) silently coupled a security
            // policy switch to a free-text display string.
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
        ClockLendInstruction::WithdrawTreasury { amount } => {
            process_withdraw_treasury(program_id, accounts, amount)
        }
        ClockLendInstruction::InitializeSkrYieldVault => {
            process_initialize_skr_yield_vault(program_id, accounts)
        }
        ClockLendInstruction::DepositSkrYield { amount } => {
            process_deposit_skr_yield(program_id, accounts, amount)
        }
        ClockLendInstruction::WithdrawUnusedYield => process_withdraw_unused_yield(program_id, accounts),
        ClockLendInstruction::ClaimSkrYield => {
            process_claim_skr_yield(program_id, accounts)
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

    // H-5: Enforce strict parameter bounds
    if min_duration <= 0 || min_duration > 365 * 86400 {
        return Err(ClockLendError::InvalidDuration.into());
    }
    if max_duration < min_duration || max_duration > 365 * 86400 {
        return Err(ClockLendError::InvalidDuration.into());
    }
    if max_ltv_bps == 0 || max_ltv_bps > 9500 {
        return Err(ClockLendError::InvalidCollateralRatio.into());
    }
    // Round 11: oracle-free pools price from hardcoded baselines with no
    // update path — cap their LTV so a stale baseline cannot create
    // unrecoverable bad debt (SKR's baseline has been 3.2x off within months).
    if is_oracle_free && max_ltv_bps > 3000 {
        return Err(ClockLendError::InvalidCollateralRatio.into());
    }
    if interest_rate_bps > 10000 {
        return Err(ClockLendError::InvalidInterestRate.into());
    }

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

    // Liquidity pools must be SPL token based: 6-decimal USD-pegged mints
    // (USDC on either cluster) or wrapped SOL. Raw native SOL is rejected.
    // The borrow path values non-native pools at $1.00/6dp, which is only
    // sound for USD pegs — other mints are rejected outright (same
    // allowlist as F5 on the P2P path).
    if *liquidity_mint.key != USDC_DEVNET_MINT
        && *liquidity_mint.key != USDC_MAINNET_MINT
        && *liquidity_mint.key != spl_token::native_mint::id()
    {
        return Err(ClockLendError::UnsupportedCollateralMint.into());
    }

    // F-02: If SPL token/WSOL liquidity mint and vault account is uninitialized, create/initialize vault token PDA
    if vault_account.owner == &solana_program::system_program::id() {
        if let Some(token_program) = token_program_opt {
            assert_token_program(token_program)?;
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

    // Front-run defense: if the vault token account pre-exists, its authority
    // must be the vault PDA — otherwise deposits would be drainable by the
    // account's pre-creator.
    if vault_account.owner == &spl_token::id() {
        let vault_token = spl_token::state::Account::unpack(&vault_account.try_borrow_data()?)?;
        if vault_token.owner != *vault_account.key {
            return Err(ClockLendError::InvalidAccountOwner.into());
        }
    }

    let pool = LendingPool {
        discriminator: DISCRIMINATOR_POOL,
        is_initialized: true,
        pool_id,
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
        has_custom_oracle: false,
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

    // H-7: Restrict deposits to pool authority (since there is no LP share accounting)
    if *depositor.key != pool.authority {
        return Err(ClockLendError::Unauthorized.into());
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
    // Front-run defense: vault authority must be the vault PDA
    if vault_token.owner != *vault_account.key {
        return Err(ClockLendError::InvalidAccountOwner.into());
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
    let (pool_account_opt, user_skr_account) = if next_acc.owner == program_id && (next_acc.data_len() == LendingPool::LEN || get_account_kind(next_acc) == AccountKind::LendingPool) {
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

    // Front-run defense: the escrow token account's authority MUST be the
    // escrow PDA itself — a pre-created account with an attacker-controlled
    // authority would let the attacker drain every token staked into it.
    let escrow_token = spl_token::state::Account::unpack(&skr_escrow_account.try_borrow_data()?)?;
    if escrow_token.owner != *skr_escrow_account.key {
        return Err(ClockLendError::InvalidAccountOwner.into());
    }
    if escrow_token.mint != SKR_MINT {
        return Err(ClockLendError::InvalidMint.into());
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
            discriminator: DISCRIMINATOR_PROFILE,
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
        if pool_account.owner == program_id
            && !pool_account.data_is_empty()
            && (pool_account.data_len() == LendingPool::LEN || get_account_kind(pool_account) == AccountKind::LendingPool)
        {
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

    // Critical fix: sync user yield position immediately upon staking.
    // Scans accounts for any SkrYieldVault and corresponding UserYieldPosition.
    let escrow_post = spl_token::state::Account::unpack(&skr_escrow_account.try_borrow_data()?)?;
    let current_escrow_skr = escrow_post.amount;
    let pre_stake_escrow = current_escrow_skr.saturating_sub(amount);

    for acc in accounts.iter() {
        if acc.owner == program_id
            && !acc.data_is_empty()
            && (acc.data_len() == SkrYieldVault::LEN || get_account_kind(acc) == AccountKind::SkrYieldVault)
        {
            let vault_res = SkrYieldVault::unpack_from_slice(&acc.try_borrow_data()?);
            if let Ok(mut vault) = vault_res {
                if !vault.is_initialized {
                    continue;
                }
                let (expected_vault_pda, _) = Pubkey::find_program_address(
                    &[SKR_YIELD_VAULT_SEED, vault.reward_mint.as_ref()],
                    program_id,
                );
                if expected_vault_pda != *acc.key {
                    continue;
                }

                let (expected_pos, pos_bump) = Pubkey::find_program_address(
                    &[USER_YIELD_SEED, user.key.as_ref(), vault.reward_mint.as_ref()],
                    program_id,
                );

                if let Some(pos_acc) = accounts.iter().find(|a| *a.key == expected_pos) {
                    let is_new_position = pos_acc.owner == &solana_program::system_program::id();
                    if is_new_position {
                        create_or_allocate_pda(
                            program_id,
                            user,
                            pos_acc,
                            system_program,
                            UserYieldPosition::LEN,
                            &[USER_YIELD_SEED, user.key.as_ref(), vault.reward_mint.as_ref(), &[pos_bump]],
                        )?;
                        let initial_pos = UserYieldPosition {
                            discriminator: DISCRIMINATOR_USER_YIELD,
                            is_initialized: true,
                            user: *user.key,
                            reward_mint: vault.reward_mint,
                            staked_skr: 0,
                            reward_debt: 0,
                            accrued_rewards: 0,
                            total_claimed: 0,
                            last_interaction_time: Clock::get()?.unix_timestamp,
                        };
                        initial_pos.pack_into_slice(&mut pos_acc.try_borrow_mut_data()?)?;
                    }

                    if pos_acc.owner == program_id && !pos_acc.data_is_empty() {
                        let pos_res = UserYieldPosition::unpack_from_slice(&pos_acc.try_borrow_data()?);
                        if let Ok(mut position) = pos_res {
                            if position.user == *user.key && position.reward_mint == vault.reward_mint {
                                // Harvest pending rewards on pre-stake escrow
                                let eff_staked = position.staked_skr.min(pre_stake_escrow);
                                let gross = ((eff_staked as u128)
                                    .checked_mul(vault.acc_reward_per_share)
                                    .ok_or(ClockLendError::AmountOverflow)?)
                                    / YIELD_SCALE;
                                let pending = gross.saturating_sub(position.reward_debt.min(gross)) as u64;
                                // Round 11: harvest only if the stake was held for the cooldown
                                // period. A stake cycled in under MIN_STAKE_AGE forfeits its
                                // interim accrual (the debt still rebases, so nothing is paid).
                                let held_ok = Clock::get()?.unix_timestamp
                                    .saturating_sub(position.last_interaction_time) >= MIN_STAKE_AGE_SECS;
                                if held_ok {
                                    position.accrued_rewards = position.accrued_rewards.saturating_add(pending);
                                }

                                // Update position & vault total staked SKR
                                if current_escrow_skr > position.staked_skr {
                                    let diff = current_escrow_skr - position.staked_skr;
                                    vault.total_staked_skr = vault.total_staked_skr.saturating_add(diff);
                                } else if current_escrow_skr < position.staked_skr {
                                    let diff = position.staked_skr - current_escrow_skr;
                                    vault.total_staked_skr = vault.total_staked_skr.saturating_sub(diff);
                                }
                                position.staked_skr = current_escrow_skr;
                                position.reward_debt = ((position.staked_skr as u128)
                                    .checked_mul(vault.acc_reward_per_share)
                                    .ok_or(ClockLendError::AmountOverflow)?)
                                    / YIELD_SCALE;
                                position.last_interaction_time = Clock::get()?.unix_timestamp;

                                position.pack_into_slice(&mut pos_acc.try_borrow_mut_data()?)?;
                                vault.pack_into_slice(&mut acc.try_borrow_mut_data()?)?;
                                msg!("ClockLend: Synced yield shares on stake: total_staked={}, user_staked={}",
                                    vault.total_staked_skr, position.staked_skr);
                            }
                        }
                    }
                }
            }
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
    // High-3 Slot Collision Fix: Only unpack if account is actually a LendingPool (length & kind check).
    if let Some(pool_account) = pool_account_opt {
        if pool_account.owner == program_id
            && !pool_account.data_is_empty()
            && (pool_account.data_len() == LendingPool::LEN || get_account_kind(pool_account) == AccountKind::LendingPool)
        {
            let pool_res = LendingPool::unpack_from_slice(&pool_account.try_borrow_data()?);
            if let Ok(mut pool) = pool_res {
                if pool.authority == *user.key {
                    pool.staked_skr_amount = pool.staked_skr_amount.saturating_sub(amount);
                    pool.pack_into_slice(&mut pool_account.try_borrow_mut_data()?)?;
                }
            }
        }
    }

    // C-1 / High-1 defense in depth: sync the user's SKR yield position down so a
    // recycled or slashed stake cannot keep earning ghost shares.
    // Clamps effective stake to pre-unstake escrow, and clamps new stake to post-unstake escrow.
    // Covers all initialized SkrYieldVault accounts in accounts.
    let escrow_token_post = spl_token::state::Account::unpack(&skr_escrow_account.try_borrow_data()?)?;
    let post_unstake_escrow = escrow_token_post.amount;
    let pre_unstake_escrow = post_unstake_escrow.saturating_add(amount);

    for acc in accounts.iter() {
        if acc.owner == program_id
            && !acc.data_is_empty()
            && (acc.data_len() == SkrYieldVault::LEN || get_account_kind(acc) == AccountKind::SkrYieldVault)
        {
            let vault_res = SkrYieldVault::unpack_from_slice(&acc.try_borrow_data()?);
            if let Ok(mut vault) = vault_res {
                if !vault.is_initialized {
                    continue;
                }
                let (expected_vault_pda, _) = Pubkey::find_program_address(
                    &[SKR_YIELD_VAULT_SEED, vault.reward_mint.as_ref()],
                    program_id,
                );
                if expected_vault_pda != *acc.key {
                    continue;
                }

                let (expected_pos, _) = Pubkey::find_program_address(
                    &[USER_YIELD_SEED, user.key.as_ref(), vault.reward_mint.as_ref()],
                    program_id,
                );
                if let Some(pos_acc) = accounts.iter().find(|a| *a.key == expected_pos) {
                    if pos_acc.owner == program_id && !pos_acc.data_is_empty() {
                        let pos_res = UserYieldPosition::unpack_from_slice(&pos_acc.try_borrow_data()?);
                        if let Ok(mut position) = pos_res {
                            if position.user == *user.key && position.reward_mint == vault.reward_mint {
                                // High-1: clamp pending reward calculation to real pre-unstake escrow balance
                                let eff_staked = position.staked_skr.min(pre_unstake_escrow);
                                let gross = ((eff_staked as u128)
                                    .checked_mul(vault.acc_reward_per_share)
                                    .ok_or(ClockLendError::AmountOverflow)?)
                                    / YIELD_SCALE;
                                let pending = gross.saturating_sub(position.reward_debt.min(gross)) as u64;
                                // Round 11: harvest only if the stake was held for the cooldown
                                // period. A stake cycled in under MIN_STAKE_AGE forfeits its
                                // interim accrual (the debt still rebases, so nothing is paid).
                                let held_ok = Clock::get()?.unix_timestamp
                                    .saturating_sub(position.last_interaction_time) >= MIN_STAKE_AGE_SECS;
                                if held_ok {
                                    position.accrued_rewards = position.accrued_rewards.saturating_add(pending);
                                }

                                // Clamp target staked SKR to post-unstake escrow
                                let target_staked = position.staked_skr.saturating_sub(amount).min(post_unstake_escrow);
                                let shares_removed = position.staked_skr.saturating_sub(target_staked);
                                vault.total_staked_skr = vault.total_staked_skr.saturating_sub(shares_removed);
                                position.staked_skr = target_staked;
                                position.reward_debt = ((position.staked_skr as u128)
                                    .checked_mul(vault.acc_reward_per_share)
                                    .ok_or(ClockLendError::AmountOverflow)?)
                                    / YIELD_SCALE;
                                position.last_interaction_time = Clock::get()?.unix_timestamp;

                                position.pack_into_slice(&mut pos_acc.try_borrow_mut_data()?)?;
                                vault.pack_into_slice(&mut acc.try_borrow_mut_data()?)?;
                                msg!("ClockLend: Synced yield shares down on unstake: total_staked={}, user_staked={}",
                                    vault.total_staked_skr, position.staked_skr);
                            }
                        }
                    }
                }
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

    // C-3 & M-1: The ONLY root is the program's on-chain upgrade authority,
    // proven via the ProgramData account. No hardcoded keys.
    let program_data_info = match next_account_info(account_info_iter) {
        Ok(acc) => acc,
        Err(_) => return Err(ClockLendError::Unauthorized.into()),
    };
    let (expected_pda, _) = Pubkey::find_program_address(
        &[program_id.as_ref()],
        &solana_program::bpf_loader_upgradeable::id(),
    );
    if *program_data_info.key != expected_pda
        || program_data_info.owner != &solana_program::bpf_loader_upgradeable::id()
    {
        return Err(ClockLendError::Unauthorized.into());
    }
    // UpgradeableLoaderState::ProgramData metadata layout (45 bytes):
    // 0..4: Discriminant = 3u32 (ProgramData)
    // 4..12: Slot (u64)
    // 12: Option<Pubkey> tag (1u8 = Some)
    // 13..45: 32-byte upgrade authority Pubkey
    let data = program_data_info.try_borrow_data()?;
    if data.len() < 45 || data[0..4] != 3u32.to_le_bytes() || data[12] != 1 {
        return Err(ClockLendError::Unauthorized.into());
    }
    let upgrade_authority = Pubkey::new_from_array(data[13..45].try_into().unwrap());
    if upgrade_authority != *authority.key {
        return Err(ClockLendError::Unauthorized.into());
    }

    let (expected_admin_pda, bump) = Pubkey::find_program_address(&[ADMIN_SEED], program_id);
    if expected_admin_pda != *admin_account.key {
        return Err(ClockLendError::InvalidSeeds.into());
    }

    if admin_account.owner == program_id && !admin_account.data_is_empty() {
        // Bind the unpack result to a let FIRST: a `try_borrow_data()` temporary
        // inside an `if let` scrutinee lives for the whole statement (Rust
        // temporary lifetime extension), so borrowing the account mutably inside
        // the body would fail with AccountBorrowFailed at runtime.
        let existing = AdminConfig::unpack_from_slice(&admin_account.try_borrow_data()?);
        if let Ok(mut existing) = existing {
            if existing.is_initialized {
                // Rotation: the caller has already proven upgrade-authority
                // credentials above, so any rotation is authorized.
                let new_admin_opt = next_account_info(account_info_iter).ok();
                if let Some(new_admin_acc) = new_admin_opt {
                    existing.admin = *new_admin_acc.key;
                }
                let new_oracle_opt = next_account_info(account_info_iter).ok();
                if let Some(new_oracle_acc) = new_oracle_opt {
                    existing.oracle_authority = *new_oracle_acc.key;
                }
                existing.pack_into_slice(&mut admin_account.try_borrow_mut_data()?)?;

                // P1: Rotate SkrYieldVault authority if any yield vault accounts are passed
                for acc in accounts.iter() {
                    if acc.owner == program_id
                        && !acc.data_is_empty()
                        && (acc.data_len() == SkrYieldVault::LEN || get_account_kind(acc) == AccountKind::SkrYieldVault)
                    {
                        let vault_res = SkrYieldVault::unpack_from_slice(&acc.try_borrow_data()?);
                        if let Ok(mut vault) = vault_res {
                            let (expected_vault_pda, _) = Pubkey::find_program_address(
                                &[SKR_YIELD_VAULT_SEED, vault.reward_mint.as_ref()],
                                program_id,
                            );
                            if expected_vault_pda == *acc.key {
                                vault.authority = existing.admin;
                                vault.pack_into_slice(&mut acc.try_borrow_mut_data()?)?;
                                msg!("ClockLend: SkrYieldVault authority rotated to: {}", vault.authority);
                            }
                        }
                    }
                }

                msg!("ClockLend: AdminConfig rotated. New admin: {}, Oracle: {}", existing.admin, existing.oracle_authority);
                return Ok(());
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
        discriminator: DISCRIMINATOR_ADMIN,
        is_initialized: true,
        admin: *authority.key,
        oracle_authority: *authority.key,
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

    // H-6: Bound price and decimals
    if price_micro_usd == 0 || price_micro_usd > 1_000_000_000_000 {
        return Err(ClockLendError::InvalidInstruction.into());
    }
    if decimals == 0 || decimals > 18 {
        return Err(ClockLendError::InvalidAccountData.into());
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
        } else if acc.owner == program_id && (acc.data_len() == LendingPool::LEN || get_account_kind(acc) == AccountKind::LendingPool) {
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
                // If feed already initialized, signer MUST be existing.authority —
                // or, for GLOBAL feeds only, the AdminConfig admin/oracle_authority.
                // Pool-scoped feeds stay under their pool authority's sole control.
                let is_auth = if existing.authority == *authority.key {
                    true
                } else if !is_pool_oracle {
                    if let Some(admin_acc) = admin_account_opt {
                        if let Ok(admin_config) = AdminConfig::unpack_from_slice(&admin_acc.try_borrow_data()?) {
                            admin_config.is_initialized && (admin_config.admin == *authority.key || admin_config.oracle_authority == *authority.key)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };
                if !is_auth {
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
            let mut pool = LendingPool::unpack_from_slice(&pool_acc.try_borrow_data()?)?;
            if pool.authority != *authority.key {
                return Err(ClockLendError::Unauthorized.into());
            }
            pool.has_custom_oracle = true;
            pool.pack_into_slice(&mut pool_acc.try_borrow_mut_data()?)?;
        } else {
            // Global oracle feed: caller MUST be AdminConfig.admin or AdminConfig.oracle_authority!
            let admin_acc = admin_account_opt.ok_or(ClockLendError::Unauthorized)?;
            if admin_acc.owner != program_id || admin_acc.data_is_empty() {
                return Err(ClockLendError::Unauthorized.into());
            }
            let admin_config = AdminConfig::unpack_from_slice(&admin_acc.try_borrow_data()?)?;
            if !admin_config.is_initialized || (admin_config.admin != *authority.key && admin_config.oracle_authority != *authority.key) {
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
            discriminator: DISCRIMINATOR_FEED,
            is_initialized: true,
            mint: *mint_account.key,
            price_micro_usd: 0,
            decimals: 0,
            last_updated_at: 0,
            authority: *authority.key,
            max_staleness_seconds: 3600,
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
    feed.max_staleness_seconds = 3600;

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

    let (expected_skr_yield_pda, _) =
        Pubkey::find_program_address(&[SKR_YIELD_VAULT_SEED, pool.liquidity_mint.as_ref()], program_id);
    let (expected_skr_yield_token_pda, _) =
        Pubkey::find_program_address(&[SKR_YIELD_TOKEN_SEED, pool.liquidity_mint.as_ref()], program_id);

    let mut user_profile_opt: Option<&AccountInfo> = None;
    let mut treasury_account_opt: Option<&AccountInfo> = None;
    let mut collateral_oracle_opt: Option<&AccountInfo> = None;
    let mut pool_oracle_opt: Option<&AccountInfo> = None;
    let mut skr_yield_vault_opt: Option<&AccountInfo> = None;
    let mut skr_yield_token_opt: Option<&AccountInfo> = None;

    // Scan trailing optional accounts
    while let Ok(acc) = next_account_info(account_info_iter) {
        if *acc.key == expected_profile_pda {
            user_profile_opt = Some(acc);
        } else if *acc.key == expected_treasury_pda {
            treasury_account_opt = Some(acc);
        } else if *acc.key == expected_skr_yield_pda {
            skr_yield_vault_opt = Some(acc);
        } else if *acc.key == expected_skr_yield_token_pda {
            skr_yield_token_opt = Some(acc);
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

    let current_time = Clock::get()?.unix_timestamp;

    // H-3: Enforce pool-scoped oracle precedence if pool has configured a custom oracle.
    // The pool-scoped feed is authoritative when it exists: a borrower must not
    // be able to omit it and get priced from the hardcoded baseline instead of
    // the authority's chosen feed.
    if pool.has_custom_oracle {
        if let Some(col_oracle) = collateral_oracle_opt {
            if *col_oracle.key != expected_pool_collateral_oracle1 && *col_oracle.key != expected_pool_collateral_oracle2 {
                return Err(ClockLendError::InvalidOracleAccount.into());
            }
        } else {
            return Err(ClockLendError::InvalidOracleAccount.into());
        }
    }

    // Resolve dynamic collateral price & decimals (mandatory unless pool.is_oracle_free)
    // Pyth (verified pull oracle) takes precedence; the admin feed remains the
    // fallback for pools that don't pass a Pyth account.
    // Pricing is admin-feed-only (Pyth pull oracles removed in round 11): the
    // collateral price comes from the pool-scoped feed when the pool opted
    // into one, otherwise the global feed, otherwise (oracle-free pools) the
    // hardcoded baselines.
    let (collateral_price_micro_usd, collateral_decimals): (u64, u8) = if let Some(oracle_acc) = collateral_oracle_opt {
        if oracle_acc.owner == program_id && !oracle_acc.data_is_empty() {
            let feed = PriceFeed::unpack_from_slice(&oracle_acc.try_borrow_data()?)?;
            if !feed.is_initialized || feed.price_micro_usd == 0 {
                return Err(ClockLendError::InvalidOracleAccount.into());
            }
            if feed.mint != *collateral_mint.key && feed.mint != canonical_collateral_mint {
                return Err(ClockLendError::InvalidOracleAccount.into());
            }
            // Round 11: pricing reads are bounded to 600s regardless of the
            // feed's stored window (3600s is retained for monitoring).
            if feed.last_updated_at <= 0 || current_time.saturating_sub(feed.last_updated_at) > feed.max_staleness_seconds.min(ADMIN_FEED_MAX_PRICE_AGE_SECS) {
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

    // C-1 guard on the admin-feed path: the feed's declared scale must match
    // the collateral's actual denomination. The Pyth path already pins this,
    // but a misconfigured admin feed (e.g. a SOL feed published with 6
    // decimals) would otherwise re-value every lamport 1000x high.
    let expected_collateral_decimals: u8 = if is_native_sol { 9 } else { 6 };
    if collateral_decimals != expected_collateral_decimals {
        return Err(ClockLendError::InvalidOracleAccount.into());
    }

    // Resolve dynamic pool liquidity price & decimals (mandatory for non-USD unless pool.is_oracle_free)
    // Round 11: when the pool opted into its own feeds, the pool-scoped
    // liquidity feed is REQUIRED (mirror of the collateral-side H-3 gate) —
    // except for native-SOL pools with native-SOL collateral, where the
    // valuation short-circuits and never reads the pool price (and the pool
    // oracle PDA is the same account the scan already assigned to the
    // collateral slot).
    if pool.has_custom_oracle && !(is_pool_native_sol && is_native_sol) {
        match pool_oracle_opt {
            Some(acc) if *acc.key == expected_pool_liq_oracle1 || *acc.key == expected_pool_liq_oracle2 => {}
            _ => return Err(ClockLendError::InvalidOracleAccount.into()),
        }
    }
    let (pool_price_micro_usd, pool_decimals): (u64, u8) = if let Some(oracle_acc) = pool_oracle_opt {
        if oracle_acc.owner == program_id && !oracle_acc.data_is_empty() {
            let feed = PriceFeed::unpack_from_slice(&oracle_acc.try_borrow_data()?)?;
            if !feed.is_initialized || feed.price_micro_usd == 0 {
                return Err(ClockLendError::InvalidOracleAccount.into());
            }
            if feed.mint != pool.liquidity_mint && feed.mint != canonical_pool_mint {
                return Err(ClockLendError::InvalidOracleAccount.into());
            }
            // Round 11: 600s pricing bound (see collateral feed above).
            if feed.last_updated_at <= 0 || current_time.saturating_sub(feed.last_updated_at) > feed.max_staleness_seconds.min(ADMIN_FEED_MAX_PRICE_AGE_SECS) {
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

    // H-6: Validate decimal bounds and use checked exponentiation
    if collateral_decimals == 0 || collateral_decimals > 18 || pool_decimals == 0 || pool_decimals > 18 {
        return Err(ClockLendError::InvalidOracleAccount.into());
    }
    // Round 11: the pool-side scale must match the liquidity mint (the C-1
    // guard mirrored) — a feed declaring the wrong decimals for the pool mint
    // re-values every collateral by 10^delta.
    let expected_pool_decimals: u8 = if is_pool_native_sol { 9 } else { 6 };
    if pool_decimals != expected_pool_decimals {
        return Err(ClockLendError::InvalidOracleAccount.into());
    }
    let col_scale = 10u128.checked_pow(collateral_decimals as u32).ok_or(ClockLendError::AmountOverflow)?;
    let pool_scale = 10u128.checked_pow(pool_decimals as u32).ok_or(ClockLendError::AmountOverflow)?;

    // Dynamic valuation normalized to pool liquidity denomination
    let collateral_value = if is_native_sol && is_pool_native_sol {
        collateral_amount as u128
    } else if !is_pool_native_sol {
        // Pool is USDC (or other 6-decimal USD pegged pool)
        (collateral_amount as u128)
            .checked_mul(collateral_price_micro_usd as u128)
            .ok_or(ClockLendError::AmountOverflow)?
            / col_scale
    } else {
        // Pool is Native SOL, collateral is SKR (or other token)
        let num = (collateral_amount as u128)
            .checked_mul(collateral_price_micro_usd as u128)
            .ok_or(ClockLendError::AmountOverflow)?
            .checked_mul(pool_scale)
            .ok_or(ClockLendError::AmountOverflow)?;
        let den = col_scale
            .checked_mul(pool_price_micro_usd as u128)
            .ok_or(ClockLendError::AmountOverflow)?;
        if den == 0 {
            return Err(ClockLendError::AmountOverflow.into());
        }
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
        // Front-run defense: collateral escrow authority must be the escrow PDA
        if escrow_token_acc.owner != *collateral_escrow_account.key {
            return Err(ClockLendError::InvalidAccountOwner.into());
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

            // Route origination fee: if skr_yield_vault is provided, split 50/50 with SKR holders
            let (treasury_fee, yield_dividend) = if let (Some(yield_vault_acc), Some(yield_token_acc)) = (skr_yield_vault_opt, skr_yield_token_opt) {
                let y_div = origination_fee / 2;
                let t_fee = origination_fee.saturating_sub(y_div);
                // The yield token account must be denominated in the POOL's
                // liquidity mint, otherwise the transfer below would revert
                // every borrow (e.g. a wrapped-SOL pool whose yield vault PDA
                // can never be initialized — the reward-mint allowlist is
                // USDC/SKR only). Fail open to the treasury instead.
                let mint_matches = spl_token::state::Account::unpack(&yield_token_acc.try_borrow_data()?)
                    .ok()
                    .map(|yt| yt.mint == pool.liquidity_mint)
                    .unwrap_or(false);
                if y_div > 0 && mint_matches {
                    invoke_signed(
                        &spl_token::instruction::transfer(
                            token_program.key,
                            vault_account.key,
                            yield_token_acc.key,
                            vault_account.key,
                            &[],
                            y_div,
                        )?,
                        &[
                            vault_account.clone(),
                            yield_token_acc.clone(),
                            token_program.clone(),
                        ],
                        &[&[VAULT_SEED, pool_account.key.as_ref(), &[vault_bump]]],
                    )?;
                    if yield_vault_acc.owner == program_id && !yield_vault_acc.data_is_empty() {
                        let mut vault = SkrYieldVault::unpack_from_slice(&yield_vault_acc.try_borrow_data()?)?;
                        if !vault.is_initialized {
                            return Err(ClockLendError::PoolInactive.into());
                        }
                        accrue_yield(&mut vault, y_div)?;
                        vault.pack_into_slice(&mut yield_vault_acc.try_borrow_mut_data()?)?;
                    }
                }
                (t_fee, y_div)
            } else {
                (origination_fee, 0)
            };

            // Route treasury portion of origination fee directly to ClockLend Treasury
            if treasury_fee > 0 {
                invoke_signed(
                    &spl_token::instruction::transfer(
                        token_program.key,
                        vault_account.key,
                        treasury_account.key,
                        vault_account.key,
                        &[],
                        treasury_fee,
                    )?,
                    &[
                        vault_account.clone(),
                        treasury_account.clone(),
                        token_program.clone(),
                    ],
                    &[&[VAULT_SEED, pool_account.key.as_ref(), &[vault_bump]]],
                )?;
            }

            msg!(
                "ClockLend: Origination fee {} (Treasury: {}, Yield: {}). Disbursed: {}",
                origination_fee,
                treasury_fee,
                yield_dividend,
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
        discriminator: DISCRIMINATOR_LOAN,
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

    // V-A1: bound the offered interest so an offer can never become
    // permanently unrepayable (interest capped at 100% of principal)
    if interest_offered > requested_amount {
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

    // C-2: Forbid PDA reuse outright — cannot re-initialize an existing offer under any status
    if p2p_offer_account.owner == program_id && !p2p_offer_account.data_is_empty() {
        return Err(ClockLendError::OfferAlreadyActive.into());
    }

    // Security check: verify escrow PDA
    let (expected_escrow_pda, escrow_bump) =
        Pubkey::find_program_address(&[ESCROW_SEED, p2p_offer_account.key.as_ref()], program_id);
    if expected_escrow_pda != *collateral_escrow_account.key {
        return Err(ClockLendError::InvalidEscrowAccount.into());
    }

    // Escrow collateral: Native SOL, canonical SKR, JitoSOL, or Sanctum INF
    let is_native_sol = *collateral_mint.key == Pubkey::default()
        || collateral_mint.key == &solana_program::system_program::ID
        || collateral_mint.key == &spl_token::native_mint::id();
    let is_skr = *collateral_mint.key == SKR_MINT;

    // F-03: Collateral Allowlist - Collateral must be Native SOL or canonical SKR
    if !is_native_sol && !is_skr {
        return Err(ClockLendError::InvalidMint.into());
    }

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    // H-4 & H-6: Strict Oracle validation and bounds
    let canonical_mint = if is_native_sol { spl_token::native_mint::id() } else { *collateral_mint.key };
    let (expected_oracle_pda, _) = Pubkey::find_program_address(&[ORACLE_SEED, canonical_mint.as_ref()], program_id);

    // Optional accounts: oracle feed account and/or liquidity mint account
    let mut oracle_feed_opt: Option<&AccountInfo> = None;
    let mut liquidity_mint = USDC_DEVNET_MINT;

    while let Ok(acc) = next_account_info(account_info_iter) {
        if *acc.key == expected_oracle_pda {
            oracle_feed_opt = Some(acc);
        } else if acc.owner == &spl_token::id() && acc.data_len() == spl_token::state::Mint::LEN {
            liquidity_mint = *acc.key;
        } else if *acc.key == USDC_DEVNET_MINT {
            liquidity_mint = *acc.key;
        } else if oracle_feed_opt.is_none() && (acc.owner == program_id || acc.data_len() == PriceFeed::LEN) {
            oracle_feed_opt = Some(acc);
        } else if oracle_feed_opt.is_none() {
            oracle_feed_opt = Some(acc);
        }
    }

    // F5: Allowlist the loan asset. Only 6-decimal USD-pegged mints are
    // supported, so requested_amount (base units) is directly comparable to
    // the micro-USD collateral value computed below (10^6 scale on both
    // sides). Non-USD loan assets would need price normalization and are
    // rejected outright.
    if liquidity_mint != USDC_DEVNET_MINT && liquidity_mint != USDC_MAINNET_MINT {
        return Err(ClockLendError::InvalidMint.into());
    }

    // Price resolution: Pyth (verified pull oracle) takes precedence when
    // present; otherwise the H-3 fail-closed admin feed is required.
    // Admin-feed-only pricing (Pyth removed in round 11).
    let (collateral_price_micro_usd, collateral_decimals): (u64, u8) = {
        // H-3: Fail closed when canonical price feed is missing or unprovisioned on value-authorizing paths
        let oracle_acc = oracle_feed_opt.ok_or(ClockLendError::InvalidOracleAccount)?;
        if *oracle_acc.key != expected_oracle_pda || oracle_acc.owner != program_id || oracle_acc.data_is_empty() {
            return Err(ClockLendError::InvalidOracleAccount.into());
        }
        let feed = PriceFeed::unpack_from_slice(&oracle_acc.try_borrow_data()?)?;
        if !feed.is_initialized || feed.price_micro_usd == 0 {
            return Err(ClockLendError::InvalidOracleAccount.into());
        }
        if feed.mint != canonical_mint && feed.mint != *collateral_mint.key {
            return Err(ClockLendError::InvalidOracleAccount.into());
        }
        if feed.decimals == 0 || feed.decimals > 18 {
            return Err(ClockLendError::InvalidOracleAccount.into());
        }
        // Round 11: 600s pricing bound.
        if now.saturating_sub(feed.last_updated_at) > feed.max_staleness_seconds.min(ADMIN_FEED_MAX_PRICE_AGE_SECS) {
            return Err(ClockLendError::StaleOraclePrice.into());
        }
        (feed.price_micro_usd, feed.decimals)
    };

    // C-1 guard on the admin-feed path (P2P): same scale-vs-mint cross-check
    // as the borrow path — a misconfigured feed must not re-value collateral.
    let expected_collateral_decimals: u8 = if is_native_sol { 9 } else { 6 };
    if collateral_decimals != expected_collateral_decimals {
        return Err(ClockLendError::InvalidOracleAccount.into());
    }

    let col_scale = 10u128.checked_pow(collateral_decimals as u32).ok_or(ClockLendError::AmountOverflow)?;
    let collateral_value_micro_usd = (collateral_amount as u128)
        .checked_mul(collateral_price_micro_usd as u128)
        .ok_or(ClockLendError::AmountOverflow)?
        / col_scale;

    // Cap P2P borrow at max 90% LTV of collateral value
    let max_requested_amount = collateral_value_micro_usd
        .checked_mul(9000)
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
        // Front-run defense: pawn escrow authority must be the escrow PDA
        if escrow_token_acc.owner != *collateral_escrow_account.key {
            return Err(ClockLendError::InvalidAccountOwner.into());
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

    let offer = P2POffer {
        discriminator: DISCRIMINATOR_OFFER,
        is_initialized: true,
        offer_id,
        creator: *creator.key,
        funder: Pubkey::default(),
        collateral_mint: *collateral_mint.key,
        liquidity_mint,
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

    // H-1: Fail-closed type discriminator check
    if AccountKind::from_slice(&p2p_offer_account.try_borrow_data()?) != AccountKind::P2POffer {
        return Err(ClockLendError::InvalidAccountData.into());
    }

    let mut offer = P2POffer::unpack_from_slice(&p2p_offer_account.try_borrow_data()?)?;
    if !offer.is_initialized || offer.status != OfferStatus::Open {
        return Err(ClockLendError::OfferNotOpen.into());
    }

    // H-1: Verify PDA derivation to ensure account was created via CreateP2POffer
    let (expected_offer_pda, _) = Pubkey::find_program_address(
        &[P2P_SEED, offer.creator.as_ref(), &offer.offer_id.to_le_bytes()],
        program_id,
    );
    if expected_offer_pda != *p2p_offer_account.key {
        return Err(ClockLendError::InvalidSeeds.into());
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

    // C-1: Enforce mint equality with offer.liquidity_mint to prevent worthless token exploits
    let funder_token_acc = spl_token::state::Account::unpack(&funder_liquidity_account.try_borrow_data()?)?;
    if funder_token_acc.mint != offer.liquidity_mint || creator_token_acc.mint != offer.liquidity_mint {
        return Err(ClockLendError::InvalidMint.into());
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
        } else if acc.owner == program_id && (acc.data_len() == LendingPool::LEN || get_account_kind(acc) == AccountKind::LendingPool) {
            pool_account_opt = Some(acc);
        } else if acc.owner == program_id && (acc.data_len() == UserProfile::LEN || get_account_kind(acc) == AccountKind::UserProfile) {
            user_profile_opt = Some(acc);
        } else if pool_account_opt.is_none() && acc.owner == program_id {
            pool_account_opt = Some(acc);
        } else {
            treasury_account_opt = Some(acc);
        }
    }

    // C-1: Fail-closed AccountKind dispatch between LoanOrder and P2POffer
    let account_kind = AccountKind::from_slice(&loan_account.try_borrow_data()?);
    match account_kind {
        AccountKind::LoanOrder => {
            let mut loan = LoanOrder::unpack_from_slice(&loan_account.try_borrow_data()?)?;
            // Security check: only the borrower can repay
            if *borrower.key != loan.borrower {
                return Err(ClockLendError::Unauthorized.into());
            }

            // Round 11: the grace window closes the repayment option (mirror
            // of the P2P branch) — after expiry only ClaimDefault may resolve
            // the loan, so a borrower cannot hold a perpetual redemption
            // option that front-runs the liquidator.
            if loan.status == LoanStatus::InGracePeriod {
                let now = Clock::get()?.unix_timestamp;
                if now >= loan.grace_period_expires {
                    return Err(ClockLendError::GracePeriodExpired.into());
                }
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
            if locked_to_release > 0 {
                let profile_account = user_profile_opt.ok_or(ClockLendError::InvalidProfileAccount)?;
                if profile_account.owner != program_id || profile_account.data_is_empty() {
                    return Err(ClockLendError::InvalidProfileAccount.into());
                }
                let mut profile = UserProfile::unpack_from_slice(&profile_account.try_borrow_data()?)?;
                if profile.user != *borrower.key {
                    return Err(ClockLendError::Unauthorized.into());
                }
                profile.total_loans_completed = profile.total_loans_completed.saturating_add(1);
                profile.reputation_score = profile.reputation_score.saturating_add(50).min(10000);
                profile.locked_skr = profile.locked_skr.saturating_sub(locked_to_release);
                profile.pack_into_slice(&mut profile_account.try_borrow_mut_data()?)?;
            } else if let Some(profile_account) = user_profile_opt {
                if profile_account.owner == program_id && !profile_account.data_is_empty() {
                    // Bind the unpack to a let first (see the InitializeAdmin
                    // rotation comment): a scrutinee borrow would still be held
                    // during the mutable borrow below.
                    let profile = UserProfile::unpack_from_slice(&profile_account.try_borrow_data()?);
                    if let Ok(mut profile) = profile {
                        if profile.user == *borrower.key {
                            profile.total_loans_completed = profile.total_loans_completed.saturating_add(1);
                            profile.reputation_score = profile.reputation_score.saturating_add(50).min(10000);
                            profile.pack_into_slice(&mut profile_account.try_borrow_mut_data()?)?;
                        }
                    }
                }
            }

            // F-09: Retain rent-exempt balance and loan history (status = Repaid) instead of zeroing account
            msg!("ClockLend: LoanOrder repaid successfully! Collateral returned.");
            Ok(())
        }
        AccountKind::P2POffer => {
            let mut offer = P2POffer::unpack_from_slice(&loan_account.try_borrow_data()?)?;
            // Security check: only the creator/borrower can repay
            if *borrower.key != offer.creator {
                return Err(ClockLendError::Unauthorized.into());
            }

            // H-1: Accept Funded OR InGracePeriod (borrower can remediate before grace expiration)
            if !offer.is_initialized || (offer.status != OfferStatus::Funded && offer.status != OfferStatus::InGracePeriod) {
                return Err(ClockLendError::InvalidInstruction.into());
            }

            let clock = Clock::get()?;
            let now = clock.unix_timestamp;
            if offer.status == OfferStatus::InGracePeriod && now >= offer.grace_period_expires {
                return Err(ClockLendError::GracePeriodActive.into());
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

            // C-1: Verify borrower and funder token accounts match offer.liquidity_mint
            let borrower_token_acc = spl_token::state::Account::unpack(&borrower_liquidity_account.try_borrow_data()?)?;
            if borrower_token_acc.mint != offer.liquidity_mint || funder_token_acc.mint != offer.liquidity_mint {
                return Err(ClockLendError::InvalidMint.into());
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

            msg!("ClockLend: P2P Offer #{} repaid! Collateral returned to creator.", offer.offer_id);
            Ok(())
        }
        _ => Err(ClockLendError::InvalidAccountData.into()),
    }
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

    // C-1: Fail-closed AccountKind dispatch
    let account_kind = AccountKind::from_slice(&loan_account.try_borrow_data()?);
    match account_kind {
        AccountKind::LoanOrder => {
            let mut loan = LoanOrder::unpack_from_slice(&loan_account.try_borrow_data()?)?;
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
            Ok(())
        }
        AccountKind::P2POffer => {
            let mut offer = P2POffer::unpack_from_slice(&loan_account.try_borrow_data()?)?;
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
            Ok(())
        }
        _ => Err(ClockLendError::InvalidAccountData.into()),
    }
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
        if *acc.key == solana_program::sysvar::clock::id() {
            continue;
        } else if *acc.key == spl_token::id() {
            token_program_opt = Some(acc);
        } else if *acc.key == solana_program::system_program::id() {
            system_program_opt = Some(acc);
        } else if acc.owner == program_id && (acc.data_len() == LendingPool::LEN || get_account_kind(acc) == AccountKind::LendingPool) {
            pool_account_opt = Some(acc);
        } else if acc.owner == program_id && (acc.data_len() == UserProfile::LEN || get_account_kind(acc) == AccountKind::UserProfile) {
            user_profile_opt = Some(acc);
        } else if *acc.key == expected_treasury_pda {
            treasury_collateral_opt = Some(acc);
        } else if acc.owner == &spl_token::id() {
            spl_token_accounts.push(acc);
        } else if treasury_collateral_opt.is_none() {
            treasury_collateral_opt = Some(acc);
        }
    }

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    // C-1: Fail-closed AccountKind dispatch
    let account_kind = AccountKind::from_slice(&loan_account.try_borrow_data()?);
    match account_kind {
        AccountKind::LoanOrder => {
            let mut loan = LoanOrder::unpack_from_slice(&loan_account.try_borrow_data()?)?;
            // C-1 Security Check: Caller must be the pool authority
            let pool_account = pool_account_opt.ok_or(ClockLendError::PoolInactive)?;
            if *pool_account.key != loan.pool {
                return Err(ClockLendError::InvalidVaultAccount.into());
            }
            let mut pool = LendingPool::unpack_from_slice(&pool_account.try_borrow_data()?)?;
            if *caller.key != pool.authority {
                return Err(ClockLendError::UnauthorizedCaller.into());
            }

            let (expected_borrower_skr_escrow, skr_bump) =
                Pubkey::find_program_address(&[b"skr_escrow", loan.borrower.as_ref()], program_id);

            let is_native_sol = loan.collateral_mint == Pubkey::default()
                || loan.collateral_mint == solana_program::system_program::ID
                || loan.collateral_mint == spl_token::native_mint::id();

            // F1: classify optional token accounts by ROLE, not by mint alone, so a
            // treasury-owned account of the collateral mint remains usable as the
            // margin treasury even when the collateral is SKR. (Previously any
            // treasury-owned SKR account was captured as the slash destination,
            // which made SPL-collateral liquidation impossible.)
            let mut treasury_token_opt: Option<&AccountInfo> = None;
            let mut slash_fallback_opt: Option<&AccountInfo> = None;

            for acc in &spl_token_accounts {
                if *acc.key == expected_borrower_skr_escrow {
                    skr_escrow_opt = Some(*acc);
                    continue;
                }
                let Ok(tok) = spl_token::state::Account::unpack(&acc.try_borrow_data()?) else {
                    continue;
                };
                // Margin treasury (SPL collateral only): treasury-owned account of the collateral mint
                if !is_native_sol
                    && tok.mint == loan.collateral_mint
                    && tok.owner == expected_treasury_pda
                    && treasury_token_opt.is_none()
                {
                    treasury_token_opt = Some(*acc);
                }
                // Slash destination: SKR accounts owned by authority/vault/treasury.
                // Prefer authority/vault-owned; a treasury-owned SKR account is only the fallback.
                if tok.mint == SKR_MINT
                    && (tok.owner == pool.authority
                        || tok.owner == pool.vault_pda
                        || tok.owner == expected_treasury_pda)
                {
                    if tok.owner == expected_treasury_pda {
                        if slash_fallback_opt.is_none() {
                            slash_fallback_opt = Some(*acc);
                        }
                    } else if skr_slash_dest_opt.is_none() {
                        skr_slash_dest_opt = Some(*acc);
                    }
                }
            }
            if skr_slash_dest_opt.is_none() {
                skr_slash_dest_opt = slash_fallback_opt;
            }
            // For SPL collateral the margin treasury is the role-classified token
            // account; for native SOL it remains the bare treasury PDA captured by
            // the outer scan.
            if !is_native_sol {
                treasury_collateral_opt = treasury_token_opt;
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

            // C-1 Security Check: Destination collateral account must belong to pool authority or pool vault
            if is_native_sol {
                if *destination_collateral_account.key != pool.authority {
                    // F6: native SOL liquidation MUST go to the pool
                    // authority's wallet — raw lamports sent to the SPL
                    // vault token account would be permanently stranded.
                    return Err(ClockLendError::Unauthorized.into());
                }
            } else {
                let dest_token_acc = spl_token::state::Account::unpack(&destination_collateral_account.try_borrow_data()?)?;
                if dest_token_acc.owner != pool.authority && dest_token_acc.owner != pool.vault_pda {
                    return Err(ClockLendError::Unauthorized.into());
                }
            }

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
                    if treasury_token.owner != expected_treasury_pda || treasury_token.mint != loan.collateral_mint {
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
                    if margin_amt > 0 {
                        transfer_native_sol_from_escrow(
                            collateral_escrow_account,
                            treasury_account,
                            system_program_opt,
                            margin_amt,
                            &[ESCROW_SEED, loan_account.key.as_ref(), &[escrow_bump]],
                        )?;
                    }
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
            } else {
                let token_program = token_program_opt.ok_or(ClockLendError::InvalidInstruction)?;
                assert_token_program(token_program)?;

                if protocol_margin > 0 {
                    let treasury_account = treasury_collateral_opt.ok_or(ClockLendError::InvalidTreasuryAccount)?;
                    if treasury_account.key != destination_collateral_account.key {
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
            let loan_locked_skr = loan.locked_skr;
            if loan_locked_skr > 0 || user_profile_opt.is_some() {
                let profile_account = user_profile_opt.ok_or(ClockLendError::InvalidInstruction)?;
                assert_owned_by(profile_account, program_id)?;
                let mut profile = UserProfile::unpack_from_slice(&profile_account.try_borrow_data()?)?;
                if profile.user != loan.borrower {
                    return Err(ClockLendError::Unauthorized.into());
                }

                profile.total_loans_defaulted = profile.total_loans_defaulted.saturating_add(1);
                profile.reputation_score = profile.reputation_score.saturating_sub(1000); // severe penalty

                // F-06: Slash locked SKR bond (or 20% of staked SKR, whichever is
                // greater) — but never consume stake that backs OTHER live
                // loans, or locked_skr would exceed staked_skr and freeze the
                // remainder of the borrower's stake forever (round-11 PoC).
                let base_slash = profile.staked_skr.saturating_mul(20) / 100;
                let max_slashable = profile
                    .staked_skr
                    .saturating_sub(profile.locked_skr.saturating_sub(loan_locked_skr));
                let slash_amount = loan_locked_skr
                    .max(base_slash.min(max_slashable))
                    .min(profile.staked_skr);
                if slash_amount > 0 {
                    let skr_escrow = skr_escrow_opt.ok_or(ClockLendError::InvalidInstruction)?;
                    let token_program = token_program_opt.ok_or(ClockLendError::InvalidInstruction)?;
                    assert_token_program(token_program)?;
                    if *skr_escrow.key != expected_borrower_skr_escrow {
                        return Err(ClockLendError::InvalidEscrowAccount.into());
                    }
                    let slash_dest = slash_destination_account.ok_or(ClockLendError::InvalidInstruction)?;
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
                    msg!("ClockLend: Slashed & transferred {} SKR to lender ({})", slash_amount, slash_dest.key);

                    // High-2: Sync borrower's yield position down to reflect slashed SKR
                    let post_slash_escrow = spl_token::state::Account::unpack(&skr_escrow.try_borrow_data()?)
                        .map(|t| t.amount)
                        .unwrap_or(0);
                    let pre_slash_escrow = post_slash_escrow.saturating_add(slash_amount);

                    for acc in accounts.iter() {
                        if acc.owner == program_id
                            && !acc.data_is_empty()
                            && (acc.data_len() == SkrYieldVault::LEN || get_account_kind(acc) == AccountKind::SkrYieldVault)
                        {
                            let vault_res = SkrYieldVault::unpack_from_slice(&acc.try_borrow_data()?);
                            if let Ok(mut vault) = vault_res {
                                if !vault.is_initialized {
                                    continue;
                                }
                                let (expected_vault_pda, _) = Pubkey::find_program_address(
                                    &[SKR_YIELD_VAULT_SEED, vault.reward_mint.as_ref()],
                                    program_id,
                                );
                                if expected_vault_pda != *acc.key {
                                    continue;
                                }

                                let (expected_pos, _) = Pubkey::find_program_address(
                                    &[USER_YIELD_SEED, loan.borrower.as_ref(), vault.reward_mint.as_ref()],
                                    program_id,
                                );
                                if let Some(pos_acc) = accounts.iter().find(|a| *a.key == expected_pos) {
                                    if pos_acc.owner == program_id && !pos_acc.data_is_empty() {
                                        let pos_res = UserYieldPosition::unpack_from_slice(&pos_acc.try_borrow_data()?);
                                        if let Ok(mut position) = pos_res {
                                            if position.user == loan.borrower && position.reward_mint == vault.reward_mint {
                                                let eff_staked = position.staked_skr.min(pre_slash_escrow);
                                                let gross = ((eff_staked as u128)
                                                    .checked_mul(vault.acc_reward_per_share)
                                                    .ok_or(ClockLendError::AmountOverflow)?)
                                                    / YIELD_SCALE;
                                                let pending = gross.saturating_sub(position.reward_debt.min(gross)) as u64;
                                                let held_ok = Clock::get()?.unix_timestamp
                                                    .saturating_sub(position.last_interaction_time) >= MIN_STAKE_AGE_SECS;
                                                if held_ok {
                                                    position.accrued_rewards = position.accrued_rewards.saturating_add(pending);
                                                }

                                                let target_staked = position.staked_skr.saturating_sub(slash_amount).min(post_slash_escrow);
                                                let shares_removed = position.staked_skr.saturating_sub(target_staked);
                                                vault.total_staked_skr = vault.total_staked_skr.saturating_sub(shares_removed);
                                                position.staked_skr = target_staked;
                                                position.reward_debt = ((position.staked_skr as u128)
                                                    .checked_mul(vault.acc_reward_per_share)
                                                    .ok_or(ClockLendError::AmountOverflow)?)
                                                    / YIELD_SCALE;
                                                position.last_interaction_time = Clock::get()?.unix_timestamp;

                                                position.pack_into_slice(&mut pos_acc.try_borrow_mut_data()?)?;
                                                vault.pack_into_slice(&mut acc.try_borrow_mut_data()?)?;
                                                msg!("ClockLend: Slashed borrower yield shares: total_staked={}, borrower_staked={}",
                                                    vault.total_staked_skr, position.staked_skr);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if loan_locked_skr > 0 {
                    profile.locked_skr = profile.locked_skr.saturating_sub(loan_locked_skr);
                }
                profile.pack_into_slice(&mut profile_account.try_borrow_mut_data()?)?;
            }

            // H-4: Update pool total_borrowed accounting on default
            pool.total_borrowed = pool.total_borrowed.saturating_sub(loan.principal_amount);
            pool.pack_into_slice(&mut pool_account.try_borrow_mut_data()?)?;

            // Checks-Effects-Interactions: Write terminal status AFTER all transfers succeed
            loan.status = LoanStatus::Defaulted;
            loan.is_active = false;
            loan.locked_skr = 0;
            loan.pack_into_slice(&mut loan_account.try_borrow_mut_data()?)?;

            msg!("ClockLend: Loan #{} defaulted! Collateral liquidated.", loan.loan_id);
            Ok(())
        }
        AccountKind::P2POffer => {
            let mut offer = P2POffer::unpack_from_slice(&loan_account.try_borrow_data()?)?;
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
                if funder_token_acc.mint != offer.collateral_mint {
                    return Err(ClockLendError::InvalidMint.into());
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

            // Checks-Effects-Interactions: Write terminal status AFTER transfers succeed
            offer.status = OfferStatus::Defaulted;
            offer.pack_into_slice(&mut loan_account.try_borrow_mut_data()?)?;

            msg!("ClockLend: P2P Offer #{} defaulted! Collateral claimed by funder.", offer.offer_id);
            Ok(())
        }
        _ => Err(ClockLendError::InvalidAccountData.into()),
    }
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

    if AccountKind::from_slice(&p2p_offer_account.try_borrow_data()?) != AccountKind::P2POffer {
        return Err(ClockLendError::InvalidAccountData.into());
    }

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

    // Verify PDA seeds
    let (expected_offer_pda, _) = Pubkey::find_program_address(
        &[P2P_SEED, offer.creator.as_ref(), &offer.offer_id.to_le_bytes()],
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
        if creator_token.mint != offer.collateral_mint {
            return Err(ClockLendError::InvalidMint.into());
        }

        let escrow_token = spl_token::state::Account::unpack(&collateral_escrow_account.try_borrow_data()?)?;
        let transfer_amount = escrow_token.amount;

        // Drain the full balance so close_account does not fail with NonNativeHasBalance (H-5)
        if transfer_amount > 0 {
            invoke_signed(
                &spl_token::instruction::transfer(
                    token_program.key,
                    collateral_escrow_account.key,
                    creator_collateral_account.key,
                    collateral_escrow_account.key,
                    &[],
                    transfer_amount,
                )?,
                &[
                    collateral_escrow_account.clone(),
                    creator_collateral_account.clone(),
                    token_program.clone(),
                ],
                &[&[ESCROW_SEED, p2p_offer_account.key.as_ref(), &[escrow_bump]]],
            )?;
        }

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
            "ClockLend: P2P Offer #{} cancelled, refunded {} collateral tokens to creator",
            offer.offer_id,
            transfer_amount
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

pub fn process_withdraw_treasury(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let admin = next_account_info(account_info_iter)?;
    let admin_config_account = next_account_info(account_info_iter)?;
    let treasury_account = next_account_info(account_info_iter)?;
    let destination_account = next_account_info(account_info_iter)?;

    assert_signer(admin)?;
    assert_owned_by(admin_config_account, program_id)?;

    if amount == 0 {
        return Err(ClockLendError::InvalidInstruction.into());
    }

    let (expected_admin_pda, _) = Pubkey::find_program_address(&[ADMIN_SEED], program_id);
    if expected_admin_pda != *admin_config_account.key {
        return Err(ClockLendError::InvalidSeeds.into());
    }

    let admin_config = AdminConfig::unpack_from_slice(&admin_config_account.try_borrow_data()?)?;
    if !admin_config.is_initialized || admin_config.admin != *admin.key {
        return Err(ClockLendError::Unauthorized.into());
    }

    let (expected_treasury_pda, treasury_bump) = Pubkey::find_program_address(&[TREASURY_SEED], program_id);
    if expected_treasury_pda != *treasury_account.key {
        return Err(ClockLendError::InvalidTreasuryAccount.into());
    }

    let treasury_token_opt = next_account_info(account_info_iter).ok();
    let token_program_opt = next_account_info(account_info_iter).ok();
    let system_program_opt = next_account_info(account_info_iter).ok();

    // L-3: validate the system program before CPI-ing into it as the treasury PDA
    if let Some(system_program) = system_program_opt {
        assert_system_program(system_program)?;
    }

    if let (Some(treasury_token_acc), Some(token_prog)) = (treasury_token_opt, token_program_opt) {
        if treasury_token_acc.owner == token_prog.key {
            assert_token_program(token_prog)?;
            let tok = spl_token::state::Account::unpack(&treasury_token_acc.try_borrow_data()?)?;
            if tok.owner != expected_treasury_pda {
                return Err(ClockLendError::InvalidTreasuryAccount.into());
            }
            if tok.amount < amount {
                return Err(ClockLendError::InsufficientLiquidity.into());
            }
            let dest_tok = spl_token::state::Account::unpack(&destination_account.try_borrow_data()?)?;
            if dest_tok.mint != tok.mint {
                return Err(ClockLendError::InvalidMint.into());
            }

            invoke_signed(
                &spl_token::instruction::transfer(
                    token_prog.key,
                    treasury_token_acc.key,
                    destination_account.key,
                    treasury_account.key,
                    &[],
                    amount,
                )?,
                &[
                    treasury_token_acc.clone(),
                    destination_account.clone(),
                    treasury_account.clone(),
                    token_prog.clone(),
                ],
                &[&[TREASURY_SEED, &[treasury_bump]]],
            )?;

            msg!("ClockLend: Withdrew {} tokens from Treasury to {}", amount, destination_account.key);
            return Ok(());
        }
    }

    // Native SOL withdrawal
    let treasury_lamports = treasury_account.lamports();
    let rent_exempt_min = solana_program::rent::Rent::default().minimum_balance(0);
    if treasury_lamports.saturating_sub(rent_exempt_min) < amount {
        return Err(ClockLendError::InsufficientLiquidity.into());
    }

    transfer_native_sol_from_escrow(
        treasury_account,
        destination_account,
        system_program_opt,
        amount,
        &[TREASURY_SEED, &[treasury_bump]],
    )?;

    msg!("ClockLend: Withdrew {} lamports native SOL from Treasury to {}", amount, destination_account.key);
    Ok(())
}

/// Scale for acc_reward_per_share (dividend accounting).
const YIELD_SCALE: u128 = 1_000_000_000_000;
/// Minimum time a stake must be held before its dividends are claimable.
/// Prevents stake-in-slot-N / claim-in-slot-N+1 yield sniping.
const MIN_STAKE_AGE_SECS: i64 = 3600;
/// Admin feeds older than this cannot price loans (the stored 3600s window is
/// for monitoring only). Kept fresh by the keeper crank (Jupiter/CoinGecko).
const ADMIN_FEED_MAX_PRICE_AGE_SECS: i64 = 600;

/// Fold `amount` into acc_reward_per_share. Rewards deposited while no staker
/// is synced are parked in `unallocated_rewards` and folded into the next
/// allocation so they are never stranded.
fn accrue_yield(vault: &mut SkrYieldVault, amount: u64) -> ProgramResult {
    if vault.total_staked_skr > 0 {
        // Round 11: the parked backlog is dripped (a quarter per accrue) so a
        // just-in-time staker cannot capture 100% of it with a single dust
        // borrow-triggered fold; long-term stakers receive the rest over the
        // following accrues.
        let fold = vault.unallocated_rewards.saturating_add(3) / 4;
        let total_to_allocate = (amount as u128)
            .checked_add(fold as u128)
            .ok_or(ClockLendError::AmountOverflow)?;
        let reward_increase = total_to_allocate
            .checked_mul(YIELD_SCALE)
            .ok_or(ClockLendError::AmountOverflow)?
            / (vault.total_staked_skr as u128);
        vault.acc_reward_per_share = vault
            .acc_reward_per_share
            .checked_add(reward_increase)
            .ok_or(ClockLendError::AmountOverflow)?;
        vault.unallocated_rewards = vault.unallocated_rewards.saturating_sub(fold);
    } else {
        vault.unallocated_rewards = vault
            .unallocated_rewards
            .checked_add(amount)
            .ok_or(ClockLendError::AmountOverflow)?;
    }
    vault.total_rewards_distributed = vault
        .total_rewards_distributed
        .checked_add(amount)
        .ok_or(ClockLendError::AmountOverflow)?;
    vault.pending_rewards = vault
        .pending_rewards
        .checked_add(amount)
        .ok_or(ClockLendError::AmountOverflow)?;
    Ok(())
}

pub fn process_initialize_skr_yield_vault(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let authority = next_account_info(account_info_iter)?;
    let yield_vault_account = next_account_info(account_info_iter)?;
    let reward_mint = next_account_info(account_info_iter)?;
    let vault_token_account = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;
    let rent_sysvar = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;
    let admin_account = next_account_info(account_info_iter)?;

    assert_signer(authority)?;
    assert_system_program(system_program)?;
    assert_token_program(token_program)?;

    // Only the protocol admin may initialize the vault — otherwise anyone
    // could squat the canonical vault PDA (derived from reward_mint only)
    // and become the recorded authority before the team does.
    let (expected_admin_pda, _) =
        Pubkey::find_program_address(&[ADMIN_SEED], program_id);
    if *admin_account.key != expected_admin_pda || admin_account.owner != program_id {
        return Err(ClockLendError::Unauthorized.into());
    }
    let admin_config = AdminConfig::unpack_from_slice(&admin_account.try_borrow_data()?)?;
    if !admin_config.is_initialized || admin_config.admin != *authority.key {
        return Err(ClockLendError::Unauthorized.into());
    }

    // Reward mint allowlist: the vault's accounting assumes 6-decimal rewards.
    if *reward_mint.key != USDC_DEVNET_MINT
        && *reward_mint.key != USDC_MAINNET_MINT
        && *reward_mint.key != SKR_MINT
    {
        return Err(ClockLendError::InvalidMint.into());
    }

    let (expected_vault_pda, vault_bump) =
        Pubkey::find_program_address(&[SKR_YIELD_VAULT_SEED, reward_mint.key.as_ref()], program_id);
    if expected_vault_pda != *yield_vault_account.key {
        return Err(ClockLendError::InvalidYieldVault.into());
    }

    let (expected_token_pda, token_bump) =
        Pubkey::find_program_address(&[SKR_YIELD_TOKEN_SEED, reward_mint.key.as_ref()], program_id);
    if expected_token_pda != *vault_token_account.key {
        return Err(ClockLendError::InvalidVaultAccount.into());
    }

    // C-2: never re-initialize — a second call would zero acc_reward_per_share,
    // total_staked_skr and pending_rewards while users' reward_debt stays,
    // permanently stranding accrued dividends.
    if yield_vault_account.owner == program_id && !yield_vault_account.data_is_empty() {
        let existing = SkrYieldVault::unpack_from_slice(&yield_vault_account.try_borrow_data()?)?;
        if existing.is_initialized {
            return Err(ClockLendError::PoolAlreadyInitialized.into());
        }
    }

    create_or_allocate_pda(
        program_id,
        authority,
        yield_vault_account,
        system_program,
        SkrYieldVault::LEN,
        &[SKR_YIELD_VAULT_SEED, reward_mint.key.as_ref(), &[vault_bump]],
    )?;

    if vault_token_account.owner == &solana_program::system_program::id() {
        create_or_allocate_token_pda(
            authority,
            vault_token_account,
            reward_mint,
            yield_vault_account,
            system_program,
            token_program,
            Some(rent_sysvar),
            &[SKR_YIELD_TOKEN_SEED, reward_mint.key.as_ref(), &[token_bump]],
        )?;
    }

    // Validate the vault token account on every path (mirror the deposit check).
    let vault_token = spl_token::state::Account::unpack(&vault_token_account.try_borrow_data()?)?;
    if vault_token.mint != *reward_mint.key || vault_token.owner != *yield_vault_account.key {
        return Err(ClockLendError::InvalidVaultAccount.into());
    }

    let vault = SkrYieldVault {
        discriminator: DISCRIMINATOR_SKR_YIELD,
        is_initialized: true,
        authority: *authority.key,
        reward_mint: *reward_mint.key,
        total_staked_skr: 0,
        acc_reward_per_share: 0,
        total_rewards_distributed: 0,
        pending_rewards: 0,
        unallocated_rewards: 0,
    };
    vault.pack_into_slice(&mut yield_vault_account.try_borrow_mut_data()?)?;

    msg!("ClockLend: Initialized SkrYieldVault for reward mint {}", reward_mint.key);
    Ok(())
}

pub fn process_deposit_skr_yield(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let depositor = next_account_info(account_info_iter)?;
    let yield_vault_account = next_account_info(account_info_iter)?;
    let depositor_token_account = next_account_info(account_info_iter)?;
    let vault_token_account = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;

    assert_signer(depositor)?;
    assert_token_program(token_program)?;

    if amount == 0 {
        return Err(ClockLendError::InvalidInstruction.into());
    }

    let mut vault = SkrYieldVault::unpack_from_slice(&yield_vault_account.try_borrow_data()?)?;
    if !vault.is_initialized {
        return Err(ClockLendError::PoolInactive.into());
    }

    // Only the vault authority (set at admin-gated init) may push rewards —
    // a permissionless deposit would let an attacker control exactly when
    // acc_reward_per_share moves and thus who is entitled.
    if *depositor.key != vault.authority {
        return Err(ClockLendError::Unauthorized.into());
    }

    let (expected_vault_pda, _) =
        Pubkey::find_program_address(&[SKR_YIELD_VAULT_SEED, vault.reward_mint.as_ref()], program_id);
    if expected_vault_pda != *yield_vault_account.key {
        return Err(ClockLendError::InvalidYieldVault.into());
    }

    let (expected_token_pda, _) =
        Pubkey::find_program_address(&[SKR_YIELD_TOKEN_SEED, vault.reward_mint.as_ref()], program_id);
    if expected_token_pda != *vault_token_account.key {
        return Err(ClockLendError::InvalidVaultAccount.into());
    }

    let dep_token = spl_token::state::Account::unpack(&depositor_token_account.try_borrow_data()?)?;
    if dep_token.mint != vault.reward_mint {
        return Err(ClockLendError::InvalidMint.into());
    }
    let vault_token = spl_token::state::Account::unpack(&vault_token_account.try_borrow_data()?)?;
    if vault_token.mint != vault.reward_mint || vault_token.owner != *yield_vault_account.key {
        return Err(ClockLendError::InvalidVaultAccount.into());
    }

    invoke(
        &spl_token::instruction::transfer(
            token_program.key,
            depositor_token_account.key,
            vault_token_account.key,
            depositor.key,
            &[],
            amount,
        )?,
        &[
            depositor_token_account.clone(),
            vault_token_account.clone(),
            depositor.clone(),
            token_program.clone(),
        ],
    )?;

    accrue_yield(&mut vault, amount)?;

    vault.pack_into_slice(&mut yield_vault_account.try_borrow_mut_data()?)?;

    msg!(
        "ClockLend: Distributed {} yield to SKR holders. acc_per_share: {}",
        amount,
        vault.acc_reward_per_share
    );
    Ok(())
}

/// Tag 18: authority-only recovery of tokens beyond pending_rewards. Covers
/// stranded dust, forfeited harvests and phantom-share residue — bounded so
/// user entitlements can never be touched.
pub fn process_withdraw_unused_yield(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let authority = next_account_info(account_info_iter)?;
    let yield_vault_account = next_account_info(account_info_iter)?;
    let vault_token_account = next_account_info(account_info_iter)?;
    let authority_token_account = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;

    assert_signer(authority)?;
    assert_token_program(token_program)?;

    let mut vault = SkrYieldVault::unpack_from_slice(&yield_vault_account.try_borrow_data()?)?;
    if !vault.is_initialized {
        return Err(ClockLendError::PoolInactive.into());
    }
    if *authority.key != vault.authority {
        return Err(ClockLendError::Unauthorized.into());
    }

    let (expected_vault_pda, vault_bump) = Pubkey::find_program_address(
        &[SKR_YIELD_VAULT_SEED, vault.reward_mint.as_ref()], program_id);
    if expected_vault_pda != *yield_vault_account.key {
        return Err(ClockLendError::InvalidYieldVault.into());
    }
    let (expected_token_pda, _) = Pubkey::find_program_address(
        &[SKR_YIELD_TOKEN_SEED, vault.reward_mint.as_ref()], program_id);
    if expected_token_pda != *vault_token_account.key {
        return Err(ClockLendError::InvalidVaultAccount.into());
    }

    let vault_token = spl_token::state::Account::unpack(&vault_token_account.try_borrow_data()?)?;
    if vault_token.mint != vault.reward_mint || vault_token.owner != *yield_vault_account.key {
        return Err(ClockLendError::InvalidVaultAccount.into());
    }
    let auth_token = spl_token::state::Account::unpack(&authority_token_account.try_borrow_data()?)?;
    if auth_token.owner != *authority.key || auth_token.mint != vault.reward_mint {
        return Err(ClockLendError::InvalidMint.into());
    }

    // Only the excess over pending_rewards is withdrawable: pending tracks
    // every deposit minus payouts and upper-bounds the sum of user
    // entitlements (share sums <= total by the fuzz-pinned invariant).
    let withdrawable = vault_token.amount.saturating_sub(vault.pending_rewards);
    if withdrawable == 0 {
        return Err(ClockLendError::InvalidInstruction.into());
    }

    invoke_signed(
        &spl_token::instruction::transfer(
            token_program.key,
            vault_token_account.key,
            authority_token_account.key,
            yield_vault_account.key,
            &[],
            withdrawable,
        )?,
        &[
            vault_token_account.clone(),
            authority_token_account.clone(),
            yield_vault_account.clone(),
            token_program.clone(),
        ],
        &[&[SKR_YIELD_VAULT_SEED, vault.reward_mint.as_ref(), &[vault_bump]]],
    )?;

    msg!("ClockLend: Withdrew {} unused yield-vault tokens", withdrawable);
    Ok(())
}

pub fn process_claim_skr_yield(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let user = next_account_info(account_info_iter)?;
    let yield_vault_account = next_account_info(account_info_iter)?;
    let user_yield_position_account = next_account_info(account_info_iter)?;
    let vault_token_account = next_account_info(account_info_iter)?;
    let user_reward_account = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;
    let skr_escrow_token = next_account_info(account_info_iter)?;

    assert_signer(user)?;
    assert_token_program(token_program)?;
    assert_system_program(system_program)?;

    // C-1: the SKR escrow token account is the SINGLE SOURCE OF TRUTH for the
    // user's stake. Deriving shares from a cached profile counter allowed a
    // recycled stake to keep earning after UnstakeSKR (ghost shares).
    let (expected_skr_escrow_pda, _) =
        Pubkey::find_program_address(&[b"skr_escrow", user.key.as_ref()], program_id);
    if *skr_escrow_token.key != expected_skr_escrow_pda {
        return Err(ClockLendError::InvalidEscrowAccount.into());
    }
    let escrow_token = spl_token::state::Account::unpack(&skr_escrow_token.try_borrow_data()?)?;
    if escrow_token.owner != expected_skr_escrow_pda || escrow_token.mint != SKR_MINT {
        return Err(ClockLendError::InvalidEscrowAccount.into());
    }

    let mut vault = SkrYieldVault::unpack_from_slice(&yield_vault_account.try_borrow_data()?)?;
    if !vault.is_initialized {
        return Err(ClockLendError::PoolInactive.into());
    }

    let (expected_vault_pda, vault_bump) =
        Pubkey::find_program_address(&[SKR_YIELD_VAULT_SEED, vault.reward_mint.as_ref()], program_id);
    if expected_vault_pda != *yield_vault_account.key {
        return Err(ClockLendError::InvalidYieldVault.into());
    }

    let (expected_token_pda, _) =
        Pubkey::find_program_address(&[SKR_YIELD_TOKEN_SEED, vault.reward_mint.as_ref()], program_id);
    if expected_token_pda != *vault_token_account.key {
        return Err(ClockLendError::InvalidVaultAccount.into());
    }

    let (expected_user_pda, user_bump) =
        Pubkey::find_program_address(&[USER_YIELD_SEED, user.key.as_ref(), vault.reward_mint.as_ref()], program_id);
    if expected_user_pda != *user_yield_position_account.key {
        return Err(ClockLendError::InvalidUserYieldPosition.into());
    }

    let is_new_position = user_yield_position_account.owner == &solana_program::system_program::id();
    create_or_allocate_pda(
        program_id,
        user,
        user_yield_position_account,
        system_program,
        UserYieldPosition::LEN,
        &[USER_YIELD_SEED, user.key.as_ref(), vault.reward_mint.as_ref(), &[user_bump]],
    )?;

    if is_new_position {
        let initial_pos = UserYieldPosition {
            discriminator: DISCRIMINATOR_USER_YIELD,
            is_initialized: true,
            user: *user.key,
            reward_mint: vault.reward_mint,
            staked_skr: 0,
            reward_debt: 0,
            accrued_rewards: 0,
            total_claimed: 0,
            last_interaction_time: Clock::get()?.unix_timestamp,
        };
        initial_pos.pack_into_slice(&mut user_yield_position_account.try_borrow_mut_data()?)?;
    }

    let mut position = UserYieldPosition::unpack_from_slice(&user_yield_position_account.try_borrow_data()?)?;
    if position.user != *user.key || position.reward_mint != vault.reward_mint {
        return Err(ClockLendError::Unauthorized.into());
    }

    // Accumulate pending rewards on the previous (cached) stake, BOUNDED by
    // the real escrow balance, then sync the position to the escrow. The
    // delta keeps total_staked_skr consistent without ever trusting the
    // cached value — a stale-high cached stake (only possible from pre-fix
    // state) can never earn ghost shares.
    let eff_staked = position.staked_skr.min(escrow_token.amount);
    let gross = ((eff_staked as u128)
        .checked_mul(vault.acc_reward_per_share)
        .ok_or(ClockLendError::AmountOverflow)?)
        / YIELD_SCALE;
    let pending = gross.saturating_sub(position.reward_debt.min(gross)) as u64;
    position.accrued_rewards = position.accrued_rewards.saturating_add(pending);

    let current_skr = escrow_token.amount;
    let stake_changed = current_skr != position.staked_skr;
    if current_skr > position.staked_skr {
        let diff = current_skr - position.staked_skr;
        vault.total_staked_skr = vault.total_staked_skr.saturating_add(diff);
    } else if current_skr < position.staked_skr {
        let diff = position.staked_skr - current_skr;
        vault.total_staked_skr = vault.total_staked_skr.saturating_sub(diff);
    }
    position.staked_skr = current_skr;
    position.reward_debt = ((position.staked_skr as u128)
        .checked_mul(vault.acc_reward_per_share)
        .ok_or(ClockLendError::AmountOverflow)?)
        / YIELD_SCALE;

    let claimable = position.accrued_rewards;
    let mut paid = false;
    if claimable > 0 {
        // H-2: minimum hold before dividends are claimable. Without it a
        // staker can register shares in slot N, collect the slot N+1 dividend
        // and unstake in N+2 — a zero-risk yield snipe.
        let now = Clock::get()?.unix_timestamp;
        if now.saturating_sub(position.last_interaction_time) < MIN_STAKE_AGE_SECS {
            return Err(ClockLendError::YieldCooldown.into());
        }

        let user_tok = spl_token::state::Account::unpack(&user_reward_account.try_borrow_data()?)?;
        if user_tok.owner != *user.key || user_tok.mint != vault.reward_mint {
            return Err(ClockLendError::InvalidMint.into());
        }

        let vault_tok = spl_token::state::Account::unpack(&vault_token_account.try_borrow_data()?)?;
        let payout = claimable.min(vault_tok.amount);

        if payout > 0 {
            invoke_signed(
                &spl_token::instruction::transfer(
                    token_program.key,
                    vault_token_account.key,
                    user_reward_account.key,
                    yield_vault_account.key,
                    &[],
                    payout,
                )?,
                &[
                    vault_token_account.clone(),
                    user_reward_account.clone(),
                    yield_vault_account.clone(),
                    token_program.clone(),
                ],
                &[&[SKR_YIELD_VAULT_SEED, vault.reward_mint.as_ref(), &[vault_bump]]],
            )?;

            vault.pending_rewards = vault.pending_rewards.saturating_sub(payout);
            position.total_claimed = position.total_claimed.saturating_add(payout);
            position.accrued_rewards = position.accrued_rewards.saturating_sub(payout);
            paid = true;
        }
    }

    // The cooldown timer measures time since the last PAYOUT or STAKE CHANGE.
    // Zero-payout syncs (share registration) do not refresh it — otherwise
    // every sync would re-arm the full cooldown for long-term stakers.
    if stake_changed || paid {
        position.last_interaction_time = Clock::get()?.unix_timestamp;
    }
    position.pack_into_slice(&mut user_yield_position_account.try_borrow_mut_data()?)?;
    vault.pack_into_slice(&mut yield_vault_account.try_borrow_mut_data()?)?;

    msg!("ClockLend: Claimed {} yield ({}-second stake cooldown applies)", claimable, MIN_STAKE_AGE_SECS);
    Ok(())
}
