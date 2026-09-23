import React, { useState } from 'react';
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
  Modal,
  TextInput,
  Alert,
  Linking,
} from 'react-native';
import { useTheme } from '../theme/ThemeContext';
import { LendingPool, P2POffer } from '../types';
import { sharePawnToTardis, openTardisCommunity } from '../services/tardisIntegration';

interface MerchantDesksViewProps {
  pools: LendingPool[];
  offers: P2POffer[];
  userPubkey?: string;
  onSelectPool: (pool: LendingPool) => void;
  onFundPawnOffer: (offerId: number) => void;
  onCreatePawnOffer: (name: string, reqAmount: number, profit: number, days: number) => void;
  onRepayPawnOffer?: (offer: P2POffer) => void;
  onCancelPawnOffer?: (offer: P2POffer) => void;
  onCreatePool?: (
    name: string,
    poolType: 'Individual' | 'Circle',
    aprPercent: number,
    maxLtvPercent: number,
    minDays: number,
    maxDays: number,
    initialLiquidity: number
  ) => void;
  onDepositLiquidity?: (pool: LendingPool, amount: number) => void;
  onNfcBumpCircle: () => void;
}

export const MerchantDesksView: React.FC<MerchantDesksViewProps> = ({
  pools,
  offers,
  userPubkey,
  onSelectPool,
  onFundPawnOffer,
  onCreatePawnOffer,
  onRepayPawnOffer,
  onCancelPawnOffer,
  onCreatePool,
  onDepositLiquidity,
  onNfcBumpCircle,
}) => {
  const { colors } = useTheme();
  const [subTab, setSubTab] = useState<'POOLS' | 'PAWNS'>('POOLS');
  const [nfcModal, setNfcModal] = useState<boolean>(false);
  const [isNfcActive, setIsNfcActive] = useState<boolean>(false);
  const [fundAmount, setFundAmount] = useState<string>('500');

  // New desk / pool modal state
  const [createPoolModal, setCreatePoolModal] = useState<boolean>(false);
  const [deskName, setDeskName] = useState<string>('Solana Chad Vault');
  const [deskType, setDeskType] = useState<'Individual' | 'Circle'>('Individual');
  const [deskApr, setDeskApr] = useState<string>('8.0');
  const [deskLtv, setDeskLtv] = useState<string>('85');
  const [deskMinDays, setDeskMinDays] = useState<string>('7');
  const [deskMaxDays, setDeskMaxDays] = useState<string>('30');
  const [deskLiquidity, setDeskLiquidity] = useState<string>('500');

  const handleCreatePoolSubmit = () => {
    const apr = parseFloat(deskApr);
    const ltv = parseFloat(deskLtv);
    const minD = parseInt(deskMinDays, 10);
    const maxD = parseInt(deskMaxDays, 10);
    const liq = parseFloat(deskLiquidity);

    if (!deskName.trim()) {
      Alert.alert('Missing Name', 'Please enter a name for your lending desk.');
      return;
    }
    if (isNaN(apr) || apr <= 0 || apr > 100) {
      Alert.alert('Invalid APR', 'Please enter a valid fixed APR between 1% and 100%.');
      return;
    }
    if (isNaN(ltv) || ltv <= 10 || ltv > 95) {
      Alert.alert('Invalid LTV', 'Max LTV must be between 10% and 95%.');
      return;
    }
    if (isNaN(minD) || isNaN(maxD) || minD < 1 || maxD < minD) {
      Alert.alert('Invalid Duration', 'Max duration must be greater than or equal to min duration.');
      return;
    }
    if (isNaN(liq) || liq <= 0) {
      Alert.alert('Invalid Liquidity', 'Please enter a valid initial liquidity amount.');
      return;
    }

    setCreatePoolModal(false);
    if (onCreatePool) {
      onCreatePool(deskName.trim(), deskType, apr, ltv, minD, maxD, liq);
    }
  };

  // New pawn modal
  const [pawnModal, setPawnModal] = useState<boolean>(false);
  const [assetName, setAssetName] = useState<string>('1,000 SKR');
  const [reqAmount, setReqAmount] = useState<string>('20');
  const [profitAmount, setProfitAmount] = useState<string>('2');
  const [duration, setDuration] = useState<string>('7');

  // Pawn filtering: All, My Pawns, Funded by Me, Completed
  const [pawnFilter, setPawnFilter] = useState<'ALL' | 'MY_PAWNS' | 'FUNDED' | 'COMPLETED'>('ALL');

  const myPawnsCount = offers.filter((o) => userPubkey && o.creator === userPubkey).length;
  const fundedByMeCount = offers.filter((o) => userPubkey && o.funder === userPubkey).length;
  const completedCount = offers.filter((o) => o.status === 'Repaid' || o.status === 'Defaulted').length;

  const filteredOffers = offers.filter((offer) => {
    if (pawnFilter === 'MY_PAWNS') {
      return Boolean(userPubkey && offer.creator === userPubkey);
    }
    if (pawnFilter === 'FUNDED') {
      return Boolean(userPubkey && offer.funder === userPubkey);
    }
    if (pawnFilter === 'COMPLETED') {
      return offer.status === 'Repaid' || offer.status === 'Defaulted';
    }
    return true;
  });

  const triggerNfcBump = () => {
    setIsNfcActive(true);
    setTimeout(() => {
      setIsNfcActive(false);
      setNfcModal(false);
      onNfcBumpCircle();
      Alert.alert('🤝 Circle Synced!', 'Connected via Seeker NFC. Joined "Seeker Genesis Circle" with 90% LTV.');
    }, 1200);
  };

  const handleCreatePawn = () => {
    const amt = parseFloat(reqAmount);
    const prof = parseFloat(profitAmount);
    const d = parseInt(duration, 10);
    if (!assetName || isNaN(amt) || isNaN(prof) || isNaN(d) || amt <= 0) {
      Alert.alert('Invalid Input', 'Please enter a valid asset name, amount, and duration.');
      return;
    }
    setPawnModal(false);
    setSubTab('PAWNS');
    onCreatePawnOffer(assetName, amt, prof, d);
  };

  return (
    <View style={[styles.container, { backgroundColor: colors.background }]}>
      {/* Top Controls: Segmented Switcher & Action */}
      <View style={styles.topBar}>
        <View style={[styles.segmentControl, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
          <TouchableOpacity
            style={[
              styles.segmentBtn,
              subTab === 'POOLS' && { backgroundColor: colors.card, borderColor: colors.cardBorder, borderWidth: 1 },
            ]}
            onPress={() => setSubTab('POOLS')}
            activeOpacity={0.7}
          >
            <Text
              style={[
                styles.segmentText,
                { color: colors.textSecondary },
                subTab === 'POOLS' && { color: colors.text, fontWeight: '800' },
              ]}
            >
              Lending Desks ({pools.length})
            </Text>
          </TouchableOpacity>

          <TouchableOpacity
            style={[
              styles.segmentBtn,
              subTab === 'PAWNS' && { backgroundColor: colors.card, borderColor: colors.cardBorder, borderWidth: 1 },
            ]}
            onPress={() => setSubTab('PAWNS')}
            activeOpacity={0.7}
          >
            <Text
              style={[
                styles.segmentText,
                { color: colors.textSecondary },
                subTab === 'PAWNS' && { color: colors.text, fontWeight: '800' },
              ]}
            >
              P2P Pawns ({offers.length})
            </Text>
          </TouchableOpacity>
        </View>

        {subTab === 'POOLS' ? (
          <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
            {onCreatePool && (
              <TouchableOpacity
                style={[styles.newPawnChip, { backgroundColor: colors.primary }]}
                onPress={() => setCreatePoolModal(true)}
                activeOpacity={0.7}
              >
                <Text style={[styles.newPawnChipText, { color: colors.primaryText }]}>+ Create Desk</Text>
              </TouchableOpacity>
            )}
            <TouchableOpacity
              style={[styles.nfcChip, { backgroundColor: colors.badgeBg, borderColor: colors.badgeBorder }]}
              onPress={() => setNfcModal(true)}
              activeOpacity={0.7}
            >
              <Text style={[styles.nfcChipText, { color: colors.primary }]}>📡 NFC (Soon)</Text>
            </TouchableOpacity>
          </View>
        ) : (
          <TouchableOpacity
            style={[styles.newPawnChip, { backgroundColor: colors.primary }]}
            onPress={() => setPawnModal(true)}
            activeOpacity={0.7}
          >
            <Text style={[styles.newPawnChipText, { color: colors.primaryText }]}>+ New Pawn</Text>
          </TouchableOpacity>
        )}
      </View>

      <ScrollView contentContainerStyle={styles.scrollContent} showsVerticalScrollIndicator={false}>
        {/* SUBTAB 1: LENDING DESKS */}
        {subTab === 'POOLS' && (
          <View>
            {pools.length === 0 ? (
              <View style={[styles.emptyCard, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
                <Text style={styles.emptyIcon}>🏦</Text>
                <Text style={[styles.emptyTitle, { color: colors.text }]}>No Lending Desks Found</Text>
                <Text style={[styles.emptySub, { color: colors.textSecondary }]}>
                  Be the first to create an on-chain lending desk with your own terms and liquidity!
                </Text>
                {onCreatePool && (
                  <TouchableOpacity
                    style={[styles.newPawnActionBtn, { backgroundColor: colors.primary, marginTop: 8 }]}
                    onPress={() => setCreatePoolModal(true)}
                    activeOpacity={0.8}
                  >
                    <Text style={[styles.newPawnActionText, { color: colors.primaryText }]}>+ Create Lending Desk</Text>
                  </TouchableOpacity>
                )}
              </View>
            ) : (
              pools.map((pool) => {
                const isMyDesk = Boolean(userPubkey && pool.authority.toLowerCase() === userPubkey.toLowerCase());
                return (
                  <View
                    key={pool.id}
                    style={[
                      styles.deskCard,
                      { backgroundColor: colors.card, borderColor: isMyDesk ? colors.primary : colors.cardBorder },
                      isMyDesk && { borderWidth: 1.5 },
                    ]}
                  >
                    <View style={styles.deskHeader}>
                      <View style={styles.deskTitleRow}>
                        <Text style={styles.deskIcon}>{pool.poolType === 'Circle' ? '⭕' : '🏛️'}</Text>
                        <View style={{ flex: 1 }}>
                          <View style={[styles.deskNameRow, { flexWrap: 'wrap' }]}>
                            <Text style={[styles.deskName, { color: colors.text }]}>{pool.name}</Text>
                            {pool.isVerifiedMerchant && (
                              <View style={[styles.verifiedTag, { backgroundColor: colors.badgeBg }]}>
                                <Text style={[styles.verifiedTagText, { color: colors.primary }]}>VERIFIED</Text>
                              </View>
                            )}
                            {isMyDesk && (
                              <View style={[styles.verifiedTag, { backgroundColor: 'rgba(234, 179, 8, 0.15)', borderColor: '#eab308', borderWidth: 1 }]}>
                                <Text style={[styles.verifiedTagText, { color: '#eab308' }]}>👑 YOUR DESK</Text>
                              </View>
                            )}
                            <View
                              style={[
                                styles.verifiedTag,
                                {
                                  backgroundColor:
                                    pool.poolType === 'Circle' ? 'rgba(168, 85, 247, 0.15)' : 'rgba(59, 130, 246, 0.15)',
                                },
                              ]}
                            >
                              <Text
                                style={[
                                  styles.verifiedTagText,
                                  { color: pool.poolType === 'Circle' ? '#c084fc' : '#60a5fa' },
                                ]}
                              >
                                {pool.poolType.toUpperCase()}
                              </Text>
                            </View>
                            {pool.poolType === 'Circle' && (
                              <TouchableOpacity
                                style={[styles.tardisCircleBadge, { backgroundColor: 'rgba(50, 212, 222, 0.12)' }]}
                                onPress={() => openTardisCommunity(pool.name)}
                                activeOpacity={0.7}
                              >
                                <Text style={styles.tardisCircleBadgeText}>🌌 Open in TARDIS ↗</Text>
                              </TouchableOpacity>
                            )}
                          </View>
                          <Text style={[styles.deskAuthority, { color: colors.textMuted }]} numberOfLines={1}>
                            {pool.authority.slice(0, 4)}...{pool.authority.slice(-4)} • {pool.minDurationDays}-{pool.maxDurationDays}d term
                          </Text>
                        </View>
                      </View>

                      <View style={styles.rateCol}>
                        <Text style={[styles.rateValue, { color: colors.primary }]}>
                          {(pool.interestRateBps / 100).toFixed(1)}%
                        </Text>
                        <Text style={[styles.rateLabel, { color: colors.textMuted }]}>Fixed APR</Text>
                      </View>
                    </View>

                  <View style={[styles.metricsRow, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
                    <View style={styles.metric}>
                      <Text style={[styles.mLabel, { color: colors.textMuted }]}>Available</Text>
                      <Text style={[styles.mValue, { color: colors.text }]}>
                        ${pool.totalLiquidity.toLocaleString()}
                      </Text>
                    </View>
                    <View style={styles.metric}>
                      <Text style={[styles.mLabel, { color: colors.textMuted }]}>Max LTV</Text>
                      <Text style={[styles.mValue, { color: colors.text }]}>
                        {(pool.maxLtvBps / 100).toFixed(0)}%
                      </Text>
                    </View>
                    <View style={styles.metric}>
                      <Text style={[styles.mLabel, { color: colors.textMuted }]}>Repayment Rate</Text>
                      <Text style={[styles.mValue, { color: colors.primary }]}>{pool.successRate === null ? '—' : `${pool.successRate}%`}</Text>
                    </View>
                  </View>

                  <TouchableOpacity
                    style={[styles.borrowDeskBtn, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}
                    onPress={() => onSelectPool(pool)}
                    activeOpacity={0.7}
                  >
                    <Text style={[styles.borrowDeskBtnText, { color: colors.text }]}>Borrow from this Desk →</Text>
                  </TouchableOpacity>

                  {onDepositLiquidity && userPubkey && pool.authority === userPubkey && (
                    <View style={[styles.fundRow, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
                      <View style={styles.fundInputGroup}>
                        <Text style={[styles.inputLabel, { color: colors.textSecondary }]}>DEPOSIT USDC</Text>
                        <TextInput
                          style={[styles.inputBox, { backgroundColor: colors.inputBg, borderColor: colors.inputBorder, color: colors.text }]}
                          value={fundAmount}
                          onChangeText={setFundAmount}
                          keyboardType="decimal-pad"
                          placeholder="500"
                          placeholderTextColor={colors.textMuted}
                        />
                      </View>
                      <TouchableOpacity
                        style={[styles.fundDeskBtn, { backgroundColor: colors.primary }]}
                        onPress={() => {
                          const amt = parseFloat(fundAmount);
                          if (!isNaN(amt) && amt > 0) {
                            onDepositLiquidity(pool, amt);
                          }
                        }}
                        activeOpacity={0.7}
                      >
                        <Text style={styles.fundDeskBtnText}>Fund Desk</Text>
                      </TouchableOpacity>
                    </View>
                  )}
                </View>
              );
            })
          )}
        </View>
        )}

        {/* SUBTAB 2: P2P PAWNS */}
        {subTab === 'PAWNS' && (
          <View>
            {/* Filter Bar */}
            <ScrollView
              horizontal
              showsHorizontalScrollIndicator={false}
              contentContainerStyle={styles.pawnFilterBar}
            >
              {[
                { id: 'ALL', label: `All (${offers.length})` },
                { id: 'MY_PAWNS', label: `My Pawns (${myPawnsCount})` },
                { id: 'FUNDED', label: `Funded (${fundedByMeCount})` },
                { id: 'COMPLETED', label: `Completed (${completedCount})` },
              ].map((f) => (
                <TouchableOpacity
                  key={f.id}
                  style={[
                    styles.pawnFilterChip,
                    { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder },
                    pawnFilter === f.id && { backgroundColor: colors.badgeBg, borderColor: colors.primary },
                  ]}
                  onPress={() => setPawnFilter(f.id as any)}
                  activeOpacity={0.7}
                >
                  <Text
                    style={[
                      styles.pawnFilterChipText,
                      { color: colors.textSecondary },
                      pawnFilter === f.id && { color: colors.primary, fontWeight: '800' },
                    ]}
                  >
                    {f.label}
                  </Text>
                </TouchableOpacity>
              ))}
            </ScrollView>

            {filteredOffers.length === 0 ? (
              <View style={[styles.emptyCard, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
                <Text style={styles.emptyIcon}>🃏</Text>
                <Text style={[styles.emptyTitle, { color: colors.text }]}>
                  {pawnFilter === 'MY_PAWNS'
                    ? 'No Pawns Created Yet'
                    : pawnFilter === 'FUNDED'
                    ? 'No Funded Pawns Yet'
                    : pawnFilter === 'COMPLETED'
                    ? 'No Completed Pawns Yet'
                    : 'No P2P Pawns Listed Yet'}
                </Text>
                <Text style={[styles.emptySub, { color: colors.textSecondary }]}>
                  {pawnFilter === 'MY_PAWNS'
                    ? 'You have not listed any pawns yet. Tap "+ New Pawn" to escrow an asset and borrow directly from peers.'
                    : pawnFilter === 'FUNDED'
                    ? 'You have not funded any peer pawns. Browse open pawns to fund and earn high APY yield.'
                    : pawnFilter === 'COMPLETED'
                    ? 'Completed and repaid pawn loans will appear in this history.'
                    : 'Be the first to list a digital asset or NFT for peer funding!'}
                </Text>
                {pawnFilter === 'MY_PAWNS' && (
                  <TouchableOpacity
                    style={[styles.newPawnActionBtn, { backgroundColor: colors.primary }]}
                    onPress={() => setPawnModal(true)}
                    activeOpacity={0.8}
                  >
                    <Text style={[styles.newPawnActionText, { color: colors.primaryText }]}>+ List First Pawn</Text>
                  </TouchableOpacity>
                )}
              </View>
            ) : (
              filteredOffers.map((offer) => {
                const isCreator = Boolean(userPubkey && offer.creator.toLowerCase() === userPubkey.toLowerCase());
                const isFunder = Boolean(userPubkey && offer.funder && offer.funder.toLowerCase() === userPubkey.toLowerCase());
                const totalDue = parseFloat((offer.requestedAmount + offer.interestOffered).toFixed(2));
                const isRepaid = offer.status === 'Repaid';

                return (
                  <View
                    key={offer.id}
                    style={[
                      styles.pawnCard,
                      { backgroundColor: colors.card, borderColor: colors.cardBorder },
                      isRepaid && { borderColor: 'rgba(34, 197, 94, 0.3)' },
                    ]}
                  >
                    <View style={styles.pawnHeader}>
                      <View>
                        <View style={{ flexDirection: 'row', alignItems: 'center', gap: 6 }}>
                          <Text style={[styles.pawnTitle, { color: colors.text }]}>{offer.collateralName}</Text>
                          {isCreator && (
                            <View style={[styles.ownerTag, { backgroundColor: colors.badgeBg }]}>
                              <Text style={[styles.ownerTagText, { color: colors.primary }]}>YOUR PAWN</Text>
                            </View>
                          )}
                          {isFunder && (
                            <View style={[styles.ownerTag, { backgroundColor: 'rgba(59, 130, 246, 0.15)' }]}>
                              <Text style={[styles.ownerTagText, { color: '#3b82f6' }]}>FUNDED BY YOU</Text>
                            </View>
                          )}
                        </View>
                        <Text style={[styles.pawnBorrower, { color: colors.textMuted }]}>
                          Creator: {isCreator ? 'You' : `${offer.creator.slice(0, 4)}...${offer.creator.slice(-4)}`}
                        </Text>
                      </View>
                      <View
                        style={[
                          styles.statusChip,
                          {
                            backgroundColor: isRepaid
                              ? 'rgba(34, 197, 94, 0.15)'
                              : offer.status === 'Funded'
                              ? 'rgba(59, 130, 246, 0.15)'
                              : colors.badgeBg,
                          },
                        ]}
                      >
                        <Text
                          style={[
                            styles.statusText,
                            {
                              color: isRepaid
                                ? '#22c55e'
                                : offer.status === 'Funded'
                                ? '#3b82f6'
                                : colors.primary,
                            },
                          ]}
                        >
                          {isRepaid ? 'COMPLETED' : offer.status.toUpperCase()}
                        </Text>
                      </View>
                    </View>

                    <View style={[styles.metricsRow, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
                      <View style={styles.metric}>
                        <Text style={[styles.mLabel, { color: colors.textMuted }]}>Ask Principal</Text>
                        <Text style={[styles.mValue, { color: colors.text }]}>${offer.requestedAmount} USDC</Text>
                      </View>
                      <View style={styles.metric}>
                        <Text style={[styles.mLabel, { color: colors.textMuted }]}>Lender Yield</Text>
                        <Text style={[styles.mValue, { color: colors.primary }]}>+${offer.interestOffered}</Text>
                      </View>
                      <View style={styles.metric}>
                        <Text style={[styles.mLabel, { color: colors.textMuted }]}>Duration</Text>
                        <Text style={[styles.mValue, { color: colors.text }]}>{offer.durationDays}d</Text>
                      </View>
                    </View>

                    {offer.escrowAddress && (
                      <View style={[styles.escrowRow, { borderColor: colors.cardBorder }]}>
                        <View style={styles.escrowInfo}>
                          <Text style={[styles.escrowLabel, { color: colors.textMuted }]}>Escrow:</Text>
                          <Text style={[styles.escrowValue, { color: colors.primary }]}>
                            {offer.escrowAddress.slice(0, 6)}...{offer.escrowAddress.slice(-6)}
                          </Text>
                        </View>
                        {offer.solscanUrl && (
                          <TouchableOpacity
                            onPress={() => Linking.openURL(offer.solscanUrl!)}
                            style={[styles.solscanChip, { backgroundColor: colors.badgeBg }]}
                            activeOpacity={0.7}
                          >
                            <Text style={[styles.solscanChipText, { color: colors.primary }]}>Solscan ↗</Text>
                          </TouchableOpacity>
                        )}
                      </View>
                    )}

                    {/* CONTEXT-AWARE ACTION SECTION */}
                    {offer.status === 'Open' ? (
                      isCreator ? (
                        <View style={{ gap: 8 }}>
                          <View style={{ flexDirection: 'row', gap: 10, alignItems: 'center' }}>
                            <View style={[styles.fundedNote, { flex: 1, backgroundColor: colors.cardAlt }]}>
                              <Text style={[styles.fundedNoteText, { color: colors.textSecondary }]}>
                                ⏳ Awaiting Peer Funder
                              </Text>
                            </View>
                            {onCancelPawnOffer && (
                              <TouchableOpacity
                                style={[styles.cancelBtn, { borderColor: colors.cardBorder }]}
                                onPress={() => onCancelPawnOffer(offer)}
                                activeOpacity={0.8}
                              >
                                <Text style={[styles.cancelBtnText, { color: colors.textMuted }]}>Cancel & Withdraw</Text>
                              </TouchableOpacity>
                            )}
                          </View>
                          <TouchableOpacity
                            style={[styles.tardisShareBtn, { backgroundColor: 'rgba(50, 212, 222, 0.12)', borderColor: '#32D4DE' }]}
                            onPress={() => sharePawnToTardis(offer)}
                            activeOpacity={0.85}
                          >
                            <Text style={styles.tardisShareBtnText}>🌌 Share to TARDIS Feed (Blink)</Text>
                          </TouchableOpacity>
                        </View>
                      ) : (
                        <View style={{ flexDirection: 'row', gap: 8, alignItems: 'center' }}>
                          <TouchableOpacity
                            style={[styles.fundBtn, { flex: 1, backgroundColor: colors.primary }]}
                            onPress={() => onFundPawnOffer(offer.id)}
                            activeOpacity={0.85}
                          >
                            <Text style={[styles.fundBtnText, { color: colors.primaryText }]}>
                              ⚡ Fund & Earn +${offer.interestOffered} USDC
                            </Text>
                          </TouchableOpacity>
                          <TouchableOpacity
                            style={[styles.tardisIconBtn, { backgroundColor: 'rgba(50, 212, 222, 0.12)', borderColor: 'rgba(50, 212, 222, 0.3)' }]}
                            onPress={() => sharePawnToTardis(offer)}
                            activeOpacity={0.8}
                          >
                            <Text style={styles.tardisIconBtnText}>🌌 Blink</Text>
                          </TouchableOpacity>
                        </View>
                      )
                    ) : offer.status === 'Funded' ? (
                      isCreator ? (
                        <View>
                          <View style={[styles.fundedNote, { backgroundColor: 'rgba(239, 68, 68, 0.08)', marginBottom: 10 }]}>
                            <Text style={[styles.fundedNoteText, { color: '#ef4444' }]}>
                              🚨 Funded! Repay ${totalDue} USDC to unlock your {offer.collateralName} from escrow.
                            </Text>
                          </View>
                          {onRepayPawnOffer && (
                            <TouchableOpacity
                              style={[styles.fundBtn, { backgroundColor: colors.primary }]}
                              onPress={() => onRepayPawnOffer(offer)}
                              activeOpacity={0.85}
                            >
                              <Text style={[styles.fundBtnText, { color: colors.primaryText }]}>
                                ⚡ Repay ${totalDue} USDC & Unlock Collateral
                              </Text>
                            </TouchableOpacity>
                          )}
                        </View>
                      ) : isFunder ? (
                        <View style={[styles.fundedNote, { backgroundColor: 'rgba(59, 130, 246, 0.08)' }]}>
                          <Text style={[styles.fundedNoteText, { color: '#3b82f6' }]}>
                            💼 Funded by You — Awaiting borrower repayment (+${offer.interestOffered} USDC yield).
                          </Text>
                        </View>
                      ) : (
                        <View style={[styles.fundedNote, { backgroundColor: colors.cardAlt }]}>
                          <Text style={[styles.fundedNoteText, { color: colors.textMuted }]}>🔒 Funded & In Escrow</Text>
                        </View>
                      )
                    ) : (
                      <View style={[styles.fundedNote, { backgroundColor: 'rgba(34, 197, 94, 0.08)' }]}>
                        <Text style={[styles.fundedNoteText, { color: '#22c55e', fontWeight: '700' }]}>
                          ✅ Completed — Collateral Unlocked & Returned
                        </Text>
                      </View>
                    )}
                  </View>
                );
              })
            )}
          </View>
        )}
      </ScrollView>

      {/* CREATE NEW LENDING DESK MODAL */}
      <Modal visible={createPoolModal} transparent animationType="slide">
        <View style={styles.modalOverlay}>
          <View style={[styles.modalCard, { backgroundColor: colors.card, borderColor: colors.cardBorder, maxHeight: '88%', paddingHorizontal: 16 }]}>
            <ScrollView showsVerticalScrollIndicator={false} style={{ width: '100%' }} contentContainerStyle={{ alignItems: 'center', paddingBottom: 16 }}>
              <Text style={styles.modalNfcIcon}>🏛️</Text>
              <Text style={[styles.modalTitle, { color: colors.text }]}>Create Lending Desk</Text>
              <Text style={[styles.modalDesc, { color: colors.textSecondary }]}>
                Deploy an on-chain lending pool with your own custom interest rate, LTV, and liquidity terms.
              </Text>

              {/* Pool Type Selection */}
              <View style={styles.inputGroup}>
                <Text style={[styles.inputLabel, { color: colors.textSecondary }]}>DESK TYPE</Text>
                <View style={{ flexDirection: 'row', gap: 10, marginTop: 4 }}>
                  <TouchableOpacity
                    style={[
                      styles.typeSelectorBtn,
                      { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder },
                      deskType === 'Individual' && { borderColor: colors.primary, backgroundColor: colors.badgeBg },
                    ]}
                    onPress={() => setDeskType('Individual')}
                    activeOpacity={0.7}
                  >
                    <Text style={{ fontSize: 20, marginBottom: 4 }}>🏛️</Text>
                    <Text
                      style={[
                        styles.typeSelectorText,
                        { color: colors.textSecondary },
                        deskType === 'Individual' && { color: colors.primary, fontWeight: '700' },
                      ]}
                    >
                      Individual Desk
                    </Text>
                    <Text style={[styles.typeSelectorSub, { color: colors.textMuted }]}>Direct 1-on-1 lending</Text>
                  </TouchableOpacity>

                  <TouchableOpacity
                    style={[
                      styles.typeSelectorBtn,
                      { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder },
                      deskType === 'Circle' && { borderColor: colors.primary, backgroundColor: colors.badgeBg },
                    ]}
                    onPress={() => setDeskType('Circle')}
                    activeOpacity={0.7}
                  >
                    <Text style={{ fontSize: 20, marginBottom: 4 }}>⭕</Text>
                    <Text
                      style={[
                        styles.typeSelectorText,
                        { color: colors.textSecondary },
                        deskType === 'Circle' && { color: colors.primary, fontWeight: '700' },
                      ]}
                    >
                      Circle Pool
                    </Text>
                    <Text style={[styles.typeSelectorSub, { color: colors.textMuted }]}>Trusted peer circle</Text>
                  </TouchableOpacity>
                </View>
              </View>

              {/* Desk Name */}
              <View style={styles.inputGroup}>
                <Text style={[styles.inputLabel, { color: colors.textSecondary }]}>DESK NAME</Text>
                <TextInput
                  style={[styles.inputBox, { backgroundColor: colors.inputBg, borderColor: colors.inputBorder, color: colors.text }]}
                  value={deskName}
                  onChangeText={setDeskName}
                  placeholder="e.g. Solana Chad Vault"
                  placeholderTextColor={colors.textMuted}
                />
                <View style={styles.quickChipsRow}>
                  {['Alpha Vault', 'Chad Lending', 'Seeker Genesis', 'DeFi Circle'].map((preset) => (
                    <TouchableOpacity
                      key={preset}
                      style={[
                        styles.quickChip,
                        { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder },
                        deskName === preset && { borderColor: colors.primary, backgroundColor: colors.badgeBg },
                      ]}
                      onPress={() => setDeskName(preset)}
                    >
                      <Text
                        style={[
                          styles.quickChipText,
                          { color: colors.textSecondary },
                          deskName === preset && { color: colors.primary, fontWeight: '700' },
                        ]}
                      >
                        {preset}
                      </Text>
                    </TouchableOpacity>
                  ))}
                </View>
              </View>

              {/* APR & Max LTV */}
              <View style={styles.inputRow}>
                <View style={[styles.inputGroup, { flex: 1 }]}>
                  <Text style={[styles.inputLabel, { color: colors.textSecondary }]}>FIXED APR (%)</Text>
                  <TextInput
                    style={[styles.inputBox, { backgroundColor: colors.inputBg, borderColor: colors.inputBorder, color: colors.text }]}
                    value={deskApr}
                    onChangeText={setDeskApr}
                    keyboardType="numeric"
                  />
                  <View style={styles.quickChipsRow}>
                    {['5.0', '8.0', '12.0'].map((apr) => (
                      <TouchableOpacity
                        key={apr}
                        style={[
                          styles.quickChip,
                          { flex: 1, backgroundColor: colors.cardAlt, borderColor: colors.cardBorder },
                          deskApr === apr && { borderColor: colors.primary, backgroundColor: colors.badgeBg },
                        ]}
                        onPress={() => setDeskApr(apr)}
                      >
                        <Text
                          style={[
                            styles.quickChipText,
                            { color: colors.textSecondary },
                            deskApr === apr && { color: colors.primary, fontWeight: '700' },
                          ]}
                        >
                          {apr}%
                        </Text>
                      </TouchableOpacity>
                    ))}
                  </View>
                </View>

                <View style={[styles.inputGroup, { flex: 1, marginLeft: 10 }]}>
                  <Text style={[styles.inputLabel, { color: colors.textSecondary }]}>MAX LTV (%)</Text>
                  <TextInput
                    style={[styles.inputBox, { backgroundColor: colors.inputBg, borderColor: colors.inputBorder, color: colors.text }]}
                    value={deskLtv}
                    onChangeText={setDeskLtv}
                    keyboardType="numeric"
                  />
                  <View style={styles.quickChipsRow}>
                    {['75', '85', '90'].map((ltv) => (
                      <TouchableOpacity
                        key={ltv}
                        style={[
                          styles.quickChip,
                          { flex: 1, backgroundColor: colors.cardAlt, borderColor: colors.cardBorder },
                          deskLtv === ltv && { borderColor: colors.primary, backgroundColor: colors.badgeBg },
                        ]}
                        onPress={() => setDeskLtv(ltv)}
                      >
                        <Text
                          style={[
                            styles.quickChipText,
                            { color: colors.textSecondary },
                            deskLtv === ltv && { color: colors.primary, fontWeight: '700' },
                          ]}
                        >
                          {ltv}%
                        </Text>
                      </TouchableOpacity>
                    ))}
                  </View>
                </View>
              </View>

              {/* Duration range */}
              <View style={styles.inputRow}>
                <View style={[styles.inputGroup, { flex: 1 }]}>
                  <Text style={[styles.inputLabel, { color: colors.textSecondary }]}>MIN TERM (DAYS)</Text>
                  <TextInput
                    style={[styles.inputBox, { backgroundColor: colors.inputBg, borderColor: colors.inputBorder, color: colors.text }]}
                    value={deskMinDays}
                    onChangeText={setDeskMinDays}
                    keyboardType="numeric"
                  />
                </View>
                <View style={[styles.inputGroup, { flex: 1, marginLeft: 10 }]}>
                  <Text style={[styles.inputLabel, { color: colors.textSecondary }]}>MAX TERM (DAYS)</Text>
                  <TextInput
                    style={[styles.inputBox, { backgroundColor: colors.inputBg, borderColor: colors.inputBorder, color: colors.text }]}
                    value={deskMaxDays}
                    onChangeText={setDeskMaxDays}
                    keyboardType="numeric"
                  />
                </View>
              </View>

              {/* Initial Capacity / Liquidity */}
              <View style={styles.inputGroup}>
                <Text style={[styles.inputLabel, { color: colors.textSecondary }]}>INITIAL LIQUIDITY (USDC)</Text>
                <TextInput
                  style={[styles.inputBox, { backgroundColor: colors.inputBg, borderColor: colors.inputBorder, color: colors.text }]}
                  value={deskLiquidity}
                  onChangeText={setDeskLiquidity}
                  keyboardType="numeric"
                />
                <View style={styles.quickChipsRow}>
                  {['100', '250', '500', '1000'].map((liq) => (
                    <TouchableOpacity
                      key={liq}
                      style={[
                        styles.quickChip,
                        { flex: 1, backgroundColor: colors.cardAlt, borderColor: colors.cardBorder },
                        deskLiquidity === liq && { borderColor: colors.primary, backgroundColor: colors.badgeBg },
                      ]}
                      onPress={() => setDeskLiquidity(liq)}
                    >
                      <Text
                        style={[
                          styles.quickChipText,
                          { color: colors.textSecondary },
                          deskLiquidity === liq && { color: colors.primary, fontWeight: '700' },
                        ]}
                      >
                        ${liq}
                      </Text>
                    </TouchableOpacity>
                  ))}
                </View>
              </View>

              {/* Protocol Note */}
              <View style={[styles.protocolNote, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
                <Text style={[styles.protocolNoteText, { color: colors.textSecondary }]}>
                  ⚡ Initializing this desk creates an on-chain Pool PDA and Vault PDA via ClockLend Program (HAjGx...jsH3). Other users will see your desk immediately.
                </Text>
              </View>

              <TouchableOpacity
                style={[styles.bumpActionBtn, { backgroundColor: colors.primary, marginTop: 8 }]}
                onPress={handleCreatePoolSubmit}
                activeOpacity={0.85}
              >
                <Text style={[styles.bumpActionText, { color: colors.primaryText }]}>
                  🚀 Deploy Desk on Solana
                </Text>
              </TouchableOpacity>

              <TouchableOpacity style={styles.modalCloseBtn} onPress={() => setCreatePoolModal(false)}>
                <Text style={[styles.modalCloseText, { color: colors.textSecondary }]}>Cancel</Text>
              </TouchableOpacity>
            </ScrollView>
          </View>
        </View>
      </Modal>

      {/* NFC BUMP MODAL */}
      <Modal visible={nfcModal} transparent animationType="fade">
        <View style={styles.modalOverlay}>
          <View style={[styles.modalCard, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
            <Text style={styles.modalNfcIcon}>📡</Text>
            <View style={[styles.verifiedTag, { backgroundColor: 'rgba(234, 179, 8, 0.15)', borderColor: '#eab308', borderWidth: 1, marginBottom: 12 }]}>
              <Text style={[styles.verifiedTagText, { color: '#eab308' }]}>HARDWARE NFC • COMING SOON</Text>
            </View>
            <Text style={[styles.modalTitle, { color: colors.text }]}>Seeker Phone Bump (NFC)</Text>
            <Text style={[styles.modalDesc, { color: colors.textSecondary }]}>
              Hold your Seeker smartphone back-to-back with a trusted peer to instantly establish an authenticated lending circle via hardware NFC chips.
            </Text>

            <View style={[styles.protocolNote, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder, marginBottom: 16 }]}>
              <Text style={[styles.protocolNoteText, { color: colors.textSecondary }]}>
                🔒 Requires physical Seeker Secure Element NFC driver integration. This feature will be enabled in an upcoming Solana Mobile firmware release.
              </Text>
            </View>

            <TouchableOpacity
              style={[styles.bumpActionBtn, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder, borderWidth: 1, opacity: 0.6 }]}
              disabled={true}
              activeOpacity={1}
            >
              <Text style={[styles.bumpActionText, { color: colors.textMuted }]}>
                ⏳ Hardware NFC — Coming Soon
              </Text>
            </TouchableOpacity>

            <TouchableOpacity style={styles.modalCloseBtn} onPress={() => setNfcModal(false)}>
              <Text style={[styles.modalCloseText, { color: colors.textSecondary }]}>Close</Text>
            </TouchableOpacity>
          </View>
        </View>
      </Modal>

      {/* CREATE NEW PAWN MODAL */}
      <Modal visible={pawnModal} transparent animationType="slide">
        <View style={styles.modalOverlay}>
          <View style={[styles.modalCard, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
            <Text style={[styles.modalTitle, { color: colors.text }]}>List Asset for Peer Pawn</Text>
            <Text style={[styles.modalDesc, { color: colors.textSecondary }]}>
              Escrow your digital asset into an on-chain smart contract lock and borrow directly from peers.
            </Text>

            <View style={styles.inputGroup}>
              <Text style={[styles.inputLabel, { color: colors.textSecondary }]}>COLLATERAL ASSET</Text>
              <TextInput
                style={[styles.inputBox, { backgroundColor: colors.inputBg, borderColor: colors.inputBorder, color: colors.text }]}
                value={assetName}
                onChangeText={setAssetName}
                placeholder="e.g. 1,000 SKR or 0.5 SOL"
                placeholderTextColor={colors.textMuted}
              />
              <View style={styles.quickChipsRow}>
                {['1,000 SKR', '2,500 SKR', '500 SKR', '0.5 SOL'].map((preset) => (
                  <TouchableOpacity
                    key={preset}
                    style={[
                      styles.quickChip,
                      { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder },
                      assetName === preset && { borderColor: colors.primary, backgroundColor: colors.badgeBg },
                    ]}
                    onPress={() => {
                      setAssetName(preset);
                      if (preset === '1,000 SKR') {
                        setReqAmount('20');
                        setProfitAmount('2');
                      } else if (preset === '2,500 SKR') {
                        setReqAmount('50');
                        setProfitAmount('5');
                      } else if (preset === '500 SKR') {
                        setReqAmount('10');
                        setProfitAmount('1');
                      } else if (preset === '0.5 SOL') {
                        setReqAmount('50');
                        setProfitAmount('5');
                      }
                    }}
                  >
                    <Text
                      style={[
                        styles.quickChipText,
                        { color: colors.textSecondary },
                        assetName === preset && { color: colors.primary, fontWeight: '700' },
                      ]}
                    >
                      {preset}
                    </Text>
                  </TouchableOpacity>
                ))}
              </View>
            </View>

            <View style={styles.inputRow}>
              <View style={[styles.inputGroup, { flex: 1 }]}>
                <Text style={[styles.inputLabel, { color: colors.textSecondary }]}>BORROW (USDC)</Text>
                <TextInput
                  style={[styles.inputBox, { backgroundColor: colors.inputBg, borderColor: colors.inputBorder, color: colors.text }]}
                  value={reqAmount}
                  onChangeText={setReqAmount}
                  keyboardType="numeric"
                />
              </View>
              <View style={[styles.inputGroup, { flex: 1, marginLeft: 10 }]}>
                <Text style={[styles.inputLabel, { color: colors.textSecondary }]}>YIELD ($)</Text>
                <TextInput
                  style={[styles.inputBox, { backgroundColor: colors.inputBg, borderColor: colors.inputBorder, color: colors.text }]}
                  value={profitAmount}
                  onChangeText={setProfitAmount}
                  keyboardType="numeric"
                />
              </View>
            </View>

            <View style={styles.inputGroup}>
              <Text style={[styles.inputLabel, { color: colors.textSecondary }]}>DURATION</Text>
              <View style={styles.quickChipsRow}>
                {['7', '14', '30'].map((d) => (
                  <TouchableOpacity
                    key={d}
                    style={[
                      styles.quickChip,
                      { flex: 1, alignItems: 'center', backgroundColor: colors.cardAlt, borderColor: colors.cardBorder },
                      duration === d && { borderColor: colors.primary, backgroundColor: colors.badgeBg },
                    ]}
                    onPress={() => setDuration(d)}
                  >
                    <Text
                      style={[
                        styles.quickChipText,
                        { color: colors.textSecondary },
                        duration === d && { color: colors.primary, fontWeight: '700' },
                      ]}
                    >
                      {d} Days
                    </Text>
                  </TouchableOpacity>
                ))}
              </View>
            </View>

            <TouchableOpacity style={[styles.bumpActionBtn, { backgroundColor: colors.primary }]} onPress={handleCreatePawn}>
              <Text style={[styles.bumpActionText, { color: colors.primaryText }]}>Lock Collateral & List on Solana</Text>
            </TouchableOpacity>

            <TouchableOpacity style={styles.modalCloseBtn} onPress={() => setPawnModal(false)}>
              <Text style={[styles.modalCloseText, { color: colors.textSecondary }]}>Cancel</Text>
            </TouchableOpacity>
          </View>
        </View>
      </Modal>
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  topBar: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: 16,
    paddingVertical: 12,
  },
  segmentControl: {
    flexDirection: 'row',
    borderRadius: 14,
    borderWidth: 1,
    padding: 3,
  },
  segmentBtn: {
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderRadius: 11,
  },
  segmentText: {
    fontSize: 12,
    fontWeight: '600',
  },
  nfcChip: {
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderRadius: 14,
    borderWidth: 1,
  },
  nfcChipText: {
    fontSize: 12,
    fontWeight: '700',
  },
  newPawnChip: {
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderRadius: 14,
  },
  newPawnChipText: {
    fontSize: 12,
    fontWeight: '800',
  },
  scrollContent: {
    padding: 16,
    paddingTop: 4,
    paddingBottom: 40,
  },
  emptyCard: {
    padding: 32,
    borderRadius: 20,
    borderWidth: 1,
    alignItems: 'center',
    marginTop: 20,
  },
  emptyIcon: {
    fontSize: 40,
    marginBottom: 12,
  },
  emptyTitle: {
    fontSize: 16,
    fontWeight: '700',
    marginBottom: 6,
  },
  emptySub: {
    fontSize: 13,
    textAlign: 'center',
    lineHeight: 18,
    marginBottom: 16,
  },
  newPawnActionBtn: {
    paddingHorizontal: 20,
    paddingVertical: 10,
    borderRadius: 12,
  },
  newPawnActionText: {
    fontSize: 14,
    fontWeight: '700',
  },
  deskCard: {
    borderRadius: 20,
    borderWidth: 1,
    padding: 18,
    marginBottom: 16,
  },
  deskHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 14,
  },
  deskTitleRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    flex: 1,
  },
  deskIcon: {
    fontSize: 28,
  },
  deskNameRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
  },
  deskName: {
    fontSize: 16,
    fontWeight: '800',
  },
  verifiedTag: {
    paddingHorizontal: 6,
    paddingVertical: 2,
    borderRadius: 6,
  },
  verifiedTagText: {
    fontSize: 9,
    fontWeight: '800',
  },
  deskAuthority: {
    fontSize: 11,
    marginTop: 2,
  },
  rateCol: {
    alignItems: 'flex-end',
  },
  rateValue: {
    fontSize: 18,
    fontWeight: '800',
  },
  rateLabel: {
    fontSize: 10,
  },
  metricsRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    borderRadius: 12,
    borderWidth: 1,
    padding: 12,
    marginBottom: 14,
  },
  metric: {
    flex: 1,
  },
  mLabel: {
    fontSize: 10,
    marginBottom: 2,
  },
  mValue: {
    fontSize: 13,
    fontWeight: '700',
  },
  borrowDeskBtn: {
    borderRadius: 12,
    borderWidth: 1,
    paddingVertical: 12,
    alignItems: 'center',
  },
  borrowDeskBtnText: {
    fontSize: 13,
    fontWeight: '700',
  },
  fundRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    borderRadius: 12,
    borderWidth: 1,
    padding: 10,
    marginTop: 10,
  },
  fundInputGroup: {
    flex: 1,
  },
  fundDeskBtn: {
    borderRadius: 10,
    paddingVertical: 10,
    paddingHorizontal: 14,
    alignItems: 'center',
    justifyContent: 'center',
    marginTop: 14,
  },
  fundDeskBtnText: {
    fontSize: 13,
    fontWeight: '700',
    color: '#ffffff',
  },
  pawnCard: {
    borderRadius: 20,
    borderWidth: 1,
    padding: 18,
    marginBottom: 16,
  },
  pawnHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 14,
  },
  pawnTitle: {
    fontSize: 15,
    fontWeight: '800',
  },
  pawnBorrower: {
    fontSize: 11,
    marginTop: 2,
  },
  statusChip: {
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 8,
  },
  statusText: {
    fontSize: 10,
    fontWeight: '800',
  },
  fundBtn: {
    height: 48,
    borderRadius: 14,
    justifyContent: 'center',
    alignItems: 'center',
  },
  fundBtnText: {
    fontSize: 14,
    fontWeight: '800',
  },
  fundedNote: {
    paddingVertical: 12,
    alignItems: 'center',
    borderRadius: 12,
  },
  fundedNoteText: {
    fontSize: 12,
    fontWeight: '600',
  },
  modalOverlay: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.7)',
    justifyContent: 'center',
    alignItems: 'center',
    padding: 24,
  },
  modalCard: {
    width: '100%',
    borderRadius: 24,
    borderWidth: 1,
    padding: 24,
    alignItems: 'center',
  },
  modalNfcIcon: {
    fontSize: 48,
    marginBottom: 14,
  },
  modalTitle: {
    fontSize: 18,
    fontWeight: '800',
    marginBottom: 8,
    textAlign: 'center',
  },
  modalDesc: {
    fontSize: 13,
    textAlign: 'center',
    lineHeight: 18,
    marginBottom: 20,
  },
  bumpActionBtn: {
    width: '100%',
    height: 50,
    borderRadius: 16,
    justifyContent: 'center',
    alignItems: 'center',
    marginBottom: 12,
  },
  bumpActionText: {
    fontSize: 15,
    fontWeight: '800',
  },
  modalCloseBtn: {
    paddingVertical: 8,
  },
  modalCloseText: {
    fontSize: 13,
    fontWeight: '600',
  },
  inputGroup: {
    width: '100%',
    marginBottom: 12,
  },
  inputRow: {
    flexDirection: 'row',
    width: '100%',
  },
  inputLabel: {
    fontSize: 10,
    fontWeight: '700',
    marginBottom: 4,
  },
  inputBox: {
    height: 44,
    borderRadius: 12,
    borderWidth: 1,
    paddingHorizontal: 12,
    fontSize: 14,
  },
  escrowRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingVertical: 8,
    paddingHorizontal: 12,
    borderRadius: 10,
    borderWidth: 1,
    marginBottom: 12,
  },
  escrowInfo: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
  },
  escrowLabel: {
    fontSize: 11,
    fontWeight: '600',
  },
  escrowValue: {
    fontSize: 12,
    fontWeight: '700',
    fontFamily: 'monospace',
  },
  solscanChip: {
    paddingHorizontal: 8,
    paddingVertical: 3,
    borderRadius: 6,
  },
  solscanChipText: {
    fontSize: 10,
    fontWeight: '700',
  },
  quickChipsRow: {
    flexDirection: 'row',
    gap: 8,
    marginTop: 8,
  },
  quickChip: {
    paddingHorizontal: 10,
    paddingVertical: 6,
    borderRadius: 8,
    borderWidth: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
  quickChipText: {
    fontSize: 11,
    fontWeight: '600',
  },
  pawnFilterBar: {
    flexDirection: 'row',
    gap: 8,
    marginBottom: 14,
    paddingHorizontal: 2,
  },
  pawnFilterChip: {
    paddingHorizontal: 12,
    paddingVertical: 7,
    borderRadius: 10,
    borderWidth: 1,
  },
  pawnFilterChipText: {
    fontSize: 12,
    fontWeight: '700',
  },
  ownerTag: {
    paddingHorizontal: 6,
    paddingVertical: 2,
    borderRadius: 6,
  },
  ownerTagText: {
    fontSize: 9,
    fontWeight: '800',
  },
  cancelBtn: {
    height: 44,
    paddingHorizontal: 14,
    borderRadius: 12,
    borderWidth: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  cancelBtnText: {
    fontSize: 12,
    fontWeight: '700',
  },
  typeSelectorBtn: {
    flex: 1,
    padding: 12,
    borderRadius: 14,
    borderWidth: 1,
    alignItems: 'center',
  },
  typeSelectorText: {
    fontSize: 13,
    fontWeight: '700',
  },
  typeSelectorSub: {
    fontSize: 10,
    marginTop: 2,
    textAlign: 'center',
  },
  protocolNote: {
    padding: 12,
    borderRadius: 12,
    borderWidth: 1,
    marginBottom: 12,
    width: '100%',
  },
  protocolNoteText: {
    fontSize: 11,
    lineHeight: 16,
    textAlign: 'center',
  },
  tardisShareBtn: {
    height: 40,
    borderRadius: 12,
    borderWidth: 1,
    justifyContent: 'center',
    alignItems: 'center',
    marginTop: 6,
  },
  tardisShareBtnText: {
    color: '#32D4DE',
    fontSize: 12,
    fontWeight: '800',
  },
  tardisIconBtn: {
    height: 48,
    paddingHorizontal: 14,
    borderRadius: 14,
    borderWidth: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  tardisIconBtnText: {
    color: '#32D4DE',
    fontSize: 12,
    fontWeight: '800',
  },
  tardisCircleBadge: {
    paddingHorizontal: 8,
    paddingVertical: 3,
    borderRadius: 6,
    borderWidth: 1,
    borderColor: 'rgba(50, 212, 222, 0.3)',
  },
  tardisCircleBadgeText: {
    color: '#32D4DE',
    fontSize: 10,
    fontWeight: '800',
  },
});
