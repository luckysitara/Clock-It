import React from 'react';
import { View, Text, StyleSheet, TouchableOpacity } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { useTheme } from '../theme/ThemeContext';
import { SolanaNetwork } from '../types';

interface HeaderProps {
  skrHandle: string;
  solBalance: number;
  network?: SolanaNetwork;
  onPressProfile: () => void;
  onPressBalance?: () => void;
  onToggleNetwork?: () => void;
  onDisconnectWallet?: () => void;
}

export const Header: React.FC<HeaderProps> = ({
  skrHandle,
  solBalance,
  network = 'devnet',
  onPressProfile,
  onPressBalance,
  onToggleNetwork,
  onDisconnectWallet,
}) => {
  const { colors, mode, toggleTheme } = useTheme();

  return (
    <View style={[styles.container, { backgroundColor: colors.card, borderBottomColor: colors.cardBorder }]}>
      {/* Left: Seeker ID User Handle */}
      <TouchableOpacity style={styles.profileButton} onPress={onPressProfile} activeOpacity={0.7}>
        <View style={[styles.avatar, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
          <Ionicons name="shield-checkmark" size={18} color={colors.primary} />
        </View>
        <View>
          <View style={styles.handleRow}>
            <Text style={[styles.handleText, { color: colors.text }]}>{skrHandle}</Text>
            <View style={[styles.verifiedDot, { backgroundColor: colors.primary }]} />
          </View>
          <Text style={[styles.subtext, { color: colors.textSecondary }]}>Seeker Genesis ID</Text>
        </View>
      </TouchableOpacity>

      {/* Right: Theme Toggle & Wallet Button */}
      <View style={styles.rightActions}>
        <TouchableOpacity
          style={[styles.themeChip, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}
          onPress={toggleTheme}
          activeOpacity={0.7}
        >
          <Ionicons name={mode === 'dark' ? 'sunny-outline' : 'moon-outline'} size={18} color={colors.text} />
        </TouchableOpacity>

        <TouchableOpacity
          style={[styles.walletChip, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}
          onPress={onPressBalance || onDisconnectWallet}
          activeOpacity={0.7}
        >
          <Ionicons name="wallet-outline" size={18} color={colors.text} />
        </TouchableOpacity>
      </View>
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    paddingTop: 52,
    paddingHorizontal: 20,
    paddingBottom: 16,
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    borderBottomWidth: 1,
  },
  profileButton: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
  },
  avatar: {
    width: 38,
    height: 38,
    borderRadius: 19,
    borderWidth: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  avatarText: {
    fontSize: 18,
  },
  handleRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
  },
  handleText: {
    fontSize: 16,
    fontWeight: '800',
    letterSpacing: 0.3,
  },
  verifiedDot: {
    width: 6,
    height: 6,
    borderRadius: 3,
  },
  subtext: {
    fontSize: 11,
    fontWeight: '600',
    marginTop: 1,
  },
  rightActions: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
  },
  themeChip: {
    width: 36,
    height: 36,
    borderRadius: 18,
    borderWidth: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  themeIcon: {
    fontSize: 16,
  },
  walletChip: {
    width: 36,
    height: 36,
    borderRadius: 18,
    borderWidth: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  walletIcon: {
    fontSize: 16,
  },
});
