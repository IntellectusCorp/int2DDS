plugins {
    java
    `maven-publish`
}

// Product version comes from the workspace Cargo.toml, matching how
// csharp/Directory.Build.props reads it. Bump the version in ../../Cargo.toml only.
val cargoToml = rootProject.file("../Cargo.toml").readText()
version = Regex("""\[workspace\.package\][\s\S]*?version\s*=\s*"([^"]+)"""")
    .find(cargoToml)?.groupValues?.get(1)
    ?: error("could not read [workspace.package] version from ../Cargo.toml")

// Source set for the JDK 22+ Panama backend, populated on
// feature/java-panama-backend. Wired now so that branch is purely additive.
val java22 by sourceSets.creating {
    java.srcDir("src/main/java22")
    compileClasspath += sourceSets.main.get().output
}

// Only meaningful once src/main/java22 has sources, and only compilable on a
// JDK 22+ toolchain. Until then the task is NO-SOURCE and never runs, so the
// build stays green on the JDK 17 baseline.
tasks.named<JavaCompile>("compileJava22Java") {
    javaCompiler.set(javaToolchains.compilerFor {
        languageVersion.set(JavaLanguageVersion.of(24))
    })
    options.release.set(22)
}

// Source set for the JDK 9+ override of NativeKeepAlive, which delegates
// straight to java.lang.ref.Reference.reachabilityFence -- unavailable
// before 9, which is exactly why the --release-8 src/main/java fallback
// exists at all. Same shape as java22 above; release 9 rather than 22 means
// this one is compilable by this build's own JDK 17 toolchain today, so
// unlike java22 it is populated immediately, not left NO-SOURCE.
val java9 by sourceSets.creating {
    java.srcDir("src/main/java9")
    compileClasspath += sourceSets.main.get().output
}

tasks.named<JavaCompile>("compileJava9Java") {
    options.release.set(9)
}

dependencies {
    testImplementation("org.junit.jupiter:junit-jupiter:5.10.2")
}

// Tests load the native library built by `cargo build --release -p int2dds-java`.
// Overridable so CI can point at a downloaded artifact instead.
tasks.withType<Test>().configureEach {
    // JarLayoutTest's multi-release check inspects the packaged jar itself,
    // not the exploded classes this task otherwise runs against -- without
    // this dependency it could read a stale jar from a previous build (or
    // none at all on a clean checkout) and silently pass or silently fail
    // for the wrong reason. int2dds.built.jar hands it that jar's actual,
    // freshly-configured location; archiveFile is exactly what the jar task
    // itself will produce, so this can never point somewhere the jar task
    // didn't actually write to.
    dependsOn(tasks.jar)
    systemProperty("int2dds.built.jar", tasks.jar.get().archiveFile.get().asFile.absolutePath)

    val fromEnv = System.getenv("INT2DDS_JAVA_LIB")
    val builtName = when {
        org.gradle.internal.os.OperatingSystem.current().isWindows -> "int2dds_java.dll"
        org.gradle.internal.os.OperatingSystem.current().isMacOsX -> "libint2dds_java.dylib"
        else -> "libint2dds_java.so"
    }
    val built = rootProject.file("../target/release/$builtName")
    environment("INT2DDS_JAVA_LIB", fromEnv ?: built.absolutePath)

    // Entity tests create real participants. INT2DDS_FORCE_LOOPBACK_MULTICAST
    // alone only forces the *egress* interface to 127.0.0.1 -- the receive
    // side still joins the SPDP group on every working IP and binds 0.0.0.0.
    // Pairing it with INT2DDS_USE_LOOPBACK_INTERFACE puts 127.0.0.1 in that
    // working-IP list too, so loopback-sent multicast is actually received
    // rather than sent into a void (docs/guide/env.md, "Interaction with
    // Other Settings"). Without both, a run cannot discover -- or be
    // discovered by -- anything else on the subnet. testDomain also picks
    // one domain id per JVM well away from the default 0 and the low numbers
    // people choose by hand, and is exported as DDS_DOMAIN_ID so that a
    // participant constructed with the -1 (DEFAULT_DOMAIN_ID) sentinel
    // resolves into this run's isolated domain too, rather than escaping to
    // the real default domain.
    val testDomain = (100..199).random()
    environment("INT2DDS_FORCE_LOOPBACK_MULTICAST", "true")
    environment("INT2DDS_USE_LOOPBACK_INTERFACE", "true")
    environment("DDS_DOMAIN_ID", testDomain.toString())
    systemProperty("int2dds.test.domain", testDomain.toString())

    // testDomain is randomized per Gradle invocation (configuration time),
    // which today happens to make this task's input hash differ every run
    // and so it is never seen as UP-TO-DATE or restored from the build
    // cache -- but that is a side effect of a changing input, not a
    // decision, and it would silently stop working the moment Gradle's
    // configuration cache (not enabled in this build, but a future option)
    // froze testDomain along with the rest of the configuration, turning
    // "one random domain per run" into "one random domain forever". State
    // the real requirement directly instead: this task must always execute.
    doNotTrackState(
        "creates real participants with a per-run random domain id; " +
            "must always re-run, never be treated as up-to-date or cached"
    )
}

tasks.jar {
    archiveBaseName.set("int2dds-api")
    into("META-INF/versions/9") {
        from(java9.output)
    }
    into("META-INF/versions/22") {
        from(java22.output)
    }
    manifest {
        attributes(
            "Multi-Release" to "true",
            // JDK 24+ (JEP 472) restricts JNI; without this, loading the native
            // library prints a warning. Applies equally to Panama.
            "Enable-Native-Access" to "ALL-UNNAMED",
            "Implementation-Title" to "int2dds-api",
            "Implementation-Version" to project.version.toString()
        )
    }
}

publishing {
    publications {
        create<MavenPublication>("maven") {
            artifactId = "int2dds-api"
            from(components["java"])
        }
    }
}
