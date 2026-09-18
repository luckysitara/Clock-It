import React, { useState, useEffect, useRef } from 'react';
import {
  StyleSheet,
  View,
  Text,
  TouchableOpacity,
  SafeAreaView,
  Alert,
  Linking,
  AppState,
  AppStateStatus,
} from 'react-native';
import { StatusBar } from 'expo-status-bar';
import { Ionicons } from '@expo/vector-icons';
import { PublicKey } from '@solana/web3.js';
import { ThemeProvider, useTheme } from './src/theme/ThemeContext';
import { Header } from './src/components/Header';
import { P2PExpressView } from './src/components/P2PExpressView';
import { MerchantDesksView } from './src/components/MerchantDesksView';
import { ActiveOrdersView } from './src/components/ActiveOrdersView';
import { CreditProfileView } from './src/components/CreditProfileView';
import { ConnectWalletView } from './src/components/ConnectWalletView';
import { WalletAssetsModal } from './src/components/WalletAssetsModal';
import { TransactionNoticeModal, TransactionNoticeData } from './src/components/TransactionNoticeModal';
import { SplashScreenView } from './src/components/SplashScreenView';
import { SecurityLockScreen, LockScreenMode } from './src/components/SecurityLockScreen';
import { isLockEnabled } from './src/services/securityService';
import {
  fetchLivePools,
  fetchLiveUserOrders,
  fetchLiveP2POffers,
  fetchLiveUserProfile,
  fetchLiveWalletAssets,
  requestDevnetAirdrop,
  buildBorrowTx,
  buildRepayTx,
  buildCreateP2POfferTx,
  buildFundP2POfferTx,
  buildRepayPawnOfferTx,
  buildCancelPawnOfferTx,
  buildCreatePoolTx,
  buildStakeSkrTx,
} from './src/solana/onChainService';
import {
  signAndSendSeekerTransaction,
  deriveSkrUsername,
  SeekerSession,
} from './src/solana/seekerWallet';
import { LendingPool, LoanOrder, P2POffer, OfferStatus, UserProfile, WalletAssets, SolanaNetwork } from './src/types';

type Tab = 'BORROW' | 'MARKET' | 'LOANS' | 'PROFILE';

const INITIAL_COMMUNITY_OFFERS: P2POffer[] = [
  {
    id: 9012,
    creator: '9aJbM6GZ8YQ1b8U2E7f3Wv1qV1pL7k9Xm2Y4z5N8qR7s',
    collateralName: 'Saga Monke Genesis #482',
    collateralType: 'NFT',
    collateralAmount: 1,
    requestedAmount: 180,
    interestOffered: 15,
    durationDays: 7,
    createdAt: Math.floor(Date.now() / 1000) - 3600 * 8,
    status: 'Open',
    escrowAddress: '7uL4Qv7yH9d2aX6kM1pZ8w4bC3eT5yG2jR6mN8sV9pX',
  },
  {
    id: 9013,
    creator: '4Zao8ocPhmMgq7PdsYWyxvqySMGx7xb9cMftPMkEokRG',
    collateralName: '1,500 SKR Token',
    collateralType: 'Token',
    collateralAmount: 1500,
    requestedAmount: 25,
    interestOffered: 3.5,
    durationDays: 14,
    createdAt: Math.floor(Date.now() / 1000) - 3600 * 20,
    status: 'Open',
    escrowAddress: '3zR1Kp5sX8mY2aL7v9bC4eT6yH1d9jN2mW8qV4pG7sL',
  },
  {
    id: 9014,
    creator: 'BEmX1nfeZT5i4VpSEeZmhiYxpZ9z4Y1LQLjAtPR9c3re',
    collateralName: 'Seeker Chapter 2 Preorder cNFT',
    collateralType: 'cNFT',
    collateralAmount: 1,
    requestedAmount: 350,
    interestOffered: 28,
    durationDays: 30,
    createdAt: Math.floor(Date.now() / 1000) - 3600 * 48,
    status: 'Open',
    escrowAddress: '8qM4V2yT7xK1pL6sZ9bC3eW5jR2mN8sV1pX7uL4Qv9d',
  },
];

function MainApp() {
  const { colors, mode } = useTheme();

  // Seeker Wallet & Network Session
  const [showSplash, setShowSplash] = useState<boolean>(true);
  const [isLocked, setIsLocked] = useState<boolean>(false);
  const [lockScreenMode, setLockScreenMode] = useState<LockScreenMode>('unlock');
  const [session, setSession] = useState<SeekerSession | null>(null);
  const [selectedNetwork, setSelectedNetwork] = useState<SolanaNetwork>('devnet');
  const [activeTab, setActiveTab] = useState<Tab>('BORROW');
  const [transactionNotice, setTransactionNotice] = useState<TransactionNoticeData | null>(null);

  // Auto-lock on app launch and background resume
  useEffect(() => {
    checkInitialLock();
    const sub = AppState.addEventListener('change', (nextState: AppStateStatus) => {
      if (nextState === 'active') {
        checkAppResumeLock();
      }
    });
    return () => sub.remove();
  }, []);

  const checkInitialLock = async () => {
    const enabled = await isLockEnabled();
    if (enabled) {
      setLockScreenMode('unlock');
      setIsLocked(true);
    }
  };

  const checkAppResumeLock = async () => {
    const enabled = await isLockEnabled();
    if (enabled) {
      setLockScreenMode('unlock');
      setIsLocked(true);
    }
  };

  const [pools, setPools] = useState<LendingPool[]>([]);
  const [orders, setOrders] = useState<LoanOrder[]>([]);
  const devnetOrdersRef = useRef<LoanOrder[]>([]);
  const devnetOffersRef = useRef<P2POffer[]>(INITIAL_COMMUNITY_OFFERS);
  const [offers, setOffers] = useState<P2POffer[]>(INITIAL_COMMUNITY_OFFERS);
  const [userProfile, setUserProfile] = useState<UserProfile | null>(null);
  const [walletAssets, setWalletAssets] = useState<WalletAssets>({
    network: 'devnet',
    solBalance: 0,
    usdcBalance: 0,
    skrBalance: 0,
    bonkBalance: 0,
    hasSeekerGenesisToken: true,
    totalUsdValue: 0,
    tokenList: [],
  });
  const [solBalance, setSolBalance] = useState<number>(0);
  const [isLoadingPools, setIsLoadingPools] = useState<boolean>(false);
  const [showAssetsModal, setShowAssetsModal] = useState<boolean>(false);
  const [toastMessage, setToastMessage] = useState<string | null>(null);
  const toastTimeoutRef = useRef<any>(null);

  const showToast = (msg: string) => {
    if (toastTimeoutRef.current) clearTimeout(toastTimeoutRef.current);
    setToastMessage(msg);
    toastTimeoutRef.current = setTimeout(() => {
      setToastMessage(null);
    }, 2800);
  };

  // Fast, independent wallet asset balance query
  const refreshWalletAssets = async (userPubkey: PublicKey, net: SolanaNetwork) => {
    try {
      const assets = await fetchLiveWalletAssets(userPubkey, net);
      if (net === 'devnet') {
        const activeBorrowAmount = devnetOrdersRef.current
          .filter((o) => o.status === 'Active' || o.status === 'InGracePeriod')
          .reduce((sum, o) => sum + o.principalAmount, 0);
        const activeLockedCollateral = devnetOrdersRef.current
          .filter((o) => o.status === 'Active' || o.status === 'InGracePeriod')
          .reduce((sum, o) => sum + (o.collateralName.includes('SOL') ? o.collateralAmount : 0), 0);
        const activeLockedSkr = devnetOrdersRef.current
          .filter((o) => o.status === 'Active' || o.status === 'InGracePeriod')
          .reduce((sum, o) => sum + (o.collateralName.includes('SKR') ? o.collateralAmount : 0), 0);

        const currentUsdc = parseFloat((assets.usdcBalance + activeBorrowAmount).toFixed(2));
        const currentSol = Math.max(0, parseFloat((assets.solBalance - activeLockedCollateral).toFixed(3)));
        const currentSkr = Math.max(0, parseFloat((assets.skrBalance - activeLockedSkr).toFixed(0)));
        const totalUsd = parseFloat((currentSol * 101.12 + currentUsdc + currentSkr * 0.0192).toFixed(2));

        setWalletAssets({
          ...assets,
          usdcBalance: currentUsdc,
          solBalance: currentSol,
          skrBalance: currentSkr,
          totalUsdValue: totalUsd,
        });
        setSolBalance(currentSol);
      } else {
        setWalletAssets(assets);
        setSolBalance(assets.solBalance);
      }
    } catch (e) {
      console.log('Error refreshing assets:', e);
    }
  };

  // Concurrent loading of protocol data
  const loadProtocolData = async (userPubkey: PublicKey, skrHandle: string, net: SolanaNetwork = selectedNetwork) => {
    try {
      setIsLoadingPools(true);
      const [livePools, profile, liveOrders, liveOffers] = await Promise.allSettled([
        fetchLivePools(),
        fetchLiveUserProfile(userPubkey, skrHandle),
        fetchLiveUserOrders(userPubkey),
        fetchLiveP2POffers(),
      ]);

      if (livePools.status === 'fulfilled') setPools(livePools.value);
      if (profile.status === 'fulfilled') setUserProfile(profile.value);

      if (liveOffers.status === 'fulfilled') {
        if (net === 'devnet') {
          const onChainOffers = liveOffers.value;
          const onChainIds = new Set(onChainOffers.map((o) => o.id));
          const sessionActive = devnetOffersRef.current.filter((o) => !onChainIds.has(o.id));
          const combined = [...sessionActive, ...onChainOffers];
          devnetOffersRef.current = combined;
          setOffers(combined);
        } else {
          setOffers([]);
        }
      }

      if (liveOrders.status === 'fulfilled') {
        if (net === 'devnet') {
          const onChainOrders = liveOrders.value;
          const onChainIds = new Set(onChainOrders.map((o) => o.id));
          const sessionActive = devnetOrdersRef.current.filter((o) => !onChainIds.has(o.id));
          const combined = [...onChainOrders, ...sessionActive];
          devnetOrdersRef.current = combined;
          setOrders(combined);
        } else {
          setOrders([]);
        }
      }
    } catch (e) {
      console.log('Error loading protocol data:', e);
    } finally {
      setIsLoadingPools(false);
    }
  };

  // When wallet connects or network changes, trigger instant asset query and parallel protocol fetch
  useEffect(() => {
    if (session?.publicKey) {
      refreshWalletAssets(session.publicKey, selectedNetwork);
      loadProtocolData(session.publicKey, session.skrHandle, selectedNetwork);
    }
  }, [session?.publicKey, selectedNetwork]);

  // Request Devnet SOL / USDC airdrop
  const handleAirdrop = async () => {
    if (!session) return;
    try {
      await requestDevnetAirdrop(session.publicKey);
      await refreshWalletAssets(session.publicKey, selectedNetwork);
      showToast('Devnet SOL & USDC Airdropped!');
    } catch (err: any) {
      Alert.alert('Funding Notice', err?.message || 'Faucet limit reached. Please wait a moment.');
    }
  };

  // Logout handler with sleek toast feedback (no annoying OS alert popup)
  const handleDisconnect = () => {
    setShowAssetsModal(false);
    const handle = session?.skrHandle ? `@${session.skrHandle}` : 'wallet';
    setSession(null);
    setOrders([]);
    devnetOrdersRef.current = [];
    devnetOffersRef.current = INITIAL_COMMUNITY_OFFERS;
    setOffers(INITIAL_COMMUNITY_OFFERS);
    setUserProfile(null);
    setSolBalance(0);
    setWalletAssets({
      network: 'devnet',
      solBalance: 0,
      usdcBalance: 0,
      skrBalance: 0,
      bonkBalance: 0,
      hasSeekerGenesisToken: false,
      totalUsdValue: 0,
      tokenList: [],
    });
    showToast(`Logged out of ${handle}`);
  };

  // Execute real Borrow transaction
  const handleBorrow = async (
    borrowAmount: number,
    collateralUnits: number,
    collateralName: string,
    pool: LendingPool
  ) => {
    if (!session) return;

    // Smart network routing: check if user is on Mainnet
    if (selectedNetwork === 'mainnet-beta') {
      Alert.alert(
        'Devnet Testing Mode',
        'ClockLend smart contracts are currently running on Solana Devnet (HAjGxuih14imCMaWvCnJQ3nSdWmS8PQKzp74gyAgjsH3).\n\nMainnet is currently read-only for asset balance tracking. Would you like to switch to Devnet to test borrowing and escrow locking?',
        [
          { text: 'Cancel', style: 'cancel' },
          {
            text: 'Switch to Devnet',
            onPress: () => {
              setSelectedNetwork('devnet');
              Alert.alert('Switched to Devnet', 'Network set to Devnet. You can now test instant borrowing.');
            },
          },
        ]
      );
      return;
    }

    const poolAuthority = new PublicKey(pool.authority);
    const collateralLamports = collateralName === 'SOL'
      ? Math.round(collateralUnits * 1_000_000_000)
      : Math.round(collateralUnits);
    const isPoolLiquid = pool.totalLiquidity >= borrowAmount;

    const { tx, escrowPDA, loanId } = await buildBorrowTx(
      session.publicKey,
      poolAuthority,
      pool.id,
      borrowAmount,
      collateralLamports,
      7,
      collateralName,
      isPoolLiquid
    );

    try {
      // 1. Sign transaction with Seeker Hardware / MWA
      const sig = await signAndSendSeekerTransaction(tx, session, selectedNetwork);
      console.log('Borrow tx confirmed on-chain:', sig);

      const solscanUrl = `https://solscan.io/tx/${sig}?cluster=devnet`;

      // 2. Create active loan order in state
      const interestDue = parseFloat((borrowAmount * (pool.interestRateBps / 10000) * (7 / 365)).toFixed(2));
      const newOrder: LoanOrder = {
        id: loanId,
        poolId: pool.id,
        poolName: pool.name,
        borrower: session.publicKey.toBase58(),
        principalAmount: borrowAmount,
        collateralName: `${collateralUnits} ${collateralName}`,
        collateralMint: collateralName === 'SOL' ? 'So11111111111111111111111111111111111111112' : 'SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3',
        collateralAmount: collateralUnits,
        interestDue,
        originationTime: Math.floor(Date.now() / 1000),
        dueTime: Math.floor(Date.now() / 1000) + 7 * 86400,
        gracePeriodExpires: 0,
        status: 'Active',
        txSignature: sig,
        escrowAddress: escrowPDA.toBase58(),
        solscanUrl,
      };

      devnetOrdersRef.current = [newOrder, ...devnetOrdersRef.current.filter((o) => o.id !== loanId)];
      setOrders(devnetOrdersRef.current);

      // 3. Update wallet assets (credit borrowed USDC, deduct locked collateral)
      setWalletAssets((prev) => {
        const currentUsdc = parseFloat((prev.usdcBalance + borrowAmount).toFixed(2));
        const currentSol = collateralName === 'SOL' ? Math.max(0, parseFloat((prev.solBalance - collateralUnits).toFixed(3))) : prev.solBalance;
        const currentSkr = collateralName === 'SKR' ? Math.max(0, parseFloat((prev.skrBalance - collateralUnits).toFixed(0))) : prev.skrBalance;
        return {
          ...prev,
          usdcBalance: currentUsdc,
          solBalance: currentSol,
          skrBalance: currentSkr,
          totalUsdValue: parseFloat((currentSol * 101.12 + currentUsdc + currentSkr * 0.0192).toFixed(2)),
        };
      });

      // 4. Show sleek production transaction notice
      setTransactionNotice({
        type: 'borrow',
        title: 'Loan Disbursed on Solana!',
        subtitle: `Received $${borrowAmount} USDC with ${collateralUnits} ${collateralName} locked in escrow.`,
        amount: `$${borrowAmount} USDC`,
        collateral: `${collateralUnits} ${collateralName}`,
        txSignature: sig,
        escrowAddress: escrowPDA.toBase58(),
        solscanUrl,
        primaryBtnText: 'View on Solscan ↗',
        secondaryBtnText: 'Go to Loans',
        onSecondaryPress: () => setActiveTab('LOANS'),
      });

      // 5. Immediately switch to LOANS tab
      setActiveTab('LOANS');
    } catch (err: any) {
      if (err?.message?.includes('Cancellation') || err?.name?.includes('Cancellation')) {
        setTransactionNotice({
          type: 'error',
          title: 'Borrow Cancelled',
          subtitle: 'Transaction was cancelled in your wallet.',
          primaryBtnText: 'Dismiss',
        });
        return;
      }
      console.warn('Borrow transaction failed:', err);
      setTransactionNotice({
        type: 'error',
        title: 'Transaction Notice',
        subtitle: err?.message || 'Could not complete transaction with wallet.',
        primaryBtnText: 'Dismiss',
      });
    }
  };

  // Execute real Repay transaction
  const handleRepay = async (order: LoanOrder) => {
    if (!session) return;

    const poolAuthority = new PublicKey(
      pools.find((p) => p.id === order.poolId)?.authority || 'BEmX1nfeZT5i4VpSEeZmhiYxpZ9z4Y1LQLjAtPR9c3re'
    );
    const totalDue = parseFloat((order.principalAmount + order.interestDue).toFixed(2));

    try {
      const tx = await buildRepayTx(
        session.publicKey,
        poolAuthority,
        order.poolId,
        order.id,
        totalDue,
        false
      );

      const sig = await signAndSendSeekerTransaction(tx, session, selectedNetwork);
      const solscanUrl = `https://solscan.io/tx/${sig}?cluster=devnet`;

      // Remove / mark order as repaid
      devnetOrdersRef.current = devnetOrdersRef.current.filter((o) => o.id !== order.id);
      setOrders(devnetOrdersRef.current);

      // Return collateral to wallet and deduct repaid USDC
      setWalletAssets((prev) => {
        const isSol = order.collateralName.includes('SOL');
        const isSkr = order.collateralName.includes('SKR');
        const currentUsdc = Math.max(0, parseFloat((prev.usdcBalance - totalDue).toFixed(2)));
        const currentSol = isSol ? parseFloat((prev.solBalance + order.collateralAmount).toFixed(3)) : prev.solBalance;
        const currentSkr = isSkr ? parseFloat((prev.skrBalance + order.collateralAmount).toFixed(0)) : prev.skrBalance;
        return {
          ...prev,
          usdcBalance: currentUsdc,
          solBalance: currentSol,
          skrBalance: currentSkr,
          totalUsdValue: parseFloat((currentSol * 101.12 + currentUsdc + currentSkr * 0.0192).toFixed(2)),
        };
      });

      // Boost credit score & reputation
      setUserProfile((prev) => {
        if (!prev) return null;
        return {
          ...prev,
          reputationScore: Math.min(100, prev.reputationScore + 5),
          aprDiscount: Math.min(2.5, parseFloat((prev.aprDiscount + 0.2).toFixed(1))),
        };
      });

      setTransactionNotice({
        type: 'repay',
        title: 'Loan Repaid & Released!',
        subtitle: `Successfully repaid $${totalDue} USDC. Your ${order.collateralName} has been unlocked from escrow back to your wallet.`,
        amount: `$${totalDue} USDC`,
        collateral: order.collateralName,
        reputationGain: 5,
        txSignature: sig,
        escrowAddress: order.escrowAddress,
        solscanUrl,
        primaryBtnText: 'View on Solscan ↗',
        secondaryBtnText: 'Done',
      });
    } catch (err: any) {
      if (err?.message?.includes('Cancellation') || err?.name?.includes('Cancellation')) {
        setTransactionNotice({
          type: 'error',
          title: 'Repay Cancelled',
          subtitle: 'Transaction was cancelled in your wallet.',
          primaryBtnText: 'Dismiss',
        });
        return;
      }
      setTransactionNotice({
        type: 'error',
        title: 'Repay Notice',
        subtitle: err?.message || 'Repayment failed. Please check your balance and try again.',
        primaryBtnText: 'Dismiss',
      });
    }
  };

  // Execute real on-chain P2P Pawn Listing transaction
  const handleCreatePawnOffer = async (
    name: string,
    amt: number,
    prof: number,
    days: number
  ) => {
    if (!session) return;

    if (selectedNetwork === 'mainnet-beta') {
      Alert.alert(
        'Devnet Testing Mode',
        'ClockLend smart contracts and P2P pawn escrows are currently running on Solana Devnet.\n\nWould you like to switch to Devnet to list this pawn offer?',
        [
          { text: 'Cancel', style: 'cancel' },
          { text: 'Switch to Devnet', onPress: () => setSelectedNetwork('devnet') },
        ]
      );
      return;
    }

    const offerId = Math.floor(1000 + Math.random() * 9000);

    try {
      const { tx, escrowPDA } = await buildCreateP2POfferTx(
        session.publicKey,
        offerId,
        name,
        amt,
        prof,
        days
      );

      const sig = await signAndSendSeekerTransaction(tx, session, selectedNetwork);
      console.log('P2P pawn offer created on-chain:', sig);
      const solscanUrl = `https://solscan.io/tx/${sig}?cluster=devnet`;

      const solMatch = name.match(/([0-9]*\.?[0-9]+)\s*SOL/i);
      const skrMatch = name.match(/([0-9]*\.?[0-9]+)\s*SKR/i);
      const parsedSol = solMatch ? parseFloat(solMatch[1]) : 0;
      const parsedSkr = skrMatch ? parseFloat(skrMatch[1]) : 0;
      const parsedUnits = solMatch ? parsedSol : (skrMatch ? parsedSkr : 1);
      const isNft = name.toLowerCase().includes('nft') || name.toLowerCase().includes('monke');

      const newOffer: P2POffer = {
        id: offerId,
        creator: session.publicKey.toBase58(),
        collateralName: name,
        collateralType: isNft ? 'NFT' : 'Token',
        collateralAmount: parsedUnits,
        requestedAmount: amt,
        interestOffered: prof,
        durationDays: days,
        createdAt: Math.floor(Date.now() / 1000),
        status: 'Open',
        txSignature: sig,
        escrowAddress: escrowPDA.toBase58(),
        solscanUrl,
      };

      devnetOffersRef.current = [newOffer, ...devnetOffersRef.current.filter((o) => o.id !== offerId)];
      setOffers(devnetOffersRef.current);

      // Deduct SOL or SKR for collateral or escrow rent
      setWalletAssets((prev) => {
        const deductSol = solMatch ? parsedSol : 0.005;
        const newSol = Math.max(0, parseFloat((prev.solBalance - deductSol).toFixed(3)));
        const newSkr = skrMatch ? Math.max(0, Math.round(prev.skrBalance - parsedSkr)) : prev.skrBalance;
        return {
          ...prev,
          solBalance: newSol,
          skrBalance: newSkr,
          totalUsdValue: parseFloat((newSol * 101.12 + prev.usdcBalance + newSkr * 0.0192).toFixed(2)),
        };
      });

      setTransactionNotice({
        type: 'borrow',
        title: 'P2P Pawn Listed On-Chain!',
        subtitle: `Asset "${name}" escrowed. Open for peer funding on Circle Deck.`,
        amount: `$${amt} USDC`,
        collateral: name,
        txSignature: sig,
        escrowAddress: escrowPDA.toBase58(),
        solscanUrl,
        primaryBtnText: 'View on Solscan ↗',
        secondaryBtnText: 'View P2P Desks',
      });
    } catch (err: any) {
      if (err?.message?.includes('Cancellation') || err?.name?.includes('Cancellation')) {
        setTransactionNotice({
          type: 'error',
          title: 'Listing Cancelled',
          subtitle: 'Transaction was cancelled in your wallet.',
          primaryBtnText: 'Dismiss',
        });
        return;
      }
      setTransactionNotice({
        type: 'error',
        title: 'Listing Notice',
        subtitle: err?.message || 'Failed to list pawn offer on-chain.',
        primaryBtnText: 'Dismiss',
      });
    }
  };

  // Execute real on-chain P2P Pawn Funding transaction
  const handleFundPawnOffer = async (offerId: number) => {
    if (!session) return;

    const targetOffer = offers.find((o) => o.id === offerId);
    if (!targetOffer) return;

    if (targetOffer.status !== 'Open') {
      Alert.alert('Offer Unavailable', 'This pawn offer is already funded or closed.');
      return;
    }

    try {
      const tx = await buildFundP2POfferTx(session.publicKey, targetOffer);
      const sig = await signAndSendSeekerTransaction(tx, session, selectedNetwork);
      console.log('P2P pawn funded on-chain:', sig);
      const solscanUrl = `https://solscan.io/tx/${sig}?cluster=devnet`;

      devnetOffersRef.current = devnetOffersRef.current.map((o) =>
        o.id === offerId
          ? {
              ...o,
              status: 'Funded' as OfferStatus,
              funder: session.publicKey.toBase58(),
              txSignature: sig,
              solscanUrl,
            }
          : o
      );
      setOffers([...devnetOffersRef.current]);

      // Deduct funded principal from user's USDC balance
      setWalletAssets((prev) => {
        const newUsdc = Math.max(0, parseFloat((prev.usdcBalance - targetOffer.requestedAmount).toFixed(2)));
        return {
          ...prev,
          usdcBalance: newUsdc,
          totalUsdValue: parseFloat((prev.solBalance * 101.12 + newUsdc + prev.skrBalance * 0.0192).toFixed(2)),
        };
      });

      // Reward lender reputation score
      setUserProfile((prev) => {
        if (!prev) return null;
        return {
          ...prev,
          reputationScore: Math.min(100, prev.reputationScore + 10),
        };
      });

      setTransactionNotice({
        type: 'repay',
        title: 'P2P Pawn Funded!',
        subtitle: `Funded $${targetOffer.requestedAmount} USDC for Pawn #${offerId}. You will receive +$${targetOffer.interestOffered} USDC yield upon borrower repayment.`,
        amount: `$${targetOffer.requestedAmount} USDC`,
        collateral: targetOffer.collateralName,
        txSignature: sig,
        solscanUrl,
        reputationGain: 10,
        primaryBtnText: 'View on Solscan ↗',
        secondaryBtnText: 'Done',
      });
    } catch (err: any) {
      if (err?.message?.includes('Cancellation') || err?.name?.includes('Cancellation')) {
        setTransactionNotice({
          type: 'error',
          title: 'Funding Cancelled',
          subtitle: 'Transaction was cancelled in your wallet.',
          primaryBtnText: 'Dismiss',
        });
        return;
      }
      setTransactionNotice({
        type: 'error',
        title: 'Funding Notice',
        subtitle: err?.message || 'Failed to fund pawn offer on-chain.',
        primaryBtnText: 'Dismiss',
      });
    }
  };

  // Execute real on-chain P2P Pawn Repay & Collateral Unlock
  const handleRepayPawnOffer = async (offer: P2POffer) => {
    if (!session) return;

    const totalDue = parseFloat((offer.requestedAmount + offer.interestOffered).toFixed(2));
    const solMatch = offer.collateralName.match(/([0-9]*\.?[0-9]+)\s*SOL/i);
    const skrMatch = offer.collateralName.match(/([0-9]*\.?[0-9]+)\s*SKR/i);
    const returnSol = solMatch ? parseFloat(solMatch[1]) : 0;
    const returnSkr = skrMatch ? parseFloat(skrMatch[1]) : 0;

    try {
      const tx = await buildRepayPawnOfferTx(session.publicKey, offer);
      const sig = await signAndSendSeekerTransaction(tx, session, selectedNetwork);
      const solscanUrl = `https://solscan.io/tx/${sig}?cluster=devnet`;

      // Update offer status to 'Repaid'
      devnetOffersRef.current = devnetOffersRef.current.map((o) =>
        o.id === offer.id ? { ...o, status: 'Repaid' as OfferStatus, solscanUrl, txSignature: sig } : o
      );
      setOffers([...devnetOffersRef.current]);

      // Release collateral back to user's wallet and deduct repaid USDC
      setWalletAssets((prev) => {
        const newUsdc = Math.max(0, parseFloat((prev.usdcBalance - totalDue).toFixed(2)));
        const newSol = parseFloat((prev.solBalance + returnSol).toFixed(3));
        const newSkr = Math.round(prev.skrBalance + returnSkr);
        return {
          ...prev,
          usdcBalance: newUsdc,
          solBalance: newSol,
          skrBalance: newSkr,
          totalUsdValue: parseFloat((newSol * 101.12 + newUsdc + newSkr * 0.0192).toFixed(2)),
        };
      });

      setUserProfile((prev) => {
        if (!prev) return null;
        return {
          ...prev,
          reputationScore: Math.min(100, prev.reputationScore + 10),
          totalLoansCompleted: prev.totalLoansCompleted + 1,
        };
      });

      setTransactionNotice({
        type: 'repay',
        title: 'Pawn Repaid & Collateral Unlocked!',
        subtitle: `Repaid $${totalDue} USDC. Your ${offer.collateralName} has been unlocked from the Escrow PDA and returned to your wallet.`,
        amount: `$${totalDue} USDC`,
        collateral: offer.collateralName,
        txSignature: sig,
        escrowAddress: offer.escrowAddress,
        solscanUrl,
        primaryBtnText: 'View on Solscan ↗',
        secondaryBtnText: 'Done',
      });
    } catch (err: any) {
      if (err?.message?.includes('Cancellation') || err?.name?.includes('Cancellation')) {
        setTransactionNotice({
          type: 'error',
          title: 'Repayment Cancelled',
          subtitle: 'Transaction was cancelled in your wallet.',
          primaryBtnText: 'Dismiss',
        });
        return;
      }
      setTransactionNotice({
        type: 'error',
        title: 'Repayment Notice',
        subtitle: err?.message || 'Failed to complete pawn repayment.',
        primaryBtnText: 'Dismiss',
      });
    }
  };

  // Cancel P2P Pawn and withdraw collateral
  const handleCancelPawnOffer = async (offer: P2POffer) => {
    if (!session) return;

    const solMatch = offer.collateralName.match(/([0-9]*\.?[0-9]+)\s*SOL/i);
    const skrMatch = offer.collateralName.match(/([0-9]*\.?[0-9]+)\s*SKR/i);
    const returnSol = solMatch ? parseFloat(solMatch[1]) : (skrMatch ? 0 : 0.005);
    const returnSkr = skrMatch ? parseFloat(skrMatch[1]) : 0;

    try {
      const tx = await buildCancelPawnOfferTx(session.publicKey, offer);
      const sig = await signAndSendSeekerTransaction(tx, session, selectedNetwork);
      const solscanUrl = `https://solscan.io/tx/${sig}?cluster=devnet`;

      // Remove offer from active list
      devnetOffersRef.current = devnetOffersRef.current.filter((o) => o.id !== offer.id);
      setOffers([...devnetOffersRef.current]);

      // Return collateral to wallet
      setWalletAssets((prev) => {
        const newSol = parseFloat((prev.solBalance + returnSol).toFixed(3));
        const newSkr = Math.round(prev.skrBalance + returnSkr);
        return {
          ...prev,
          solBalance: newSol,
          skrBalance: newSkr,
          totalUsdValue: parseFloat((newSol * 101.12 + prev.usdcBalance + newSkr * 0.0192).toFixed(2)),
        };
      });

      setTransactionNotice({
        type: 'success',
        title: 'Pawn Cancelled',
        subtitle: `Your ${offer.collateralName} has been unlocked from escrow back to your wallet.`,
        amount: `${returnSol} SOL`,
        collateral: offer.collateralName,
        txSignature: sig,
        solscanUrl,
        primaryBtnText: 'View on Solscan ↗',
        secondaryBtnText: 'Done',
      });
    } catch (err: any) {
      if (err?.message?.includes('Cancellation') || err?.name?.includes('Cancellation')) {
        return;
      }
      setTransactionNotice({
        type: 'error',
        title: 'Cancel Notice',
        subtitle: err?.message || 'Failed to cancel pawn offer.',
        primaryBtnText: 'Dismiss',
      });
    }
  };

  // Execute real on-chain InitializePool transaction for Individual / Circle
  const handleCreatePool = async (
    name: string,
    poolType: 'Individual' | 'Circle',
    aprPercent: number,
    maxLtvPercent: number,
    minDays: number,
    maxDays: number,
    initialLiquidity: number
  ) => {
    if (!session) return;

    if (selectedNetwork === 'mainnet-beta') {
      Alert.alert(
        'Devnet Testing Mode',
        'ClockLend smart contracts are currently running on Solana Devnet.\n\nWould you like to switch to Devnet to create this lending desk?',
        [
          { text: 'Cancel', style: 'cancel' },
          { text: 'Switch to Devnet', onPress: () => setSelectedNetwork('devnet') },
        ]
      );
      return;
    }

    const poolId = Math.floor(100 + Math.random() * 900);
    const interestRateBps = Math.round(aprPercent * 100);
    const maxLtvBps = Math.round(maxLtvPercent * 100);

    try {
      const { tx, poolPDA } = await buildCreatePoolTx(
        session.publicKey,
        poolId,
        poolType,
        name,
        interestRateBps,
        maxLtvBps,
        minDays,
        maxDays
      );

      const sig = await signAndSendSeekerTransaction(tx, session, selectedNetwork);
      console.log('Lending pool created on-chain:', sig);
      const solscanUrl = `https://solscan.io/tx/${sig}?cluster=devnet`;

      const newPool: LendingPool = {
        id: poolId,
        poolType,
        authority: session.publicKey.toBase58(),
        name,
        liquidityMint: 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v',
        totalLiquidity: initialLiquidity,
        totalBorrowed: 0,
        stakedSkrAmount: poolType === 'Individual' ? 500 : 2500,
        interestRateBps,
        maxLtvBps,
        minDurationDays: minDays,
        maxDurationDays: maxDays,
        loansOriginated: 0,
        loansRepaid: 0,
        successRate: 100,
        isVerifiedMerchant: false,
      };

      setPools((prev) => [newPool, ...prev.filter((p) => p.id !== poolId)]);

      // Adjust mock wallet assets for rent-exemption fees
      setWalletAssets((prev) => {
        const deduct = 0.005;
        const newSol = Math.max(0, parseFloat((prev.solBalance - deduct).toFixed(3)));
        return {
          ...prev,
          solBalance: newSol,
          totalUsdValue: parseFloat((newSol * 101.12 + prev.usdcBalance + prev.skrBalance * 0.0192).toFixed(2)),
        };
      });

      setTransactionNotice({
        type: 'borrow',
        title: 'Lending Desk Initialized!',
        subtitle: `"${name}" (${poolType}) is now live on Solana Devnet. Borrowers can now request loans directly against your desk!`,
        amount: `$${initialLiquidity} USDC Capacity`,
        collateral: `${aprPercent.toFixed(1)}% APR • ${maxLtvPercent.toFixed(0)}% Max LTV`,
        txSignature: sig,
        escrowAddress: poolPDA.toBase58(),
        solscanUrl,
        primaryBtnText: 'View on Solscan ↗',
        secondaryBtnText: 'Dismiss',
      });
    } catch (err: any) {
      if (err?.message?.includes('Cancellation') || err?.name?.includes('Cancellation')) {
        setTransactionNotice({
          type: 'error',
          title: 'Initialization Cancelled',
          subtitle: 'Transaction was cancelled in your wallet.',
          primaryBtnText: 'Dismiss',
        });
        return;
      }
      console.warn('InitializePool transaction failed:', err);
      setTransactionNotice({
        type: 'error',
        title: 'Initialization Notice',
        subtitle: err?.message || 'Could not initialize pool on-chain.',
        primaryBtnText: 'Dismiss',
      });
    }
  };

  // Execute real on-chain Stake SKR Reputation Bond transaction
  const handleStakeSkr = async (amount: number) => {
    if (!session) return;

    if (selectedNetwork === 'mainnet-beta') {
      Alert.alert(
        'Devnet Testing Mode',
        'ClockLend smart contracts and reputation escrows are running on Solana Devnet.\n\nWould you like to switch to Devnet to stake your SKR bond?',
        [
          { text: 'Cancel', style: 'cancel' },
          { text: 'Switch to Devnet', onPress: () => setSelectedNetwork('devnet') },
        ]
      );
      return;
    }

    try {
      const { tx, profilePDA, escrowPDA } = await buildStakeSkrTx(session.publicKey, amount);
      const sig = await signAndSendSeekerTransaction(tx, session, selectedNetwork);
      console.log('SKR staked on-chain:', sig);
      const solscanUrl = `https://solscan.io/tx/${sig}?cluster=devnet`;

      // Update user profile reputation & tier
      setUserProfile((prev) => {
        const currentStaked = (prev?.stakedSkr || 0) + amount;
        let newTier: 'Diamond' | 'Gold' | 'Silver' | 'Standard' = 'Standard';
        if (currentStaked >= 5000) newTier = 'Diamond';
        else if (currentStaked >= 2500) newTier = 'Gold';
        else if (currentStaked >= 1000) newTier = 'Silver';

        const currentScore = prev?.reputationScore || 70;
        const newScore = Math.min(100, currentScore + Math.max(5, Math.floor(amount / 200)));
        const newDiscount = Math.min(3.0, parseFloat(((prev?.aprDiscount || 0) + 0.5).toFixed(1)));

        return {
          pubkey: session.publicKey.toBase58(),
          stakedSkr: currentStaked,
          totalLoansCompleted: prev?.totalLoansCompleted || 0,
          totalLoansDefaulted: prev?.totalLoansDefaulted || 0,
          reputationScore: newScore,
          tier: newTier,
          aprDiscount: newDiscount,
        };
      });

      // Deduct staked SKR and 0.002 SOL rent
      setWalletAssets((prev) => {
        const newSkr = Math.max(0, Math.round(prev.skrBalance - amount));
        const newSol = Math.max(0, parseFloat((prev.solBalance - 0.002).toFixed(3)));
        return {
          ...prev,
          skrBalance: newSkr,
          solBalance: newSol,
          totalUsdValue: parseFloat((newSol * 101.12 + prev.usdcBalance + newSkr * 0.0192).toFixed(2)),
        };
      });

      setTransactionNotice({
        type: 'borrow',
        title: '💎 SKR Reputation Bond Staked!',
        subtitle: `Staked ${amount.toLocaleString()} SKR into Protocol Escrow (${escrowPDA.toBase58().slice(0, 8)}...). Your credit score, 90% LTV, and tier discount are now active on-chain!`,
        amount: `${amount.toLocaleString()} SKR`,
        collateral: 'Seeker Reputation Escrow',
        txSignature: sig,
        escrowAddress: escrowPDA.toBase58(),
        solscanUrl,
        primaryBtnText: 'View on Solscan ↗',
        secondaryBtnText: 'Done',
      });
    } catch (err: any) {
      if (err?.message?.includes('Cancellation') || err?.name?.includes('Cancellation')) {
        setTransactionNotice({
          type: 'error',
          title: 'Staking Cancelled',
          subtitle: 'Transaction was cancelled in your wallet.',
          primaryBtnText: 'Dismiss',
        });
        return;
      }
      console.warn('Stake SKR failed:', err);
      setTransactionNotice({
        type: 'error',
        title: 'Staking Notice',
        subtitle: err?.message || 'Could not complete SKR reputation bond on-chain.',
        primaryBtnText: 'Dismiss',
      });
    }
  };

  // 1. Splash Screen
  if (showSplash) {
    return <SplashScreenView onFinish={() => setShowSplash(false)} />;
  }

  // 2. Security Lock Screen (Biometrics & Custom PIN)
  if (isLocked) {
    return (
      <SecurityLockScreen
        mode={lockScreenMode}
        onUnlock={() => setIsLocked(false)}
        onCancel={lockScreenMode !== 'unlock' ? () => setIsLocked(false) : undefined}
      />
    );
  }

  // 3. If no wallet connected, show the Seeker Onboarding Gate
  if (!session) {
    return (
      <SafeAreaView style={[styles.container, { backgroundColor: colors.background }]}>
        <StatusBar style={mode === 'dark' ? 'light' : 'dark'} />
        <ConnectWalletView
          onConnected={(newSession) => {
            setSession(newSession);
          }}
        />
        {toastMessage && (
          <View
            style={[styles.toastContainer, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}
            pointerEvents="none"
          >
            <Ionicons name="checkmark-circle" size={18} color={colors.primary} style={{ marginRight: 8 }} />
            <Text style={[styles.toastText, { color: colors.text }]}>{toastMessage}</Text>
          </View>
        )}
      </SafeAreaView>
    );
  }

  const activeCount = orders.filter((o) => o.status.toUpperCase().includes('ACTIVE') || o.status.toUpperCase().includes('GRACE')).length;

  const currentProfile: UserProfile = userProfile || {
    pubkey: session.publicKey.toBase58(),
    stakedSkr: 0,
    totalLoansCompleted: 0,
    totalLoansDefaulted: 0,
    reputationScore: 10000,
    tier: 'Silver',
    aprDiscount: 0,
  };

  const handleSwitchAddress = async (newPubkey: PublicKey) => {
    const newSkr = await deriveSkrUsername(newPubkey);
    const newSession: SeekerSession = {
      publicKey: newPubkey,
      skrHandle: newSkr,
      isSeekerGenesisVerified: true,
    };
    setSession(newSession);
    await refreshWalletAssets(newPubkey, selectedNetwork);
    await loadProtocolData(newPubkey, newSkr);
  };

  return (
    <SafeAreaView style={[styles.container, { backgroundColor: colors.background }]}>
      <StatusBar style={mode === 'dark' ? 'light' : 'dark'} />

      {/* Top Header */}
      <Header
        skrHandle={session.skrHandle}
        solBalance={solBalance}
        network={selectedNetwork}
        onPressProfile={() => setActiveTab('PROFILE')}
        onPressBalance={() => setShowAssetsModal(true)}
        onToggleNetwork={() =>
          setSelectedNetwork((prev) => (prev === 'devnet' ? 'mainnet-beta' : 'devnet'))
        }
        onDisconnectWallet={handleDisconnect}
      />

      {/* Main Content Area */}
      <View style={styles.body}>
        {activeTab === 'BORROW' && (
          <P2PExpressView
            pools={pools}
            userProfile={currentProfile}
            walletAssets={walletAssets}
            onBorrow={handleBorrow}
            onRequestAirdrop={handleAirdrop}
            isLoadingPools={isLoadingPools}
          />
        )}

        {activeTab === 'MARKET' && (
          <MerchantDesksView
            pools={pools}
            offers={offers}
            userPubkey={session.publicKey.toBase58()}
            onSelectPool={(pool) => {
              setActiveTab('BORROW');
            }}
            onFundPawnOffer={handleFundPawnOffer}
            onCreatePawnOffer={handleCreatePawnOffer}
            onRepayPawnOffer={handleRepayPawnOffer}
            onCancelPawnOffer={handleCancelPawnOffer}
            onCreatePool={handleCreatePool}
            onNfcBumpCircle={() => {
              loadProtocolData(session.publicKey, session.skrHandle);
            }}
          />
        )}

        {activeTab === 'LOANS' && (
          <ActiveOrdersView
            orders={orders}
            onRepay={handleRepay}
            onTriggerGrace={(orderId) => {
              devnetOrdersRef.current = devnetOrdersRef.current.map((o) =>
                o.id === orderId ? { ...o, status: 'InGracePeriod' } : o
              );
              setOrders(devnetOrdersRef.current);
              setTransactionNotice({
                type: 'grace',
                title: 'Social Grace Activated',
                subtitle: '24-hour grace window started on-chain. Circle peers have priority buyout rights before any liquidation.',
                primaryBtnText: 'Understood',
              });
            }}
            onNavigateBorrow={() => setActiveTab('BORROW')}
          />
        )}

        {activeTab === 'PROFILE' && (
          <CreditProfileView
            userProfile={currentProfile}
            skrHandle={session.skrHandle}
            walletAssets={walletAssets}
            onStakeSkr={handleStakeSkr}
            onOpenAssetsModal={() => setShowAssetsModal(true)}
            onDisconnectWallet={handleDisconnect}
            onLockApp={() => {
              setLockScreenMode('unlock');
              setIsLocked(true);
            }}
            onSetupPin={() => {
              setLockScreenMode('setup');
              setIsLocked(true);
            }}
            onChangePin={() => {
              setLockScreenMode('change_pin');
              setIsLocked(true);
            }}
          />
        )}
      </View>

      {/* Modern Bottom Navigation Bar */}
      <View style={[styles.tabBar, { backgroundColor: colors.card, borderTopColor: colors.cardBorder }]}>
        <TouchableOpacity
          style={styles.tabItem}
          onPress={() => setActiveTab('BORROW')}
          activeOpacity={0.7}
        >
          <Ionicons
            name={activeTab === 'BORROW' ? 'flash' : 'flash-outline'}
            size={22}
            color={activeTab === 'BORROW' ? colors.primary : colors.textSecondary}
          />
          <Text
            style={[
              styles.tabLabel,
              { color: colors.textSecondary },
              activeTab === 'BORROW' && { color: colors.primary, fontWeight: '800' },
            ]}
          >
            Borrow
          </Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.tabItem}
          onPress={() => setActiveTab('MARKET')}
          activeOpacity={0.7}
        >
          <Ionicons
            name={activeTab === 'MARKET' ? 'storefront' : 'storefront-outline'}
            size={22}
            color={activeTab === 'MARKET' ? colors.primary : colors.textSecondary}
          />
          <Text
            style={[
              styles.tabLabel,
              { color: colors.textSecondary },
              activeTab === 'MARKET' && { color: colors.primary, fontWeight: '800' },
            ]}
          >
            P2P Desks
          </Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.tabItem}
          onPress={() => setActiveTab('LOANS')}
          activeOpacity={0.7}
        >
          <View style={{ position: 'relative' }}>
            <Ionicons
              name={activeTab === 'LOANS' ? 'time' : 'time-outline'}
              size={22}
              color={activeTab === 'LOANS' ? colors.primary : colors.textSecondary}
            />
            {activeCount > 0 && (
              <View style={[styles.tabBadge, { backgroundColor: colors.accent }]}>
                <Text style={styles.tabBadgeText}>{activeCount}</Text>
              </View>
            )}
          </View>
          <Text
            style={[
              styles.tabLabel,
              { color: colors.textSecondary },
              activeTab === 'LOANS' && { color: colors.primary, fontWeight: '800' },
            ]}
          >
            Active Loans
          </Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.tabItem}
          onPress={() => setActiveTab('PROFILE')}
          activeOpacity={0.7}
        >
          <Ionicons
            name={activeTab === 'PROFILE' ? 'person-circle' : 'person-circle-outline'}
            size={23}
            color={activeTab === 'PROFILE' ? colors.primary : colors.textSecondary}
          />
          <Text
            style={[
              styles.tabLabel,
              { color: colors.textSecondary },
              activeTab === 'PROFILE' && { color: colors.primary, fontWeight: '800' },
            ]}
          >
            Account
          </Text>
        </TouchableOpacity>
      </View>

      {/* Detailed Wallet Assets & Holdings Modal */}
      <WalletAssetsModal
        visible={showAssetsModal}
        onClose={() => setShowAssetsModal(false)}
        walletAddress={session.publicKey}
        skrHandle={session.skrHandle}
        assets={walletAssets}
        network={selectedNetwork}
        onSelectNetwork={(net) => setSelectedNetwork(net)}
        onRefresh={() => refreshWalletAssets(session.publicKey, selectedNetwork)}
        onSwitchAddress={handleSwitchAddress}
        onDisconnect={handleDisconnect}
      />

      {/* Production-Grade Transaction Notice & Notification Modal */}
      <TransactionNoticeModal
        visible={!!transactionNotice}
        data={transactionNotice}
        onClose={() => setTransactionNotice(null)}
      />

      {/* Floating In-App Toast Notification */}
      {toastMessage && (
        <View
          style={[styles.toastContainer, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}
          pointerEvents="none"
        >
          <Ionicons name="checkmark-circle" size={18} color={colors.primary} style={{ marginRight: 8 }} />
          <Text style={[styles.toastText, { color: colors.text }]}>{toastMessage}</Text>
        </View>
      )}
    </SafeAreaView>
  );
}

export default function App() {
  return (
    <ThemeProvider>
      <MainApp />
    </ThemeProvider>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  body: {
    flex: 1,
  },
  tabBar: {
    flexDirection: 'row',
    borderTopWidth: 1,
    paddingVertical: 12,
    paddingBottom: 22,
    paddingHorizontal: 8,
    justifyContent: 'space-around',
    alignItems: 'center',
  },
  tabItem: {
    alignItems: 'center',
    flex: 1,
  },
  tabIcon: {
    fontSize: 22,
    marginBottom: 4,
    opacity: 0.6,
  },
  tabLabel: {
    fontSize: 11,
    fontWeight: '600',
  },
  tabBadge: {
    position: 'absolute',
    top: -2,
    right: -8,
    width: 16,
    height: 16,
    borderRadius: 8,
    justifyContent: 'center',
    alignItems: 'center',
  },
  tabBadgeText: {
    color: '#FFFFFF',
    fontSize: 10,
    fontWeight: '900',
  },
  toastContainer: {
    position: 'absolute',
    bottom: 84,
    alignSelf: 'center',
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: 12,
    paddingHorizontal: 20,
    borderRadius: 24,
    borderWidth: 1,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.25,
    shadowRadius: 8,
    elevation: 8,
    zIndex: 9999,
  },
  toastText: {
    fontSize: 14,
    fontWeight: '700',
  },
});
