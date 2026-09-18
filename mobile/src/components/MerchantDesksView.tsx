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

interface MerchantDesksViewProps {
  pools: LendingPool[];
  offers: P2POffer[];
  onSelectPool: (pool: LendingPool) => void;
  onFundPawnOffer: (offerId: number) => void;
  onCreatePawnOffer: (name: string, reqAmount: number, profit: number, days: number) => void;
  onNfcBumpCircle: () => void;
}

export const MerchantDesksView: React.FC<MerchantDesksViewProps> = ({
  pools,
  offers,
  onSelectPool,
  onFundPawnOffer,
  onCreatePawnOffer,
  onNfcBumpCircle,
}) => {
  const { colors } = useTheme();
  const [subTab, setSubTab] = useState<'POOLS' | 'PAWNS'>('POOLS');
  const [nfcModal, setNfcModal] = useState<boolean>(false);
  const [isNfcActive, setIsNfcActive] = useState<boolean>(false);

  // New pawn modal
  const [pawnModal, setPawnModal] = useState<boolean>(false);
  const [assetName, setAssetName] = useState<string>('0.5 SOL');
  const [reqAmount, setReqAmount] = useState<string>('50');
  const [profitAmount, setProfitAmount] = useState<string>('5');
  const [duration, setDuration] = useState<string>('7');

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
          <TouchableOpacity
            style={[styles.nfcChip, { backgroundColor: colors.badgeBg, borderColor: colors.badgeBorder }]}
            onPress={() => setNfcModal(true)}
            activeOpacity={0.7}
          >
            <Text style={[styles.nfcChipText, { color: colors.primary }]}>📡 Bump NFC</Text>
          </TouchableOpacity>
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
                <Text style={[styles.emptyTitle, { color: colors.text }]}>Discovering Lending Desks</Text>
                <Text style={[styles.emptySub, { color: colors.textSecondary }]}>
                  Searching for active peer lending desks...
                </Text>
              </View>
            ) : (
              pools.map((pool) => (
                <View
                  key={pool.id}
                  style={[styles.deskCard, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}
                >
                  <View style={styles.deskHeader}>
                    <View style={styles.deskTitleRow}>
                      <Text style={styles.deskIcon}>{pool.poolType === 'Circle' ? '⭕' : '🏛️'}</Text>
                      <View>
                        <View style={styles.deskNameRow}>
                          <Text style={[styles.deskName, { color: colors.text }]}>{pool.name}</Text>
                          {pool.isVerifiedMerchant && (
                            <View style={[styles.verifiedTag, { backgroundColor: colors.badgeBg }]}>
                              <Text style={[styles.verifiedTagText, { color: colors.primary }]}>VERIFIED</Text>
                            </View>
                          )}
                        </View>
                        <Text style={[styles.deskAuthority, { color: colors.textMuted }]} numberOfLines={1}>
                          {pool.authority.slice(0, 4)}...{pool.authority.slice(-4)}
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
                      <Text style={[styles.mValue, { color: colors.primary }]}>{pool.successRate}%</Text>
                    </View>
                  </View>

                  <TouchableOpacity
                    style={[styles.borrowDeskBtn, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}
                    onPress={() => onSelectPool(pool)}
                    activeOpacity={0.7}
                  >
                    <Text style={[styles.borrowDeskBtnText, { color: colors.text }]}>Borrow from this Desk →</Text>
                  </TouchableOpacity>
                </View>
              ))
            )}
          </View>
        )}

        {/* SUBTAB 2: P2P PAWNS */}
        {subTab === 'PAWNS' && (
          <View>
            {offers.length === 0 ? (
              <View style={[styles.emptyCard, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
                <Text style={styles.emptyIcon}>🃏</Text>
                <Text style={[styles.emptyTitle, { color: colors.text }]}>No P2P Pawns Listed Yet</Text>
                <Text style={[styles.emptySub, { color: colors.textSecondary }]}>
                  Be the first to list a cNFT or digital asset for peer funding!
                </Text>
                <TouchableOpacity
                  style={[styles.newPawnActionBtn, { backgroundColor: colors.primary }]}
                  onPress={() => setPawnModal(true)}
                  activeOpacity={0.8}
                >
                  <Text style={[styles.newPawnActionText, { color: colors.primaryText }]}>+ List First Pawn</Text>
                </TouchableOpacity>
              </View>
            ) : (
              offers.map((offer) => (
                <View
                  key={offer.id}
                  style={[styles.pawnCard, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}
                >
                  <View style={styles.pawnHeader}>
                    <View>
                      <Text style={[styles.pawnTitle, { color: colors.text }]}>{offer.collateralName}</Text>
                      <Text style={[styles.pawnBorrower, { color: colors.textMuted }]}>
                        Creator: {offer.creator.slice(0, 4)}...{offer.creator.slice(-4)}
                      </Text>
                    </View>
                    <View style={[styles.statusChip, { backgroundColor: colors.badgeBg }]}>
                      <Text style={[styles.statusText, { color: colors.primary }]}>{offer.status.toUpperCase()}</Text>
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

                  {offer.status === 'Open' ? (
                    <TouchableOpacity
                      style={[styles.fundBtn, { backgroundColor: colors.primary }]}
                      onPress={() => onFundPawnOffer(offer.id)}
                      activeOpacity={0.85}
                    >
                      <Text style={[styles.fundBtnText, { color: colors.primaryText }]}>
                        ⚡ Fund & Earn +${offer.interestOffered} USDC
                      </Text>
                    </TouchableOpacity>
                  ) : (
                    <View style={[styles.fundedNote, { backgroundColor: colors.cardAlt }]}>
                      <Text style={[styles.fundedNoteText, { color: colors.textMuted }]}>🔒 Funded & Escrowed</Text>
                    </View>
                  )}
                </View>
              ))
            )}
          </View>
        )}
      </ScrollView>

      {/* NFC BUMP MODAL */}
      <Modal visible={nfcModal} transparent animationType="fade">
        <View style={styles.modalOverlay}>
          <View style={[styles.modalCard, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
            <Text style={styles.modalNfcIcon}>📡</Text>
            <Text style={[styles.modalTitle, { color: colors.text }]}>Seeker Phone Bump (NFC)</Text>
            <Text style={[styles.modalDesc, { color: colors.textSecondary }]}>
              Hold your Seeker smartphone back-to-back with a trusted peer to instantly establish an authenticated lending circle.
            </Text>

            <TouchableOpacity
              style={[styles.bumpActionBtn, { backgroundColor: colors.primary }]}
              onPress={triggerNfcBump}
              disabled={isNfcActive}
            >
              <Text style={[styles.bumpActionText, { color: colors.primaryText }]}>
                {isNfcActive ? 'Reading Hardware NFC...' : 'Simulate Phone Bump'}
              </Text>
            </TouchableOpacity>

            <TouchableOpacity style={styles.modalCloseBtn} onPress={() => setNfcModal(false)}>
              <Text style={[styles.modalCloseText, { color: colors.textSecondary }]}>Cancel</Text>
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
                placeholder="e.g. 0.5 SOL or 500 SKR"
                placeholderTextColor={colors.textMuted}
              />
              <View style={styles.quickChipsRow}>
                {['0.5 SOL', '1.0 SOL', '500 SKR', 'Saga Monke NFT'].map((preset) => (
                  <TouchableOpacity
                    key={preset}
                    style={[
                      styles.quickChip,
                      { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder },
                      assetName === preset && { borderColor: colors.primary, backgroundColor: colors.badgeBg },
                    ]}
                    onPress={() => {
                      setAssetName(preset);
                      if (preset === '0.5 SOL') {
                        setReqAmount('50');
                        setProfitAmount('5');
                      } else if (preset === '1.0 SOL') {
                        setReqAmount('100');
                        setProfitAmount('9');
                      } else if (preset === '500 SKR') {
                        setReqAmount('10');
                        setProfitAmount('1.5');
                      } else {
                        setReqAmount('150');
                        setProfitAmount('12');
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
              <Text style={[styles.bumpActionText, { color: colors.primaryText }]}>Lock Collateral & List on Devnet</Text>
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
});
