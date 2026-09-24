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
} from 'react-native';
import { LinearGradient } from 'expo-linear-gradient';
import { P2POffer } from '../types';

interface CircleDeckViewProps {
  offers: P2POffer[];
  onFundOffer: (offerId: number) => void;
  onCreateOffer: (collateralName: string, requestedAmount: number, interestOffered: number, durationDays: number) => void;
}

export const CircleDeckView: React.FC<CircleDeckViewProps> = ({
  offers,
  onFundOffer,
  onCreateOffer,
}) => {
  const [createModalVisible, setCreateModalVisible] = useState<boolean>(false);
  const [collateralName, setCollateralName] = useState<string>('500 SKR Token');
  const [requestedAmount, setRequestedAmount] = useState<string>('200');
  const [interestOffered, setInterestOffered] = useState<string>('12');
  const [durationDays, setDurationDays] = useState<string>('14');

  const handleCreateCard = () => {
    const numReq = parseFloat(requestedAmount);
    const numInt = parseFloat(interestOffered);
    const numDur = parseInt(durationDays, 10);

    if (!collateralName || isNaN(numReq) || numReq <= 0 || isNaN(numInt) || isNaN(numDur)) {
      Alert.alert('Invalid Fields', 'Please enter valid loan terms.');
      return;
    }

    onCreateOffer(collateralName, numReq, numInt, numDur);
    setCreateModalVisible(false);
    Alert.alert('🃏 Pawn Card Listed!', `Listed "${collateralName}" requesting ${numReq} USDC on Circle Deck.`);
  };

  const shorten = (addr: string) => `${addr.slice(0, 4)}...${addr.slice(-4)}`;

  return (
    <View style={styles.container}>
      {/* Top Header */}
      <View style={styles.topSection}>
        <View style={styles.headingRow}>
          <View>
            <Text style={styles.heading}>Circle <Text style={styles.highlight}>Pawn Deck</Text></Text>
            <Text style={styles.subheading}>1-on-1 social micro-credit against SOL & SKR tokens</Text>
          </View>
          <TouchableOpacity
            style={styles.createButton}
            onPress={() => setCreateModalVisible(true)}
            activeOpacity={0.8}
          >
            <Text style={styles.createButtonText}>+ New Pawn</Text>
          </TouchableOpacity>
        </View>
      </View>

      {/* Offers List */}
      <ScrollView style={styles.list} contentContainerStyle={styles.listContent} showsVerticalScrollIndicator={false}>
        {offers.map((offer) => {
          const isFunded = offer.status === 'Funded';
          return (
            <View key={offer.id} style={styles.offerCard}>
              {/* Card Header */}
              <View style={styles.cardHeader}>
                <View style={styles.creatorRow}>
                  <View style={styles.avatar}>
                    <Text style={styles.avatarText}>👤</Text>
                  </View>
                  <View>
                    <Text style={styles.creatorName}>{shorten(offer.creator)}</Text>
                    <Text style={styles.listedTime}>Listed {offer.durationDays}d tenure</Text>
                  </View>
                </View>

                <View style={styles.assetTypeBadge}>
                  <Text style={styles.assetTypeText}>
                    {offer.collateralType === 'cNFT' ? '📱 cNFT' : offer.collateralType === 'NFT' ? '🖼️ NFT' : '🪙 Token'}
                  </Text>
                </View>
              </View>

              {/* Collateral Showcase Box */}
              <View style={styles.assetShowcase}>
                <Text style={styles.assetIcon}>
                  {offer.collateralType === 'cNFT' ? '📱' : offer.collateralType === 'NFT' ? '🎨' : '◎'}
                </Text>
                <View style={styles.assetDetails}>
                  <Text style={styles.assetName}>{offer.collateralName}</Text>
                  <Text style={styles.assetEscrowNote}>🔒 Locked in ClockLend Escrow PDA</Text>
                </View>
              </View>

              <View style={styles.divider} />

              {/* Financial Terms */}
              <View style={styles.termsRow}>
                <View>
                  <Text style={styles.termLabel}>Borrow Request</Text>
                  <Text style={styles.termValue}>${offer.requestedAmount} USDC</Text>
                </View>

                <View style={styles.profitBox}>
                  <Text style={styles.profitLabel}>Funder Profit</Text>
                  <Text style={styles.profitValue}>+${offer.interestOffered} USDC</Text>
                </View>

                <View>
                  <Text style={styles.termLabel}>Status</Text>
                  <Text style={[styles.statusText, isFunded ? styles.fundedText : styles.openText]}>
                    {isFunded ? '✅ FUNDED' : '🟢 OPEN'}
                  </Text>
                </View>
              </View>

              {/* Action Button */}
              {!isFunded ? (
                <TouchableOpacity
                  style={styles.fundButton}
                  onPress={() => onFundOffer(offer.id)}
                  activeOpacity={0.8}
                >
                  <LinearGradient
                    colors={['#572DFD', '#23ABF4']}
                    start={{ x: 0, y: 0 }}
                    end={{ x: 1, y: 0 }}
                    style={styles.fundGradient}
                  >
                    <Text style={styles.fundButtonText}>
                      🤝 FUND LOAN & EARN +${offer.interestOffered}
                    </Text>
                  </LinearGradient>
                </TouchableOpacity>
              ) : (
                <View style={styles.fundedBanner}>
                  <Text style={styles.fundedBannerText}>Loan is Active • Due in {offer.durationDays} days</Text>
                </View>
              )}
            </View>
          );
        })}
      </ScrollView>

      {/* Create Pawn Card Modal */}
      <Modal visible={createModalVisible} transparent animationType="slide">
        <View style={styles.modalOverlay}>
          <View style={styles.modalContent}>
            <Text style={styles.modalTitle}>🃏 List Pawn Card</Text>
            <Text style={styles.modalSubtitle}>
              Deposit your NFT or token into Escrow and request custom micro-credit from circle peers
            </Text>

            <View style={styles.formGroup}>
              <Text style={styles.inputLabel}>COLLATERAL ASSET</Text>
              <TextInput
                style={styles.modalInput}
                value={collateralName}
                onChangeText={setCollateralName}
                placeholder="e.g. 500 SKR or 1.5 SOL"
                placeholderTextColor="#64748B"
              />
            </View>

            <View style={styles.formRow}>
              <View style={[styles.formGroup, { flex: 1 }]}>
                <Text style={styles.inputLabel}>BORROW (USDC)</Text>
                <TextInput
                  style={styles.modalInput}
                  value={requestedAmount}
                  onChangeText={setRequestedAmount}
                  keyboardType="numeric"
                  placeholder="200"
                  placeholderTextColor="#64748B"
                />
              </View>

              <View style={[styles.formGroup, { flex: 1 }]}>
                <Text style={styles.inputLabel}>OFFER PROFIT ($)</Text>
                <TextInput
                  style={styles.modalInput}
                  value={interestOffered}
                  onChangeText={setInterestOffered}
                  keyboardType="numeric"
                  placeholder="12"
                  placeholderTextColor="#64748B"
                />
              </View>
            </View>

            <View style={styles.formGroup}>
              <Text style={styles.inputLabel}>DURATION (DAYS)</Text>
              <TextInput
                style={styles.modalInput}
                value={durationDays}
                onChangeText={setDurationDays}
                keyboardType="numeric"
                placeholder="14"
                placeholderTextColor="#64748B"
              />
            </View>

            <TouchableOpacity style={styles.submitBtn} onPress={handleCreateCard} activeOpacity={0.8}>
              <Text style={styles.submitBtnText}>🔒 Lock Collateral & List Card</Text>
            </TouchableOpacity>

            <TouchableOpacity style={styles.cancelBtn} onPress={() => setCreateModalVisible(false)}>
              <Text style={styles.cancelBtnText}>Cancel</Text>
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
    backgroundColor: '#090D16',
  },
  topSection: {
    paddingHorizontal: 16,
    paddingTop: 12,
    paddingBottom: 8,
  },
  headingRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  heading: {
    fontSize: 22,
    fontWeight: '900',
    color: '#FFFFFF',
  },
  highlight: {
    color: '#F7A600',
  },
  subheading: {
    color: '#64748B',
    fontSize: 12,
    marginTop: 4,
  },
  createButton: {
    backgroundColor: '#F7A600',
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderRadius: 8,
  },
  createButtonText: {
    color: '#000000',
    fontSize: 12,
    fontWeight: '900',
  },
  list: {
    flex: 1,
  },
  listContent: {
    padding: 16,
    paddingBottom: 40,
  },
  offerCard: {
    backgroundColor: '#111827',
    borderRadius: 14,
    padding: 16,
    marginBottom: 14,
    borderWidth: 1,
    borderColor: '#1F2937',
  },
  cardHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 12,
  },
  creatorRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
  },
  avatar: {
    width: 32,
    height: 32,
    borderRadius: 16,
    backgroundColor: '#1F2937',
    justifyContent: 'center',
    alignItems: 'center',
  },
  avatarText: {
    fontSize: 14,
  },
  creatorName: {
    color: '#F1F5F9',
    fontSize: 13,
    fontWeight: '700',
    fontFamily: 'monospace',
  },
  listedTime: {
    color: '#64748B',
    fontSize: 11,
  },
  assetTypeBadge: {
    backgroundColor: 'rgba(153, 69, 255, 0.15)',
    paddingHorizontal: 8,
    paddingVertical: 3,
    borderRadius: 6,
    borderWidth: 1,
    borderColor: 'rgba(153, 69, 255, 0.3)',
  },
  assetTypeText: {
    color: '#C084FC',
    fontSize: 11,
    fontWeight: '700',
  },
  assetShowcase: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#0A0E1A',
    padding: 12,
    borderRadius: 10,
    gap: 12,
    borderWidth: 1,
    borderColor: '#1E293B',
  },
  assetIcon: {
    fontSize: 28,
  },
  assetDetails: {
    flex: 1,
  },
  assetName: {
    color: '#FFFFFF',
    fontSize: 14,
    fontWeight: '800',
  },
  assetEscrowNote: {
    color: '#23ABF4',
    fontSize: 11,
    marginTop: 2,
  },
  divider: {
    height: 1,
    backgroundColor: '#1E293B',
    marginVertical: 12,
  },
  termsRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 14,
  },
  termLabel: {
    color: '#64748B',
    fontSize: 10,
    marginBottom: 2,
  },
  termValue: {
    color: '#FFFFFF',
    fontSize: 14,
    fontWeight: '800',
  },
  profitBox: {
    alignItems: 'center',
  },
  profitLabel: {
    color: '#64748B',
    fontSize: 10,
    marginBottom: 2,
  },
  profitValue: {
    color: '#23ABF4',
    fontSize: 15,
    fontWeight: '900',
  },
  statusText: {
    fontSize: 12,
    fontWeight: '800',
  },
  openText: {
    color: '#23ABF4',
  },
  fundedText: {
    color: '#60A5FA',
  },
  fundButton: {
    borderRadius: 8,
    overflow: 'hidden',
  },
  fundGradient: {
    paddingVertical: 12,
    alignItems: 'center',
  },
  fundButtonText: {
    color: '#000000',
    fontSize: 13,
    fontWeight: '900',
  },
  fundedBanner: {
    backgroundColor: '#1E293B',
    paddingVertical: 10,
    borderRadius: 8,
    alignItems: 'center',
  },
  fundedBannerText: {
    color: '#94A3B8',
    fontSize: 12,
    fontWeight: '700',
  },
  modalOverlay: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.85)',
    justifyContent: 'center',
    padding: 20,
  },
  modalContent: {
    backgroundColor: '#111827',
    borderRadius: 16,
    padding: 20,
    borderWidth: 1,
    borderColor: '#374151',
  },
  modalTitle: {
    color: '#FFFFFF',
    fontSize: 18,
    fontWeight: '900',
    marginBottom: 4,
  },
  modalSubtitle: {
    color: '#94A3B8',
    fontSize: 12,
    lineHeight: 16,
    marginBottom: 16,
  },
  formGroup: {
    marginBottom: 12,
  },
  formRow: {
    flexDirection: 'row',
    gap: 10,
  },
  inputLabel: {
    color: '#94A3B8',
    fontSize: 10,
    fontWeight: '800',
    marginBottom: 4,
  },
  modalInput: {
    backgroundColor: '#090D16',
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 10,
    color: '#FFFFFF',
    fontSize: 13,
    borderWidth: 1,
    borderColor: '#1F2937',
  },
  submitBtn: {
    backgroundColor: '#F7A600',
    paddingVertical: 13,
    borderRadius: 8,
    alignItems: 'center',
    marginTop: 8,
  },
  submitBtnText: {
    color: '#000000',
    fontSize: 13,
    fontWeight: '900',
  },
  cancelBtn: {
    alignItems: 'center',
    marginTop: 12,
  },
  cancelBtnText: {
    color: '#64748B',
    fontSize: 12,
  },
});
