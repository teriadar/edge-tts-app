-keep class com.xdgetts.teriadar.bridge.XdgettsNativeBridge {
    *;
}
-keepclasseswithmembernames,includedescriptorclasses class * {
    native <methods>;
}

-keepattributes *Annotation*, InnerClasses
-dontnote kotlinx.serialization.AnnotationsKt

-keepclassmembers class kotlinx.serialization.json.** {
    *** Companion;
}
-keepclasseswithmembers class kotlinx.serialization.json.** {
    kotlinx.serialization.KSerializer serializer(...);
}

-keep,includedescriptorclasses class com.xdgetts.teriadar.**$$serializer { *; }
-keepclassmembers class com.xdgetts.teriadar.** {
    *** Companion;
}
-keepclasseswithmembers class com.xdgetts.teriadar.** {
    kotlinx.serialization.KSerializer serializer(...);
}
