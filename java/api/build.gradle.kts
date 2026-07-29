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
    options.release.set(22)
}

dependencies {
    testImplementation("org.junit.jupiter:junit-jupiter:5.10.2")
}

tasks.jar {
    archiveBaseName.set("int2dds-api")
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
