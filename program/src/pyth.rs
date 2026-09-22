// Pyth pull-oracle integration (PythNet on Solana via the Pyth Solana Receiver).
//
// The borrower's transaction carries a Pyth PriceUpdateV2 account (owned by
// the receiver program). The account is written exclusively by the Pyth
// receiver after verifying Wormhole guardian signatures, so ownership of the
// account IS the trust root: no keeper, no admin price key.
//
// This module parses the PriceUpdateV2 wire format DIRECTLY (no anchor-lang /
// pyth-solana-receiver-sdk dependency) — the same layout the SDK deserializes:
//
//   [8]  anchor discriminator  sha256("account:PriceUpdateV2")[..8]
//   [32] write_authority
//   [1]  verification_level tag: 0 = Partial{u8 num_signatures}, 1 = Full
//   [+1] num_signatures (Partial only)
//   [32] feed_id
//   [8]  price (i64 LE)   [8] conf (u64 LE)   [4] exponent (i32 LE)
//   [8]  publish_time (i64 LE)  [8] prev_publish_time  [8] ema_price  [8] ema_conf
//   [8]  posted_slot (u64 LE)
//
// Verified here: receiver ownership, discriminator, verification level >= 3
// guardian signatures (the receiver's own Config minimum_signatures), feed id
// match, freshness window, price > 0, confidence <= price, bounded micro-USD
// normalization.

use crate::error::ClockLendError;
use solana_program::{
    account_info::AccountInfo,
    clock::Clock,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::Sysvar,
};

/// Pyth Solana Receiver program (mainnet + devnet share the same address).
pub const PYTH_RECEIVER_ID: Pubkey =
    solana_program::pubkey!("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");

/// Compile-time 64-char hex string -> [u8; 32].
macro_rules! hex32 {
    ($s:literal) => {{
        const fn hex_val(c: u8) -> u8 {
            match c {
                b'0'..=b'9' => c - b'0',
                b'a'..=b'f' => c - b'a' + 10,
                _ => 0,
            }
        }
        const fn decode<const N: usize>(s: &str) -> [u8; N] {
            let bytes = s.as_bytes();
            let mut out = [0u8; N];
            let mut i = 0;
            while i < N {
                out[i] = hex_val(bytes[i * 2]) << 4 | hex_val(bytes[i * 2 + 1]);
                i += 1;
            }
            out
        }
        decode::<32>($s)
    }};
}


/// Canonical PythNet feed ids (32-byte identifiers, hex).
pub const SOL_USD_FEED_ID: [u8; 32] = hex32!("ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d");
pub const SKR_USD_FEED_ID: [u8; 32] = hex32!("38846ec4d0dbe808091817f5c0d6ab8058e25422348ddf97db52b6c378a93bf9");

/// Per-feed freshness windows. SOL is cranked on a ~30-60s cadence; SKR is a
/// low-frequency feed, so it gets a wider window.
pub const SOL_MAX_AGE_SECS: i64 = 120;
pub const SKR_MAX_AGE_SECS: i64 = 300;

/// The receiver's on-chain Config enforces this many guardian signatures
/// before writing any price update account.
const MIN_GUARDIAN_SIGNATURES: u8 = 3;

/// sha256("account:PriceUpdateV2")[..8] — Anchor account discriminator.
const PRICE_UPDATE_DISCRIMINATOR: [u8; 8] = [0x22, 0xf1, 0x23, 0x63, 0x9d, 0x7e, 0xf4, 0xcd];

/// A Pyth-verified price normalized to micro-USD (1_000_000 = $1.00).
pub struct PythPrice {
    pub price_micro_usd: u64,
    pub publish_time: i64,
}

fn rd_u64(d: &[u8], off: usize) -> Result<u64, ProgramError> {
    let b: [u8; 8] = d
        .get(off..off + 8)
        .ok_or(ClockLendError::InvalidOracleAccount)?
        .try_into()
        .unwrap();
    Ok(u64::from_le_bytes(b))
}

fn rd_i64(d: &[u8], off: usize) -> Result<i64, ProgramError> {
    let b: [u8; 8] = d
        .get(off..off + 8)
        .ok_or(ClockLendError::InvalidOracleAccount)?
        .try_into()
        .unwrap();
    Ok(i64::from_le_bytes(b))
}

fn rd_i32(d: &[u8], off: usize) -> Result<i32, ProgramError> {
    let b: [u8; 4] = d
        .get(off..off + 4)
        .ok_or(ClockLendError::InvalidOracleAccount)?
        .try_into()
        .unwrap();
    Ok(i32::from_le_bytes(b))
}

/// Verify a Pyth price-update account against a canonical feed id and return
/// the price normalized to micro-USD. Fails closed on any mismatch.
pub fn try_verify_pyth_price(
    price_account: &AccountInfo,
    feed_id: &[u8; 32],
    max_age_secs: i64,
) -> Result<PythPrice, ProgramError> {
    // 1. Trust root: only accounts written by the Pyth receiver program.
    if price_account.owner != &PYTH_RECEIVER_ID {
        return Err(ClockLendError::InvalidOracleAccount.into());
    }

    let data = price_account.try_borrow_data()?;

    // 2. Anchor account discriminator.
    if data.len() < 8 || data[..8] != PRICE_UPDATE_DISCRIMINATOR {
        return Err(ClockLendError::InvalidOracleAccount.into());
    }
    let mut off = 8;

    // 3. write_authority (32) — informational; the receiver sets it.
    off += 32;

    // 4. Verification level: tag 1 = Full (19 signatures); tag 0 = Partial{n}.
    let num_signatures: u8 = match data.get(off) {
        Some(1) => {
            off += 1;
            19
        }
        Some(0) => {
            let n = *data.get(off + 1).ok_or(ClockLendError::InvalidOracleAccount)?;
            off += 2;
            n
        }
        _ => return Err(ClockLendError::InvalidOracleAccount.into()),
    };
    if num_signatures < MIN_GUARDIAN_SIGNATURES {
        return Err(ClockLendError::InvalidOracleAccount.into());
    }

    // 5. Feed id must match the canonical id for this collateral.
    let stored_feed: &[u8; 32] = data
        .get(off..off + 32)
        .ok_or(ClockLendError::InvalidOracleAccount)?
        .try_into()
        .unwrap();
    if stored_feed != feed_id {
        return Err(ClockLendError::InvalidOracleAccount.into());
    }
    off += 32;

    // 6. Price fields.
    let price = rd_i64(&data, off)?;
    off += 8;
    let conf = rd_u64(&data, off)?;
    off += 8;
    let exponent = rd_i32(&data, off)?;
    off += 4;
    let publish_time = rd_i64(&data, off)?;

    // 7. Freshness: the price message's publish time must be recent.
    let now = Clock::get()?.unix_timestamp;
    if publish_time.saturating_add(max_age_secs) < now {
        return Err(ClockLendError::StaleOraclePrice.into());
    }

    // 8. Sanity bounds on the price and its confidence interval.
    if price <= 0 {
        return Err(ClockLendError::InvalidOracleAccount.into());
    }
    if conf > price as u64 {
        // Confidence wider than the price itself = unusable measurement.
        return Err(ClockLendError::InvalidOracleAccount.into());
    }

    // 9. Normalize to micro-USD (6 decimals): micro_usd = price * 10^(exp + 6).
    let exponent = exponent as i64;
    let micro = if exponent >= -6 {
        (price as u128)
            .checked_mul(10u128.checked_pow((exponent + 6) as u32).ok_or(ClockLendError::AmountOverflow)?)
            .ok_or(ClockLendError::AmountOverflow)?
    } else {
        (price as u128) / 10u128.checked_pow((-exponent - 6) as u32).ok_or(ClockLendError::AmountOverflow)?
    };
    let micro_u64 = u64::try_from(micro).map_err(|_| ClockLendError::AmountOverflow)?;
    if micro_u64 == 0 || micro_u64 > 1_000_000_000_000 {
        return Err(ClockLendError::InvalidOracleAccount.into());
    }

    Ok(PythPrice {
        price_micro_usd: micro_u64,
        publish_time,
    })
}
