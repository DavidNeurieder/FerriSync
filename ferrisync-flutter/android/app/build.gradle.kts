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
        // You can update the following values to match your app needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
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

// ─── Rust (ferrisync-core cdylib) ────────────────────────────────────────────
// Rebuild the Rust core on every Gradle build so plain `flutter run` /
// `flutter build apk` can never ship stale native libraries. The cdylib file
// name must match the stem in lib/gen/frb_generated.dart (kDefaultExternal-
// LibraryLoaderConfig), which flutter_rust_bridge derives from the crate the
// bindings were generated against (see flutter_rust_bridge.yaml: ../ferrisy-
// nc-core → libferrisync_core.so). Linker and NDK toolchain settings come
// from the repo-root .cargo/config.toml.
val rustProjectDir = File(projectDir, "../../../ferrisync-core")
val rustLibName = "libferrisync_core.so"
val rustAbiTargets = mapOf(
    "arm64-v8a" to "aarch64-linux-android",
    "x86_64" to "x86_64-linux-android",
)

// ferrisync-core is a workspace member, so cargo places build artifacts in
// the WORKSPACE-ROOT target/ dir, not ferrisync-core/target/.
val rustTargetDir = File(rustProjectDir, "../target")

rustAbiTargets.forEach { (abi, triple) ->
    tasks.register<Exec>("buildRust_$abi") {
        workingDir = rustProjectDir
        commandLine("cargo", "build", "--target", triple, "--release", "--lib")
    }
}

tasks.register<Copy>("syncRustLibs") {
    dependsOn(rustAbiTargets.keys.map { "buildRust_$it" })
    rustAbiTargets.forEach { (abi, triple) ->
        from(File(rustTargetDir, "$triple/release/$rustLibName")) {
            into(abi)
        }
    }
    into(File(projectDir, "src/main/jniLibs"))
}

tasks.named("preBuild") {
    dependsOn("syncRustLibs")
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
