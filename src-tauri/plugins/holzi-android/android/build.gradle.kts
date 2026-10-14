import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "space.haex.holzi.android"
    compileSdk = 36

    defaultConfig {
        minSdk = 26
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
}

// Spec 043 (research R2): the Kotlin half of rustls-platform-verifier, which checks TLS
// certificates against the phone's trust store. Its version must match
// `rustls-platform-verifier-android` in src-tauri/Cargo.lock. Upstream publishes it only on a
// GitHub branch (maven-archive), whose answers failed CI builds with HTTP 500, so the .aar and
// .pom of that branch lie in maven/ here. The app resolves this module's dependencies with its
// own repositories, so the repository is added to every project.
val vendoredMaven = file("maven")
rootProject.allprojects {
    repositories {
        maven {
            url = uri(vendoredMaven)
            content { includeGroup("org.rustls") }
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.9.0")
    implementation("org.rustls:rustls-platform-verifier:0.2.0")
    implementation(project(":tauri-android"))
}
