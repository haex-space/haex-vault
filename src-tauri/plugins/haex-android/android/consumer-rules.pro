# Keep rules this plugin needs in the app's release build.

# rustls-platform-verifier calls these classes from Rust through JNI (spec 043, research R2).
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
