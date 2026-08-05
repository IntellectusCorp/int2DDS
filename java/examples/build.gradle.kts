plugins {
    java
    application
}

dependencies {
    implementation(project(":api"))
}

application {
    mainClass.set("com.intellectus.int2dds.examples.HelloWorldPub")
}

// The example loads the native library the same way the tests do.
tasks.named<JavaExec>("run") {
    environment("INT2DDS_JAVA_LIB",
            rootProject.file("../target/release/libint2dds_java.so").absolutePath)
    environment("INT2DDS_FORCE_LOOPBACK_MULTICAST", "true")
}
