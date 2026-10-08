plugins {
    java
    id("com.diffplug.spotless") version "7.0.2" apply false
}

allprojects {
    group = "kr.co.intellectus.int2dds"
}

// The native library the test, example and benchmark tasks load: the one
// `cargo build --release -p int2dds-java` produces, unless INT2DDS_JAVA_LIB names another.
val int2ddsJavaLib by extra(
    System.getenv("INT2DDS_JAVA_LIB") ?: file(
        "../target/release/" + when {
            org.gradle.internal.os.OperatingSystem.current().isWindows -> "int2dds_java.dll"
            org.gradle.internal.os.OperatingSystem.current().isMacOsX -> "libint2dds_java.dylib"
            else -> "libint2dds_java.so"
        }
    ).absolutePath
)

subprojects {
    apply(plugin = "java")

    repositories { mavenCentral() }

    // `./gradlew spotlessApply` formats, `spotlessCheck` verifies. Kept out of
    // `check` until the tree is formatted. Generated sources are excluded: CI
    // diffs them against their generators' output.
    apply(plugin = "com.diffplug.spotless")
    configure<com.diffplug.gradle.spotless.SpotlessExtension> {
        isEnforceCheck = false
        java {
            target("src/**/*.java")
            targetExclude(
                "**/internal/ffi/Ffi.java",
                "**/types/CdrGolden*.java",
                "**/examples/HelloWorld.java"
            )
            googleJavaFormat("1.25.2").aosp()
        }
    }

    // A single JAR must run on JDK 8 through 25. Compiling with --release 8
    // makes the bytecode and the API surface both Java 8 compatible, verified
    // by the compiler rather than by convention.
    tasks.withType<JavaCompile>().configureEach {
        options.release.set(8)
        options.encoding = "UTF-8"
    }

    tasks.withType<Test>().configureEach {
        useJUnitPlatform()
    }

    // -PtestJavaVersion=<N> runs the test suite on JDK N, to check that one JAR
    // works across 8 through 25. Unset, the build JDK is used.
    val testJavaVersion = providers.gradleProperty("testJavaVersion").orNull
    if (testJavaVersion != null) {
        tasks.withType<Test>().configureEach {
            javaLauncher.set(
                javaToolchains.launcherFor {
                    languageVersion.set(JavaLanguageVersion.of(testJavaVersion))
                }
            )
        }
    }
}
