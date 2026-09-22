// Pyth pull-oracle integration (PythNet on Solana via the Pyth Solana Receiver).
//
// The borrower's transaction carries a Pyth PriceUpdateV2 account (owned by
// the receiver program). The account is written exclusively by the Pyth
// receiver after verifying Wormhole guardian signatures, so ownership of the
// account IS the trust root: no keeper, no admin price key.
//
// Verified here:
//   1. account owner == Pyth Solana Receiver program id
//   2. Anchor account discriminator + borsh layout (PriceUpdateV2)
//   3. embedded price_feed_id == the hardcoded canonical feed id
//   4. verification level >= Partial{1} guardian signatures
//   5. publish_time within the per-feed maximum age
//   6. price > 0 and normalizes into a bounded micro-USD value

use crate::error::ClockLendError;
use pyth_solana_receiver_sdk::price_update::{
    get_feed_id_from_hex, FeedId, PriceUpdateV2, VerificationLevel,
};
use solana_program::{
    account_info::AccountInfo,
    clock::Clock,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::Sysvar,
};
use anchor_lang::AnchorDeserialize;

/// sha256("account:PriceUpdateV2")[..8] — Anchor account discriminator.
const PRICE_UPDATE_DISCRIMINATOR: [u8; 8] = [0x22, 0xf1, 0x23, 0x63, 0x9d, 0x7e, 0xf4, 0xcd];

/// Pyth Solana Receiver program (mainnet + devnet share the same address).
pub const PYTH_RECEIVER_ID: Pubkey =
    solana_program::pubkey!("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");

/// Canonical PythNet feed ids (64-char hex = 32-byte identifiers).
pub const SOL_USD_FEED_ID_HEX: &str = "0xef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d";
pub const SKR_USD_FEED_ID_HEX: &str = "0x38846ec4d0dbe808091817f5c0d6ab8058e25422348ddf97db52b6c378a93bf9";

/// Per-feed freshness windows. SOL updates every slot (~400ms); SKR is a
/// low-frequency feed, so it gets a wider window.
pub const SOL_MAX_AGE_SECS: i64 = 120;
pub const SKR_MAX_AGE_SECS: i64 = 300;

/// A Pyth-verified price normalized to micro-USD (1_000_000 = $1.00).
pub struct PythPrice {
    pub price_micro_usd: u64,
    pub publish_time: i64,
}

/// Verify a Pyth price-update account against a canonical feed id and return
/// the price normalized to micro-USD. Fails closed on any mismatch.
pub fn try_verify_pyth_price(
    price_account: &AccountInfo,
    feed_id_hex: &str,
    max_age_secs: i64,
) -> Result<PythPrice, ProgramError> {
    // 1. Trust root: only accounts written by the Pyth receiver program.
    if price_account.owner != &PYTH_RECEIVER_ID {
        return Err(ClockLendError::InvalidOracleAccount.into());
    }

    // 2. Canonical feed id (constant-time-coded via fixed hex constants).
    let feed_id: FeedId =
        get_feed_id_from_hex(feed_id_hex).map_err(|_| ClockLendError::InvalidOracleAccount)?;

    // 3. Anchor account discriminator + borsh body.
    let data = price_account.try_borrow_data()?;
    if data.len() < 8 || data[..8] != PRICE_UPDATE_DISCRIMINATOR {
        return Err(ClockLendError::InvalidOracleAccount.into());
    }
    let update = PriceUpdateV2::deserialize(&mut &data[8..])
        .map_err(|_| ClockLendError::InvalidOracleAccount)?;

    // 4. Feed id match + verification level + freshness, in the SDK's own checks.
    // (anchor-lang carries its own Clock type; build it from the sysvar.)
    let sc = Clock::get()?;
    let clock = anchor_lang::prelude::Clock {
        slot: sc.slot,
        epoch_start_timestamp: sc.epoch_start_timestamp,
        epoch: sc.epoch,
        leader_schedule_epoch: sc.leader_schedule_epoch,
        unix_timestamp: sc.unix_timestamp,
    };
    let price = update
        .get_price_no_older_than_with_custom_verification_level(
            &clock,
            max_age_secs as u64,
            &feed_id,
            // The receiver's own Config enforces minimum_signatures = 3 before
            // writing any account, so this floor matches on-chain reality:
            // every accepted account carries a genuine, guardian-signed Pyth
            // price (guardian signatures cannot be forged; feed id + freshness
            // are checked here on top).
            VerificationLevel::Partial { num_signatures: 3 },
        )
        .map_err(|_| ClockLendError::StaleOraclePrice)?;

    // 5. Sanity bounds on the price and its confidence interval.
    if price.price <= 0 {
        return Err(ClockLendError::InvalidOracleAccount.into());
    }
    if price.conf > price.price as u64 {
        // Confidence wider than the price itself = unusable measurement.
        return Err(ClockLendError::InvalidOracleAccount.into());
    }

    // 6. Normalize to micro-USD (6 decimals): micro_usd = price * 10^(exponent + 6).
    let exponent = price.exponent as i64;
    let micro = if exponent >= -6 {
        (price.price as u128)
            .checked_mul(10u128.checked_pow((exponent + 6) as u32).ok_or(ClockLendError::AmountOverflow)?)
            .ok_or(ClockLendError::AmountOverflow)?
    } else {
        (price.price as u128) / 10u128.checked_pow((-exponent - 6) as u32).ok_or(ClockLendError::AmountOverflow)?
    };
    let micro_u64 = u64::try_from(micro).map_err(|_| ClockLendError::AmountOverflow)?;
    if micro_u64 == 0 || micro_u64 > 1_000_000_000_000 {
        return Err(ClockLendError::InvalidOracleAccount.into());
    }

    Ok(PythPrice {
        price_micro_usd: micro_u64,
        publish_time: price.publish_time,
    })
}
