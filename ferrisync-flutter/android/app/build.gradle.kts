plugins {
    id("com.android.application")
    id("kotlin-android")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

import java.util.Properties

android {
    namespace = "io.github.davidneurieder.ferrisync"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = JavaVersion.VERSION_17.toString()
    }

    defaultConfig {
        applicationId = "io.github.davidneurieder.ferrisync"
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    signingConfigs {
        create("release") {
            val keystorePropertiesFile = rootProject.file("key.properties")
            if (keystorePropertiesFile.exists()) {
                val keystoreProperties = Properties()
                keystorePropertiesFile.inputStream().use { keystoreProperties.load(it) }
                keyAlias = keystoreProperties["keyAlias"] as String
                keyPassword = keystoreProperties["keyPassword"] as String
                storeFile = file(keystoreProperties["storeFile"] as String)
                storePassword = keystoreProperties["storePassword"] as String
            }
        }
    }

    buildTypes {
        release {
            val keystorePropertiesFile = rootProject.file("key.properties")
            if (keystorePropertiesFile.exists()) {
                signingConfig = signingConfigs.getByName("release")
            }
            // No signingConfig when key.properties is absent: F-Droid signs its
            // own builds. For local unsigned release builds use
            // `flutter build apk --release` and sign separately if needed.
        }
    }
}

flutter {
    source = "../.."
}

// ─── Rust (ferrisync-core cdylib) ────────────────────────────────────────────
// Rebuild the Rust core on every Gradle build so plain `flutter run` /
// `flutter build apk` can never ship stale native libraries. The cdylib file
// name must match the stem in lib/gen/frb_generated.dart (kDefaultExternal-
// LibraryLoaderConfig): libferrisync_core.so.
//
// Toolchain configuration is environment-driven (no machine-specific paths in
// the repo). Requires: Rust targets for the ABIs below, `cargo-ndk`, and
// ANDROID_NDK_HOME (or an NDK installed under ANDROID_HOME/ndk/<ndkVersion>).
val rustProjectDir = File(projectDir, "../../../ferrisync-core")
val jniLibsDir = File(projectDir, "src/main/jniLibs")
val androidMinSdk = 24

val buildRust = tasks.register<Exec>("buildRust") {
    val ndkDir = android.ndkDirectory
    val ndkHome = System.getenv("ANDROID_NDK_HOME")
        ?: System.getenv("ANDROID_NDK_ROOT")
        ?: ndkDir.absolutePath

    environment("ANDROID_NDK_HOME", ndkHome)
    environment("ANDROID_NDK_ROOT", ndkHome)
    workingDir = rustProjectDir
    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a",
        "-t", "x86_64",
        "--platform", androidMinSdk.toString(),
        "-o", jniLibsDir.absolutePath,
        "build", "--release", "--lib",
    )

    inputs.dir(File(rustProjectDir, "src"))
    inputs.file(File(rustProjectDir, "Cargo.toml"))
    inputs.file(File(rustProjectDir, "../Cargo.toml"))
    inputs.file(File(rustProjectDir, "../Cargo.lock"))
    outputs.dir(jniLibsDir)

    doFirst {
        if (!ndkDir.exists()) {
            throw GradleException(
                "Android NDK not found. Set ANDROID_NDK_HOME or install NDK ${android.ndkVersion}."
            )
        }
    }
}

tasks.named("preBuild") {
    dependsOn(buildRust)
}

dependencies {
    // Kept aligned with the versions Flutter's toolchain pins via the
    // integration_test plugin (androidx.test:runner {strictly 1.2.0}):
    // newer androidx.test lines fail resolution, older ones fail the
    // Android-12 manifest export check. Only the runner is declared here;
    // the suite uses plain JUnit4 + InstrumentationRegistry (no ext:junit,
    // which transitively pulls androidx.test:core and its un-exported
    // legacy activities).
    androidTestImplementation("androidx.test:runner:1.2.0")
}
