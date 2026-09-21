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
pub const ADMIN_SEED: &[u8] = b"admin";
pub const SKR_MINT: Pubkey = solana_program::pubkey!("SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3");
pub const USDC_DEVNET_MINT: Pubkey = solana_program::pubkey!("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU");
pub const USDC_MAINNET_MINT: Pubkey = solana_program::pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

pub const DISCRIMINATOR_POOL: [u8; 8] = *b"CLK_POOL";
pub const DISCRIMINATOR_LOAN: [u8; 8] = *b"CLK_LOAN";
pub const DISCRIMINATOR_OFFER: [u8; 8] = *b"CLK_PAWN";
pub const DISCRIMINATOR_PROFILE: [u8; 8] = *b"CLK_PROF";
pub const DISCRIMINATOR_FEED: [u8; 8] = *b"CLK_FEED";
pub const DISCRIMINATOR_ADMIN: [u8; 8] = *b"CLK_ADMN";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountKind {
    LendingPool,
    LoanOrder,
    P2POffer,
    UserProfile,
    PriceFeed,
    AdminConfig,
    Unknown,
}

impl AccountKind {
    pub fn from_slice(src: &[u8]) -> Self {
        if src.len() == 182 && src[0] == 1 {
            return AccountKind::LendingPool;
        }
        if src.len() < 8 {
            return AccountKind::Unknown;
        }
        match &src[0..8] {
            b"CLK_POOL" => AccountKind::LendingPool,
            b"CLK_LOAN" => AccountKind::LoanOrder,
            b"CLK_PAWN" => AccountKind::P2POffer,
            b"CLK_PROF" => AccountKind::UserProfile,
            b"CLK_FEED" => AccountKind::PriceFeed,
            b"CLK_ADMN" => AccountKind::AdminConfig,
            _ => AccountKind::Unknown,
        }
    }
}

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
    pub discriminator: [u8; 8],
    pub is_initialized: bool,
    pub pool_id: u64,
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
    pub is_oracle_free: bool,
    pub has_custom_oracle: bool,
}

impl LendingPool {
    pub const DISCRIMINATOR: [u8; 8] = DISCRIMINATOR_POOL;
    pub const LEN: usize = 8 + 1 + 8 + 1 + 32 + 32 + 32 + 8 + 8 + 8 + 2 + 2 + 8 + 8 + 4 + 4 + 32 + 1 + 1; // 200 bytes

    pub fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src.len() >= Self::LEN && &src[0..8] == &Self::DISCRIMINATOR {
            BorshDeserialize::try_from_slice(&src[..Self::LEN]).map_err(|_| ProgramError::InvalidAccountData)
        } else if src.len() == 182 && src[0] == 1 {
            // Legacy 182-byte LendingPool from devnet v1 deployment
            let is_initialized = src[0] == 1;
            let pool_type = match src[1] {
                0 => PoolType::Individual,
                1 => PoolType::Circle,
                2 => PoolType::Institutional,
                _ => return Err(ProgramError::InvalidAccountData),
            };
            let authority = Pubkey::new_from_array(src[2..34].try_into().unwrap());
            let liquidity_mint = Pubkey::new_from_array(src[34..66].try_into().unwrap());
            let vault_pda = Pubkey::new_from_array(src[66..98].try_into().unwrap());
            let total_liquidity = u64::from_le_bytes(src[98..106].try_into().unwrap());
            let total_borrowed = u64::from_le_bytes(src[106..114].try_into().unwrap());
            let staked_skr_amount = u64::from_le_bytes(src[114..122].try_into().unwrap());
            let interest_rate_bps = u16::from_le_bytes(src[122..124].try_into().unwrap());
            let max_ltv_bps = u16::from_le_bytes(src[124..126].try_into().unwrap());
            let min_duration = i64::from_le_bytes(src[126..134].try_into().unwrap());
            let max_duration = i64::from_le_bytes(src[134..142].try_into().unwrap());
            let loans_originated = u32::from_le_bytes(src[142..146].try_into().unwrap());
            let loans_repaid = u32::from_le_bytes(src[146..150].try_into().unwrap());
            let mut name = [0u8; 32];
            name.copy_from_slice(&src[150..182]);

            Ok(Self {
                discriminator: Self::DISCRIMINATOR,
                is_initialized,
                pool_id: 1,
                pool_type,
                authority,
                liquidity_mint,
                vault_pda,
                total_liquidity,
                total_borrowed,
                staked_skr_amount,
                interest_rate_bps,
                max_ltv_bps,
                min_duration,
                max_duration,
                loans_originated,
                loans_repaid,
                name,
                is_oracle_free: false,
                has_custom_oracle: false,
            })
        } else {
            Err(ProgramError::InvalidAccountData)
        }
    }

    pub fn pack_into_slice(&self, dst: &mut [u8]) -> Result<(), ProgramError> {
        if dst.len() >= Self::LEN {
            let serialized = borsh::to_vec(self).map_err(|_| ProgramError::InvalidAccountData)?;
            dst[..serialized.len()].copy_from_slice(&serialized);
            Ok(())
        } else if dst.len() == 182 {
            dst[0] = if self.is_initialized { 1 } else { 0 };
            dst[1] = match self.pool_type {
                PoolType::Individual => 0,
                PoolType::Circle => 1,
                PoolType::Institutional => 2,
            };
            dst[2..34].copy_from_slice(self.authority.as_ref());
            dst[34..66].copy_from_slice(self.liquidity_mint.as_ref());
            dst[66..98].copy_from_slice(self.vault_pda.as_ref());
            dst[98..106].copy_from_slice(&self.total_liquidity.to_le_bytes());
            dst[106..114].copy_from_slice(&self.total_borrowed.to_le_bytes());
            dst[114..122].copy_from_slice(&self.staked_skr_amount.to_le_bytes());
            dst[122..124].copy_from_slice(&self.interest_rate_bps.to_le_bytes());
            dst[124..126].copy_from_slice(&self.max_ltv_bps.to_le_bytes());
            dst[126..134].copy_from_slice(&self.min_duration.to_le_bytes());
            dst[134..142].copy_from_slice(&self.max_duration.to_le_bytes());
            dst[142..146].copy_from_slice(&self.loans_originated.to_le_bytes());
            dst[146..150].copy_from_slice(&self.loans_repaid.to_le_bytes());
            dst[150..182].copy_from_slice(&self.name);
            Ok(())
        } else {
            Err(ProgramError::AccountDataTooSmall)
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct LoanOrder {
    pub discriminator: [u8; 8],
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
    pub const DISCRIMINATOR: [u8; 8] = DISCRIMINATOR_LOAN;
    pub const LEN: usize = 8 + 1 + 8 + 32 + 32 + 8 + 32 + 8 + 8 + 8 + 8 + 8 + 1 + 8; // 170 bytes

    pub fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src.len() >= Self::LEN && &src[0..8] == &Self::DISCRIMINATOR {
            BorshDeserialize::try_from_slice(&src[..Self::LEN]).map_err(|_| ProgramError::InvalidAccountData)
        } else {
            Err(ProgramError::InvalidAccountData)
        }
    }

    pub fn pack_into_slice(&self, dst: &mut [u8]) -> Result<(), ProgramError> {
        if dst.len() < Self::LEN {
            return Err(ProgramError::AccountDataTooSmall);
        }
        let serialized = borsh::to_vec(self).map_err(|_| ProgramError::InvalidAccountData)?;
        dst[..serialized.len()].copy_from_slice(&serialized);
        Ok(())
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct P2POffer {
    pub discriminator: [u8; 8],
    pub is_initialized: bool,
    pub offer_id: u64,
    pub creator: Pubkey,
    pub funder: Pubkey,
    pub collateral_mint: Pubkey,
    pub liquidity_mint: Pubkey,
    pub collateral_amount: u64,
    pub requested_amount: u64,
    pub interest_offered: u64,
    pub duration_seconds: i64,
    pub created_at: i64,
    pub due_time: i64,
    pub grace_period_expires: i64,
    pub status: OfferStatus,
}

#[derive(BorshSerialize, BorshDeserialize)]
struct LegacyP2POffer {
    discriminator: [u8; 8],
    is_initialized: bool,
    offer_id: u64,
    creator: Pubkey,
    funder: Pubkey,
    collateral_mint: Pubkey,
    collateral_amount: u64,
    requested_amount: u64,
    interest_offered: u64,
    duration_seconds: i64,
    created_at: i64,
    due_time: i64,
    grace_period_expires: i64,
    status: OfferStatus,
}

impl P2POffer {
    pub const DISCRIMINATOR: [u8; 8] = DISCRIMINATOR_OFFER;
    pub const LEN: usize = 8 + 1 + 8 + 32 + 32 + 32 + 32 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 1; // 202 bytes

    pub fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src.len() >= Self::LEN && &src[0..8] == &Self::DISCRIMINATOR {
            BorshDeserialize::try_from_slice(&src[..Self::LEN]).map_err(|_| ProgramError::InvalidAccountData)
        } else if src.len() >= 170 && &src[0..8] == &Self::DISCRIMINATOR {
            let legacy = LegacyP2POffer::try_from_slice(&src[..170]).map_err(|_| ProgramError::InvalidAccountData)?;
            Ok(Self {
                discriminator: legacy.discriminator,
                is_initialized: legacy.is_initialized,
                offer_id: legacy.offer_id,
                creator: legacy.creator,
                funder: legacy.funder,
                collateral_mint: legacy.collateral_mint,
                liquidity_mint: USDC_DEVNET_MINT,
                collateral_amount: legacy.collateral_amount,
                requested_amount: legacy.requested_amount,
                interest_offered: legacy.interest_offered,
                duration_seconds: legacy.duration_seconds,
                created_at: legacy.created_at,
                due_time: legacy.due_time,
                grace_period_expires: legacy.grace_period_expires,
                status: legacy.status,
            })
        } else {
            Err(ProgramError::InvalidAccountData)
        }
    }

    pub fn pack_into_slice(&self, dst: &mut [u8]) -> Result<(), ProgramError> {
        if dst.len() >= Self::LEN {
            let serialized = borsh::to_vec(self).map_err(|_| ProgramError::InvalidAccountData)?;
            dst[..serialized.len()].copy_from_slice(&serialized);
            Ok(())
        } else if dst.len() >= 170 {
            let legacy = LegacyP2POffer {
                discriminator: self.discriminator,
                is_initialized: self.is_initialized,
                offer_id: self.offer_id,
                creator: self.creator,
                funder: self.funder,
                collateral_mint: self.collateral_mint,
                collateral_amount: self.collateral_amount,
                requested_amount: self.requested_amount,
                interest_offered: self.interest_offered,
                duration_seconds: self.duration_seconds,
                created_at: self.created_at,
                due_time: self.due_time,
                grace_period_expires: self.grace_period_expires,
                status: self.status,
            };
            let serialized = borsh::to_vec(&legacy).map_err(|_| ProgramError::InvalidAccountData)?;
            dst[..serialized.len()].copy_from_slice(&serialized);
            Ok(())
        } else {
            Err(ProgramError::AccountDataTooSmall)
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct UserProfile {
    pub discriminator: [u8; 8],
    pub is_initialized: bool,
    pub user: Pubkey,
    pub staked_skr: u64,
    pub total_loans_completed: u32,
    pub total_loans_defaulted: u32,
    pub reputation_score: u16, // 0 - 10000 bps (e.g. 10000 = 100.0%)
    pub locked_skr: u64,
}

impl UserProfile {
    pub const DISCRIMINATOR: [u8; 8] = DISCRIMINATOR_PROFILE;
    pub const LEN: usize = 8 + 1 + 32 + 8 + 4 + 4 + 2 + 8; // 67 bytes

    pub fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src.len() >= Self::LEN && &src[0..8] == &Self::DISCRIMINATOR {
            BorshDeserialize::try_from_slice(&src[..Self::LEN]).map_err(|_| ProgramError::InvalidAccountData)
        } else {
            Err(ProgramError::InvalidAccountData)
        }
    }

    pub fn pack_into_slice(&self, dst: &mut [u8]) -> Result<(), ProgramError> {
        if dst.len() < Self::LEN {
            return Err(ProgramError::AccountDataTooSmall);
        }
        let serialized = borsh::to_vec(self).map_err(|_| ProgramError::InvalidAccountData)?;
        dst[..serialized.len()].copy_from_slice(&serialized);
        Ok(())
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct PriceFeed {
    pub discriminator: [u8; 8],
    pub is_initialized: bool,
    pub mint: Pubkey,
    pub price_micro_usd: u64, // Price in micro-USD (6 decimals: 1_000_000 = $1.00)
    pub decimals: u8,          // Token decimals (e.g. 9 for SOL, 6 for SKR, 6 for USDC)
    pub last_updated_at: i64,  // Unix timestamp of last keeper update
    pub authority: Pubkey,     // Oracle keeper or admin authority
    pub max_staleness_seconds: i64,
}

impl PriceFeed {
    pub const DISCRIMINATOR: [u8; 8] = DISCRIMINATOR_FEED;
    pub const LEN: usize = 8 + 1 + 32 + 8 + 1 + 8 + 32 + 8; // 98 bytes

    pub fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src.len() >= Self::LEN && &src[0..8] == &Self::DISCRIMINATOR {
            BorshDeserialize::try_from_slice(&src[..Self::LEN]).map_err(|_| ProgramError::InvalidAccountData)
        } else {
            Err(ProgramError::InvalidAccountData)
        }
    }

    pub fn pack_into_slice(&self, dst: &mut [u8]) -> Result<(), ProgramError> {
        if dst.len() < Self::LEN {
            return Err(ProgramError::AccountDataTooSmall);
        }
        let serialized = borsh::to_vec(self).map_err(|_| ProgramError::InvalidAccountData)?;
        dst[..serialized.len()].copy_from_slice(&serialized);
        Ok(())
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct AdminConfig {
    pub discriminator: [u8; 8],
    pub is_initialized: bool,
    pub admin: Pubkey,
    pub oracle_authority: Pubkey,
}

impl AdminConfig {
    pub const DISCRIMINATOR: [u8; 8] = DISCRIMINATOR_ADMIN;
    pub const LEN: usize = 8 + 1 + 32 + 32; // 73 bytes

    pub fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src.len() >= Self::LEN && &src[0..8] == &Self::DISCRIMINATOR {
            BorshDeserialize::try_from_slice(&src[..Self::LEN]).map_err(|_| ProgramError::InvalidAccountData)
        } else {
            Err(ProgramError::InvalidAccountData)
        }
    }

    pub fn pack_into_slice(&self, dst: &mut [u8]) -> Result<(), ProgramError> {
        if dst.len() < Self::LEN {
            return Err(ProgramError::AccountDataTooSmall);
        }
        let serialized = borsh::to_vec(self).map_err(|_| ProgramError::InvalidAccountData)?;
        dst[..serialized.len()].copy_from_slice(&serialized);
        Ok(())
    }
}
