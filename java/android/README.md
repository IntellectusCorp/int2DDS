# int2DDS Android HelloWorld

Minimal app exercising the Java binding on Android. It compiles `java/api/src/main/java`
directly (JNI backend) and packages `libint2dds_java.so` built for `aarch64-linux-android`.
A standalone Gradle build: it is not part of `java/settings.gradle.kts`, so the `java/` build
and CI need no Android SDK.

## Build

1. Native library (NDK r28 is used below; adjust the path to your install):

   ```bash
   NDK="$ANDROID_HOME/ndk/28.2.13676358/toolchains/llvm/prebuilt/windows-x86_64/bin"
   export CC_aarch64_linux_android="$NDK/aarch64-linux-android26-clang.cmd"
   export AR_aarch64_linux_android="$NDK/llvm-ar.exe"
   export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$NDK/aarch64-linux-android26-clang.cmd"
   cargo build --release -p int2dds-java --target aarch64-linux-android --target-dir target/android
   ```

   `rustup target add aarch64-linux-android` once beforehand.

2. APK (`ANDROID_HOME` must point at an SDK with platform 35):

   ```bash
   cd java/android && ./gradlew assembleDebug
   adb install -r app/build/outputs/apk/debug/app-debug.apk
   ```

   `-PnativeLibDir=<dir>` points the build at a `libint2dds_java.so` elsewhere.

## Run

Pick a domain, optionally enter initial peers (`<pc-ip>:7400`) when the Wi-Fi filters multicast,
then Publish or Subscribe. The PC side can be any HelloWorld example, e.g.
`cargo run --release --example hello_world_sub` or `cd java && ./gradlew :examples:runSub`.
