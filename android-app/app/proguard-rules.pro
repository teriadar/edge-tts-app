-keep class com.nuvola.reader.bridge.NuvolaNativeBridge {
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

-keep,includedescriptorclasses class com.nuvola.reader.**$$serializer { *; }
-keepclassmembers class com.nuvola.reader.** {
    *** Companion;
}
-keepclasseswithmembers class com.nuvola.reader.** {
    kotlinx.serialization.KSerializer serializer(...);
}
