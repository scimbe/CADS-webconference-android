pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "cads-webconference-android"
include(":app")

// rust-core/ lives outside the android/ Gradle root (sibling directory, shared with the
// non-Android cargo test/CI workflow) -- NOT verified to actually resolve/build; no
// Gradle/Android toolchain available in the environment this was written in. See
// rust-core/build.gradle.kts's own doc comment for the same caveat.
include(":rust-core")
project(":rust-core").projectDir = file("../rust-core")
