# ClockLend ProGuard & R8 Hardening Configuration

# Code obfuscation & optimization
-dontusemixedcaseclassnames
-verbose

# Strip all debug and verbose logging from release binaries
-assumenosideeffects class android.util.Log {
    public static boolean isLoggable(java.lang.String, int);
    public static int v(...);
    public static int d(...);
    public static int i(...);
    public static int w(...);
}

# React Native & TurboModules preservation
-keep,allowobfuscation @interface com.facebook.common.internal.DoNotStrip
-keep,allowobfuscation @interface com.facebook.proguard.annotations.DoNotStrip
-keep,allowobfuscation @interface com.facebook.proguard.annotations.KeepGettersAndSetters

-keep @com.facebook.proguard.annotations.DoNotStrip class *
-keepclassmembers class * {
    @com.facebook.proguard.annotations.DoNotStrip *;
}

-keep class com.facebook.react.** { *; }
-keep class com.facebook.jni.** { *; }
-keep class com.facebook.soloader.** { *; }
-keep class com.facebook.react.turbomodule.** { *; }

# Hermes JavaScript Engine
-keep class com.facebook.hermes.unicode.** { *; }
-keep class com.facebook.hermes.descriptor.** { *; }
-keep class com.facebook.hermes.instrumentation.** { *; }

# Expo Modules runtime reflection
-keep class expo.modules.** { *; }

# Reanimated
-keep class com.swmansion.reanimated.** { *; }

# ClockLend Security Native Bridge (keep React Native module entry points, allow full obfuscation of SecurityIntegrity)
-keep class com.clocklend.app.security.SecurityPackage { *; }
-keep class com.clocklend.app.security.SecurityModule {
    public <init>(...);
    @com.facebook.react.bridge.ReactMethod *;
}
