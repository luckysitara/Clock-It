import React, { useState } from 'react';
import {
  StyleSheet,
  View,
  Text,
  TouchableOpacity,
  ActivityIndicator,
  Alert,
  ScrollView,
  TextInput,
} from 'react-native';
import { PublicKey } from '@solana/web3.js';
import { useTheme } from '../theme/ThemeContext';
import { connectSeekerWallet, deriveSkrUsername, SeekerSession } from '../solana/seekerWallet';

interface ConnectWalletViewProps {
  onConnected: (session: SeekerSession) => void;
}

export const ConnectWalletView: React.FC<ConnectWalletViewProps> = ({ onConnected }) => {
  const { colors, mode, toggleTheme } = useTheme();
  const [isConnecting, setIsConnecting] = useState(false);
  const [showManualInput, setShowManualInput] = useState(false);
  const [customAddress, setCustomAddress] = useState('');

  const handleMwaConnect = async () => {
    setIsConnecting(true);
    try {
      const session = await connectSeekerWallet();
      onConnected(session);
    } catch (err: any) {
      console.log('MWA Connection notice:', err);
      Alert.alert(
        'Seeker Hardware Connection',
        'Could not detect an active Seeker Seed Vault host on this environment.\n\nWould you like to enter your Solana address or continue in preview mode?',
        [
          { text: 'Cancel', style: 'cancel' },
          {
            text: 'Enter Address',
            onPress: () => setShowManualInput(true),
          },
          {
            text: 'Preview Mode',
            onPress: async () => {
              const devnetPubkey = new PublicKey('BEmX1nfeZT5i4VpSEeZmhiYxpZ9z4Y1LQLjAtPR9c3re');
              const skrHandle = await deriveSkrUsername(devnetPubkey);
              onConnected({
                publicKey: devnetPubkey,
                skrHandle,
                isSeekerGenesisVerified: true,
              });
            },
          },
        ]
      );
    } finally {
      setIsConnecting(false);
    }
  };

  const handleManualConnect = async () => {
    const trimmed = customAddress.trim();
    if (!trimmed) {
      Alert.alert('Empty Address', 'Please enter or paste a valid Solana address.');
      return;
    }
    try {
      const pk = new PublicKey(trimmed);
      const skrHandle = await deriveSkrUsername(pk);
      onConnected({
        publicKey: pk,
        skrHandle,
        isSeekerGenesisVerified: true,
      });
    } catch {
      Alert.alert('Invalid Address', 'The address entered is not a valid Solana public key.');
    }
  };

  return (
    <ScrollView
      style={[styles.container, { backgroundColor: colors.background }]}
      contentContainerStyle={styles.scrollContent}
      showsVerticalScrollIndicator={false}
    >
      {/* Top bar with network status pill & theme switcher */}
      <View style={styles.topBar}>
        <View style={[styles.badge, { backgroundColor: colors.badgeBg, borderColor: colors.badgeBorder }]}>
          <View style={[styles.dot, { backgroundColor: colors.primary }]} />
          <Text style={[styles.badgeText, { color: colors.primary }]}>SOLANA NETWORK</Text>
        </View>

        <TouchableOpacity
          style={[styles.themeBtn, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}
          onPress={toggleTheme}
          activeOpacity={0.7}
        >
          <Text style={[styles.themeBtnText, { color: colors.textSecondary }]}>
            {mode === 'light' ? '🌙 Dark' : '☀️ Light'}
          </Text>
        </TouchableOpacity>
      </View>

      {/* Hero Section */}
      <View style={styles.heroSection}>
        <View style={[styles.iconContainer, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
          <Text style={styles.phoneIcon}>⚡</Text>
          <View style={[styles.verifiedBadge, { backgroundColor: colors.primary }]}>
            <Text style={[styles.checkIcon, { color: colors.primaryText }]}>✓</Text>
          </View>
        </View>

        <Text style={[styles.title, { color: colors.text }]}>ClockLend</Text>
        <Text style={[styles.subtitle, { color: colors.textSecondary }]}>
          Decentralized P2P Lending & Social Pawn Protocol
        </Text>
      </View>

      {/* Primary Connect Action Card */}
      <View style={[styles.actionCard, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
        <TouchableOpacity
          style={[styles.connectBtn, { backgroundColor: colors.primary }]}
          onPress={handleMwaConnect}
          disabled={isConnecting}
          activeOpacity={0.85}
        >
          {isConnecting ? (
            <ActivityIndicator color={colors.primaryText} />
          ) : (
            <Text style={[styles.connectBtnText, { color: colors.primaryText }]}>
              Connect Seeker Wallet
            </Text>
          )}
        </TouchableOpacity>

        {/* Manual Address Entry Option */}
        {showManualInput ? (
          <View style={styles.manualBox}>
            <TextInput
              style={[
                styles.addressInput,
                { backgroundColor: colors.cardAlt, color: colors.text, borderColor: colors.cardBorder },
              ]}
              placeholder="Paste your Solana public key..."
              placeholderTextColor={colors.textMuted}
              value={customAddress}
              onChangeText={setCustomAddress}
              autoCapitalize="none"
              autoCorrect={false}
            />
            <View style={styles.manualActionRow}>
              <TouchableOpacity
                style={[styles.cancelBtn, { borderColor: colors.cardBorder }]}
                onPress={() => setShowManualInput(false)}
              >
                <Text style={[styles.cancelBtnText, { color: colors.textSecondary }]}>Cancel</Text>
              </TouchableOpacity>
              <TouchableOpacity
                style={[styles.confirmAddressBtn, { backgroundColor: colors.primary }]}
                onPress={handleManualConnect}
              >
                <Text style={[styles.confirmAddressText, { color: colors.primaryText }]}>Load Wallet</Text>
              </TouchableOpacity>
            </View>
          </View>
        ) : (
          <TouchableOpacity
            style={styles.toggleManualBtn}
            onPress={() => setShowManualInput(true)}
            activeOpacity={0.7}
          >
            <Text style={[styles.toggleManualText, { color: colors.primary }]}>
              Or connect with Solana address →
            </Text>
          </TouchableOpacity>
        )}

        <Text style={[styles.securityNote, { color: colors.textMuted }]}>
          🔒 Hardware Seed Vault Protection • Non-Custodial
        </Text>
      </View>

      {/* Product Feature Highlights */}
      <View style={styles.featuresList}>
        <View style={[styles.featureRow, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
          <View style={[styles.featureIconBox, { backgroundColor: colors.badgeBg }]}>
            <Text style={styles.featureIcon}>⚡</Text>
          </View>
          <View style={{ flex: 1 }}>
            <Text style={[styles.featureTitle, { color: colors.text }]}>Instant Micro-Liquidity</Text>
            <Text style={[styles.featureDesc, { color: colors.textSecondary }]}>
              Draw instant USDC liquidity against SOL, cNFTs, and digital collateral.
            </Text>
          </View>
        </View>

        <View style={[styles.featureRow, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
          <View style={[styles.featureIconBox, { backgroundColor: colors.badgeBg }]}>
            <Text style={styles.featureIcon}>🤝</Text>
          </View>
          <View style={{ flex: 1 }}>
            <Text style={[styles.featureTitle, { color: colors.text }]}>P2P Lending Desks</Text>
            <Text style={[styles.featureDesc, { color: colors.textSecondary }]}>
              Borrow from competitive community pools or establish direct social pawn agreements.
            </Text>
          </View>
        </View>

        <View style={[styles.featureRow, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
          <View style={[styles.featureIconBox, { backgroundColor: colors.badgeBg }]}>
            <Text style={styles.featureIcon}>🛡️</Text>
          </View>
          <View style={{ flex: 1 }}>
            <Text style={[styles.featureTitle, { color: colors.text }]}>Hardware Security</Text>
            <Text style={[styles.featureDesc, { color: colors.textSecondary }]}>
              Cryptographic keys remain securely isolated in your device hardware enclave.
            </Text>
          </View>
        </View>
      </View>

      {/* Footer */}
      <View style={styles.footer}>
        <Text style={[styles.footerText, { color: colors.textMuted }]}>
          ClockLend Protocol • Powered by Solana
        </Text>
      </View>
    </ScrollView>
  );
};

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  scrollContent: {
    padding: 20,
    paddingTop: 48,
    paddingBottom: 36,
  },
  topBar: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 36,
  },
  badge: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 20,
    borderWidth: 1,
  },
  dot: {
    width: 7,
    height: 7,
    borderRadius: 4,
    marginRight: 7,
  },
  badgeText: {
    fontSize: 11,
    fontWeight: '800',
    letterSpacing: 0.5,
  },
  themeBtn: {
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 16,
    borderWidth: 1,
  },
  themeBtnText: {
    fontSize: 12,
    fontWeight: '700',
  },
  heroSection: {
    alignItems: 'center',
    marginBottom: 32,
  },
  iconContainer: {
    width: 88,
    height: 88,
    borderRadius: 28,
    borderWidth: 1,
    justifyContent: 'center',
    alignItems: 'center',
    marginBottom: 18,
    position: 'relative',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.06,
    shadowRadius: 10,
    elevation: 2,
  },
  phoneIcon: {
    fontSize: 42,
  },
  verifiedBadge: {
    position: 'absolute',
    bottom: -4,
    right: -4,
    width: 26,
    height: 26,
    borderRadius: 13,
    justifyContent: 'center',
    alignItems: 'center',
    borderWidth: 2,
    borderColor: '#FFFFFF',
  },
  checkIcon: {
    fontSize: 14,
    fontWeight: '900',
  },
  title: {
    fontSize: 34,
    fontWeight: '900',
    letterSpacing: -0.5,
    marginBottom: 8,
  },
  subtitle: {
    fontSize: 14,
    textAlign: 'center',
    lineHeight: 20,
    paddingHorizontal: 16,
  },
  actionCard: {
    borderRadius: 24,
    borderWidth: 1,
    padding: 20,
    marginBottom: 24,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.04,
    shadowRadius: 10,
    elevation: 2,
  },
  connectBtn: {
    height: 56,
    borderRadius: 18,
    justifyContent: 'center',
    alignItems: 'center',
    marginBottom: 10,
  },
  connectBtnText: {
    fontSize: 16,
    fontWeight: '800',
  },
  toggleManualBtn: {
    alignItems: 'center',
    paddingVertical: 8,
    marginBottom: 6,
  },
  toggleManualText: {
    fontSize: 13,
    fontWeight: '700',
  },
  manualBox: {
    marginBottom: 12,
    gap: 8,
  },
  addressInput: {
    height: 48,
    borderRadius: 14,
    borderWidth: 1,
    paddingHorizontal: 14,
    fontSize: 13,
  },
  manualActionRow: {
    flexDirection: 'row',
    gap: 10,
  },
  cancelBtn: {
    flex: 1,
    height: 44,
    borderRadius: 14,
    borderWidth: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  cancelBtnText: {
    fontSize: 13,
    fontWeight: '700',
  },
  confirmAddressBtn: {
    flex: 2,
    height: 44,
    borderRadius: 14,
    justifyContent: 'center',
    alignItems: 'center',
  },
  confirmAddressText: {
    fontSize: 13,
    fontWeight: '800',
  },
  securityNote: {
    fontSize: 11,
    textAlign: 'center',
    fontWeight: '600',
    marginTop: 4,
  },
  featuresList: {
    gap: 12,
    marginBottom: 28,
  },
  featureRow: {
    flexDirection: 'row',
    alignItems: 'center',
    padding: 16,
    borderRadius: 20,
    borderWidth: 1,
    gap: 14,
  },
  featureIconBox: {
    width: 44,
    height: 44,
    borderRadius: 14,
    justifyContent: 'center',
    alignItems: 'center',
  },
  featureIcon: {
    fontSize: 22,
  },
  featureTitle: {
    fontSize: 14,
    fontWeight: '800',
    marginBottom: 2,
  },
  featureDesc: {
    fontSize: 12,
    lineHeight: 16,
  },
  footer: {
    alignItems: 'center',
    paddingVertical: 12,
  },
  footerText: {
    fontSize: 11,
    fontWeight: '600',
  },
});
