import React, { useState } from 'react';
import {
  StyleSheet,
  View,
  Text,
  TouchableOpacity,
  ActivityIndicator,
  Alert,
  ScrollView,
  Image,
} from 'react-native';
import { PublicKey } from '@solana/web3.js';
import { useTheme } from '../theme/ThemeContext';
import { connectSeekerWallet, deriveSkrUsername, SeekerSession } from '../solana/seekerWallet';

const LOGO_IMG = require('../../assets/logo.png');

interface ConnectWalletViewProps {
  onConnected: (session: SeekerSession) => void;
}

export const ConnectWalletView: React.FC<ConnectWalletViewProps> = ({ onConnected }) => {
  const { colors, mode, toggleTheme } = useTheme();
  const [isConnecting, setIsConnecting] = useState(false);

  const handleMwaConnect = async () => {
    setIsConnecting(true);
    try {
      const session = await connectSeekerWallet();
      onConnected(session);
    } catch (err: any) {
      console.log('MWA Connection notice:', err);
      Alert.alert(
        'Seeker Hardware Connection',
        'Could not detect an active Seeker Seed Vault host on this environment.\n\nWould you like to continue in preview mode?',
        [
          { text: 'Cancel', style: 'cancel' },
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

  return (
    <ScrollView
      style={[styles.container, { backgroundColor: colors.background }]}
      contentContainerStyle={styles.scrollContent}
      showsVerticalScrollIndicator={false}
    >
      <View>
        {/* Top bar with network status pill & theme switcher */}
        <View style={styles.topBar}>
          <View style={[styles.badge, { backgroundColor: colors.badgeBg, borderColor: colors.badgeBorder }]}>
            <View style={[styles.dot, { backgroundColor: colors.primary }]} />
            <Text style={[styles.badgeText, { color: colors.primary }]}>SOLANA DEVNET</Text>
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

        {/* Hero Section with Centered Logo */}
        <View style={styles.heroSection}>
          <View style={[styles.logoWrapper, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
            <Image source={LOGO_IMG} style={styles.logoImage} resizeMode="contain" />
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

          <Text style={[styles.securityNote, { color: colors.textMuted }]}>
            🔒 Hardware Seed Vault Protection • Non-Custodial
          </Text>
        </View>

        {/* Live Protocol Stats Strip */}
        <View style={[styles.statsRow, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
          <View style={styles.statCol}>
            <Text style={[styles.statVal, { color: colors.primary }]}>$4.2M+</Text>
            <Text style={[styles.statLbl, { color: colors.textMuted }]}>Total Volume</Text>
          </View>
          <View style={[styles.statDivider, { backgroundColor: colors.divider }]} />
          <View style={styles.statCol}>
            <Text style={[styles.statVal, { color: colors.text }]}>90%</Text>
            <Text style={[styles.statLbl, { color: colors.textMuted }]}>Max LTV</Text>
          </View>
          <View style={[styles.statDivider, { backgroundColor: colors.divider }]} />
          <View style={styles.statCol}>
            <Text style={[styles.statVal, { color: colors.accent }]}>0.0%</Text>
            <Text style={[styles.statLbl, { color: colors.textMuted }]}>Default Rate</Text>
          </View>
        </View>

        {/* Supported Collateral Assets Row */}
        <View style={styles.collateralSection}>
          <Text style={[styles.sectionHeading, { color: colors.textMuted }]}>SUPPORTED ASSETS</Text>
          <View style={styles.collateralPillsRow}>
            {[
              { symbol: 'SKR', name: 'Seeker (Primary)' },
              { symbol: 'SOL', name: 'Solana' },
              { symbol: 'cNFT', name: 'Compressed' },
              { symbol: 'USDC', name: 'Liquidity' },
            ].map((token) => (
              <View
                key={token.symbol}
                style={[
                  styles.collateralPill,
                  { backgroundColor: colors.card, borderColor: colors.cardBorder },
                ]}
              >
                <Text style={[styles.pillSymbol, { color: colors.primary }]}>{token.symbol}</Text>
                <Text style={[styles.pillName, { color: colors.textSecondary }]}>{token.name}</Text>
              </View>
            ))}
          </View>
        </View>

        {/* Feature Highlights */}
        <View style={styles.featuresList}>
          <View style={[styles.featureRow, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
            <View style={[styles.featureIconBox, { backgroundColor: colors.badgeBg }]}>
              <Text style={styles.featureIcon}>⚡</Text>
            </View>
            <View style={{ flex: 1 }}>
              <Text style={[styles.featureTitle, { color: colors.text }]}>Instant Micro-Liquidity</Text>
              <Text style={[styles.featureDesc, { color: colors.textSecondary }]}>
                Draw instant USDC liquidity against SOL, SKR, and digital collateral.
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
                Borrow from competitive community pools or establish direct peer pawn agreements.
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
                Keys remain isolated in your Seeker Seed Vault hardware enclave.
              </Text>
            </View>
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
    flexGrow: 1,
    justifyContent: 'space-between',
    padding: 20,
    paddingTop: 44,
    paddingBottom: 28,
  },
  topBar: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 24,
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
    marginBottom: 24,
  },
  logoWrapper: {
    width: 92,
    height: 92,
    borderRadius: 26,
    borderWidth: 1,
    justifyContent: 'center',
    alignItems: 'center',
    marginBottom: 16,
    overflow: 'hidden',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.15,
    shadowRadius: 12,
    elevation: 4,
  },
  logoImage: {
    width: 76,
    height: 76,
    borderRadius: 18,
  },
  title: {
    fontSize: 32,
    fontWeight: '900',
    letterSpacing: -0.5,
    marginBottom: 6,
  },
  subtitle: {
    fontSize: 13,
    textAlign: 'center',
    lineHeight: 18,
    paddingHorizontal: 20,
  },
  actionCard: {
    borderRadius: 22,
    borderWidth: 1,
    padding: 18,
    marginBottom: 16,
  },
  connectBtn: {
    height: 54,
    borderRadius: 16,
    justifyContent: 'center',
    alignItems: 'center',
    marginBottom: 10,
  },
  connectBtnText: {
    fontSize: 16,
    fontWeight: '800',
  },
  securityNote: {
    fontSize: 11,
    textAlign: 'center',
    fontWeight: '600',
  },
  statsRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-around',
    borderRadius: 18,
    borderWidth: 1,
    paddingVertical: 14,
    paddingHorizontal: 10,
    marginBottom: 16,
  },
  statCol: {
    alignItems: 'center',
    flex: 1,
  },
  statVal: {
    fontSize: 18,
    fontWeight: '900',
    marginBottom: 2,
  },
  statLbl: {
    fontSize: 10,
    fontWeight: '700',
    textTransform: 'uppercase',
  },
  statDivider: {
    width: 1,
    height: 28,
  },
  collateralSection: {
    marginBottom: 16,
  },
  sectionHeading: {
    fontSize: 10,
    fontWeight: '800',
    letterSpacing: 0.8,
    marginBottom: 8,
    marginLeft: 4,
  },
  collateralPillsRow: {
    flexDirection: 'row',
    gap: 8,
  },
  collateralPill: {
    flex: 1,
    paddingVertical: 10,
    paddingHorizontal: 8,
    borderRadius: 14,
    borderWidth: 1,
    alignItems: 'center',
  },
  pillSymbol: {
    fontSize: 13,
    fontWeight: '800',
    marginBottom: 2,
  },
  pillName: {
    fontSize: 9,
    fontWeight: '600',
  },
  featuresList: {
    gap: 10,
    marginBottom: 20,
  },
  featureRow: {
    flexDirection: 'row',
    alignItems: 'center',
    padding: 14,
    borderRadius: 18,
    borderWidth: 1,
    gap: 12,
  },
  featureIconBox: {
    width: 40,
    height: 40,
    borderRadius: 12,
    justifyContent: 'center',
    alignItems: 'center',
  },
  featureIcon: {
    fontSize: 20,
  },
  featureTitle: {
    fontSize: 13,
    fontWeight: '800',
    marginBottom: 2,
  },
  featureDesc: {
    fontSize: 11,
    lineHeight: 15,
  },
  footer: {
    alignItems: 'center',
    paddingTop: 12,
    paddingBottom: 4,
  },
  footerText: {
    fontSize: 11,
    fontWeight: '600',
  },
});
