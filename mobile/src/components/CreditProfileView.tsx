import React, { useState, useEffect } from 'react';
import { View, Text, StyleSheet, TouchableOpacity, ScrollView, Alert, Linking, TextInput, Switch } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { useTheme } from '../theme/ThemeContext';
import { UserProfile, WalletAssets } from '../types';
import { PROGRAM_ID } from '../solana/program';
import {
  isLockEnabled,
  setLockEnabled,
  isBiometricsEnabled,
  setBiometricsEnabled,
  checkBiometricHardware,
  getUserPin,
} from '../services/securityService';

interface CreditProfileViewProps {
  userProfile: UserProfile;
  skrHandle: string;
  walletAssets?: WalletAssets;
  onStakeSkr: (amount: number) => void;
  onUnstakeSkr?: (amount: number) => void;
  onOpenAssetsModal?: () => void;
  onDisconnectWallet: () => void;
  onLockApp?: () => void;
  onSetupPin?: () => void;
  onChangePin?: () => void;
}

export const CreditProfileView: React.FC<CreditProfileViewProps> = ({
  userProfile,
  skrHandle,
  walletAssets,
  onStakeSkr,
  onUnstakeSkr,
  onOpenAssetsModal,
  onDisconnectWallet,
  onLockApp,
  onSetupPin,
  onChangePin,
}) => {
  const { colors, mode, toggleTheme } = useTheme();
  const [lockEnabled, setLockEnabledState] = useState<boolean>(false);
  const [bioEnabled, setBioEnabledState] = useState<boolean>(true);
  const [hasBioHardware, setHasBioHardware] = useState<boolean>(false);
  const [hasCustomPin, setHasCustomPin] = useState<boolean>(false);

  useEffect(() => {
    loadSecurityPrefs();
  }, []);

  const loadSecurityPrefs = async () => {
    const l = await isLockEnabled();
    const b = await isBiometricsEnabled();
    const p = await getUserPin();
    const { hasHardware, isEnrolled } = await checkBiometricHardware();
    setLockEnabledState(l);
    setBioEnabledState(b);
    setHasCustomPin(!!p);
    setHasBioHardware(hasHardware && isEnrolled);
  };

  const openExplorer = () => {
    Linking.openURL(`https://explorer.solana.com/address/${PROGRAM_ID.toBase58()}?cluster=devnet`);
  };

  const shorten = (addr: string) => `${addr.slice(0, 6)}...${addr.slice(-6)}`;

  return (
    <ScrollView
      style={[styles.container, { backgroundColor: colors.background }]}
      contentContainerStyle={styles.content}
      showsVerticalScrollIndicator={false}
    >
      {/* Seeker ID Passport Card */}
      <View style={[styles.passportCard, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
        <View style={styles.passportHeader}>
          <View style={[styles.passportAvatar, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
            <Text style={{ fontSize: 24 }}>📱</Text>
          </View>
          <View style={{ flex: 1 }}>
            <View style={styles.handleTouchable}>
              <Text style={[styles.passportHandle, { color: colors.text }]}>{skrHandle}</Text>
              <View style={[styles.hardwareTag, { backgroundColor: colors.badgeBg }]}>
                <Text style={[styles.hardwareTagText, { color: colors.primary }]}>✓ SEED VAULT</Text>
              </View>
            </View>
            <Text style={[styles.walletSub, { color: colors.textMuted }]}>
              {shorten(userProfile.pubkey)} • Seeker Genesis Verified
            </Text>
          </View>
        </View>

        {/* Reputation Rating */}
        <View style={styles.scoreRow}>
          <View>
            <Text style={[styles.scoreLabel, { color: colors.textMuted }]}>On-Chain Reputation</Text>
            <Text style={[styles.scoreNumber, { color: colors.text }]}>
              {(userProfile.reputationScore / 100).toFixed(1)}%
            </Text>
          </View>
          <View style={[styles.tierTag, { backgroundColor: colors.badgeBg, borderColor: colors.badgeBorder }]}>
            <Text style={[styles.tierTagText, { color: colors.primary }]}>⭐ {userProfile.tier} Tier</Text>
          </View>
        </View>

        <Text style={[styles.statsNote, { color: colors.textSecondary }]}>
          {userProfile.totalLoansCompleted} loans completed on time • {userProfile.totalLoansDefaulted} defaults
        </Text>
      </View>

      {/* SKR Staking & Reputation Desk */}
      <View style={[styles.section, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
        <View style={styles.sectionTitleRow}>
          <Text style={[styles.sectionTitle, { color: colors.text }]}>SKR Reputation Bond</Text>
          <View style={[styles.trackPill, { backgroundColor: colors.badgeBg }]}>
            <Text style={[styles.trackPillText, { color: colors.primary }]}>$10,000 SKR Track</Text>
          </View>
        </View>
        <Text style={[styles.sectionDesc, { color: colors.textSecondary }]}>
          Stake SKR into the protocol escrow to unlock 90% LTV borrowing and 50% APR fee discounts.
        </Text>

        <View style={[styles.stakedHero, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
          <Text style={[styles.stakedLabel, { color: colors.textMuted }]}>Staked in Protocol Escrow</Text>
          <Text style={[styles.stakedVal, { color: colors.text }]}>
            {userProfile.stakedSkr.toLocaleString()} SKR
          </Text>
        </View>

        <View style={styles.stakeBtnRow}>
          {[500, 1000, 2500, 5000].map((amt) => (
            <TouchableOpacity
              key={amt}
              style={[styles.stakePresetBtn, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}
              onPress={() => onStakeSkr(amt)}
              activeOpacity={0.7}
            >
              <Text style={[styles.stakePresetText, { color: colors.primary }]}>+{amt} SKR</Text>
            </TouchableOpacity>
          ))}
        </View>

        {onUnstakeSkr && userProfile.stakedSkr > 0 && (
          <TouchableOpacity
            style={[styles.stakePresetBtn, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder, marginTop: 10 }]}
            onPress={() => onUnstakeSkr(userProfile.stakedSkr)}
            activeOpacity={0.7}
          >
            <Text style={[styles.stakePresetText, { color: '#ff6b6b' }]}>
              ↩ Unstake {userProfile.stakedSkr.toLocaleString()} SKR
            </Text>
          </TouchableOpacity>
        )}
      </View>

      {/* Wallet Assets & Holdings */}
      <View style={[styles.section, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
        <View style={styles.sectionTitleRow}>
          <Text style={[styles.sectionTitle, { color: colors.text }]}>
            Wallet Holdings ({walletAssets?.network === 'mainnet-beta' ? 'Mainnet' : 'Devnet'})
          </Text>
          {walletAssets && (
            <Text style={[styles.totalUsdText, { color: colors.primary }]}>
              ${walletAssets.totalUsdValue.toFixed(2)}
            </Text>
          )}
        </View>

        <View style={[styles.assetsMiniGrid, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
          <View style={styles.miniAsset}>
            <Text style={[styles.miniAssetLabel, { color: colors.textMuted }]}>SOL Balance</Text>
            <Text style={[styles.miniAssetVal, { color: colors.text }]}>
              {(walletAssets?.solBalance || 0).toFixed(3)} SOL
            </Text>
          </View>
          <View style={styles.miniAsset}>
            <Text style={[styles.miniAssetLabel, { color: colors.textMuted }]}>USDC</Text>
            <Text style={[styles.miniAssetVal, { color: colors.text }]}>
              ${(walletAssets?.usdcBalance || 0).toFixed(2)}
            </Text>
          </View>
          <View style={styles.miniAsset}>
            <Text style={[styles.miniAssetLabel, { color: colors.textMuted }]}>SKR Tokens</Text>
            <Text style={[styles.miniAssetVal, { color: colors.text }]}>
              {(walletAssets?.skrBalance || 0).toFixed(0)} SKR
            </Text>
          </View>
        </View>

        {onOpenAssetsModal && (
          <TouchableOpacity
            style={[styles.manageAssetsBtn, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}
            onPress={onOpenAssetsModal}
            activeOpacity={0.7}
          >
            <Text style={[styles.manageAssetsText, { color: colors.primary }]}>
              View & Manage Wallet Assets →
            </Text>
          </TouchableOpacity>
        )}
      </View>

      {/* App Appearance & Preferences */}
      <View style={[styles.section, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
        <Text style={[styles.sectionTitle, { color: colors.text }]}>App Settings & Theme</Text>
        <View style={styles.prefRow}>
          <View>
            <Text style={[styles.prefLabel, { color: colors.text }]}>Appearance Mode</Text>
            <Text style={[styles.prefSub, { color: colors.textSecondary }]}>
              Current: {mode === 'dark' ? 'Dark Mode' : 'Light Mode'}
            </Text>
          </View>
          <TouchableOpacity
            style={[styles.themeSwitchBtn, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}
            onPress={toggleTheme}
            activeOpacity={0.7}
          >
            <Text style={[styles.themeSwitchText, { color: colors.text }]}>
              {mode === 'dark' ? '☀️ Switch to Light' : '🌙 Switch to Dark'}
            </Text>
          </TouchableOpacity>
        </View>
      </View>

      {/* App Lock & Biometrics (Multi-Method Lock: PIN + Fingerprint/Face ID) */}
      <View style={[styles.section, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
        <View style={styles.secHeaderRow}>
          <Text style={[styles.sectionTitle, { color: colors.text, marginBottom: 0 }]}>
            App Security & Biometrics
          </Text>
          <View style={[styles.methodsBadge, { backgroundColor: colors.badgeBg, borderColor: colors.badgeBorder }]}>
            <Text style={[styles.methodsBadgeText, { color: colors.primary }]}>
              {lockEnabled
                ? hasBioHardware && bioEnabled
                  ? 'PIN + Biometrics (2/2 Active)'
                  : 'PIN Only (1/2 Active)'
                : 'Protection Disabled'}
            </Text>
          </View>
        </View>

        {/* 1. Master Lock Toggle */}
        <View style={styles.prefRow}>
          <View style={{ flex: 1, paddingRight: 12 }}>
            <Text style={[styles.prefLabel, { color: colors.text }]}>Require PIN on App Launch</Text>
            <Text style={[styles.prefSub, { color: colors.textSecondary }]}>
              Lock app whenever it opens or resumes from background.
            </Text>
          </View>
          <Switch
            value={lockEnabled}
            onValueChange={async (val) => {
              if (val && !hasCustomPin) {
                if (onSetupPin) onSetupPin();
              } else {
                await setLockEnabled(val);
                setLockEnabledState(val);
              }
            }}
            trackColor={{ false: colors.cardBorder, true: colors.primary }}
            thumbColor={lockEnabled ? '#FFFFFF' : colors.textMuted}
          />
        </View>

        {/* 2. Biometric Fast Unlock (if supported on device) */}
        {hasBioHardware && (
          <View style={[styles.prefRow, { borderTopWidth: 1, borderTopColor: colors.cardBorder, paddingTop: 12, marginTop: 8 }]}>
            <View style={{ flex: 1, paddingRight: 12 }}>
              <View style={{ flexDirection: 'row', alignItems: 'center', gap: 6 }}>
                <Ionicons name="finger-print" size={16} color={colors.primary} />
                <Text style={[styles.prefLabel, { color: colors.text }]}>Fingerprint / Face ID</Text>
              </View>
              <Text style={[styles.prefSub, { color: colors.textSecondary }]}>
                Fast 1-tap unlock using device biometric sensor.
              </Text>
            </View>
            <Switch
              value={bioEnabled}
              disabled={!lockEnabled}
              onValueChange={async (val) => {
                await setBiometricsEnabled(val);
                setBioEnabledState(val);
              }}
              trackColor={{ false: colors.cardBorder, true: colors.primary }}
              thumbColor={bioEnabled ? '#FFFFFF' : colors.textMuted}
            />
          </View>
        )}

        {/* 3. Action Buttons */}
        <View style={styles.secBtnRow}>
          <TouchableOpacity
            style={[styles.secActionBtn, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}
            onPress={() => {
              if (hasCustomPin) {
                if (onChangePin) onChangePin();
              } else {
                if (onSetupPin) onSetupPin();
              }
            }}
            activeOpacity={0.7}
          >
            <Ionicons name="key-outline" size={15} color={colors.primary} />
            <Text style={[styles.secActionBtnText, { color: colors.text }]}>
              {hasCustomPin ? 'Change PIN' : 'Set Custom PIN'}
            </Text>
          </TouchableOpacity>

          {onLockApp && lockEnabled && (
            <TouchableOpacity
              style={[styles.secActionBtn, { backgroundColor: colors.primary, borderColor: colors.primary }]}
              onPress={onLockApp}
              activeOpacity={0.85}
            >
              <Ionicons name="lock-closed" size={14} color={colors.primaryText} />
              <Text style={[styles.secActionBtnText, { color: colors.primaryText, fontWeight: '800' }]}>
                Lock Now
              </Text>
            </TouchableOpacity>
          )}
        </View>
      </View>

      {/* Security & Verification Card */}
      <View style={[styles.section, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
        <Text style={[styles.sectionTitle, { color: colors.text }]}>Seeker Hardware Security</Text>
        <View style={styles.verifyItem}>
          <Text style={styles.verifyIcon}>🛡️</Text>
          <View style={{ flex: 1 }}>
            <Text style={[styles.verifyTitle, { color: colors.text }]}>Seed Vault Enclave</Text>
            <Text style={[styles.verifySub, { color: colors.textSecondary }]}>
              Cryptographic keys isolated in hardware enclave. Zero remote key exfiltration.
            </Text>
          </View>
        </View>

        <View style={styles.verifyItem}>
          <Text style={styles.verifyIcon}>📜</Text>
          <View style={{ flex: 1 }}>
            <Text style={[styles.verifyTitle, { color: colors.text }]}>Audited Protocol Logic</Text>
            <Text style={[styles.verifySub, { color: colors.textSecondary }]}>
              Non-custodial smart contracts executed with atomic escrow settlement on Solana.
            </Text>
          </View>
        </View>

        <TouchableOpacity
          style={[styles.explorerBtn, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}
          onPress={openExplorer}
          activeOpacity={0.7}
        >
          <Text style={[styles.explorerBtnText, { color: colors.primary }]}>View Protocol on Solana Explorer ↗</Text>
        </TouchableOpacity>
      </View>

      {/* Logout Button */}
      <TouchableOpacity
        style={[styles.disconnectBtn, { backgroundColor: 'rgba(239, 68, 68, 0.1)', borderColor: colors.danger }]}
        onPress={onDisconnectWallet}
        activeOpacity={0.8}
      >
        <Text style={[styles.disconnectBtnText, { color: colors.danger }]}>Logout</Text>
      </TouchableOpacity>
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
  passportCard: {
    borderRadius: 20,
    borderWidth: 1,
    padding: 20,
    marginBottom: 16,
  },
  passportHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 14,
    marginBottom: 16,
  },
  passportAvatar: {
    width: 48,
    height: 48,
    borderRadius: 24,
    borderWidth: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  handleTouchable: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
  },
  passportHandle: {
    fontSize: 20,
    fontWeight: '800',
  },
  hardwareTag: {
    paddingHorizontal: 8,
    paddingVertical: 3,
    borderRadius: 8,
  },
  hardwareTagText: {
    fontSize: 10,
    fontWeight: '800',
  },
  walletSub: {
    fontSize: 11,
    marginTop: 2,
  },
  scoreRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 8,
  },
  scoreLabel: {
    fontSize: 11,
    marginBottom: 2,
  },
  scoreNumber: {
    fontSize: 28,
    fontWeight: '800',
  },
  tierTag: {
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 12,
    borderWidth: 1,
  },
  tierTagText: {
    fontSize: 12,
    fontWeight: '800',
  },
  statsNote: {
    fontSize: 12,
  },
  section: {
    borderRadius: 20,
    borderWidth: 1,
    padding: 18,
    marginBottom: 16,
  },
  sectionTitleRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 6,
  },
  sectionTitle: {
    fontSize: 15,
    fontWeight: '800',
    marginBottom: 6,
  },
  trackPill: {
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 8,
  },
  trackPillText: {
    fontSize: 10,
    fontWeight: '800',
  },
  sectionDesc: {
    fontSize: 12,
    lineHeight: 16,
    marginBottom: 14,
  },
  stakedHero: {
    borderRadius: 14,
    borderWidth: 1,
    padding: 14,
    marginBottom: 12,
  },
  stakedLabel: {
    fontSize: 11,
    marginBottom: 2,
  },
  stakedVal: {
    fontSize: 20,
    fontWeight: '800',
  },
  stakeBtnRow: {
    flexDirection: 'row',
    gap: 8,
  },
  stakePresetBtn: {
    flex: 1,
    paddingVertical: 10,
    borderRadius: 12,
    borderWidth: 1,
    alignItems: 'center',
  },
  stakePresetText: {
    fontSize: 12,
    fontWeight: '800',
  },
  prefRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  prefLabel: {
    fontSize: 13,
    fontWeight: '700',
  },
  prefSub: {
    fontSize: 11,
    marginTop: 2,
  },
  themeSwitchBtn: {
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderRadius: 12,
    borderWidth: 1,
  },
  themeSwitchText: {
    fontSize: 12,
    fontWeight: '700',
  },
  verifyItem: {
    flexDirection: 'row',
    gap: 12,
    marginBottom: 12,
  },
  verifyIcon: {
    fontSize: 20,
  },
  verifyTitle: {
    fontSize: 13,
    fontWeight: '700',
  },
  verifySub: {
    fontSize: 11,
    marginTop: 2,
    lineHeight: 15,
  },
  explorerBtn: {
    borderRadius: 12,
    borderWidth: 1,
    paddingVertical: 12,
    alignItems: 'center',
    marginTop: 6,
  },
  explorerBtnText: {
    fontSize: 12,
    fontWeight: '700',
  },
  disconnectBtn: {
    borderRadius: 14,
    borderWidth: 1,
    paddingVertical: 14,
    alignItems: 'center',
    marginBottom: 20,
  },
  disconnectBtnText: {
    fontSize: 14,
    fontWeight: '800',
  },
  totalUsdText: {
    fontSize: 14,
    fontWeight: '800',
  },
  assetsMiniGrid: {
    flexDirection: 'row',
    borderRadius: 14,
    borderWidth: 1,
    padding: 12,
    marginBottom: 12,
  },
  miniAsset: {
    flex: 1,
  },
  miniAssetLabel: {
    fontSize: 10,
    marginBottom: 2,
  },
  miniAssetVal: {
    fontSize: 12,
    fontWeight: '700',
  },
  manageAssetsBtn: {
    paddingVertical: 10,
    borderRadius: 12,
    borderWidth: 1,
    alignItems: 'center',
  },
  manageAssetsText: {
    fontSize: 12,
    fontWeight: '700',
  },
  secHeaderRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 12,
  },
  methodsBadge: {
    paddingHorizontal: 8,
    paddingVertical: 3,
    borderRadius: 8,
    borderWidth: 1,
  },
  methodsBadgeText: {
    fontSize: 10,
    fontWeight: '800',
  },
  secBtnRow: {
    flexDirection: 'row',
    gap: 10,
    marginTop: 14,
  },
  secActionBtn: {
    flex: 1,
    flexDirection: 'row',
    justifyContent: 'center',
    alignItems: 'center',
    gap: 6,
    height: 42,
    borderRadius: 12,
    borderWidth: 1,
  },
  secActionBtnText: {
    fontSize: 12,
    fontWeight: '700',
  },
});
