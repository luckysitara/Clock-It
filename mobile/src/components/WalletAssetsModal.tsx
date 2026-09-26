import React, { useState } from 'react';
import {
  Modal,
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
  ActivityIndicator,
  Alert,
  TextInput,
  Image,
  Linking,
} from 'react-native';
import { PublicKey } from '@solana/web3.js';
import { useTheme } from '../theme/ThemeContext';
import { WalletAssets, SolanaNetwork } from '../types';
import { livePrices } from '../solana/onChainService';

const SOL_LOGO = require('../../assets/tokens/sol.png');
const SKR_LOGO = require('../../assets/tokens/skr.png');
const USDC_LOGO = require('../../assets/tokens/usdc.png');

interface WalletAssetsModalProps {
  visible: boolean;
  onClose: () => void;
  walletAddress: PublicKey;
  skrHandle: string;
  assets: WalletAssets;
  network: SolanaNetwork;
  onRefresh: () => Promise<void>;
  onSwitchAddress?: (pubkey: PublicKey) => void;
  onDisconnect: () => void;
}

export const WalletAssetsModal: React.FC<WalletAssetsModalProps> = ({
  visible,
  onClose,
  walletAddress,
  skrHandle,
  assets,
  network,
  onRefresh,
  onSwitchAddress,
  onDisconnect,
}) => {
  const { colors } = useTheme();
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [showSwitchInput, setShowSwitchInput] = useState(false);
  const [switchAddrText, setSwitchAddrText] = useState('');

  const base58 = walletAddress.toBase58();

  const solHolding = assets.tokenList?.find((t) => t.symbol === 'SOL');
  const skrHolding = assets.tokenList?.find((t) => t.symbol === 'SKR');
  const solPrice = livePrices.sol;
  const skrPrice = livePrices.skr;
  const solUsd = assets.solBalance * solPrice;
  const skrUsd = assets.skrBalance * skrPrice;



  const handleManualRefresh = async () => {
    setIsRefreshing(true);
    try {
      await onRefresh();
    } finally {
      setIsRefreshing(false);
    }
  };

  const handleConfirmSwitch = () => {
    const trimmed = switchAddrText.trim();
    if (!trimmed) {
      Alert.alert('Empty Address', 'Please enter a valid Solana public key.');
      return;
    }
    try {
      const pk = new PublicKey(trimmed);
      if (onSwitchAddress) {
        onSwitchAddress(pk);
        setShowSwitchInput(false);
        setSwitchAddrText('');
      }
    } catch {
      Alert.alert('Invalid Address', 'The address entered is not a valid Solana public key.');
    }
  };

  return (
    <Modal visible={visible} transparent animationType="slide" onRequestClose={onClose}>
      <View style={styles.overlay}>
        <View style={[styles.card, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
          {/* Top Header */}
          <View style={styles.topRow}>
            <View>
              <Text style={[styles.title, { color: colors.text }]}>Seeker Wallet Assets</Text>
              <Text style={[styles.subtitle, { color: colors.textSecondary }]}>
                Hardware Seed Vault • @{skrHandle}
              </Text>
            </View>
            <TouchableOpacity
              style={[styles.closeBtn, { backgroundColor: colors.cardAlt }]}
              onPress={onClose}
              activeOpacity={0.7}
            >
              <Text style={[styles.closeText, { color: colors.textSecondary }]}>✕</Text>
            </TouchableOpacity>
          </View>

          {/* Connected Address Card with Copy & Switch */}
          <View style={[styles.addressBox, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
            <View style={{ flex: 1 }}>
              <Text style={[styles.addressLabel, { color: colors.textMuted }]}>CONNECTED PUBLIC KEY</Text>
              <Text style={[styles.addressFull, { color: colors.text }]} numberOfLines={1} ellipsizeMode="middle">
                {base58}
              </Text>
            </View>
            <TouchableOpacity
              style={[styles.copyBtn, { backgroundColor: colors.badgeBg }]}
              onPress={() => Alert.alert('Connected Address', base58)}
              activeOpacity={0.7}
            >
              <Text style={[styles.copyBtnText, { color: colors.primary }]}>Copy</Text>
            </TouchableOpacity>
            {onSwitchAddress && (
              <TouchableOpacity
                style={[styles.switchAddrBtn, { backgroundColor: colors.badgeBg }]}
                onPress={() => setShowSwitchInput(!showSwitchInput)}
                activeOpacity={0.7}
              >
                <Text style={[styles.switchAddrBtnText, { color: colors.primary }]}>
                  {showSwitchInput ? 'Hide' : 'Switch'}
                </Text>
              </TouchableOpacity>
            )}
          </View>

          {/* Expandable Switch Address Input */}
          {showSwitchInput && (
            <View style={[styles.switchInputBox, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
              <TextInput
                style={[
                  styles.switchInput,
                  { backgroundColor: colors.card, color: colors.text, borderColor: colors.cardBorder },
                ]}
                placeholder="Paste Solana address to inspect (watch-only)..."
                placeholderTextColor={colors.textMuted}
                value={switchAddrText}
                onChangeText={setSwitchAddrText}
                autoCapitalize="none"
                autoCorrect={false}
              />
              <TouchableOpacity
                style={[styles.switchConfirmBtn, { backgroundColor: colors.primary }]}
                onPress={handleConfirmSwitch}
                activeOpacity={0.8}
              >
                <Text style={[styles.switchConfirmText, { color: colors.primaryText }]}>Load</Text>
              </TouchableOpacity>
            </View>
          )}

          <ScrollView showsVerticalScrollIndicator={false} contentContainerStyle={styles.scroll}>
            {/* Total Balance Card */}
            <View style={[styles.totalBanner, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
              <View>
                <View style={styles.totalRow}>
                  <Text style={[styles.totalLabel, { color: colors.textMuted }]}>
                    {network === 'mainnet-beta' ? 'MAINNET PORTFOLIO VALUE' : 'DEVNET PORTFOLIO VALUE'}
                  </Text>
                  <View
                    style={[
                      styles.networkPill,
                      {
                        backgroundColor:
                          network === 'mainnet-beta'
                            ? 'rgba(168, 85, 247, 0.15)'
                            : 'rgba(16, 185, 129, 0.15)',
                      },
                    ]}
                  >
                    <Text
                      style={[
                        styles.networkPillText,
                        { color: network === 'mainnet-beta' ? '#A855F7' : '#10B981' },
                      ]}
                    >
                      {network === 'mainnet-beta' ? 'MAINNET' : 'DEVNET'}
                    </Text>
                  </View>
                </View>
                <Text style={[styles.totalVal, { color: colors.text }]}>
                  ${assets.totalUsdValue.toFixed(2)}
                </Text>
              </View>
              <View style={[styles.identityTag, { backgroundColor: colors.badgeBg, borderColor: colors.badgeBorder }]}>
                <Text style={[styles.identityTagText, { color: colors.primary }]}>@{skrHandle}</Text>
              </View>
            </View>

            {/* Asset Rows */}
            <Text style={[styles.sectionLabel, { color: colors.textSecondary }]}>ASSETS & HOLDINGS</Text>

            {/* 1. SOL */}
            <View style={[styles.assetItem, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
              <View style={styles.assetLeft}>
                <View style={[styles.assetIconBox, { backgroundColor: 'rgba(0, 255, 163, 0.15)' }]}>
                  <Image source={SOL_LOGO} style={styles.assetLogo} resizeMode="contain" />
                </View>
                <View>
                  <Text style={[styles.assetName, { color: colors.text }]}>Solana (SOL)</Text>
                  <Text style={[styles.assetSymbol, { color: colors.textMuted }]}>Native Gas & Collateral</Text>
                </View>
              </View>
              <View style={styles.assetRight}>
                <Text style={[styles.assetAmount, { color: colors.text }]}>{assets.solBalance.toFixed(3)} SOL</Text>
                <Text style={[styles.assetUsd, { color: colors.textSecondary }]}>
                  ≈ ${solUsd.toFixed(2)}
                </Text>
              </View>
            </View>

            {/* 2. USDC */}
            <View style={[styles.assetItem, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
              <View style={styles.assetLeft}>
                <View style={[styles.assetIconBox, { backgroundColor: 'rgba(59, 130, 246, 0.15)' }]}>
                  <Image source={USDC_LOGO} style={styles.assetLogo} resizeMode="contain" />
                </View>
                <View>
                  <Text style={[styles.assetName, { color: colors.text }]}>USD Coin (USDC)</Text>
                  <Text style={[styles.assetSymbol, { color: colors.textMuted }]}>Lending Liquidity</Text>
                </View>
              </View>
              <View style={styles.assetRight}>
                <Text style={[styles.assetAmount, { color: colors.text }]}>{assets.usdcBalance.toFixed(2)} USDC</Text>
                <Text style={[styles.assetUsd, { color: colors.textSecondary }]}>
                  ≈ ${assets.usdcBalance.toFixed(2)}
                </Text>
              </View>
            </View>

            {/* 3. SKR Token */}
            <View style={[styles.assetItem, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
              <View style={styles.assetLeft}>
                <View style={[styles.assetIconBox, { backgroundColor: 'rgba(139, 92, 246, 0.15)' }]}>
                  <Image source={SKR_LOGO} style={styles.assetLogo} resizeMode="contain" />
                </View>
                <View>
                  <Text style={[styles.assetName, { color: colors.text }]}>Seeker Token (SKR)</Text>
                  <Text style={[styles.assetSymbol, { color: colors.textMuted }]}>Reputation & Staking</Text>
                </View>
              </View>
              <View style={styles.assetRight}>
                <Text style={[styles.assetAmount, { color: colors.text }]}>
                  {assets.skrBalance > 1000 ? assets.skrBalance.toLocaleString() : assets.skrBalance.toFixed(0)} SKR
                </Text>
                <Text style={[styles.assetUsd, { color: colors.textSecondary }]}>
                  ≈ ${skrUsd.toFixed(2)}
                </Text>
              </View>
            </View>

            {/* 4. BONK (if held) */}
            {assets.bonkBalance > 0 && (
              <View style={[styles.assetItem, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
                <View style={styles.assetLeft}>
                  <View style={[styles.assetIconBox, { backgroundColor: 'rgba(234, 88, 12, 0.15)' }]}>
                    <Text style={styles.assetIcon}>🐕</Text>
                  </View>
                  <View>
                    <Text style={[styles.assetName, { color: colors.text }]}>Bonk (BONK)</Text>
                    <Text style={[styles.assetSymbol, { color: colors.textMuted }]}>Community Token</Text>
                  </View>
                </View>
                <View style={styles.assetRight}>
                  <Text style={[styles.assetAmount, { color: colors.text }]}>{assets.bonkBalance.toLocaleString()}</Text>
                  <Text style={[styles.assetUsd, { color: colors.textSecondary }]}>
                    ≈ ${(assets.bonkBalance * 0.00002).toFixed(2)}
                  </Text>
                </View>
              </View>
            )}

            {/* 5. Additional detected tokens */}
            {assets.tokenList &&
              assets.tokenList
                .filter((t) => t.symbol !== 'USDC' && t.symbol !== 'SKR' && t.symbol !== 'BONK' && t.symbol !== 'SGT')
                .map((token, idx) => (
                  <View key={token.mint + idx} style={[styles.assetItem, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
                    <View style={styles.assetLeft}>
                      <View style={[styles.assetIconBox, { backgroundColor: 'rgba(16, 185, 129, 0.15)' }]}>
                        <Text style={styles.assetIcon}>🪙</Text>
                      </View>
                      <View>
                        <Text style={[styles.assetName, { color: colors.text }]}>{token.name}</Text>
                        <Text style={[styles.assetSymbol, { color: colors.textMuted }]}>
                          {token.symbol} {token.isToken2022 ? '• Token-2022' : ''}
                        </Text>
                      </View>
                    </View>
                    <View style={styles.assetRight}>
                      <Text style={[styles.assetAmount, { color: colors.text }]}>
                        {token.amount > 1000 ? token.amount.toLocaleString() : token.amount.toFixed(2)}
                      </Text>
                    </View>
                  </View>
                ))}

            {/* 6. Seeker Genesis NFT */}
            <View style={[styles.assetItem, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
              <View style={styles.assetLeft}>
                <View style={[styles.assetIconBox, { backgroundColor: 'rgba(245, 158, 11, 0.15)' }]}>
                  <Text style={styles.assetIcon}>📱</Text>
                </View>
                <View>
                  <Text style={[styles.assetName, { color: colors.text }]}>Seeker Genesis Token</Text>
                  <Text style={[styles.assetSymbol, { color: colors.textMuted }]}>Device Hardware Enclave</Text>
                </View>
              </View>
              <View style={styles.assetRight}>
                <View style={[styles.verifiedPill, { backgroundColor: colors.badgeBg }]}>
                  <Text style={[styles.verifiedPillText, { color: colors.primary }]}>
                    {assets.hasSeekerGenesisToken ? 'VERIFIED' : 'NOT DETECTED'}
                  </Text>
                </View>
              </View>
            </View>

            {/* Quick Actions */}
            <View style={styles.actionGrid}>
              <TouchableOpacity
                style={[styles.refreshBtn, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}
                onPress={handleManualRefresh}
                disabled={isRefreshing}
                activeOpacity={0.7}
              >
                {isRefreshing ? (
                  <ActivityIndicator color={colors.textSecondary} />
                ) : (
                  <Text style={[styles.refreshBtnText, { color: colors.text }]}>🔄 Refresh Balances</Text>
                )}
              </TouchableOpacity>
            </View>

            <TouchableOpacity style={styles.disconnectLink} onPress={onDisconnect}>
              <Text style={[styles.disconnectLinkText, { color: colors.danger }]}>
                Logout
              </Text>
            </TouchableOpacity>
          </ScrollView>
        </View>
      </View>
    </Modal>
  );
};

const styles = StyleSheet.create({
  overlay: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.6)',
    justifyContent: 'flex-end',
  },
  card: {
    borderTopLeftRadius: 28,
    borderTopRightRadius: 28,
    borderTopWidth: 1,
    padding: 20,
    maxHeight: '90%',
  },
  topRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 14,
  },
  title: {
    fontSize: 20,
    fontWeight: '800',
  },
  subtitle: {
    fontSize: 12,
    marginTop: 2,
  },
  closeBtn: {
    width: 32,
    height: 32,
    borderRadius: 16,
    justifyContent: 'center',
    alignItems: 'center',
  },
  closeText: {
    fontSize: 14,
    fontWeight: '700',
  },
  networkSegment: {
    flexDirection: 'row',
    borderRadius: 14,
    borderWidth: 1,
    padding: 3,
    marginBottom: 12,
  },
  segmentBtn: {
    flex: 1,
    flexDirection: 'row',
    justifyContent: 'center',
    alignItems: 'center',
    paddingVertical: 9,
    borderRadius: 11,
    gap: 6,
  },
  segmentBtnActive: {
    borderWidth: 1,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.06,
    shadowRadius: 4,
    elevation: 1,
  },
  segDot: {
    width: 7,
    height: 7,
    borderRadius: 4,
  },
  segmentText: {
    fontSize: 13,
    fontWeight: '600',
  },
  segmentTextBold: {
    fontWeight: '800',
  },
  addressBox: {
    flexDirection: 'row',
    alignItems: 'center',
    padding: 12,
    borderRadius: 14,
    borderWidth: 1,
    marginBottom: 12,
    gap: 8,
  },
  addressLabel: {
    fontSize: 9,
    fontWeight: '800',
    letterSpacing: 0.5,
    marginBottom: 2,
  },
  addressFull: {
    fontSize: 12,
    fontWeight: '700',
  },
  copyBtn: {
    paddingHorizontal: 8,
    paddingVertical: 5,
    borderRadius: 8,
  },
  copyBtnText: {
    fontSize: 11,
    fontWeight: '800',
  },
  switchAddrBtn: {
    paddingHorizontal: 8,
    paddingVertical: 5,
    borderRadius: 8,
  },
  switchAddrBtnText: {
    fontSize: 11,
    fontWeight: '800',
  },
  switchInputBox: {
    flexDirection: 'row',
    padding: 10,
    borderRadius: 14,
    borderWidth: 1,
    marginBottom: 12,
    gap: 8,
  },
  switchInput: {
    flex: 1,
    height: 40,
    borderRadius: 10,
    borderWidth: 1,
    paddingHorizontal: 10,
    fontSize: 12,
  },
  switchConfirmBtn: {
    paddingHorizontal: 14,
    borderRadius: 10,
    justifyContent: 'center',
    alignItems: 'center',
  },
  switchConfirmText: {
    fontSize: 12,
    fontWeight: '800',
  },
  scroll: {
    paddingBottom: 24,
  },
  totalBanner: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    borderRadius: 18,
    borderWidth: 1,
    padding: 16,
    marginBottom: 20,
  },
  totalRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    marginBottom: 4,
  },
  totalLabel: {
    fontSize: 10,
    fontWeight: '800',
    letterSpacing: 0.5,
  },
  networkPill: {
    paddingHorizontal: 6,
    paddingVertical: 2,
    borderRadius: 6,
  },
  networkPillText: {
    fontSize: 9,
    fontWeight: '800',
    letterSpacing: 0.5,
  },
  totalVal: {
    fontSize: 28,
    fontWeight: '900',
  },
  identityTag: {
    paddingHorizontal: 10,
    paddingVertical: 6,
    borderRadius: 12,
    borderWidth: 1,
  },
  identityTagText: {
    fontSize: 12,
    fontWeight: '800',
  },
  sectionLabel: {
    fontSize: 11,
    fontWeight: '800',
    letterSpacing: 0.5,
    marginBottom: 10,
  },
  assetItem: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: 14,
    borderRadius: 16,
    borderWidth: 1,
    marginBottom: 10,
  },
  assetLeft: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    flex: 1,
  },
  assetIconBox: {
    width: 40,
    height: 40,
    borderRadius: 12,
    justifyContent: 'center',
    alignItems: 'center',
  },
  assetIcon: {
    fontSize: 20,
  },
  assetLogo: {
    width: 26,
    height: 26,
    borderRadius: 13,
  },
  assetName: {
    fontSize: 14,
    fontWeight: '700',
  },
  assetSymbol: {
    fontSize: 11,
    marginTop: 2,
  },
  assetRight: {
    alignItems: 'flex-end',
  },
  assetAmount: {
    fontSize: 14,
    fontWeight: '800',
  },
  assetUsd: {
    fontSize: 11,
    marginTop: 2,
  },
  verifiedPill: {
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 8,
  },
  verifiedPillText: {
    fontSize: 10,
    fontWeight: '800',
  },
  actionGrid: {
    gap: 10,
    marginTop: 14,
  },
  actionBtn: {
    height: 50,
    borderRadius: 16,
    justifyContent: 'center',
    alignItems: 'center',
  },
  actionBtnText: {
    fontSize: 14,
    fontWeight: '800',
  },
  refreshBtn: {
    height: 46,
    borderRadius: 16,
    borderWidth: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  refreshBtnText: {
    fontSize: 13,
    fontWeight: '700',
  },
  disconnectLink: {
    alignItems: 'center',
    paddingVertical: 14,
  },
  disconnectLinkText: {
    fontSize: 12,
    fontWeight: '700',
  },
});
