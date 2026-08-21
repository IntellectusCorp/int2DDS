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

// The example loads the native library the same way the tests do. It sets
// nothing else: an example is copied, and network settings copied out of one
// are the hardest kind of setting to later explain. INT2DDS_FORCE_LOOPBACK_
// MULTICAST in particular would be wrong here twice over -- it contradicts
// this example's own claim to publish on the real default domain, and alone
// it forces only the egress interface, so anyone who pairs it later without
// INT2DDS_USE_LOOPBACK_INTERFACE gets a receive side that never has 127.0.0.1
// in its working-IP list. The tests set both, deliberately, because they must
// not discover each other across a subnet; an example has no such need.
tasks.named<JavaExec>("run") {
    environment("INT2DDS_JAVA_LIB",
            rootProject.file("../target/release/libint2dds_java.so").absolutePath)
}

// Launches DynamicHelloWorld the same way "run" launches HelloWorldPub: same
// native lib env var, nothing else added (see the note above "run" for why).
tasks.register<JavaExec>("runDynamic") {
    group = "application"
    description = "Runs the DynamicHelloWorld (XTypes) example."
    mainClass.set("com.intellectus.int2dds.examples.DynamicHelloWorld")
    classpath = sourceSets["main"].runtimeClasspath
    environment("INT2DDS_JAVA_LIB",
            rootProject.file("../target/release/libint2dds_java.so").absolutePath)
}
