import '../polyfill';
import { PublicKey, Transaction, Keypair } from '@solana/web3.js';
import { transact, Web3MobileWallet } from '@solana-mobile/mobile-wallet-adapter-protocol-web3js';
import { base64ToUint8Array, base64ToBase58 } from '@solana-mobile/mobile-wallet-adapter-protocol/encoding';
import { Buffer } from 'buffer';
import * as SecureStore from 'expo-secure-store';
import {
  getConnection,
  mainnetConnection,
  PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  MEMO_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from './onChainService';
import { SolanaNetwork } from '../types';

export interface SeekerSession {
  publicKey: PublicKey;
  skrHandle: string; // e.g. "rootkit.skr"
  authToken?: string;
  isSeekerGenesisVerified: boolean;
}

const APP_IDENTITY = {
  name: 'ClockLend',
  uri: 'https://clocklend.xyz',
  icon: 'favicon.png',
};

const SNS_PROGRAM_ID = new PublicKey('namesLPneVptA9Z5rqUDD9tMTWEJwofgaYwp8cawRkX');

// Parse an MWA account address (handles Base58, Base64, Uint8Array, or Buffer)
export function parseMwaAddress(addressInput: any): PublicKey {
  if (!addressInput) {
    throw new Error('No wallet address provided from adapter');
  }

  if (addressInput instanceof PublicKey) {
    return addressInput;
  }

  if (addressInput instanceof Uint8Array || Buffer.isBuffer(addressInput)) {
    if (addressInput.length === 32) {
      return new PublicKey(addressInput);
    }
  }

  if (typeof addressInput === 'string') {
    const trimmed = addressInput.trim();

    // 1. Is it a standard Base58 address?
    // Solana Base58 string contains only Base58 characters [1-9A-HJ-NP-Za-km-z] and is 32 to 44 chars
    if (/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(trimmed)) {
      try {
        const pk = new PublicKey(trimmed);
        if (pk.toBase58() === trimmed) {
          return pk;
        }
      } catch {
        // continue
      }
    }

    // 2. Decode from Base64 (official MWA Account.address format)
    try {
      const bytes = base64ToUint8Array(trimmed);
      if (bytes && bytes.length === 32) {
        return new PublicKey(bytes);
      }
    } catch {
      // continue
    }

    try {
      const b58 = base64ToBase58(trimmed);
      if (b58) {
        return new PublicKey(b58);
      }
    } catch {
      // continue
    }

    try {
      const buf = Buffer.from(trimmed, 'base64');
      if (buf.length === 32) {
        return new PublicKey(buf);
      }
    } catch {
      // continue
    }

    return new PublicKey(trimmed);
  }

  throw new Error(`Unsupported wallet address format: ${typeof addressInput}`);
}

// Automatically derive the .skr username from the connected wallet
export async function deriveSkrUsername(pubkey: PublicKey, mwaLabel?: string): Promise<string> {
  // 1. Check if MWA / Seed Vault account label has a valid custom name
  if (mwaLabel && mwaLabel.trim().length > 0) {
    const clean = mwaLabel.trim().toLowerCase();
    const genericLabels = ['account 1', 'wallet', 'main', 'default', 'primary', 'key 1'];
    if (!genericLabels.includes(clean)) {
      const sanitized = clean.replace(/[^a-z0-9_.-]/g, '');
      if (sanitized.length >= 2) {
        return sanitized.endsWith('.skr') ? sanitized : `${sanitized}.skr`;
      }
    }
  }

  const base58 = pubkey.toBase58();

  // 2. Query on-chain SNS registry with a 1500ms timeout guard so it never blocks UI
  try {
    const snsPromise = mainnetConnection.getProgramAccounts(SNS_PROGRAM_ID, {
      filters: [{ memcmp: { offset: 32, bytes: base58 } }],
    });
    const timeoutPromise = new Promise<any[]>((_, reject) =>
      setTimeout(() => reject(new Error('SNS timeout')), 1500)
    );
    const snsAccounts = await Promise.race([snsPromise, timeoutPromise]);
    if (snsAccounts && snsAccounts.length > 0) {
      const data = snsAccounts[0].account.data;
      if (data.length > 96) {
        const rawName = new TextDecoder().decode(data.subarray(96)).replace(/\0/g, '').trim().toLowerCase();
        const cleanName = rawName.replace(/[^a-z0-9-]/g, '');
        if (cleanName.length > 0) {
          return `${cleanName}.skr`;
        }
      }
    }
  } catch {
    // Non-blocking fallback
  }

  // 4. Deterministic Seeker Genesis hardware device handle
  const head = base58.slice(0, 4).toLowerCase();
  const tail = base58.slice(-4).toLowerCase();
  return `skr_${head}${tail}.skr`;
}

// Connect to Seeker Wallet via Mobile Wallet Adapter
export async function connectSeekerWallet(cluster: SolanaNetwork = 'mainnet-beta'): Promise<SeekerSession> {
  // Execute MWA authorization with immediate return to prevent session timeout
  const authPayload = await transact(async (wallet: Web3MobileWallet) => {
    const authResult = await wallet.authorize({
      cluster,
      identity: APP_IDENTITY,
    });

    return {
      account: authResult.accounts[0],
      authToken: authResult.auth_token,
    };
  });

  // Perform address parsing and handle derivation outside the MWA session
  const pubkey = parseMwaAddress(authPayload.account.address);
  console.log('[SeekerWallet] Authorized Public Key:', pubkey.toBase58());
  const skrHandle = await deriveSkrUsername(pubkey, authPayload.account.label);

  return {
    publicKey: pubkey,
    skrHandle,
    authToken: authPayload.authToken,
    // Honest default: SGT ownership is confirmed on-chain by fetchLiveWalletAssets
    // (exact mint match) and surfaced via assets.hasSeekerGenesisToken.
    isSeekerGenesisVerified: false,
  };
}

// Create a session from any manually entered Solana address (e.g. for testing / custom address)
export async function createManualSession(pubkeyInput: string | PublicKey): Promise<SeekerSession> {
  const pubkey = typeof pubkeyInput === 'string' ? new PublicKey(pubkeyInput.trim()) : pubkeyInput;
  const skrHandle = await deriveSkrUsername(pubkey);
  return {
    publicKey: pubkey,
    skrHandle,
    // Honest default: SGT ownership is confirmed on-chain by fetchLiveWalletAssets
    // (exact mint match) and surfaced via assets.hasSeekerGenesisToken.
    isSeekerGenesisVerified: false,
  };
}

const PREVIEW_WALLET_KEY = 'clocklend_preview_demo_key';

export async function getOrCreatePreviewWallet(): Promise<PublicKey> {
  try {
    const saved = await SecureStore.getItemAsync(PREVIEW_WALLET_KEY);
    if (saved && saved.length >= 32) {
      return new PublicKey(saved);
    }
    const ephemeralKey = Keypair.generate().publicKey;
    await SecureStore.setItemAsync(PREVIEW_WALLET_KEY, ephemeralKey.toBase58());
    return ephemeralKey;
  } catch {
    return Keypair.generate().publicKey;
  }
}

export async function createPreviewSession(): Promise<SeekerSession> {
  const previewPubkey = await getOrCreatePreviewWallet();
  const skrHandle = await deriveSkrUsername(previewPubkey);
  return {
    publicKey: previewPubkey,
    skrHandle,
    // Honest default: SGT ownership is confirmed on-chain by fetchLiveWalletAssets
    // (exact mint match) and surfaced via assets.hasSeekerGenesisToken.
    isSeekerGenesisVerified: false,
  };
}

export const ALLOWED_PROGRAM_IDS = new Set<string>([
  PROGRAM_ID.toBase58(),
  '11111111111111111111111111111111', // System Program
  TOKEN_PROGRAM_ID.toBase58(),
  TOKEN_2022_PROGRAM_ID.toBase58(),
  ASSOCIATED_TOKEN_PROGRAM_ID.toBase58(),
  MEMO_PROGRAM_ID.toBase58(),
  'ComputeBudget111111111111111111111111111111', // Compute Budget Program
  'rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ', // Pyth Solana Receiver (price updates)
]);

/**
 * Security Guard (M-08): Validates that every instruction in the transaction
 * targets an allowlisted program ID before presenting to the user or MWA for signing.
 */
export function validateTransactionInstructions(transaction: Transaction): void {
  if (!transaction.instructions || transaction.instructions.length === 0) {
    throw new Error('[Security Exception] Attempted to sign empty transaction');
  }

  for (let i = 0; i < transaction.instructions.length; i++) {
    const ix = transaction.instructions[i];
    const pid = ix.programId.toBase58();
    if (!ALLOWED_PROGRAM_IDS.has(pid)) {
      throw new Error(
        `[Security Exception] Transaction contains unauthorized program ID: ${pid} (Instruction #${i}). Signing rejected to prevent blind wallet drain.`
      );
    }
  }
}

// Sign and broadcast transaction via Seeker Wallet
export async function signAndSendSeekerTransaction(
  transaction: Transaction,
  session: SeekerSession,
  network: SolanaNetwork = 'mainnet-beta'
): Promise<string> {
  // Validate instructions against program ID allowlist before signing
  validateTransactionInstructions(transaction);

  const conn = getConnection(network);
  const { blockhash, lastValidBlockHeight } = await conn.getLatestBlockhash('confirmed');
  // Builders may have pre-set the blockhash/feePayer and collected ephemeral
  // signer signatures over that exact message (Pyth attachment). Never
  // overwrite those — the pre-collected signature would no longer match.
  const hadPreexistingBlockhash = !!transaction.recentBlockhash;
  if (!transaction.recentBlockhash) transaction.recentBlockhash = blockhash;
  if (!transaction.feePayer) transaction.feePayer = session.publicKey;

  const signature = await transact(async (wallet: Web3MobileWallet) => {
    if (session.authToken) {
      try {
        await wallet.reauthorize({
          auth_token: session.authToken,
          identity: APP_IDENTITY,
        });
      } catch {
        await wallet.authorize({
          cluster: network,
          identity: APP_IDENTITY,
        });
      }
    } else {
      await wallet.authorize({
        cluster: network,
        identity: APP_IDENTITY,
      });
    }

    const signatures = await wallet.signAndSendTransactions({
      transactions: [transaction],
    });

    return signatures[0];
  });

  // Wait for confirmation on the active cluster. When the transaction carried
  // its own pre-set blockhash (Pyth attachment), the fresh blockhash's
  // validity window does not apply — poll from the current height instead.
  if (hadPreexistingBlockhash) {
    const currentHeight = await conn.getBlockHeight('confirmed');
    await conn.confirmTransaction(
      {
        signature,
        blockhash: transaction.recentBlockhash!,
        lastValidBlockHeight: currentHeight + 300,
      },
      'confirmed'
    );
  } else {
    await conn.confirmTransaction({ signature, blockhash, lastValidBlockHeight }, 'confirmed');
  }

  return signature;
}
