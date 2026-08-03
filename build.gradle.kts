// Root build file -- plugin versions declared once here, applied per-module.
//
// No `org.jetbrains.kotlin.android` plugin declared: AGP 9.0+ has built-in Kotlin
// support and actively REJECTS that plugin ("no longer required... since AGP 9.0",
// verified against the real failure in Dependabot PR #8's run 30863199045, then
// confirmed against developer.android.com's AGP 9.0.0 release notes). AGP now pulls
// in its own Kotlin Gradle Plugin (>= 2.2.10) automatically.
plugins {
    id("com.android.application") version "9.3.1" apply false
}
