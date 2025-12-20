# Blinc ProGuard rules
# Keep Blinc runtime classes
-keep class blinc.** { *; }

# Keep native methods
-keepclasseswithmembernames class * {
    native <methods>;
}
