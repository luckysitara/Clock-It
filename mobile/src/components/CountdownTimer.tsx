import React, { useState, useEffect } from 'react';
import { View, Text, StyleSheet } from 'react-native';
import { useTheme } from '../theme/ThemeContext';

interface CountdownTimerProps {
  dueTime: number;
}

export const CountdownTimer: React.FC<CountdownTimerProps> = ({ dueTime }) => {
  const { colors } = useTheme();
  const [time, setTime] = useState({
    days: 0,
    hours: 0,
    mins: 0,
    secs: 0,
    isUrgent: false,
  });

  useEffect(() => {
    const update = () => {
      const now = Math.floor(Date.now() / 1000);
      const diff = Math.max(0, dueTime - now);

      setTime({
        days: Math.floor(diff / 86400),
        hours: Math.floor((diff % 86400) / 3600),
        mins: Math.floor((diff % 3600) / 60),
        secs: diff % 60,
        isUrgent: diff < 86400 && diff > 0,
      });
    };

    update();
    const timer = setInterval(update, 1000);
    return () => clearInterval(timer);
  }, [dueTime]);

  const pad = (n: number) => n.toString().padStart(2, '0');

  return (
    <View
      style={[
        styles.container,
        { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder },
        time.isUrgent && { borderColor: colors.danger, backgroundColor: 'rgba(239, 68, 68, 0.08)' },
      ]}
    >
      <Text
        style={[
          styles.label,
          { color: colors.textSecondary },
          time.isUrgent && { color: colors.danger, fontWeight: '800' },
        ]}
      >
        {time.isUrgent ? '⚠️ REPAYMENT DUE SOON' : 'SETTLEMENT COUNTDOWN'}
      </Text>

      <View style={styles.clockRow}>
        <View style={styles.unit}>
          <Text
            style={[
              styles.digits,
              { color: colors.primary },
              time.isUrgent && { color: colors.danger },
            ]}
          >
            {pad(time.days)}
          </Text>
          <Text style={[styles.unitLabel, { color: colors.textMuted }]}>d</Text>
        </View>
        <Text style={[styles.separator, { color: colors.textMuted }]}>:</Text>
        <View style={styles.unit}>
          <Text
            style={[
              styles.digits,
              { color: colors.primary },
              time.isUrgent && { color: colors.danger },
            ]}
          >
            {pad(time.hours)}
          </Text>
          <Text style={[styles.unitLabel, { color: colors.textMuted }]}>h</Text>
        </View>
        <Text style={[styles.separator, { color: colors.textMuted }]}>:</Text>
        <View style={styles.unit}>
          <Text
            style={[
              styles.digits,
              { color: colors.primary },
              time.isUrgent && { color: colors.danger },
            ]}
          >
            {pad(time.mins)}
          </Text>
          <Text style={[styles.unitLabel, { color: colors.textMuted }]}>m</Text>
        </View>
        <Text style={[styles.separator, { color: colors.textMuted }]}>:</Text>
        <View style={styles.unit}>
          <Text
            style={[
              styles.digits,
              { color: colors.primary },
              time.isUrgent && { color: colors.danger },
            ]}
          >
            {pad(time.secs)}
          </Text>
          <Text style={[styles.unitLabel, { color: colors.textMuted }]}>s</Text>
        </View>
      </View>
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    borderRadius: 14,
    borderWidth: 1,
    paddingVertical: 12,
    paddingHorizontal: 16,
    alignItems: 'center',
    marginVertical: 12,
  },
  label: {
    fontSize: 10,
    fontWeight: '700',
    letterSpacing: 0.8,
    marginBottom: 6,
  },
  clockRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 6,
  },
  unit: {
    flexDirection: 'row',
    alignItems: 'baseline',
    gap: 2,
  },
  digits: {
    fontSize: 24,
    fontWeight: '800',
    fontVariant: ['tabular-nums'],
  },
  unitLabel: {
    fontSize: 11,
    fontWeight: '600',
  },
  separator: {
    fontSize: 20,
    fontWeight: '600',
    marginBottom: 4,
  },
});
