// Pyth pull-oracle client: fetches verified price updates from Hermes and
// builds the receiver postUpdate instructions that the program consumes.
// The program falls back to the admin feed if this fails, so the app degrades
// gracefully (borrow still works while the keeper feed is fresh).
import { PublicKey, Keypair, TransactionInstruction, Connection } from '@solana/web3.js';
import { PythSolanaReceiver } from '@pythnetwork/pyth-solana-receiver';

export const PYTH_RECEIVER_ID = new PublicKey('rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ');
export const WORMHOLE_CORE_BRIDGE_ID = new PublicKey('worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth');

// Canonical PythNet feed ids (must match the program's pyth.rs constants).
export const SOL_USD_FEED_ID = '0xef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d';
export const SKR_USD_FEED_ID = '0x38846ec4d0dbe808091817f5c0d6ab8058e25422348ddf97db52b6c378a93bf9';

export interface PythAttachment {
  instructions: TransactionInstruction[];
  signers: Keypair[];
  priceUpdateAccount: PublicKey;
}

/**
 * Fetch fully-verified price updates (base64 VAAs) from the public Hermes
 * endpoint. No API key required.
 */
export async function fetchPythPriceUpdates(feedIds: string[]): Promise<string[]> {
  const q = feedIds.map((id) => `ids[]=${encodeURIComponent(id)}`).join('&');
  const res = await fetch(
    `https://hermes.pyth.network/v2/updates/price/latest?${q}&encoding=base64`
  );
  if (!res.ok) throw new Error(`Pyth Hermes fetch failed: ${res.status}`);
  const j = await res.json();
  return j?.binary?.data ?? [];
}

/**
 * Build the receiver postUpdate instructions for one feed, plus the ephemeral
 * price-update account keypair. The returned account is what the program
 * verifies against the canonical feed id.
 */
export async function buildPythAttachment(
  connection: Connection,
  payer: PublicKey,
  feedId: string
): Promise<PythAttachment> {
  const [vaa] = await fetchPythPriceUpdates([feedId]);
  if (!vaa) throw new Error('Pyth: no price update returned for feed');

  // The builder wants an Anchor Wallet for rent/treasury accounting; the real
  // signing happens later via MWA, so a payer shim with no-op signers is fine.
  const walletShim = {
    publicKey: payer,
    signTransaction: async (tx: any) => tx,
    signAllTransactions: async (txs: any[]) => txs,
  } as any;

  const receiver = new PythSolanaReceiver({
    connection,
    wallet: walletShim,
    receiverProgramId: PYTH_RECEIVER_ID,
  });
  const builder = receiver.newTransactionBuilder({ closeUpdateAccounts: true });
  await builder.addPostPriceUpdates([vaa]);

  const chunk = builder.transactionInstructions[0];
  if (!chunk) throw new Error('Pyth: builder produced no instructions');

  return {
    instructions: chunk.instructions,
    signers: chunk.signers as Keypair[],
    priceUpdateAccount: builder.getPriceUpdateAccount(feedId),
  };
}

/** Feed id for a collateral name (program allowlist: SOL or SKR only). */
export function pythFeedIdForCollateral(collateralName: string): string {
  return collateralName.toUpperCase().includes('SOL') ? SOL_USD_FEED_ID : SKR_USD_FEED_ID;
}
