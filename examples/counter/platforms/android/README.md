# counter - Android

Android platform files for counter.

## Building

```bash
# From project root
blinc build --target android --release

# Or using Gradle directly
cd platforms/android
./gradlew assembleRelease
```

## Requirements

- Android SDK with API 35
- Gradle 8.x
- JDK 17+

## Configuration

Edit `app/build.gradle.kts` to modify:
- Package name
- Min/Target SDK versions
- Build settings
