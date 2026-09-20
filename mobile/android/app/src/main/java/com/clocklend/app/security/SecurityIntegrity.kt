package com.clocklend.app.security

import android.content.Context
import android.content.pm.ApplicationInfo
import android.os.Build
import android.os.Debug
import java.io.File

object SecurityIntegrity {

    /**
     * Detects if the app is executing inside an Android simulator/emulator (QEMU, Goldfish, Ranchu,
     * Genymotion, BlueStacks, LDPlayer, Nox, Android Studio Virtual Device).
     */
    fun isEmulator(context: Context): Boolean {
        // 1. Check Build & Hardware Fingerprints
        val checkBuild = (
            Build.FINGERPRINT.startsWith("generic") ||
            Build.FINGERPRINT.startsWith("unknown") ||
            Build.FINGERPRINT.contains("google/sdk_gphone") ||
            Build.FINGERPRINT.contains("vbox") ||
            Build.MODEL.contains("google_sdk") ||
            Build.MODEL.contains("Emulator") ||
            Build.MODEL.contains("Android SDK built for") ||
            Build.MODEL.contains("sdk_gphone") ||
            Build.MODEL.contains("VirtualBox") ||
            Build.MANUFACTURER.contains("Genymotion") ||
            Build.HARDWARE.contains("goldfish") ||
            Build.HARDWARE.contains("ranchu") ||
            Build.HARDWARE.contains("vbox86") ||
            Build.HARDWARE.contains("nox") ||
            Build.PRODUCT.contains("sdk_gphone") ||
            Build.PRODUCT.contains("google_sdk") ||
            Build.PRODUCT.contains("vbox86p") ||
            Build.PRODUCT.contains("emulator") ||
            Build.PRODUCT.contains("nox") ||
            Build.BOARD.lowercase().contains("goldfish") ||
            (Build.BRAND.startsWith("generic") && Build.DEVICE.startsWith("generic"))
        )
        if (checkBuild) return true

        // 2. Check Virtualization Driver & Pipe Nodes
        val qemuPipes = arrayOf(
            "/dev/socket/qemud",
            "/dev/qemu_pipe",
            "/system/lib/libc_malloc_debug_qemu.so",
            "/sys/qemu_trace",
            "/system/bin/nox-prop",
            "/system/bin/androVM-prop"
        )
        for (path in qemuPipes) {
            try {
                if (File(path).exists()) return true
            } catch (_: Throwable) {}
        }

        // 3. Check QEMU and Virtual System Properties
        try {
            val systemProperties = Class.forName("android.os.SystemProperties")
            val getMethod = systemProperties.getMethod("get", String::class.java)
            val qemuProp = getMethod.invoke(null, "ro.kernel.qemu") as? String
            if (qemuProp == "1") return true

            val virtualProp = getMethod.invoke(null, "ro.hardware.virtual") as? String
            if (virtualProp != null && (virtualProp == "1" || virtualProp.equals("true", ignoreCase = true))) {
                return true
            }
        } catch (_: Throwable) {}

        return false
    }

    /**
     * Detects if the device is rooted (Magisk, SuperSU, test-keys, or presence of su binaries).
     */
    fun isRooted(): Boolean {
        // Check test-keys
        val buildTags = Build.TAGS
        if (buildTags != null && buildTags.contains("test-keys")) {
            return true
        }

        // Check common su binary paths
        val suPaths = arrayOf(
            "/system/app/Superuser.apk",
            "/sbin/su",
            "/system/bin/su",
            "/system/xbin/su",
            "/data/local/xbin/su",
            "/data/local/bin/su",
            "/system/sd/xbin/su",
            "/system/bin/failsafe/su",
            "/data/local/su",
            "/su/bin/su",
            "/sbin/.magisk",
            "/data/adb/magisk"
        )
        for (path in suPaths) {
            try {
                if (File(path).exists()) return true
            } catch (_: Throwable) {}
        }

        return false
    }

    /**
     * Detects if dynamic binary instrumentation / hooking frameworks (Frida, Xposed, LSPosed)
     * are injected into the process memory.
     */
    fun isHookingDetected(): Boolean {
        try {
            // Scan /proc/self/maps for injected libraries
            val mapsFile = File("/proc/self/maps")
            if (mapsFile.exists()) {
                val found = mapsFile.useLines { lines ->
                    lines.any { line ->
                        val lower = line.lowercase()
                        lower.contains("frida-agent") ||
                        lower.contains("frida-gadget") ||
                        lower.contains("libfrida") ||
                        lower.contains("gum-js-loop") ||
                        lower.contains("linjector") ||
                        lower.contains("xposedbridge.jar")
                    }
                }
                if (found) return true
            }
        } catch (_: Throwable) {}

        // Check Xposed classloader presence
        try {
            Class.forName("de.robv.android.xposed.XposedBridge")
            return true
        } catch (_: ClassNotFoundException) {}
        catch (_: Throwable) {}

        return false
    }

    /**
     * Detects if an active debugger is attached, or if the package was recompiled with debuggable flag.
     */
    fun isDebuggerAttached(context: Context, isRelease: Boolean = true): Boolean {
        if (Debug.isDebuggerConnected() || Debug.waitingForDebugger()) {
            return true
        }
        if (isRelease) {
            val isDebuggable = (context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0
            if (isDebuggable) {
                return true
            }
        }
        return false
    }
}
