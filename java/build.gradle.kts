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
}
