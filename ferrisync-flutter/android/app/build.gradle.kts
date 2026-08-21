plugins {
    id("com.android.application")
    id("kotlin-android")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

android {
    namespace = "com.example.ferrisync"
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
        // TODO: Specify your own unique Application ID (https://developer.android.com/studio/build/application-id.html).
        applicationId = "com.example.ferrisync"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    buildTypes {
        release {
            // TODO: Add your own signing config for the release build.
            // Signing with the debug keys for now, so `flutter run --release` works.
            signingConfig = signingConfigs.getByName("debug")
        }
    }
}

flutter {
    source = "../.."
}

// ─── Rust (ferrisync_flutter cdylib) ─────────────────────────────────────────
// Rebuild the Rust core on every Gradle build so plain `flutter run` /
// `flutter build apk` can never ship stale native libraries. Linker and NDK
// toolchain settings come from the repo-root .cargo/config.toml.
val rustProjectDir = File(projectDir, "../../rust")
val rustAbiTargets = mapOf(
    "arm64-v8a" to "aarch64-linux-android",
    "x86_64" to "x86_64-linux-android",
)

rustAbiTargets.forEach { (abi, triple) ->
    tasks.register<Exec>("buildRust_$abi") {
        workingDir = rustProjectDir
        commandLine("cargo", "build", "--target", triple, "--release", "--lib")
    }
}

tasks.register<Copy>("syncRustLibs") {
    dependsOn(rustAbiTargets.keys.map { "buildRust_$it" })
    rustAbiTargets.forEach { (abi, triple) ->
        from(File(rustProjectDir, "target/$triple/release/libferrisync_flutter.so")) {
            into(abi)
        }
    }
    into(File(projectDir, "src/main/jniLibs"))
}

tasks.named("preBuild") {
    dependsOn("syncRustLibs")
}

dependencies {
    androidTestImplementation("androidx.test:runner:1.6.1")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.6.1")
}
