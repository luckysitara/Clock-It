import React, { useState, useEffect, useRef } from 'react';
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  Animated,
  Image,
} from 'react-native';
import * as Haptics from 'expo-haptics';
import { Ionicons } from '@expo/vector-icons';
import { useTheme } from '../theme/ThemeContext';
import {
  getUserPin,
  setUserPin,
  verifyUserPin,
  getLockoutRemaining,
  recordFailedAttempt,
  resetFailedAttempts,
  isPinConfigured,
  setLockEnabled,
  isBiometricsEnabled,
  checkBiometricHardware,
  authenticateWithBiometrics,
} from '../services/securityService';

const LOGO_IMG = require('../../assets/logo.png');

export type LockScreenMode = 'unlock' | 'setup' | 'change_pin';

interface SecurityLockScreenProps {
  mode?: LockScreenMode;
  onUnlock: () => void;
  onCancel?: () => void;
}

export const SecurityLockScreen: React.FC<SecurityLockScreenProps> = ({
  mode = 'unlock',
  onUnlock,
  onCancel,
}) => {
  const { colors } = useTheme();

  // Screen State
  const [currentMode, setCurrentMode] = useState<LockScreenMode>(mode);
  const [step, setStep] = useState<'enter_current' | 'enter_new' | 'confirm_new' | 'unlock'>('unlock');
  const [pin, setPin] = useState<string>('');
  const [lockoutSeconds, setLockoutSeconds] = useState<number>(0);
  const [firstEnteredPin, setFirstEnteredPin] = useState<string>('');
  const [hasBiometrics, setHasBiometrics] = useState<boolean>(false);
  const [biometricsActive, setBiometricsActive] = useState<boolean>(false);
  const [errorMsg, setErrorMsg] = useState<string>('');
  const [attempts, setAttempts] = useState<number>(0);

  // Animations
  const shakeAnim = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    initSecurity();
  }, [mode]);

  useEffect(() => {
    if (lockoutSeconds <= 0) return;
    const timer = setInterval(() => {
      setLockoutSeconds((prev) => {
        if (prev <= 1) {
          clearInterval(timer);
          setErrorMsg('');
          return 0;
        }
        return prev - 1;
      });
    }, 1000);
    return () => clearInterval(timer);
  }, [lockoutSeconds]);

  const initSecurity = async () => {
    const remaining = await getLockoutRemaining();
    if (remaining > 0) {
      setLockoutSeconds(remaining);
      setErrorMsg(`Device temporarily locked. Retry in ${remaining}s.`);
    }

    const pinConfigured = await isPinConfigured();

    // If in unlock mode but no PIN has EVER been configured, switch to setup mode
    if (mode === 'unlock' && !pinConfigured) {
      setCurrentMode('setup');
      setStep('enter_new');
      return;
    }

    if (mode === 'setup') {
      setCurrentMode('setup');
      setStep('enter_new');
      return;
    }

    if (mode === 'change_pin') {
      setCurrentMode('change_pin');
      setStep('enter_current');
      return;
    }

    // Default unlock mode (FAIL CLOSED: stays in unlock mode even if read has a transient error)
    setCurrentMode('unlock');
    setStep('unlock');

    // Check hardware biometrics & auto-prompt
    const { hasHardware, isEnrolled } = await checkBiometricHardware();
    const bioPref = await isBiometricsEnabled();
    const canUseBio = hasHardware && isEnrolled && bioPref;
    setHasBiometrics(hasHardware && isEnrolled);
    setBiometricsActive(canUseBio);

    if (canUseBio && remaining === 0) {
      setTimeout(() => {
        triggerBiometric();
      }, 350);
    }
  };

  const triggerBiometric = async () => {
    if (lockoutSeconds > 0) return;
    const success = await authenticateWithBiometrics('Unlock ClockLend with Biometrics');
    if (success) {
      await resetFailedAttempts();
      Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success).catch(() => {});
      onUnlock();
    }
  };

  const shake = () => {
    Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error).catch(() => {});
    Animated.sequence([
      Animated.timing(shakeAnim, { toValue: 12, duration: 55, useNativeDriver: true }),
      Animated.timing(shakeAnim, { toValue: -12, duration: 55, useNativeDriver: true }),
      Animated.timing(shakeAnim, { toValue: 8, duration: 55, useNativeDriver: true }),
      Animated.timing(shakeAnim, { toValue: -8, duration: 55, useNativeDriver: true }),
      Animated.timing(shakeAnim, { toValue: 0, duration: 55, useNativeDriver: true }),
    ]).start();
  };

  const handleKeyPress = async (digit: string) => {
    if (lockoutSeconds > 0) return;
    if (pin.length >= 4) return;
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(() => {});
    const newPin = pin + digit;
    setPin(newPin);
    setErrorMsg('');

    if (newPin.length === 4) {
      processCompletedPin(newPin);
    }
  };

  const processCompletedPin = async (inputPin: string) => {
    if (lockoutSeconds > 0) {
      setErrorMsg(`Device temporarily locked. Retry in ${lockoutSeconds}s.`);
      setPin('');
      return;
    }

    if (currentMode === 'unlock') {
      // Validate with hashed PIN & salt
      const isValid = await verifyUserPin(inputPin);
      if (isValid) {
        await resetFailedAttempts();
        Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success).catch(() => {});
        setTimeout(() => onUnlock(), 100);
      } else {
        const { locked, remainingSeconds } = await recordFailedAttempt();
        shake();
        if (locked) {
          setLockoutSeconds(remainingSeconds);
          setErrorMsg(`Too many incorrect attempts. Locked for ${remainingSeconds}s.`);
        } else {
          setErrorMsg('Incorrect PIN. Please try again.');
        }
        setTimeout(() => setPin(''), 450);
      }
    } else if (currentMode === 'setup') {
      if (step === 'enter_new') {
        setFirstEnteredPin(inputPin);
        setPin('');
        setStep('confirm_new');
      } else if (step === 'confirm_new') {
        if (inputPin === firstEnteredPin) {
          // PIN verified and confirmed!
          await setUserPin(inputPin);
          await setLockEnabled(true);
          Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success).catch(() => {});
          setTimeout(() => onUnlock(), 150);
        } else {
          shake();
          setErrorMsg('PINs do not match. Please try again.');
          setTimeout(() => {
            setPin('');
            setStep('enter_new');
            setFirstEnteredPin('');
          }, 500);
        }
      }
    } else if (currentMode === 'change_pin') {
      if (step === 'enter_current') {
        const isValid = await verifyUserPin(inputPin);
        if (isValid) {
          setPin('');
          setStep('enter_new');
        } else {
          shake();
          setErrorMsg('Current PIN incorrect.');
          setTimeout(() => setPin(''), 450);
        }
      } else if (step === 'enter_new') {
        setFirstEnteredPin(inputPin);
        setPin('');
        setStep('confirm_new');
      } else if (step === 'confirm_new') {
        if (inputPin === firstEnteredPin) {
          await setUserPin(inputPin);
          Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success).catch(() => {});
          setTimeout(() => onUnlock(), 150);
        } else {
          shake();
          setErrorMsg('PINs do not match. Try again.');
          setTimeout(() => {
            setPin('');
            setStep('enter_new');
            setFirstEnteredPin('');
          }, 500);
        }
      }
    }
  };

  const handleDelete = () => {
    if (pin.length > 0) {
      Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(() => {});
      setPin(pin.slice(0, -1));
      setErrorMsg('');
    }
  };

  const getHeaderTitle = () => {
    if (currentMode === 'unlock') return 'ClockLend Security';
    if (currentMode === 'setup') {
      return step === 'confirm_new' ? 'Confirm Your PIN' : 'Create 4-Digit PIN';
    }
    if (currentMode === 'change_pin') {
      if (step === 'enter_current') return 'Enter Current PIN';
      if (step === 'confirm_new') return 'Confirm New PIN';
      return 'Enter New 4-Digit PIN';
    }
    return 'Security Verification';
  };

  const getSubtitle = () => {
    if (currentMode === 'unlock') {
      return biometricsActive
        ? 'Use Fingerprint / Face ID or enter your PIN'
        : 'Enter your 4-digit PIN to access ClockLend';
    }
    if (currentMode === 'setup') {
      return step === 'confirm_new'
        ? 'Re-enter your 4-digit PIN to confirm'
        : 'Choose a personal 4-digit PIN for device security';
    }
    if (currentMode === 'change_pin') {
      if (step === 'enter_current') return 'Enter your existing security PIN to continue';
      if (step === 'confirm_new') return 'Re-enter your new PIN to confirm change';
      return 'Enter your new 4-digit security PIN';
    }
    return '';
  };

  return (
    <View style={[styles.container, { backgroundColor: colors.background }]}>
      {/* Top Bar with Cancel (if available in setup/change mode) */}
      <View style={styles.topBar}>
        {onCancel && (
          <TouchableOpacity onPress={onCancel} style={styles.cancelBtn} activeOpacity={0.7}>
            <Text style={[styles.cancelBtnText, { color: colors.textSecondary }]}>Cancel</Text>
          </TouchableOpacity>
        )}
      </View>

      {/* Brand Header */}
      <View style={styles.header}>
        <View style={styles.logoBox}>
          <Image source={LOGO_IMG} style={styles.logo} resizeMode="contain" />
        </View>
        <Text style={[styles.title, { color: colors.text }]}>{getHeaderTitle()}</Text>
        <Text style={[styles.sub, { color: colors.textSecondary }]}>{getSubtitle()}</Text>

        {/* Security Methods Status Pill */}
        <View style={styles.badgeRow}>
          <View style={[styles.methodBadge, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}>
            <Ionicons name="key" size={12} color={colors.primary} />
            <Text style={[styles.badgeText, { color: colors.text }]}>PIN: Active</Text>
          </View>
          {hasBiometrics && (
            <View
              style={[
                styles.methodBadge,
                {
                  backgroundColor: biometricsActive ? 'rgba(16, 185, 129, 0.12)' : colors.cardAlt,
                  borderColor: biometricsActive ? colors.primary : colors.cardBorder,
                },
              ]}
            >
              <Ionicons
                name="finger-print"
                size={12}
                color={biometricsActive ? colors.primary : colors.textMuted}
              />
              <Text
                style={[
                  styles.badgeText,
                  { color: biometricsActive ? colors.primary : colors.textMuted },
                ]}
              >
                Biometrics: {biometricsActive ? 'Active (2/2)' : 'Off'}
              </Text>
            </View>
          )}
        </View>
      </View>

      {/* 4 PIN Dots */}
      <Animated.View style={[styles.dotsRow, { transform: [{ translateX: shakeAnim }] }]}>
        {[0, 1, 2, 3].map((i) => {
          const filled = pin.length > i;
          return (
            <View
              key={i}
              style={[
                styles.dot,
                {
                  borderColor: filled ? colors.primary : colors.cardBorder,
                  backgroundColor: filled ? colors.primary : colors.cardAlt,
                },
              ]}
            />
          );
        })}
      </Animated.View>

      {/* Error / Status Indicator */}
      <View style={styles.hintBox}>
        {errorMsg ? (
          <Text style={[styles.errorText, { color: colors.danger }]}>{errorMsg}</Text>
        ) : null}
      </View>

      {/* Keypad */}
      <View style={styles.keypad}>
        {[
          ['1', '2', '3'],
          ['4', '5', '6'],
          ['7', '8', '9'],
        ].map((row, rIdx) => (
          <View key={rIdx} style={styles.keyRow}>
            {row.map((digit) => (
              <TouchableOpacity
                key={digit}
                style={[styles.keyBtn, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}
                onPress={() => handleKeyPress(digit)}
                activeOpacity={0.7}
              >
                <Text style={[styles.keyText, { color: colors.text }]}>{digit}</Text>
              </TouchableOpacity>
            ))}
          </View>
        ))}

        {/* Bottom Row */}
        <View style={styles.keyRow}>
          {currentMode === 'unlock' && hasBiometrics && biometricsActive ? (
            <TouchableOpacity
              style={[styles.keyBtnSpecial, { backgroundColor: 'rgba(16, 185, 129, 0.12)' }]}
              onPress={triggerBiometric}
              activeOpacity={0.7}
            >
              <Ionicons name="finger-print" size={28} color={colors.primary} />
            </TouchableOpacity>
          ) : (
            <View style={styles.keyBtnEmpty} />
          )}

          <TouchableOpacity
            style={[styles.keyBtn, { backgroundColor: colors.cardAlt, borderColor: colors.cardBorder }]}
            onPress={() => handleKeyPress('0')}
            activeOpacity={0.7}
          >
            <Text style={[styles.keyText, { color: colors.text }]}>0</Text>
          </TouchableOpacity>

          <TouchableOpacity
            style={[styles.keyBtnSpecial, { backgroundColor: colors.cardAlt }]}
            onPress={handleDelete}
            activeOpacity={0.7}
          >
            <Ionicons name="backspace-outline" size={24} color={colors.textSecondary} />
          </TouchableOpacity>
        </View>
      </View>
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    flex: 1,
    justifyContent: 'space-between',
    paddingHorizontal: 28,
    paddingTop: 54,
    paddingBottom: 44,
  },
  topBar: {
    height: 30,
    alignItems: 'flex-start',
  },
  cancelBtn: {
    paddingVertical: 4,
    paddingHorizontal: 8,
  },
  cancelBtnText: {
    fontSize: 13,
    fontWeight: '700',
  },
  header: {
    alignItems: 'center',
  },
  logoBox: {
    width: 68,
    height: 68,
    borderRadius: 20,
    justifyContent: 'center',
    alignItems: 'center',
    marginBottom: 14,
    shadowColor: '#572DFD',
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.4,
    shadowRadius: 16,
    elevation: 8,
  },
  logo: {
    width: '100%',
    height: '100%',
    borderRadius: 20,
  },
  title: {
    fontSize: 21,
    fontWeight: '900',
    letterSpacing: 0.3,
    marginBottom: 6,
    textAlign: 'center',
  },
  sub: {
    fontSize: 13,
    textAlign: 'center',
    lineHeight: 18,
    paddingHorizontal: 12,
  },
  badgeRow: {
    flexDirection: 'row',
    gap: 8,
    marginTop: 12,
  },
  methodBadge: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 10,
    paddingVertical: 4,
    borderRadius: 12,
    borderWidth: 1,
    gap: 5,
  },
  badgeText: {
    fontSize: 10,
    fontWeight: '700',
  },
  dotsRow: {
    flexDirection: 'row',
    justifyContent: 'center',
    alignItems: 'center',
    gap: 18,
    marginVertical: 14,
  },
  dot: {
    width: 18,
    height: 18,
    borderRadius: 9,
    borderWidth: 2,
  },
  hintBox: {
    height: 24,
    justifyContent: 'center',
    alignItems: 'center',
  },
  errorText: {
    fontSize: 12,
    fontWeight: '700',
  },
  keypad: {
    gap: 14,
  },
  keyRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    gap: 14,
  },
  keyBtn: {
    flex: 1,
    height: 64,
    borderRadius: 20,
    borderWidth: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  keyText: {
    fontSize: 24,
    fontWeight: '700',
  },
  keyBtnSpecial: {
    flex: 1,
    height: 64,
    borderRadius: 20,
    justifyContent: 'center',
    alignItems: 'center',
  },
  keyBtnEmpty: {
    flex: 1,
    height: 64,
  },
});
