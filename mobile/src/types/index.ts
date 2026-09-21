export type PoolType = 'Individual' | 'Circle' | 'Institutional';

export type LoanStatus = 'Active' | 'InGracePeriod' | 'Repaid' | 'Defaulted';

export type OfferStatus = 'Open' | 'Funded' | 'InGracePeriod' | 'Repaid' | 'Defaulted';

export interface LendingPool {
  id: number;
  poolType: PoolType;
  authority: string;
  name: string;
  liquidityMint: string;
  totalLiquidity: number; // in USDC
  totalBorrowed: number;
  stakedSkrAmount: number;
  interestRateBps: number; // e.g. 800 = 8.0%
  maxLtvBps: number; // e.g. 8500 = 85%
  minDurationDays: number;
  maxDurationDays: number;
  loansOriginated: number;
  loansRepaid: number;
  successRate: number; // e.g. 99.4%
  isVerifiedMerchant: boolean;
}

export interface LoanOrder {
  id: number;
  poolId: number;
  poolName: string;
  borrower: string;
  principalAmount: number; // in USDC
  collateralName: string;
  collateralMint: string;
  collateralAmount: number;
  interestDue: number;
  originationTime: number; // unix timestamp seconds
  dueTime: number; // unix timestamp seconds
  gracePeriodExpires: number;
  status: LoanStatus;
  txSignature?: string;
  escrowAddress?: string;
  solscanUrl?: string;
  poolPubkey?: string;
}

export interface P2POffer {
  id: number;
  creator: string;
  creatorAvatar?: string;
  funder?: string;
  collateralName: string;
  collateralType: 'NFT' | 'cNFT' | 'Token';
  collateralAmount: number;
  collateralMint?: string;
  liquidityMint?: string;
  collateralImage?: string;
  requestedAmount: number; // USDC
  interestOffered: number; // USDC
  durationDays: number;
  createdAt: number;
  dueTime?: number;
  status: OfferStatus;
  txSignature?: string;
  escrowAddress?: string;
  solscanUrl?: string;
}

export interface UserProfile {
  pubkey: string;
  stakedSkr: number;
  totalLoansCompleted: number;
  totalLoansDefaulted: number;
  reputationScore: number; // 0 - 10000 bps
  tier: 'Diamond' | 'Gold' | 'Silver' | 'Standard';
  aprDiscount: number; // e.g. 50%
}

export type SolanaNetwork = 'devnet' | 'mainnet-beta';

export interface TokenAssetItem {
  mint: string;
  name: string;
  symbol: string;
  amount: number;
  decimals: number;
  usdValue: number;
  isToken2022?: boolean;
}

export interface WalletAssets {
  network: SolanaNetwork;
  solBalance: number;
  usdcBalance: number;
  skrBalance: number;
  bonkBalance: number;
  hasSeekerGenesisToken: boolean;
  totalUsdValue: number;
  tokenList?: TokenAssetItem[];
}
