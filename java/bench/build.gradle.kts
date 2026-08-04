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
    description = "Runs the CDR encoding benchmarks."
    mainClass.set("org.openjdk.jmh.Main")
    classpath = sourceSets["jmh"].runtimeClasspath + sourceSets["jmh"].compileClasspath
    // The native library the api module loads at class-init time.
    environment("INT2DDS_JAVA_LIB",
            rootProject.file("../target/release/libint2dds_java.so").absolutePath)
}
