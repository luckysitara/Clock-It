import { NativeModules, Platform } from 'react-native';
import * as SecureStore from 'expo-secure-store';
import * as LocalAuthentication from 'expo-local-authentication';

const KEY_LOCK_ENABLED = 'clocklend_security_lock_enabled';
const KEY_PIN_SALT = 'clocklend_security_pin_salt';
const KEY_PIN_HASH = 'clocklend_security_pin_hash';
const KEY_PIN_CONFIGURED = 'clocklend_security_pin_configured';
const KEY_FAILED_ATTEMPTS = 'clocklend_security_failed_attempts';
const KEY_LOCKOUT_UNTIL = 'clocklend_security_lockout_until';
const KEY_BIOMETRICS_ENABLED = 'clocklend_security_biometrics_enabled';

// Standalone, deterministic SHA-256 implementation
function sha256(ascii: string): string {
  function rightRotate(value: number, amount: number) {
    return (value >>> amount) | (value << (32 - amount));
  }
  let result = '';
  const words: number[] = [];
  const asciiBitLength = ascii.length * 8;
  const hash = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
  ];
  const k = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
  ];

  for (let i = 0; i < ascii.length; i++) {
    words[i >> 2] |= (ascii.charCodeAt(i) & 0xff) << (24 - (i % 4) * 8);
  }
  words[ascii.length >> 2] |= 0x80 << (24 - (ascii.length % 4) * 8);
  words[(((ascii.length + 8) >> 6) << 4) + 15] = asciiBitLength;

  for (let j = 0; j < words.length; j += 16) {
    const w = words.slice(j, j + 16);
    const oldHash = hash.slice(0);
    for (let i = 0; i < 64; i++) {
      if (i >= 16) {
        const s0 = rightRotate(w[i - 15], 7) ^ rightRotate(w[i - 15], 18) ^ (w[i - 15] >>> 3);
        const s1 = rightRotate(w[i - 2], 17) ^ rightRotate(w[i - 2], 19) ^ (w[i - 2] >>> 10);
        w[i] = (w[i - 16] + s0 + w[i - 7] + s1) | 0;
      }
      const s1 = rightRotate(hash[4], 6) ^ rightRotate(hash[4], 11) ^ rightRotate(hash[4], 25);
      const ch = (hash[4] & hash[5]) ^ (~hash[4] & hash[6]);
      const temp1 = (hash[7] + s1 + ch + k[i] + (w[i] | 0)) | 0;
      const s0 = rightRotate(hash[0], 2) ^ rightRotate(hash[0], 13) ^ rightRotate(hash[0], 22);
      const maj = (hash[0] & hash[1]) ^ (hash[0] & hash[2]) ^ (hash[1] & hash[2]);
      const temp2 = (s0 + maj) | 0;

      hash[7] = hash[6];
      hash[6] = hash[5];
      hash[5] = hash[4];
      hash[4] = (hash[3] + temp1) | 0;
      hash[3] = hash[2];
      hash[2] = hash[1];
      hash[1] = hash[0];
      hash[0] = (temp1 + temp2) | 0;
    }
    for (let i = 0; i < 8; i++) {
      hash[i] = (hash[i] + oldHash[i]) | 0;
    }
  }
  for (let i = 0; i < 8; i++) {
    result += (hash[i] >>> 0).toString(16).padStart(8, '0');
  }
  return result;
}

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

export async function isPinConfigured(): Promise<boolean> {
  try {
    const val = await SecureStore.getItemAsync(KEY_PIN_CONFIGURED);
    return val === 'true';
  } catch {
    // Fail closed: assume configured if error
    return true;
  }
}

export async function getUserPin(): Promise<string | null> {
  try {
    return await SecureStore.getItemAsync(KEY_PIN_HASH);
  } catch (err) {
    console.warn('Error reading user PIN hash:', err);
    return null;
  }
}

export async function setUserPin(pin: string): Promise<void> {
  try {
    // Generate or fetch salt
    let salt = await SecureStore.getItemAsync(KEY_PIN_SALT);
    if (!salt) {
      salt = Math.floor(Date.now() * Math.random()).toString(36) + '-' + Math.floor(Math.random() * 1e9).toString(36);
      await SecureStore.setItemAsync(KEY_PIN_SALT, salt);
    }
    const hash = sha256(`${salt}:${pin}`);
    await SecureStore.setItemAsync(KEY_PIN_HASH, hash);
    await SecureStore.setItemAsync(KEY_PIN_CONFIGURED, 'true');
    await resetFailedAttempts();
  } catch (err) {
    console.warn('Error saving user PIN hash:', err);
    throw err;
  }
}

export async function verifyUserPin(inputPin: string): Promise<boolean> {
  try {
    const storedHash = await SecureStore.getItemAsync(KEY_PIN_HASH);
    if (!storedHash) return false;
    const salt = await SecureStore.getItemAsync(KEY_PIN_SALT);
    if (!salt) return false;

    const computedHash = sha256(`${salt}:${inputPin}`);
    return computedHash === storedHash;
  } catch (err) {
    console.warn('Error verifying user PIN:', err);
    return false;
  }
}

export async function getLockoutRemaining(): Promise<number> {
  try {
    const val = await SecureStore.getItemAsync(KEY_LOCKOUT_UNTIL);
    if (!val) return 0;
    const lockoutUntil = parseInt(val, 10);
    const now = Math.floor(Date.now() / 1000);
    if (lockoutUntil > now) {
      return lockoutUntil - now;
    }
    return 0;
  } catch {
    return 0;
  }
}

export async function recordFailedAttempt(): Promise<{ locked: boolean; remainingSeconds: number }> {
  try {
    let attempts = 1;
    const storedAttempts = await SecureStore.getItemAsync(KEY_FAILED_ATTEMPTS);
    if (storedAttempts) {
      attempts = parseInt(storedAttempts, 10) + 1;
    }
    await SecureStore.setItemAsync(KEY_FAILED_ATTEMPTS, attempts.toString());

    let lockDuration = 0;
    if (attempts >= 10) {
      lockDuration = 300; // 5 minute lockout
    } else if (attempts >= 5) {
      lockDuration = 30; // 30 second lockout
    }

    if (lockDuration > 0) {
      const lockoutUntil = Math.floor(Date.now() / 1000) + lockDuration;
      await SecureStore.setItemAsync(KEY_LOCKOUT_UNTIL, lockoutUntil.toString());
      return { locked: true, remainingSeconds: lockDuration };
    }

    return { locked: false, remainingSeconds: 0 };
  } catch {
    return { locked: false, remainingSeconds: 0 };
  }
}

export async function resetFailedAttempts(): Promise<void> {
  try {
    await SecureStore.deleteItemAsync(KEY_FAILED_ATTEMPTS);
    await SecureStore.deleteItemAsync(KEY_LOCKOUT_UNTIL);
  } catch {}
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

