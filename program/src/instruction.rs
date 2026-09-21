use borsh::{BorshDeserialize, BorshSerialize};
use crate::state::PoolType;

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub enum ClockLendInstruction {
    /// 0. Initialize a new Lending Pool (Individual Merchant or Circle)
    /// Accounts:
    /// 0. `[signer]` Authority (merchant or circle admin)
    /// 1. `[writable]` LendingPool PDA `[b"pool", authority, pool_id]`
    /// 2. `[]` Liquidity Mint (USDC/SOL)
    /// 3. `[writable]` Vault PDA `[b"vault", pool_pda]`
    /// 4. `[]` System Program
    /// 5. `[]` Token Program
    /// 6. `[]` Rent Sysvar
    InitializePool {
        pool_id: u64,
        pool_type: PoolType,
        interest_rate_bps: u16,
        max_ltv_bps: u16,
        min_duration: i64,
        max_duration: i64,
        name: [u8; 32],
    },

    /// 1. Deposit liquidity into pool vault
    /// Accounts:
    /// 0. `[signer]` Depositor
    /// 1. `[writable]` LendingPool PDA
    /// 2. `[writable]` Depositor Token Account
    /// 3. `[writable]` Vault PDA
    /// 4. `[]` Token Program
    DepositLiquidity {
        amount: u64,
    },

    /// 2. Stake SKR for reputation & bonding (unlocks 90% LTV & APR discounts)
    /// Accounts:
    /// 0. `[signer]` User
    /// 1. `[writable]` UserProfile PDA `[b"profile", user]`
    /// 2. `[writable, optional]` LendingPool PDA (if staking to back a pool)
    /// 3. `[writable]` User SKR Token Account
    /// 4. `[writable]` SKR Escrow Account
    /// 5. `[]` System Program
    /// 6. `[]` Token Program
    StakeSKR {
        amount: u64,
    },

    /// 3. Borrow from pool (Express / Circle Vault) with atomic collateral lock
    /// Accounts:
    /// 0. `[signer]` Borrower
    /// 1. `[writable]` LendingPool PDA
    /// 2. `[writable]` LoanOrder PDA `[b"loan", pool_pda, borrower, loan_id]`
    /// 3. `[writable]` Vault PDA (source of liquidity)
    /// 4. `[writable]` Borrower Liquidity Token Account (receives loan)
    /// 5. `[writable]` Borrower Collateral Token Account
    /// 6. `[writable]` Collateral Escrow PDA `[b"escrow", loan_order_pda]`
    /// 7. `[]` Collateral Mint
    /// 8. `[]` Token Program
    /// 9. `[]` System Program
    /// 10. `[writable, optional]` UserProfile PDA (for SKR discount check)
    /// 11. `[writable, optional]` Treasury Account
    /// 12. `[]` Clock Sysvar
    BorrowFromPool {
        loan_id: u64,
        borrow_amount: u64,
        collateral_amount: u64,
        duration_seconds: i64,
    },

    /// 4. Create a 1-on-1 P2P Pawn Offer on Circle Deck
    /// Accounts:
    /// 0. `[signer]` Creator (borrower)
    /// 1. `[writable]` P2POffer PDA `[b"p2p_offer", creator, offer_id]`
    /// 2. `[writable]` Creator Collateral Token Account
    /// 3. `[writable]` P2P Collateral Escrow PDA `[b"escrow", p2p_offer_pda]`
    /// 4. `[]` Collateral Mint
    /// 5. `[]` Token Program
    /// 6. `[]` System Program
    /// 7. `[]` Clock Sysvar
    CreateP2POffer {
        offer_id: u64,
        requested_amount: u64,
        collateral_amount: u64,
        interest_offered: u64,
        duration_seconds: i64,
    },

    /// 5. Fund a P2P Pawn Offer (Circle peer matches the card)
    /// Accounts:
    /// 0. `[signer]` Funder (peer)
    /// 1. `[writable]` P2POffer PDA
    /// 2. `[writable]` Funder Liquidity Token Account
    /// 3. `[writable]` Creator Liquidity Token Account (receives principal)
    /// 4. `[]` Token Program
    /// 5. `[]` Clock Sysvar
    FundP2POffer,

    /// 6. Repay active loan (Pool loan or P2P loan) & boost reputation score
    /// Accounts:
    /// 0. `[signer]` Borrower
    /// 1. `[writable]` LoanOrder PDA OR P2POffer PDA
    /// 2. `[writable]` Borrower Liquidity Token Account
    /// 3. `[writable]` Repayment Destination Account (Pool Vault or P2P Funder)
    /// 4. `[writable]` Collateral Escrow PDA
    /// 5. `[writable]` Borrower Collateral Token Account (receives collateral back)
    /// 6. `[writable, optional]` LendingPool PDA (if pool loan)
    /// 7. `[writable, optional]` UserProfile PDA (for credit score boost)
    /// 8. `[]` Token Program
    /// 9. `[]` Clock Sysvar
    RepayLoan {
        repay_amount: u64,
    },

    /// 7. Trigger 24-Hour Social Grace Period
    /// Accounts:
    /// 0. `[signer]` Caller (Borrower or Circle Member)
    /// 1. `[writable]` LoanOrder PDA OR P2POffer PDA
    /// 2. `[]` Clock Sysvar
    TriggerGracePeriod,

    /// 8. Liquidate defaulted collateral after grace period expires
    /// Accounts:
    /// 0. `[signer]` Caller (Pool Authority, LP, or P2P Funder)
    /// 1. `[writable]` LoanOrder PDA OR P2POffer PDA
    /// 2. `[writable]` Collateral Escrow PDA
    /// 3. `[writable]` Destination Collateral Account (Pool Vault/Authority or Funder)
    /// Optional / Context-specific Accounts:
    /// 4. `[writable, optional]` LendingPool PDA (required for pool loans)
    /// 5. `[writable, optional]` UserProfile PDA (for credit penalty & SKR slashing)
    /// 6. `[writable, optional]` Treasury Account (required for 5% liquidation margin when collateral > 0)
    /// 7. `[writable, optional]` SKR Escrow Account PDA `[b"skr_escrow", borrower]` (if borrower has staked SKR)
    /// 8. `[writable, optional]` SKR Slash Destination Token Account (required for SOL loans when borrower has staked SKR)
    /// 9. `[]` Token Program (required for SPL collateral or SKR slashing)
    /// 10. `[]` System Program (required for Native SOL collateral)
    /// 11. `[]` Clock Sysvar
    ClaimDefault,

    /// 9. Withdraw liquidity from pool vault (Pool Authority only)
    /// Accounts:
    /// 0. `[signer]` Authority
    /// 1. `[writable]` LendingPool PDA
    /// 2. `[writable]` Vault PDA
    /// 3. `[writable]` Authority Token Account
    /// 4. `[]` Token Program
    WithdrawLiquidity {
        amount: u64,
    },

    /// 10. Cancel an unfunded P2P Pawn Offer & reclaim locked collateral + rent
    /// Accounts:
    /// 0. `[signer]` Creator
    /// 1. `[writable]` P2POffer PDA
    /// 2. `[writable]` Collateral Escrow PDA
    /// 3. `[writable]` Creator Collateral Destination Account (receives collateral)
    /// 4. `[optional]` Token Program (if SPL token collateral)
    /// 5. `[]` System Program
    CancelP2POffer,
    /// 11. Unstake SKR tokens and return to user wallet
    /// Accounts:
    /// 0. `[signer]` User
    /// 1. `[writable]` UserProfile PDA `[b"profile", user]`
    /// 2. `[writable]` User SKR Token Account
    /// 3. `[writable]` SKR Escrow Account PDA `[b"skr_escrow", user]`
    /// 4. `[]` Token Program
    UnstakeSKR {
        amount: u64,
    },
    /// 12. Set or update on-chain oracle price feed for an asset mint
    /// Accounts:
    /// 0. `[signer]` Authority (Oracle keeper or pool authority)
    /// 1. `[writable]` PriceFeed PDA `[b"oracle", mint]`
    /// 2. `[]` Asset Mint
    /// 3. `[]` System Program
    /// 4. `[optional]` Clock Sysvar
    SetPriceFeed {
        price_micro_usd: u64,
        decimals: u8,
    },
}
