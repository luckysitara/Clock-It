use num_derive::FromPrimitive;
use solana_program::program_error::ProgramError;
use thiserror::Error;

#[derive(Error, Debug, Copy, Clone, FromPrimitive, PartialEq)]
pub enum ClockLendError {
    #[error("Invalid Instruction")]
    InvalidInstruction,
    #[error("Not Rent Exempt")]
    NotRentExempt,
    #[error("Expected Amount Mismatch")]
    ExpectedAmountMismatch,
    #[error("Amount Overflow")]
    AmountOverflow,
    #[error("Pool Inactive or Uninitialized")]
    PoolInactive,
    #[error("Unauthorized Signer")]
    Unauthorized,
    #[error("Loan Not Yet Due")]
    LoanNotDue,
    #[error("Loan Expired")]
    LoanExpired,
    #[error("Grace Period Still Active")]
    GracePeriodActive,
    #[error("Grace Period Expired")]
    GracePeriodExpired,
    #[error("Invalid Collateral Ratio")]
    InvalidCollateralRatio,
    #[error("Insufficient Liquidity in Vault")]
    InsufficientLiquidity,
    #[error("Invalid PDA Derived Seeds")]
    InvalidSeeds,
    #[error("Offer Already Funded")]
    OfferAlreadyFunded,
    #[error("Offer Not Open")]
    OfferNotOpen,
    #[error("Loan Already Repaid")]
    LoanAlreadyRepaid,
    #[error("Loan In Default")]
    LoanInDefault,
    #[error("Invalid Token Mint")]
    InvalidMint,
    #[error("Invalid Account Owner (Expected Program ID)")]
    InvalidAccountOwner,
    #[error("Invalid Vault Account")]
    InvalidVaultAccount,
    #[error("Invalid Escrow Account")]
    InvalidEscrowAccount,
    #[error("Invalid Repayment Destination (Must Be Pool Vault or Funder)")]
    InvalidRepaymentDestination,
    #[error("Loan Order Already Active")]
    LoanAlreadyActive,
    #[error("Unauthorized Caller (Must be Pool Authority, Funder, or Borrower)")]
    UnauthorizedCaller,
    #[error("Invalid Treasury Account")]
    InvalidTreasuryAccount,
    #[error("Pool Already Initialized")]
    PoolAlreadyInitialized,
    #[error("Insufficient Collateral Balance")]
    InsufficientCollateral,
    #[error("Unsupported Collateral Mint")]
    UnsupportedCollateralMint,
    #[error("Invalid Oracle Account")]
    InvalidOracleAccount,
    #[error("Stale Oracle Price Feed")]
    StaleOraclePrice,
    #[error("Staked SKR is locked by active loans")]
    StakeLocked,
    #[error("Invalid Account Data or Discriminator")]
    InvalidAccountData,
    #[error("Invalid Duration Bounds")]
    InvalidDuration,
    #[error("Invalid Interest Rate Bounds")]
    InvalidInterestRate,
    #[error("Offer Already Active or ID In Use")]
    OfferAlreadyActive,
    #[error("Invalid Profile Account")]
    InvalidProfileAccount,
}

impl From<ClockLendError> for ProgramError {
    fn from(e: ClockLendError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
