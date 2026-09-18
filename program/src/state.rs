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

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, Copy, PartialEq)]
pub enum PoolType {
    Individual,
    Circle,
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
}

impl LoanOrder {
    pub const LEN: usize = 1 + 8 + 32 + 32 + 8 + 32 + 8 + 8 + 8 + 8 + 8 + 1; // 154 bytes

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
    pub const LEN: usize = 1 + 8 + 32 + 32 + 32 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 1; // 162 bytes

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
}

impl UserProfile {
    pub const LEN: usize = 1 + 32 + 8 + 4 + 4 + 2; // 51 bytes

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
