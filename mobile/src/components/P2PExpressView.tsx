import React, { useState, useEffect } from 'react';
import { View, Text, StyleSheet, TouchableOpacity, TextInput, Alert, ScrollView, ActivityIndicator, Image } from 'react-native';
import { useTheme } from '../theme/ThemeContext';
import { LendingPool, UserProfile, WalletAssets } from '../types';
import { livePrices, fetchLivePrices, subscribeToPriceUpdates, getPriceSource } from '../solana/onChainService';

const SOL_LOGO = require('../../assets/tokens/sol.png');
const SKR_LOGO = require('../../assets/tokens/skr.png');

interface P2PExpressViewProps {
  pools: LendingPool[];
  userProfile: UserProfile;
  walletAssets?: WalletAssets;
  onBorrow: (borrowAmount: number, collateralUnits: number, collateralName: string, pool: LendingPool, durationDays: number) => void;
  onRequestAirdrop?: () => void;
  isLoadingPools?: boolean;
}

export const P2PExpressView: React.FC<P2PExpressViewProps> = ({
  pools,
  userProfile,
  walletAssets,
  onBorrow,
  onRequestAirdrop,
  isLoadingPools = false,
}) => {
  const { colors, mode } = useTheme();
  const [amountStr, setAmountStr] = useState<string>('50');
  const [collateralType, setCollateralType] = useState<'SKR' | 'SOL'>('SKR');
  const [durationDays, setDurationDays] = useState<number>(7);
  const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
  const [prices, setPrices] = useState(livePrices);
  // Collateral sizing MUST use a live price: sizing with a hardcoded price
  // reverts the borrow when the market moves up and silently over-locks when
  // it moves down. If the price source is unavailable or only baseline-fallback, borrowing is disabled.
  const [priceAvailable, setPriceAvailable] = useState<boolean>(false);

  useEffect(() => {
    fetchLivePrices()
      .then((p) => {
        setPrices({ ...p });
        const src = getPriceSource();
        setPriceAvailable(src !== 'fallback-baseline');
      })
      .catch(() => setPriceAvailable(false));

    // Real-time Helius LaserStream WebSocket subscription
    const unsubscribe = subscribeToPriceUpdates((updated, source) => {
      setPrices({ ...updated });
      setPriceAvailable(source !== 'fallback-baseline');
    });
    return () => unsubscribe();
  }, []);

  const numAmount = parseFloat(amountStr) || 0;

  // Select lowest APR on-chain pool
  const bestPool = pools.length > 0
    ? pools.reduce((min, p) => (p.interestRateBps < min.interestRateBps ? p : min), pools[0])
    : null;

  const solBalance = walletAssets?.solBalance || 0;
  const skrBalance = walletAssets?.skrBalance || 0;
  const solHolding = walletAssets?.tokenList?.find((t) => t.symbol === 'SOL');
  const skrHolding = walletAssets?.tokenList?.find((t) => t.symbol === 'SKR');

  // Real-time on-chain prices pushed over Helius WebSocket (with Jupiter / CoinGecko fallbacks)
  const solPrice = prices.sol;
  const skrPrice = prices.skr;

  const collateralPrice = collateralType === 'SOL' ? solPrice : skrPrice;
  const userBalance = collateralType === 'SOL' ? solBalance : skrBalance;

  const ltv = (bestPool ? bestPool.maxLtvBps : 8500) / 10000;
  const rawCollateral = numAmount > 0 ? (numAmount / ltv) / collateralPrice : 0;
  const isDecimal = collateralType === 'SOL';
  const requiredCollateralUnits = isDecimal ? rawCollateral : Math.ceil(rawCollateral);

  const isInsufficientCollateral = requiredCollateralUnits > userBalance;

  const baseApr = bestPool ? bestPool.interestRateBps / 100 : 3.5;
  const effectiveApr = baseApr * (1 - userProfile.aprDiscount / 100);
  const estInterest = numAmount * (effectiveApr / 100) * (durationDays / 365);

  const handleBorrow = async () => {
    if (numAmount <= 0) {
      Alert.alert('Invalid Amount', 'Please enter a valid loan amount.');
      return;
    }

    if (!bestPool) {
      Alert.alert('No Pools Available', 'Loading available lending pools...');
      return;
    }

    if (isInsufficientCollateral) {
      Alert.alert(
        'Insufficient Collateral',
        `You need ${requiredCollateralUnits.toFixed(collateralType === 'SOL' ? 3 : 0)} ${collateralType}, but your connected Seeker wallet holds ${userBalance.toFixed(collateralType === 'SOL' ? 3 : 0)} ${collateralType}.`
      );
      return;
    }

    setIsSubmitting(true);
    try {
      const collUnits = collateralType === 'SOL'
        ? parseFloat(requiredCollateralUnits.toFixed(3))
        : Math.ceil(requiredCollateralUnits);
      await onBorrow(
        numAmount,
        collUnits,
        collateralType,
        bestPool,
        durationDays
      );
    } catch (e: any) {
      Alert.alert('Transaction Notice', e?.message || 'Failed to submit borrow transaction');
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <ScrollView
      style={[styles.container, { backgroundColor: colors.background }]}
      contentContainerStyle={styles.content}
      showsVerticalScrollIndicator={false}
    >
      {/* Hero Header */}
      <View style={[styles.heroSection, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
        <View style={styles.badgeRow}>
          <View style={[styles.liveBadge, { backgroundColor: colors.badgeBg, borderColor: colors.badgeBorder }]}>
            <View style={[styles.liveDot, { backgroundColor: colors.primary }]} />
            <Text style={[styles.liveBadgeText, { color: colors.primary }]}>BEST AVAILABLE ROUTE</Text>
          </View>
        </View>

        <Text style={[styles.heroSub, { color: colors.textSecondary }]}>P2P EXPRESS BORROW</Text>
        <View style={styles.amountRow}>
          <Text style={[styles.currencyPrefix, { color: colors.primary }]}>$</Text>
          <TextInput
            style={[styles.heroInput, { color: colors.text }]}
            value={amountStr}
            onChangeText={setAmountStr}
            keyboardType="numeric"
            placeholder="0"
            placeholderTextColor={colors.textMuted}
          />
          <Text style={[styles.currencySuffix, { color: colors.textSecondary }]}>USDC</Text>
        </View>

        {/* Amount Quick Presets */}
        <View style={styles.presetsRow}>
          {['25', '50', '100', '250'].map((val) => (
            <TouchableOpacity
              key={val}
              style={[
                styles.presetChip,
                { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder },
                amountStr === val && { backgroundColor: colors.primary, borderColor: colors.primary },
              ]}
              onPress={() => setAmountStr(val)}
              activeOpacity={0.7}
            >
              <Text
                style={[
                  styles.presetText,
                  { color: colors.textSecondary },
                  amountStr === val && { color: colors.primaryText, fontWeight: '800' },
                ]}
              >
                ${val}
              </Text>
            </TouchableOpacity>
          ))}
        </View>
      </View>

      {/* Collateral Selection Card */}
      <View style={[styles.sectionCard, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
        <View style={styles.sectionHeaderRow}>
          <Text style={[styles.sectionHeader, { color: colors.textSecondary }]}>COLLATERAL TO ESCROW</Text>
          {userBalance > 0 && (
            <TouchableOpacity
              style={[styles.maxBadge, { backgroundColor: colors.badgeBg }]}
              onPress={() => {
                const maxUsdc = Math.max(10, Math.floor(userBalance * 0.9 * collateralPrice * ltv));
                setAmountStr(maxUsdc.toString());
              }}
              activeOpacity={0.7}
            >
              <Text style={[styles.maxBadgeText, { color: colors.primary }]}>BORROW MAX</Text>
            </TouchableOpacity>
          )}
        </View>

        <View style={styles.collateralTabs}>
          {[
            {
              id: 'SKR' as const,
              label: 'SKR',
              logo: SKR_LOGO,
              sub: 'Stake for yield',
            },
            {
              id: 'SOL' as const,
              label: 'SOL',
              logo: SOL_LOGO,
              sub: `${solBalance.toFixed(2)} avail`,
            },
          ].map((item) => (
            <TouchableOpacity
              key={item.id}
              style={[
                styles.collateralChip,
                { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder },
                collateralType === item.id && { borderColor: colors.primary, backgroundColor: colors.badgeBg },
              ]}
              onPress={() => setCollateralType(item.id)}
              activeOpacity={0.7}
            >
              <Image source={item.logo} style={styles.collateralLogo} resizeMode="contain" />
              <Text
                style={[
                  styles.collateralLabel,
                  { color: colors.text },
                  collateralType === item.id && { color: colors.primary, fontWeight: '800' },
                ]}
              >
                {item.label}
              </Text>
              <Text style={[styles.collateralSub, { color: colors.textSecondary }]}>{item.sub}</Text>
            </TouchableOpacity>
          ))}
        </View>

        {/* Dynamic Collateral Required Display */}
        <View style={[styles.collateralDisplay, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
          <View>
            <Text style={[styles.calcLabel, { color: colors.textSecondary }]}>Required Escrow Deposit</Text>
            <View style={styles.calcValueRow}>
              <Image
                source={collateralType === 'SKR' ? SKR_LOGO : SOL_LOGO}
                style={styles.calcMiniLogo}
                resizeMode="contain"
              />
              <Text style={[styles.calcValue, { color: colors.text }]}>
                {requiredCollateralUnits > 0
                  ? `${requiredCollateralUnits.toFixed(isDecimal ? 3 : 0)} ${collateralType}`
                  : '—'}
              </Text>
            </View>
          </View>
          <View style={styles.calcRight}>
            <Text style={[styles.calcSubLabel, { color: colors.textMuted }]}>Wallet Holdings</Text>
            <View style={styles.calcValueRow}>
              <Image
                source={collateralType === 'SKR' ? SKR_LOGO : SOL_LOGO}
                style={styles.calcMiniLogo}
                resizeMode="contain"
              />
              <Text style={[styles.calcSubValue, { color: isInsufficientCollateral ? colors.danger : colors.primary }]}>
                {`${userBalance.toFixed(isDecimal ? 2 : 0)} ${collateralType}`}
              </Text>
            </View>
          </View>
        </View>

        {/* Collateral Yield Bonus Banner */}
        <View style={{ marginTop: 6, marginBottom: 8, paddingHorizontal: 4 }}>
          {collateralType === 'SKR' ? (
            <Text style={{ fontSize: 11, color: colors.primary, fontWeight: '600' }}>
              🔒 Escrowed SKR earns nothing in escrow — stake SKR in your Profile to earn protocol-fee dividends.
            </Text>
          ) : (
            <Text style={{ fontSize: 11, color: colors.accent, fontWeight: '600' }}>
              🔓 SOL in escrow is released on repayment. No yield accrues to escrowed collateral.
            </Text>
          )}
        </View>

        {/* Insufficient balance warning + quick airdrop/fallback button */}
        {isInsufficientCollateral && (
          <View
            style={[
              styles.warningBox,
              { backgroundColor: 'rgba(239, 68, 68, 0.08)', borderColor: colors.danger },
            ]}
          >
            <View style={{ flex: 1 }}>
              <Text style={[styles.warningText, { color: colors.danger }]}>
                Need {requiredCollateralUnits.toFixed(isDecimal ? 2 : 0)} {collateralType}, you hold {userBalance.toFixed(isDecimal ? 2 : 0)} {collateralType}
              </Text>
            </View>
            {collateralType !== 'SOL' && (
              <TouchableOpacity
                style={[styles.airdropBtn, { backgroundColor: colors.primary }]}
                onPress={() => setCollateralType('SOL')}
                activeOpacity={0.8}
              >
                <Text style={[styles.airdropBtnText, { color: colors.primaryText }]}>Use SOL Instead</Text>
              </TouchableOpacity>
            )}
          </View>
        )}
      </View>

      {/* Loan Term Selection */}
      <View style={[styles.sectionCard, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
        <Text style={[styles.sectionHeader, { color: colors.textSecondary }]}>LOAN DURATION</Text>
        <View style={styles.durationRow}>
          {[3, 7, 14, 30].map((days) => (
            <TouchableOpacity
              key={days}
              style={[
                styles.durationChip,
                { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder },
                durationDays === days && { borderColor: colors.primary, backgroundColor: colors.badgeBg },
              ]}
              onPress={() => setDurationDays(days)}
              activeOpacity={0.7}
            >
              <Text
                style={[
                  styles.durationText,
                  { color: colors.text },
                  durationDays === days && { color: colors.primary, fontWeight: '800' },
                ]}
              >
                {days} Days
              </Text>
            </TouchableOpacity>
          ))}
        </View>
      </View>

      {/* Execution Route Card */}
      <View style={[styles.routeCard, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
        <View style={styles.routeHeader}>
          <Text style={[styles.routeTitle, { color: colors.text }]}>
            {isLoadingPools ? 'Discovering Pools...' : (bestPool?.name || 'No lending pools available')}
          </Text>
          <View style={[styles.aprPill, { backgroundColor: colors.badgeBg }]}>
            <Text style={[styles.aprPillText, { color: colors.primary }]}>{effectiveApr.toFixed(1)}% APR</Text>
          </View>
        </View>

        <View style={styles.metricsGrid}>
          <View style={styles.metricItem}>
            <Text style={[styles.metricLabel, { color: colors.textMuted }]}>Interest Due</Text>
            <Text style={[styles.metricValue, { color: colors.text }]}>${estInterest.toFixed(3)}</Text>
          </View>
          <View style={styles.metricItem}>
            <Text style={[styles.metricLabel, { color: colors.textMuted }]}>Grace Period</Text>
            <Text style={[styles.metricValue, { color: colors.primary }]}>+24h Social</Text>
          </View>
          <View style={styles.metricItem}>
            <Text style={[styles.metricLabel, { color: colors.textMuted }]}>Seeker Tier</Text>
            <Text style={[styles.metricValue, { color: colors.accentLight }]}>{userProfile.tier}</Text>
          </View>
        </View>
      </View>

      {/* 1-Tap Borrow Button */}
      <TouchableOpacity
        style={[
          styles.borrowButton,
          { backgroundColor: colors.primary },
          isInsufficientCollateral && { opacity: 0.6 },
        ]}
        onPress={handleBorrow}
        disabled={isSubmitting || numAmount <= 0 || isInsufficientCollateral || !priceAvailable}
        activeOpacity={0.85}
      >
        {isSubmitting ? (
          <ActivityIndicator color={colors.primaryText} />
        ) : (
          <Text style={[styles.borrowButtonText, { color: colors.primaryText }]}>
            {isInsufficientCollateral
              ? `Insufficient ${collateralType} Collateral`
              : !priceAvailable
              ? 'Awaiting Live Price Feed...'
              : `⚡ Instant Borrow $${numAmount > 0 ? numAmount : 0} USDC`}
          </Text>
        )}
      </TouchableOpacity>
      <Text style={[styles.disclaimer, { color: colors.textMuted }]}>
        Atomic Escrow • Non-Custodial • Instant Settlement
      </Text>
    </ScrollView>
  );
};

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  content: {
    padding: 16,
    paddingBottom: 40,
  },
  heroSection: {
    borderRadius: 24,
    borderWidth: 1,
    padding: 24,
    alignItems: 'center',
    marginBottom: 16,
  },
  badgeRow: {
    marginBottom: 12,
  },
  liveBadge: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 10,
    paddingVertical: 4,
    borderRadius: 12,
    borderWidth: 1,
  },
  liveDot: {
    width: 6,
    height: 6,
    borderRadius: 3,
    marginRight: 6,
  },
  liveBadgeText: {
    fontSize: 10,
    fontWeight: '800',
    letterSpacing: 0.5,
  },
  heroSub: {
    fontSize: 11,
    fontWeight: '700',
    letterSpacing: 1,
    marginBottom: 8,
  },
  amountRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    marginBottom: 16,
  },
  currencyPrefix: {
    fontSize: 44,
    fontWeight: '800',
    marginRight: 4,
  },
  heroInput: {
    fontSize: 52,
    fontWeight: '800',
    minWidth: 80,
    textAlign: 'center',
  },
  currencySuffix: {
    fontSize: 18,
    fontWeight: '700',
    marginLeft: 8,
    alignSelf: 'flex-end',
    marginBottom: 12,
  },
  presetsRow: {
    flexDirection: 'row',
    gap: 8,
  },
  presetChip: {
    paddingHorizontal: 16,
    paddingVertical: 8,
    borderRadius: 16,
    borderWidth: 1,
  },
  presetText: {
    fontSize: 13,
    fontWeight: '600',
  },
  sectionCard: {
    borderRadius: 20,
    borderWidth: 1,
    padding: 16,
    marginBottom: 16,
  },
  sectionHeaderRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 12,
  },
  sectionHeader: {
    fontSize: 11,
    fontWeight: '700',
    letterSpacing: 0.5,
  },
  maxBadge: {
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 8,
  },
  maxBadgeText: {
    fontSize: 10,
    fontWeight: '800',
  },
  collateralTabs: {
    flexDirection: 'row',
    gap: 8,
    marginBottom: 12,
  },
  collateralChip: {
    flex: 1,
    paddingVertical: 12,
    alignItems: 'center',
    borderRadius: 14,
    borderWidth: 1,
  },
  collateralLogo: {
    width: 34,
    height: 34,
    borderRadius: 17,
    marginBottom: 6,
  },
  calcValueRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    marginTop: 4,
  },
  calcMiniLogo: {
    width: 18,
    height: 18,
    borderRadius: 9,
  },
  collateralLabel: {
    fontSize: 13,
    fontWeight: '700',
  },
  collateralSub: {
    fontSize: 10,
    marginTop: 2,
  },
  collateralDisplay: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    borderRadius: 14,
    borderWidth: 1,
    padding: 12,
  },
  calcLabel: {
    fontSize: 11,
    marginBottom: 2,
  },
  calcValue: {
    fontSize: 16,
    fontWeight: '800',
  },
  calcRight: {
    alignItems: 'flex-end',
  },
  calcSubLabel: {
    fontSize: 11,
    marginBottom: 2,
  },
  calcSubValue: {
    fontSize: 14,
    fontWeight: '700',
  },
  warningBox: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    borderRadius: 12,
    borderWidth: 1,
    padding: 10,
    marginTop: 10,
  },
  warningText: {
    fontSize: 11,
    fontWeight: '600',
  },
  airdropBtn: {
    paddingHorizontal: 10,
    paddingVertical: 6,
    borderRadius: 8,
    marginLeft: 8,
  },
  airdropBtnText: {
    fontSize: 11,
    fontWeight: '800',
  },
  durationRow: {
    flexDirection: 'row',
    gap: 8,
  },
  durationChip: {
    flex: 1,
    paddingVertical: 12,
    alignItems: 'center',
    borderRadius: 14,
    borderWidth: 1,
  },
  durationText: {
    fontSize: 13,
    fontWeight: '600',
  },
  routeCard: {
    borderRadius: 20,
    borderWidth: 1,
    padding: 16,
    marginBottom: 20,
  },
  routeHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 14,
  },
  routeTitle: {
    fontSize: 15,
    fontWeight: '700',
  },
  aprPill: {
    paddingHorizontal: 10,
    paddingVertical: 4,
    borderRadius: 10,
  },
  aprPillText: {
    fontSize: 13,
    fontWeight: '800',
  },
  metricsGrid: {
    flexDirection: 'row',
    justifyContent: 'space-between',
  },
  metricItem: {
    flex: 1,
  },
  metricLabel: {
    fontSize: 11,
    marginBottom: 4,
  },
  metricValue: {
    fontSize: 14,
    fontWeight: '700',
  },
  borrowButton: {
    height: 56,
    borderRadius: 18,
    justifyContent: 'center',
    alignItems: 'center',
    marginBottom: 8,
  },
  borrowButtonText: {
    fontSize: 17,
    fontWeight: '800',
  },
  disclaimer: {
    fontSize: 11,
    textAlign: 'center',
  },
});
