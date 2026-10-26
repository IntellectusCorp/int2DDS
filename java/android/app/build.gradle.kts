plugins {
    id("com.android.application")
}

// Product version comes from the workspace Cargo.toml, as in ../api.
val cargoToml = rootProject.file("../../Cargo.toml").readText()
val int2ddsVersion = Regex("""\[workspace\.package\][\s\S]*?version\s*=\s*"([^"]+)"""")
    .find(cargoToml)?.groupValues?.get(1)
    ?: error("could not read [workspace.package] version from ../../Cargo.toml")

// libint2dds_java.so built by
//   cargo build --release -p int2dds-java --target aarch64-linux-android --target-dir target/android
// Override the directory with -PnativeLibDir=<dir>.
val nativeLibDir = file(
    providers.gradleProperty("nativeLibDir").orNull
        ?: rootProject.file("../../target/android/aarch64-linux-android/release").path
)
val jniLibsDir = layout.buildDirectory.dir("jniLibs")

val copyNativeLib by tasks.registering(Copy::class) {
    doFirst {
        check(nativeLibDir.resolve("libint2dds_java.so").isFile) {
            "libint2dds_java.so not found in $nativeLibDir; build it with " +
                "'cargo build --release -p int2dds-java --target aarch64-linux-android " +
                "--target-dir target/android' or pass -PnativeLibDir=<dir>"
        }
    }
    from(nativeLibDir) { include("libint2dds_java.so") }
    into(jniLibsDir.map { it.dir("arm64-v8a") })
}

android {
    namespace = "kr.co.intellectus.int2dds.android"
    compileSdk = 35

    defaultConfig {
        applicationId = "kr.co.intellectus.int2dds.android"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = int2ddsVersion
        ndk { abiFilters += "arm64-v8a" }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // The Java API is compiled from source: the multi-release JAR's
    // META-INF/versions/{9,22} overrides are JDK-only and D8 has no use for them.
    // The examples tree contributes only the generated HelloWorld type.
    sourceSets["main"].apply {
        java.srcDir("../../api/src/main/java")
        java.srcDir("../../examples/src/main/java")
        java.exclude("**/examples/HelloWorldPub.java", "**/examples/HelloWorldSub.java")
        jniLibs.srcDir(jniLibsDir)
    }
}

tasks.named("preBuild") {
    dependsOn(copyNativeLib)
}
