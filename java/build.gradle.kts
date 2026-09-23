plugins {
    java
}

allprojects {
    group = "com.intellectus.int2dds"
}

subprojects {
    apply(plugin = "java")

    repositories { mavenCentral() }

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

    // CI runs the test suite on each supported JDK to prove one JAR works
    // across 8 through 25. Locally this is unset and the build JDK is used.
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
