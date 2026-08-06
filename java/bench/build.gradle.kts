plugins {
    java
}

// JMH needs a modern JDK to run but the library under test targets 8. The root
// build already pins --release 8 for every JavaCompile; benchmarks are not
// shipped, so they compile at the same level for consistency.
val jmhVersion = "1.37"

sourceSets {
    create("jmh") {
        java.srcDir("src/jmh/java")
        compileClasspath += sourceSets["main"].output + configurations["jmhCompileClasspath"]
        runtimeClasspath += sourceSets["main"].output
    }
}

dependencies {
    "jmhImplementation"(project(":api"))
    "jmhImplementation"("org.openjdk.jmh:jmh-core:$jmhVersion")
    "jmhAnnotationProcessor"("org.openjdk.jmh:jmh-generator-annprocess:$jmhVersion")
}

tasks.register<JavaExec>("jmh") {
    group = "verification"
    description = "Runs the CDR encoding and write-path benchmarks."
    mainClass.set("org.openjdk.jmh.Main")
    classpath = sourceSets["jmh"].runtimeClasspath + sourceSets["jmh"].compileClasspath
    // The native library the api module loads at class-init time.
    environment("INT2DDS_JAVA_LIB",
            rootProject.file("../target/release/libint2dds_java.so").absolutePath)

    // WritePathBenchmark's write arms build a real participant/topic/
    // publisher/writer (WritePathBenchmark.WriteTarget) -- the same
    // loopback isolation java/api/build.gradle.kts's Test tasks use, and for
    // the same reason spelled out there: INT2DDS_FORCE_LOOPBACK_MULTICAST
    // alone only forces the *egress* interface to 127.0.0.1, so it must be
    // paired with INT2DDS_USE_LOOPBACK_INTERFACE or a run cannot discover --
    // or be discovered by -- anything else on the subnet. The domain id is
    // picked once per Gradle invocation from the same [100, 200) range the
    // Test tasks use, well away from the real default domain 0.
    val benchDomain = (100..199).random()
    environment("INT2DDS_FORCE_LOOPBACK_MULTICAST", "true")
    environment("INT2DDS_USE_LOOPBACK_INTERFACE", "true")
    environment("DDS_DOMAIN_ID", benchDomain.toString())
    systemProperty("int2dds.bench.domain", benchDomain.toString())
}
