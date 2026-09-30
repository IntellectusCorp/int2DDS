plugins {
    id("org.gradle.toolchains.foojay-resolver-convention") version "0.8.0"
}

rootProject.name = "int2dds-java"

include("api")
include("bench")
include("examples")
