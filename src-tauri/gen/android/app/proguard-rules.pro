# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile

# ML Kit (the QR and barcode scan of the barcode plugin) finds its components through the manifest
# and builds each registrar by reflection: `ComponentDiscovery` calls
# `getDeclaredConstructor().newInstance()` on the class the meta-data names. R8 keeps the class
# (the manifest names it) but drops its constructor, nobody calls it in code. Then every registrar
# is "invalid", ML Kit has no components and the first camera frame ends in a NullPointerException
# in `InputImage.fromMediaImage`: the release app crashed as soon as a scan opened the camera.
-keep class * implements com.google.firebase.components.ComponentRegistrar {
  <init>();
}