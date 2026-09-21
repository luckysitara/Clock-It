use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    program_error::ProgramError,
    pubkey::Pubkey,
};

pub const POOL_SEED: &[u8] = b"pool";
pub const VAULT_SEED: &[u8] = b"vault";
pub const LOAN_SEED: &[u8] = b"loan";
pub const ESCROW_SEED: &[u8] = b"escrow";
pub const P2P_SEED: &[u8] = b"p2p_offer";
pub const PROFILE_SEED: &[u8] = b"profile";
pub const TREASURY_SEED: &[u8] = b"treasury";
pub const ORACLE_SEED: &[u8] = b"oracle";
pub const SKR_MINT: Pubkey = solana_program::pubkey!("SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3");
pub const USDC_DEVNET_MINT: Pubkey = solana_program::pubkey!("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU");

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, Copy, PartialEq)]
pub enum PoolType {
    Individual,
    Circle,
    Institutional,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, Copy, PartialEq)]
pub enum LoanStatus {
    Active,
    InGracePeriod,
    Repaid,
    Defaulted,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, Copy, PartialEq)]
pub enum OfferStatus {
    Open,
    Funded,
    InGracePeriod,
    Repaid,
    Defaulted,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct LendingPool {
    pub is_initialized: bool,
    pub pool_type: PoolType,
    pub authority: Pubkey,
    pub liquidity_mint: Pubkey,
    pub vault_pda: Pubkey,
    pub total_liquidity: u64,
    pub total_borrowed: u64,
    pub staked_skr_amount: u64,
    pub interest_rate_bps: u16,
    pub max_ltv_bps: u16,
    pub min_duration: i64,
    pub max_duration: i64,
    pub loans_originated: u32,
    pub loans_repaid: u32,
    pub name: [u8; 32],
}

impl LendingPool {
    pub const LEN: usize = 1 + 1 + 32 + 32 + 32 + 8 + 8 + 8 + 2 + 2 + 8 + 8 + 4 + 4 + 32; // 182 bytes

    pub fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        BorshDeserialize::try_from_slice(src).map_err(|_| ProgramError::InvalidAccountData)
    }

    pub fn pack_into_slice(&self, dst: &mut [u8]) -> Result<(), ProgramError> {
        let serialized = borsh::to_vec(self).map_err(|_| ProgramError::InvalidAccountData)?;
        if dst.len() < serialized.len() {
            return Err(ProgramError::AccountDataTooSmall);
        }
        dst[..serialized.len()].copy_from_slice(&serialized);
        Ok(())
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct LoanOrder {
    pub is_active: bool,
    pub loan_id: u64,
    pub borrower: Pubkey,
    pub pool: Pubkey,
    pub principal_amount: u64,
    pub collateral_mint: Pubkey,
    pub collateral_amount: u64,
    pub interest_due: u64,
    pub origination_time: i64,
    pub due_time: i64,
    pub grace_period_expires: i64,
    pub status: LoanStatus,
    pub locked_skr: u64,
}

impl LoanOrder {
    pub const LEN: usize = 1 + 8 + 32 + 32 + 8 + 32 + 8 + 8 + 8 + 8 + 8 + 1 + 8; // 162 bytes

    pub fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src.len() >= Self::LEN {
            BorshDeserialize::try_from_slice(&src[..Self::LEN]).map_err(|_| ProgramError::InvalidAccountData)
        } else if src.len() >= 154 {
            #[derive(BorshDeserialize)]
            struct LegacyLoanOrder {
                is_active: bool,
                loan_id: u64,
                borrower: Pubkey,
                pool: Pubkey,
                principal_amount: u64,
                collateral_mint: Pubkey,
                collateral_amount: u64,
                interest_due: u64,
                origination_time: i64,
                due_time: i64,
                grace_period_expires: i64,
                status: LoanStatus,
            }
            let legacy = LegacyLoanOrder::try_from_slice(&src[..154])
                .map_err(|_| ProgramError::InvalidAccountData)?;
            Ok(Self {
                is_active: legacy.is_active,
                loan_id: legacy.loan_id,
                borrower: legacy.borrower,
                pool: legacy.pool,
                principal_amount: legacy.principal_amount,
                collateral_mint: legacy.collateral_mint,
                collateral_amount: legacy.collateral_amount,
                interest_due: legacy.interest_due,
                origination_time: legacy.origination_time,
                due_time: legacy.due_time,
                grace_period_expires: legacy.grace_period_expires,
                status: legacy.status,
                locked_skr: 0,
            })
        } else {
            Err(ProgramError::InvalidAccountData)
        }
    }

    pub fn pack_into_slice(&self, dst: &mut [u8]) -> Result<(), ProgramError> {
        let serialized = borsh::to_vec(self).map_err(|_| ProgramError::InvalidAccountData)?;
        if dst.len() >= serialized.len() {
            dst[..serialized.len()].copy_from_slice(&serialized);
            Ok(())
        } else if dst.len() >= 154 {
            #[derive(BorshSerialize)]
            struct LegacyLoanOrder {
                is_active: bool,
                loan_id: u64,
                borrower: Pubkey,
                pool: Pubkey,
                principal_amount: u64,
                collateral_mint: Pubkey,
                collateral_amount: u64,
                interest_due: u64,
                origination_time: i64,
                due_time: i64,
                grace_period_expires: i64,
                status: LoanStatus,
            }
            let legacy = LegacyLoanOrder {
                is_active: self.is_active,
                loan_id: self.loan_id,
                borrower: self.borrower,
                pool: self.pool,
                principal_amount: self.principal_amount,
                collateral_mint: self.collateral_mint,
                collateral_amount: self.collateral_amount,
                interest_due: self.interest_due,
                origination_time: self.origination_time,
                due_time: self.due_time,
                grace_period_expires: self.grace_period_expires,
                status: self.status,
            };
            let legacy_bytes = borsh::to_vec(&legacy).map_err(|_| ProgramError::InvalidAccountData)?;
            dst[..legacy_bytes.len()].copy_from_slice(&legacy_bytes);
            Ok(())
        } else {
            Err(ProgramError::AccountDataTooSmall)
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct P2POffer {
    pub is_initialized: bool,
    pub offer_id: u64,
    pub creator: Pubkey,
    pub funder: Pubkey,
    pub collateral_mint: Pubkey,
    pub collateral_amount: u64,
    pub requested_amount: u64,
    pub interest_offered: u64,
    pub duration_seconds: i64,
    pub created_at: i64,
    pub due_time: i64,
    pub grace_period_expires: i64,
    pub status: OfferStatus,
}

impl P2POffer {
    pub const LEN: usize = 1 + 8 + 32 + 32 + 32 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 1; // 160 bytes

    pub fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        BorshDeserialize::try_from_slice(src).map_err(|_| ProgramError::InvalidAccountData)
    }

    pub fn pack_into_slice(&self, dst: &mut [u8]) -> Result<(), ProgramError> {
        let serialized = borsh::to_vec(self).map_err(|_| ProgramError::InvalidAccountData)?;
        if dst.len() < serialized.len() {
            return Err(ProgramError::AccountDataTooSmall);
        }
        dst[..serialized.len()].copy_from_slice(&serialized);
        Ok(())
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct UserProfile {
    pub is_initialized: bool,
    pub user: Pubkey,
    pub staked_skr: u64,
    pub total_loans_completed: u32,
    pub total_loans_defaulted: u32,
    pub reputation_score: u16, // 0 - 10000 bps (e.g. 10000 = 100.0%)
    pub locked_skr: u64,
}

impl UserProfile {
    pub const LEN: usize = 1 + 32 + 8 + 4 + 4 + 2 + 8; // 59 bytes

    pub fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src.len() >= Self::LEN {
            BorshDeserialize::try_from_slice(&src[..Self::LEN]).map_err(|_| ProgramError::InvalidAccountData)
        } else if src.len() >= 51 {
            #[derive(BorshDeserialize)]
            struct LegacyUserProfile {
                is_initialized: bool,
                user: Pubkey,
                staked_skr: u64,
                total_loans_completed: u32,
                total_loans_defaulted: u32,
                reputation_score: u16,
            }
            let legacy = LegacyUserProfile::try_from_slice(&src[..51])
                .map_err(|_| ProgramError::InvalidAccountData)?;
            Ok(Self {
                is_initialized: legacy.is_initialized,
                user: legacy.user,
                staked_skr: legacy.staked_skr,
                total_loans_completed: legacy.total_loans_completed,
                total_loans_defaulted: legacy.total_loans_defaulted,
                reputation_score: legacy.reputation_score,
                locked_skr: 0,
            })
        } else {
            Err(ProgramError::InvalidAccountData)
        }
    }

    pub fn pack_into_slice(&self, dst: &mut [u8]) -> Result<(), ProgramError> {
        let serialized = borsh::to_vec(self).map_err(|_| ProgramError::InvalidAccountData)?;
        if dst.len() >= serialized.len() {
            dst[..serialized.len()].copy_from_slice(&serialized);
            Ok(())
        } else if dst.len() >= 51 {
            #[derive(BorshSerialize)]
            struct LegacyUserProfile {
                is_initialized: bool,
                user: Pubkey,
                staked_skr: u64,
                total_loans_completed: u32,
                total_loans_defaulted: u32,
                reputation_score: u16,
            }
            let legacy = LegacyUserProfile {
                is_initialized: self.is_initialized,
                user: self.user,
                staked_skr: self.staked_skr,
                total_loans_completed: self.total_loans_completed,
                total_loans_defaulted: self.total_loans_defaulted,
                reputation_score: self.reputation_score,
            };
            let legacy_bytes = borsh::to_vec(&legacy).map_err(|_| ProgramError::InvalidAccountData)?;
            dst[..legacy_bytes.len()].copy_from_slice(&legacy_bytes);
            Ok(())
        } else {
            Err(ProgramError::AccountDataTooSmall)
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct PriceFeed {
    pub is_initialized: bool,
    pub mint: Pubkey,
    pub price_micro_usd: u64, // Price in micro-USD (6 decimals: 1_000_000 = $1.00)
    pub decimals: u8,          // Token decimals (e.g. 9 for SOL, 6 for SKR, 6 for USDC)
    pub last_updated_at: i64,  // Unix timestamp of last keeper update
    pub authority: Pubkey,     // Oracle keeper or admin authority
}

impl PriceFeed {
    pub const LEN: usize = 1 + 32 + 8 + 1 + 8 + 32; // 82 bytes

    pub fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        BorshDeserialize::try_from_slice(src).map_err(|_| ProgramError::InvalidAccountData)
    }

    pub fn pack_into_slice(&self, dst: &mut [u8]) -> Result<(), ProgramError> {
        let serialized = borsh::to_vec(self).map_err(|_| ProgramError::InvalidAccountData)?;
        if dst.len() < serialized.len() {
            return Err(ProgramError::AccountDataTooSmall);
        }
        dst[..serialized.len()].copy_from_slice(&serialized);
        Ok(())
    }
}
