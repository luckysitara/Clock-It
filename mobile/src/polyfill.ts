import 'react-native-get-random-values';
import { Buffer } from 'buffer';

declare const global: any;
declare const window: any;

// 1. Polyfill Buffer globally across all JS runtime environments
if (typeof (globalThis as any).Buffer === 'undefined') {
  (globalThis as any).Buffer = Buffer;
}
if (typeof (global as any).Buffer === 'undefined') {
  (global as any).Buffer = Buffer;
}

// 2. Ensure standard Web Cryptography getRandomValues exists on globalThis, global, and window
// This prevents uuid and @solana/web3.js RPC transports from throwing:
// "Error: crypto.getRandomValues() not supported. See https://github.com/uuidjs/uuid#getrandomvalues-not-supported"
function setupCryptoPolyfill() {
  const targetGlobals: any[] = [];
  if (typeof globalThis !== 'undefined') targetGlobals.push(globalThis);
  if (typeof global !== 'undefined' && global !== (globalThis as any)) targetGlobals.push(global);
  if (typeof window !== 'undefined' && window !== (globalThis as any)) targetGlobals.push(window);

  for (const g of targetGlobals) {
    if (typeof g.crypto !== 'object' || !g.crypto) {
      try {
        g.crypto = {};
      } catch {
        // ignore
      }
    }

    const existingFn = g.crypto?.getRandomValues;

    const safeRandomValues = function <T extends ArrayBufferView | null>(array: T): T {
      if (!array) return array;

      if (typeof existingFn === 'function') {
        return existingFn.call(g.crypto, array);
      }

      // Security requirement: Never fall back to predictable Math.random() for cryptographic operations
      throw new Error('CSPRNG unavailable: Cryptographic random values cannot be generated securely on this device');
    };

    try {
      if (g.crypto) {
        g.crypto.getRandomValues = safeRandomValues;
      }
    } catch {
      // ignore
    }
  }
}

setupCryptoPolyfill();
