package com.clocklend.app.security

import com.facebook.react.bridge.Arguments
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod

class SecurityModule(private val reactContext: ReactApplicationContext) :
    ReactContextBaseJavaModule(reactContext) {

    override fun getName(): String = "ClockLendSecurity"

    @ReactMethod
    fun getIntegrityStatus(promise: Promise) {
        try {
            val isEmulator = SecurityIntegrity.isEmulator(reactContext)
            val isRooted = SecurityIntegrity.isRooted()
            val isHooking = SecurityIntegrity.isHookingDetected()
            val isDebugger = SecurityIntegrity.isDebuggerAttached(reactContext, false)

            val map = Arguments.createMap().apply {
                putBoolean("isEmulator", isEmulator)
                putBoolean("isRooted", isRooted)
                putBoolean("isHooking", isHooking)
                putBoolean("isDebugger", isDebugger)
                putBoolean("isSecure", !isEmulator && !isRooted && !isHooking && !isDebugger)
            }
            promise.resolve(map)
        } catch (e: Exception) {
            promise.reject("SECURITY_CHECK_ERROR", e.message, e)
        }
    }

    @ReactMethod
    fun terminateApp() {
        android.os.Process.killProcess(android.os.Process.myPid())
    }
}
