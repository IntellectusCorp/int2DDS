// HelloWorld.java is generated, not hand-written. To regenerate it after an
// edit to idl/input/HelloWorld.idl, run from the repository root:
//
//   cargo run -p int2dds-idl -- idl/input/HelloWorld.idl \
//       -j java/examples/src/main/java \
//       --java-package kr.co.intellectus.int2dds.examples
//
// The build does not do this itself: an example that needed a Rust toolchain
// to compile would not be one anybody could copy.

plugins {
    java
    application
}

dependencies {
    implementation(project(":api"))
}

application {
    mainClass.set("kr.co.intellectus.int2dds.examples.HelloWorldPub")
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
val int2ddsJavaLib: String by rootProject.extra

tasks.withType<JavaExec>().configureEach {
    environment("INT2DDS_JAVA_LIB", int2ddsJavaLib)
}

tasks.register<JavaExec>("runPub") {
    group = "application"
    description = "Runs the HelloWorldPub example."
    mainClass.set("kr.co.intellectus.int2dds.examples.HelloWorldPub")
    classpath = sourceSets["main"].runtimeClasspath
}

tasks.register<JavaExec>("runSub") {
    group = "application"
    description = "Runs the HelloWorldSub example."
    mainClass.set("kr.co.intellectus.int2dds.examples.HelloWorldSub")
    classpath = sourceSets["main"].runtimeClasspath
}
