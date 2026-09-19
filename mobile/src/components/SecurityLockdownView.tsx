import React from 'react';
import { StyleSheet, View, Text, TouchableOpacity, SafeAreaView, StatusBar } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { terminateApplication, DeviceIntegrityResult } from '../services/securityService';

interface SecurityLockdownViewProps {
  integrity: DeviceIntegrityResult;
}

export const SecurityLockdownView: React.FC<SecurityLockdownViewProps> = ({ integrity }) => {
  return (
    <SafeAreaView style={styles.container}>
      <StatusBar barStyle="light-content" backgroundColor="#08090C" />
      
      <View style={styles.content}>
        {/* Glowing Shield Alert Icon */}
        <View style={styles.iconContainer}>
          <View style={styles.iconGlow} />
          <View style={styles.iconCircle}>
            <Ionicons name="shield-half" size={54} color="#FF3B30" />
          </View>
        </View>

        <Text style={styles.title}>SECURITY VIOLATION</Text>
        <Text style={styles.subtitle}>
          {integrity.violationReason || 'Virtualized or Compromised Runtime'}
        </Text>

        {/* Security Diagnostics Card */}
        <View style={styles.card}>
          <View style={styles.diagRow}>
            <View style={styles.diagLeft}>
              <Ionicons
                name={integrity.isEmulator ? 'close-circle' : 'checkmark-circle'}
                size={18}
                color={integrity.isEmulator ? '#FF453A' : '#30D158'}
              />
              <Text style={styles.diagLabel}>Physical Device Hardware</Text>
            </View>
            <Text style={[styles.diagStatus, integrity.isEmulator && styles.diagStatusFail]}>
              {integrity.isEmulator ? 'FAIL (Emulator)' : 'PASS'}
            </Text>
          </View>

          <View style={styles.divider} />

          <View style={styles.diagRow}>
            <View style={styles.diagLeft}>
              <Ionicons
                name={integrity.isRooted ? 'close-circle' : 'checkmark-circle'}
                size={18}
                color={integrity.isRooted ? '#FF453A' : '#30D158'}
              />
              <Text style={styles.diagLabel}>OS Integrity (Anti-Root)</Text>
            </View>
            <Text style={[styles.diagStatus, integrity.isRooted && styles.diagStatusFail]}>
              {integrity.isRooted ? 'FAIL (Rooted)' : 'PASS'}
            </Text>
          </View>

          <View style={styles.divider} />

          <View style={styles.diagRow}>
            <View style={styles.diagLeft}>
              <Ionicons
                name={integrity.isHooking ? 'close-circle' : 'checkmark-circle'}
                size={18}
                color={integrity.isHooking ? '#FF453A' : '#30D158'}
              />
              <Text style={styles.diagLabel}>Runtime Hooking (Anti-Frida)</Text>
            </View>
            <Text style={[styles.diagStatus, integrity.isHooking && styles.diagStatusFail]}>
              {integrity.isHooking ? 'FAIL (Injected)' : 'PASS'}
            </Text>
          </View>

          <View style={styles.divider} />

          <View style={styles.diagRow}>
            <View style={styles.diagLeft}>
              <Ionicons
                name={integrity.isDebugger ? 'close-circle' : 'checkmark-circle'}
                size={18}
                color={integrity.isDebugger ? '#FF453A' : '#30D158'}
              />
              <Text style={styles.diagLabel}>Process Debugger</Text>
            </View>
            <Text style={[styles.diagStatus, integrity.isDebugger && styles.diagStatusFail]}>
              {integrity.isDebugger ? 'FAIL (Attached)' : 'PASS'}
            </Text>
          </View>
        </View>

        {/* Security Policy Advisory */}
        <View style={styles.advisoryBox}>
          <Ionicons name="information-circle-outline" size={18} color="#8E8E93" style={styles.advisoryIcon} />
          <Text style={styles.advisoryText}>
            ClockLend is cryptographically locked to physical hardware (Solana Seeker). To protect escrow contracts, borrower collateral, and private key safety, execution is barred on virtualized simulators and tampered operating systems.
          </Text>
        </View>

        {/* Exit Button */}
        <TouchableOpacity
          style={styles.exitButton}
          activeOpacity={0.8}
          onPress={() => terminateApplication()}
        >
          <Ionicons name="power-outline" size={20} color="#FFFFFF" style={{ marginRight: 8 }} />
          <Text style={styles.exitButtonText}>Terminate Application</Text>
        </TouchableOpacity>
      </View>
    </SafeAreaView>
  );
};

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#08090C',
  },
  content: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    paddingHorizontal: 24,
  },
  iconContainer: {
    position: 'relative',
    alignItems: 'center',
    justifyContent: 'center',
    marginBottom: 24,
  },
  iconGlow: {
    position: 'absolute',
    width: 100,
    height: 100,
    borderRadius: 50,
    backgroundColor: 'rgba(255, 59, 48, 0.25)',
  },
  iconCircle: {
    width: 86,
    height: 86,
    borderRadius: 43,
    backgroundColor: '#1C1517',
    borderWidth: 1.5,
    borderColor: '#FF3B30',
    alignItems: 'center',
    justifyContent: 'center',
  },
  title: {
    fontSize: 22,
    fontWeight: '800',
    color: '#FF453A',
    letterSpacing: 1.2,
    textAlign: 'center',
    marginBottom: 6,
  },
  subtitle: {
    fontSize: 14,
    color: '#8E8E93',
    textAlign: 'center',
    marginBottom: 28,
  },
  card: {
    width: '100%',
    backgroundColor: '#12151C',
    borderRadius: 16,
    padding: 16,
    borderWidth: 1,
    borderColor: '#262934',
    marginBottom: 20,
  },
  diagRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingVertical: 10,
  },
  diagLeft: {
    flexDirection: 'row',
    alignItems: 'center',
  },
  diagLabel: {
    fontSize: 14,
    color: '#FFFFFF',
    fontWeight: '500',
    marginLeft: 10,
  },
  diagStatus: {
    fontSize: 12,
    fontWeight: '700',
    color: '#30D158',
  },
  diagStatusFail: {
    color: '#FF453A',
  },
  divider: {
    height: 1,
    backgroundColor: '#1E2330',
  },
  advisoryBox: {
    flexDirection: 'row',
    backgroundColor: '#161922',
    borderRadius: 12,
    padding: 14,
    borderWidth: 1,
    borderColor: '#242838',
    marginBottom: 28,
  },
  advisoryIcon: {
    marginRight: 10,
    marginTop: 2,
  },
  advisoryText: {
    flex: 1,
    fontSize: 12,
    color: '#8E8E93',
    lineHeight: 18,
  },
  exitButton: {
    width: '100%',
    height: 52,
    borderRadius: 14,
    backgroundColor: '#FF3B30',
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
  },
  exitButtonText: {
    fontSize: 16,
    fontWeight: '700',
    color: '#FFFFFF',
  },
});
