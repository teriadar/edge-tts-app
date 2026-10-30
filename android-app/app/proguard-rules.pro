-keep class com.pagina.reader.bridge.PaginaNativeBridge {
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

-keep,includedescriptorclasses class com.pagina.reader.**$$serializer { *; }
-keepclassmembers class com.pagina.reader.** {
    *** Companion;
}
-keepclasseswithmembers class com.pagina.reader.** {
    kotlinx.serialization.KSerializer serializer(...);
}
