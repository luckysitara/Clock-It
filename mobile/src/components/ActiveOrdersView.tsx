import React from 'react';
import { View, Text, StyleSheet, TouchableOpacity, ScrollView, Alert, Linking } from 'react-native';
import { useTheme } from '../theme/ThemeContext';
import { LoanOrder } from '../types';
import { CountdownTimer } from './CountdownTimer';

interface ActiveOrdersViewProps {
  orders: LoanOrder[];
  onRepay: (order: LoanOrder) => void;
  onTriggerGrace: (orderId: number) => void;
  onNavigateBorrow: () => void;
}

export const ActiveOrdersView: React.FC<ActiveOrdersViewProps> = ({
  orders,
  onRepay,
  onTriggerGrace,
  onNavigateBorrow,
}) => {
  const { colors } = useTheme();

  const activeOrders = orders.filter((o) => {
    const s = o.status.toUpperCase();
    return s === 'ACTIVE' || s === 'INGRACEPERIOD' || s === 'GRACE_PERIOD';
  });

  return (
    <ScrollView
      style={[styles.container, { backgroundColor: colors.background }]}
      contentContainerStyle={styles.content}
      showsVerticalScrollIndicator={false}
    >
      {activeOrders.length === 0 ? (
        <View style={[styles.emptyBox, { backgroundColor: colors.card, borderColor: colors.cardBorder }]}>
          <Text style={styles.emptyIcon}>⏳</Text>
          <Text style={[styles.emptyTitle, { color: colors.text }]}>No Active On-Chain Loans</Text>
          <Text style={[styles.emptySub, { color: colors.textSecondary }]}>
            You currently have no active loan orders. Draw instant liquidity from a verified lending desk.
          </Text>
          <TouchableOpacity
            style={[styles.borrowNowBtn, { backgroundColor: colors.primary }]}
            onPress={onNavigateBorrow}
            activeOpacity={0.85}
          >
            <Text style={[styles.borrowNowBtnText, { color: colors.primaryText }]}>⚡ Go to Borrow Desk</Text>
          </TouchableOpacity>
        </View>
      ) : (
        activeOrders.map((order) => {
          const totalDue = (order.principalAmount + order.interestDue).toFixed(2);
          const inGrace = order.status.toUpperCase().includes('GRACE');

          return (
            <View
              key={order.id}
              style={[
                styles.card,
                { backgroundColor: colors.card, borderColor: colors.cardBorder },
                inGrace && { borderColor: colors.warning, backgroundColor: 'rgba(245, 158, 11, 0.05)' },
              ]}
            >
              <View style={styles.cardHeader}>
                <View>
                  <Text style={[styles.poolName, { color: colors.text }]}>{order.poolName}</Text>
                  <Text style={[styles.orderId, { color: colors.textMuted }]}>Order #{order.id}</Text>
                </View>

                <View
                  style={[
                    styles.badge,
                    inGrace
                      ? { backgroundColor: 'rgba(245, 158, 11, 0.15)', borderColor: 'rgba(245, 158, 11, 0.3)' }
                      : { backgroundColor: colors.badgeBg, borderColor: colors.badgeBorder },
                  ]}
                >
                  <Text
                    style={[
                      styles.badgeText,
                      inGrace ? { color: colors.warning } : { color: colors.primary },
                    ]}
                  >
                    {inGrace ? '⚠️ Social Grace Active' : '🟢 Active in Escrow'}
                  </Text>
                </View>
              </View>

              {/* Ticking Countdown Timer */}
              <CountdownTimer dueTime={order.dueTime} />

              {/* Loan Details */}
              <View style={[styles.detailsBox, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
                <View style={styles.detailRow}>
                  <Text style={[styles.label, { color: colors.textMuted }]}>Borrowed Principal</Text>
                  <Text style={[styles.value, { color: colors.text }]}>${order.principalAmount} USDC</Text>
                </View>

                <View style={styles.detailRow}>
                  <Text style={[styles.label, { color: colors.textMuted }]}>Locked Collateral</Text>
                  <Text style={[styles.valuePurple, { color: colors.accentLight }]}>{order.collateralName}</Text>
                </View>

                <View style={styles.detailRow}>
                  <Text style={[styles.label, { color: colors.textMuted }]}>Interest Accrued</Text>
                  <Text style={[styles.value, { color: colors.text }]}>+${order.interestDue} USDC</Text>
                </View>

                <View style={[styles.detailRow, styles.totalRow, { borderTopColor: colors.cardBorder }]}>
                  <Text style={[styles.totalLabel, { color: colors.text }]}>Total to Repay</Text>
                  <Text style={[styles.totalValue, { color: colors.primary }]}>${totalDue} USDC</Text>
                </View>
              </View>

              {/* On-Chain Evidence Box */}
              <View style={[styles.evidenceBox, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
                <View style={styles.evidenceHeader}>
                  <View style={styles.evidenceTitleRow}>
                    <View style={[styles.dotLive, { backgroundColor: colors.primary }]} />
                    <Text style={[styles.evidenceTitle, { color: colors.text }]}>ON-CHAIN VERIFICATION</Text>
                  </View>
                  <View style={[styles.networkBadge, { backgroundColor: colors.badgeBg, borderColor: colors.badgeBorder }]}>
                    <Text style={[styles.networkBadgeText, { color: colors.primary }]}>Solana Devnet</Text>
                  </View>
                </View>

                {order.txSignature ? (
                  <View style={styles.evidenceRow}>
                    <Text style={[styles.evidenceLabel, { color: colors.textMuted }]}>Tx Signature</Text>
                    <View style={styles.evidenceValRow}>
                      <Text style={[styles.evidenceHash, { color: colors.accentLight }]} numberOfLines={1}>
                        {order.txSignature.slice(0, 8)}...{order.txSignature.slice(-8)}
                      </Text>
                      <TouchableOpacity
                        style={[styles.solscanChip, { backgroundColor: colors.primary }]}
                        onPress={() => {
                          const url = order.solscanUrl || `https://solscan.io/tx/${order.txSignature}?cluster=devnet`;
                          Linking.openURL(url).catch(() => {
                            Alert.alert('Solscan Transaction', order.txSignature!);
                          });
                        }}
                        activeOpacity={0.7}
                      >
                        <Text style={[styles.solscanChipText, { color: colors.primaryText }]}>Solscan ↗</Text>
                      </TouchableOpacity>
                    </View>
                  </View>
                ) : (
                  <View style={styles.evidenceRow}>
                    <Text style={[styles.evidenceLabel, { color: colors.textMuted }]}>Tx Signature</Text>
                    <TouchableOpacity
                      onPress={() => {
                        const url = `https://solscan.io/account/${order.borrower}?cluster=devnet`;
                        Linking.openURL(url).catch(() => {});
                      }}
                    >
                      <Text style={[styles.evidenceHash, { color: colors.accentLight }]}>View Wallet on Solscan ↗</Text>
                    </TouchableOpacity>
                  </View>
                )}

                {order.escrowAddress && (
                  <View style={styles.evidenceRow}>
                    <Text style={[styles.evidenceLabel, { color: colors.textMuted }]}>Escrow PDA</Text>
                    <TouchableOpacity
                      onPress={() => {
                        const url = `https://solscan.io/account/${order.escrowAddress}?cluster=devnet`;
                        Linking.openURL(url).catch(() => {
                          Alert.alert('Escrow Account', order.escrowAddress!);
                        });
                      }}
                    >
                      <Text style={[styles.evidenceHash, { color: colors.textSecondary }]} numberOfLines={1}>
                        {order.escrowAddress.slice(0, 8)}...{order.escrowAddress.slice(-8)} ↗
                      </Text>
                    </TouchableOpacity>
                  </View>
                )}
              </View>

              {/* Action Buttons */}
              <View style={styles.actions}>
                <TouchableOpacity
                  style={[styles.repayBtn, { backgroundColor: colors.primary }]}
                  onPress={() => onRepay(order)}
                  activeOpacity={0.85}
                >
                  <Text style={[styles.repayBtnText, { color: colors.primaryText }]}>
                    Repay ${totalDue} USDC
                  </Text>
                </TouchableOpacity>

                {!inGrace && (
                  <TouchableOpacity
                    style={[styles.graceBtn, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}
                    onPress={() => {
                      onTriggerGrace(order.id);
                    }}
                    activeOpacity={0.7}
                  >
                    <Text style={[styles.graceBtnText, { color: colors.textSecondary }]}>24h Grace</Text>
                  </TouchableOpacity>
                )}
              </View>
            </View>
          );
        })
      )}
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
  emptyBox: {
    padding: 36,
    borderRadius: 20,
    borderWidth: 1,
    alignItems: 'center',
    marginTop: 30,
  },
  emptyIcon: {
    fontSize: 44,
    marginBottom: 12,
  },
  emptyTitle: {
    fontSize: 18,
    fontWeight: '800',
    marginBottom: 6,
  },
  emptySub: {
    fontSize: 13,
    textAlign: 'center',
    lineHeight: 18,
    marginBottom: 20,
  },
  borrowNowBtn: {
    paddingHorizontal: 20,
    paddingVertical: 12,
    borderRadius: 14,
  },
  borrowNowBtnText: {
    fontSize: 14,
    fontWeight: '800',
  },
  card: {
    borderRadius: 20,
    borderWidth: 1,
    padding: 18,
    marginBottom: 16,
  },
  cardHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 4,
  },
  poolName: {
    fontSize: 16,
    fontWeight: '800',
  },
  orderId: {
    fontSize: 11,
    marginTop: 2,
  },
  badge: {
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 8,
    borderWidth: 1,
  },
  badgeText: {
    fontSize: 10,
    fontWeight: '800',
  },
  detailsBox: {
    borderRadius: 14,
    borderWidth: 1,
    padding: 14,
    marginVertical: 12,
  },
  detailRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    paddingVertical: 4,
  },
  label: {
    fontSize: 12,
  },
  value: {
    fontSize: 12,
    fontWeight: '700',
  },
  valuePurple: {
    fontSize: 12,
    fontWeight: '700',
  },
  totalRow: {
    borderTopWidth: 1,
    paddingTop: 8,
    marginTop: 6,
  },
  totalLabel: {
    fontSize: 13,
    fontWeight: '800',
  },
  totalValue: {
    fontSize: 15,
    fontWeight: '800',
  },
  actions: {
    flexDirection: 'row',
    gap: 10,
  },
  repayBtn: {
    flex: 1,
    height: 48,
    borderRadius: 14,
    justifyContent: 'center',
    alignItems: 'center',
  },
  repayBtnText: {
    fontSize: 14,
    fontWeight: '800',
  },
  graceBtn: {
    paddingHorizontal: 16,
    height: 48,
    borderRadius: 14,
    borderWidth: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  graceBtnText: {
    fontSize: 12,
    fontWeight: '700',
  },
  evidenceBox: {
    padding: 12,
    borderRadius: 12,
    borderWidth: 1,
    marginBottom: 12,
  },
  evidenceHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 8,
  },
  evidenceTitleRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
  },
  dotLive: {
    width: 6,
    height: 6,
    borderRadius: 3,
  },
  evidenceTitle: {
    fontSize: 11,
    fontWeight: '800',
    letterSpacing: 0.5,
  },
  networkBadge: {
    paddingHorizontal: 6,
    paddingVertical: 2,
    borderRadius: 6,
    borderWidth: 1,
  },
  networkBadgeText: {
    fontSize: 10,
    fontWeight: '700',
  },
  evidenceRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginVertical: 3,
  },
  evidenceLabel: {
    fontSize: 11,
    fontWeight: '600',
  },
  evidenceValRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
  },
  evidenceHash: {
    fontSize: 11,
    fontFamily: 'monospace',
    fontWeight: '700',
  },
  solscanChip: {
    paddingHorizontal: 8,
    paddingVertical: 3,
    borderRadius: 6,
  },
  solscanChipText: {
    fontSize: 10,
    fontWeight: '800',
  },
});
