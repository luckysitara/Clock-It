import { NativeModules, Platform } from 'react-native';
import * as SecureStore from 'expo-secure-store';
import * as LocalAuthentication from 'expo-local-authentication';

const KEY_LOCK_ENABLED = 'clocklend_security_lock_enabled';
const KEY_USER_PIN = 'clocklend_security_user_pin';
const KEY_BIOMETRICS_ENABLED = 'clocklend_security_biometrics_enabled';

export async function isLockEnabled(): Promise<boolean> {
  try {
    const val = await SecureStore.getItemAsync(KEY_LOCK_ENABLED);
    return val === 'true';
  } catch (err) {
    console.warn('Error reading lock state:', err);
    return false;
  }
}

export async function setLockEnabled(enabled: boolean): Promise<void> {
  try {
    await SecureStore.setItemAsync(KEY_LOCK_ENABLED, enabled ? 'true' : 'false');
  } catch (err) {
    console.warn('Error saving lock state:', err);
  }
}

export async function getUserPin(): Promise<string | null> {
  try {
    return await SecureStore.getItemAsync(KEY_USER_PIN);
  } catch (err) {
    console.warn('Error reading user PIN:', err);
    return null;
  }
}

export async function setUserPin(pin: string): Promise<void> {
  try {
    await SecureStore.setItemAsync(KEY_USER_PIN, pin);
  } catch (err) {
    console.warn('Error saving user PIN:', err);
  }
}

export async function isBiometricsEnabled(): Promise<boolean> {
  try {
    const val = await SecureStore.getItemAsync(KEY_BIOMETRICS_ENABLED);
    // Defaults to true if enrolled
    if (val === null) return true;
    return val === 'true';
  } catch (err) {
    return true;
  }
}

export async function setBiometricsEnabled(enabled: boolean): Promise<void> {
  try {
    await SecureStore.setItemAsync(KEY_BIOMETRICS_ENABLED, enabled ? 'true' : 'false');
  } catch (err) {
    console.warn('Error saving biometric preference:', err);
  }
}

export async function checkBiometricHardware(): Promise<{
  hasHardware: boolean;
  isEnrolled: boolean;
}> {
  try {
    const hasHardware = await LocalAuthentication.hasHardwareAsync();
    const isEnrolled = await LocalAuthentication.isEnrolledAsync();
    return { hasHardware, isEnrolled };
  } catch {
    return { hasHardware: false, isEnrolled: false };
  }
}

export async function authenticateWithBiometrics(
  prompt: string = 'Unlock ClockLend with Biometrics'
): Promise<boolean> {
  try {
    const result = await LocalAuthentication.authenticateAsync({
      promptMessage: prompt,
      fallbackLabel: 'Use PIN',
      disableDeviceFallback: false,
    });
    return result.success;
  } catch {
    return false;
  }
}

const { ClockLendSecurity } = NativeModules;

export interface DeviceIntegrityResult {
  isEmulator: boolean;
  isRooted: boolean;
  isHooking: boolean;
  isDebugger: boolean;
  isSecure: boolean;
  violationReason?: string;
}

export async function checkDeviceIntegrity(): Promise<DeviceIntegrityResult> {
  if (Platform.OS !== 'android' || !ClockLendSecurity) {
    return {
      isEmulator: false,
      isRooted: false,
      isHooking: false,
      isDebugger: false,
      isSecure: true,
    };
  }

  try {
    const status = await ClockLendSecurity.getIntegrityStatus();
    let violationReason: string | undefined;
    if (status.isEmulator) {
      violationReason = 'Virtualized Environment (Simulator / Emulator) Detected';
    } else if (status.isRooted) {
      violationReason = 'Compromised Operating System (Root / Jailbreak) Detected';
    } else if (status.isHooking) {
      violationReason = 'Dynamic Instrumentation (Frida / Hooking) Detected';
    } else if (status.isDebugger) {
      violationReason = 'Unauthorized Debugger Attached';
    }

    return {
      isEmulator: !!status.isEmulator,
      isRooted: !!status.isRooted,
      isHooking: !!status.isHooking,
      isDebugger: !!status.isDebugger,
      isSecure: !status.isEmulator && !status.isRooted && !status.isHooking && !status.isDebugger,
      violationReason,
    };
  } catch (err) {
    return {
      isEmulator: false,
      isRooted: false,
      isHooking: false,
      isDebugger: false,
      isSecure: true,
    };
  }
}

export function terminateApplication(): void {
  if (Platform.OS === 'android' && ClockLendSecurity?.terminateApp) {
    ClockLendSecurity.terminateApp();
  }
}

