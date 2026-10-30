package com.pagina.reader.bridge

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

@Serializable
enum class ServiceStatus {
    Healthy,
    Degraded,
    Offline
}

@Serializable
data class EdgeHealthReport(
    val status: ServiceStatus,
    @SerialName("is_operational") val isOperational: Boolean,
    @SerialName("voices_api_reachable") val voicesApiReachable: Boolean,
    @SerialName("websocket_handshake_ok") val websocketHandshakeOk: Boolean,
    @SerialName("http_latency_ms") val httpLatencyMs: Long? = null,
    @SerialName("ws_handshake_latency_ms") val wsHandshakeLatencyMs: Long? = null,
    @SerialName("available_voices_count") val availableVoicesCount: Int = 0,
    @SerialName("english_voices_count") val englishVoicesCount: Int = 0,
    @SerialName("clock_skew_seconds") val clockSkewSeconds: Double = 0.0,
    @SerialName("sec_ms_gec_version") val secMsGecVersion: String = "",
    val message: String = "",
    @SerialName("checked_at_unix_ms") val checkedAtUnixMs: Long = 0L
)

@Serializable
data class VoicePreset(
    @SerialName("short_name") val shortName: String,
    @SerialName("display_name") val displayName: String,
    val locale: String,
    val gender: String,
    val description: String
)

@Serializable
data class EdgeVoiceTagDto(
    @SerialName("ContentCategories") val contentCategories: List<String> = emptyList(),
    @SerialName("VoicePersonalities") val voicePersonalities: List<String> = emptyList()
)

@Serializable
data class EdgeVoiceDto(
    @SerialName("Name") val name: String = "",
    @SerialName("ShortName") val shortName: String,
    @SerialName("Gender") val gender: String = "",
    @SerialName("Locale") val locale: String = "en-US",
    @SerialName("FriendlyName") val friendlyName: String = "",
    @SerialName("VoiceTag") val voiceTag: EdgeVoiceTagDto = EdgeVoiceTagDto()
) {
    fun toVoicePreset(): VoicePreset {
        val cleanName = shortName
            .removePrefix("$locale-")
            .removeSuffix("Neural")
        val region = locale.substringAfter('-', locale)
        val personalities = voiceTag.voicePersonalities.takeIf { it.isNotEmpty() }
            ?.joinToString(", ")
            ?: "Natural neural voice"
        return VoicePreset(
            shortName = shortName,
            displayName = "$cleanName ($region)",
            locale = locale,
            gender = gender,
            description = "$personalities · $locale"
        )
    }
}

@Serializable
data class WordSpan(
    val id: Int,
    @SerialName("char_start") val charStart: Int,
    @SerialName("char_end") val charEnd: Int,
    val text: String
)

@Serializable
enum class ParagraphKind {
    Body,
    Title,
    Subtitle,
    Quote,
    Footnote
}

@Serializable
data class ParagraphData(
    val id: Int,
    val text: String,
    val kind: ParagraphKind = ParagraphKind.Body,
    @SerialName("chapter_offset") val chapterOffset: Int = 0,
    val words: List<WordSpan> = emptyList()
)

@Serializable
data class ChapterData(
    val index: Int = 0,
    val title: String,
    val href: String = "",
    val paragraphs: List<ParagraphData>
)

@Serializable
data class EpubBookData(
    val title: String,
    val author: String = "Unknown Author",
    val language: String = "en",
    val description: String? = null,
    val publisher: String? = null,
    @SerialName("published_date") val publishedDate: String? = null,
    val isbn: String? = null,
    @SerialName("cover_href") val coverHref: String? = null,
    @SerialName("cover_image_path") val coverImagePath: String? = null,
    val chapters: List<ChapterData> = emptyList()
)

@Serializable
data class OnlineBookMetadata(
    val title: String? = null,
    val author: String? = null,
    val description: String? = null,
    val publisher: String? = null,
    @SerialName("published_date") val publishedDate: String? = null,
    val language: String? = null,
    val isbn: String? = null,
    val categories: List<String> = emptyList(),
    @SerialName("page_count") val pageCount: Int? = null,
    @SerialName("cover_url") val coverUrl: String? = null,
    @SerialName("cover_image_path") val coverImagePath: String? = null,
    @SerialName("source_provider") val sourceProvider: String = "Google Books"
)

@Serializable
data class TtsCacheStats(
    @SerialName("memory_entries") val memoryEntries: Int = 0,
    @SerialName("disk_entries") val diskEntries: Int = 0,
    @SerialName("disk_bytes") val diskBytes: Long = 0L
)

@Serializable
data class SentenceChunk(
    @SerialName("chunk_index") val chunkIndex: Int,
    @SerialName("paragraph_id") val paragraphId: Int = 0,
    val text: String,
    @SerialName("char_start") val charStart: Int,
    @SerialName("char_end") val charEnd: Int,
    @SerialName("word_ids") val wordIds: List<Int>
)

@Serializable
data class WordAudioTiming(
    @SerialName("word_id") val wordId: Int,
    @SerialName("paragraph_id") val paragraphId: Int = 0,
    val text: String,
    @SerialName("char_start") val charStart: Int,
    @SerialName("char_end") val charEnd: Int,
    @SerialName("audio_offset_ms") val audioOffsetMs: Long,
    @SerialName("audio_duration_ms") val audioDurationMs: Long = 0L,
    @SerialName("audio_end_ms") val audioEndMs: Long
) {
    val audioStartMs: Long
        get() = audioOffsetMs
}

@Serializable
data class SynthesizedChunkDto(
    @SerialName("paragraph_id") val paragraphId: Int,
    @SerialName("chunk_index") val chunkIndex: Int,
    @SerialName("cache_key_sha256") val cacheKeySha256: String,
    @SerialName("audio_file_path") val audioFilePath: String? = null,
    @SerialName("audio_mp3") val audioMp3: List<Int> = emptyList(),
    @SerialName("word_timings") val wordTimings: List<WordAudioTiming> = emptyList()
) {

    fun toMp3ByteArray(): ByteArray = ByteArray(audioMp3.size) { i -> audioMp3[i].toByte() }
}

@Serializable
data class ClickToReadDto(
    @SerialName("tapped_word") val tappedWord: WordSpan,
    @SerialName("target_chunk_index") val targetChunkIndex: Int,
    val chunks: List<SentenceChunk>,
    val generation: Long
)

@Serializable
data class BridgeResponse<T>(
    val ok: Boolean,
    val data: T? = null,
    val error: String? = null
)

object PaginaNativeBridge {

    private val json = Json {
        ignoreUnknownKeys = true
        isLenient = true
        encodeDefaults = true
    }

    val isNativeLoaded: Boolean = try {
        System.loadLibrary("pagina_core")
        true
    } catch (_: Throwable) {
        false
    }

    private val fallbackPresets = listOf(
        VoicePreset("it-IT-IsabellaNeural", "Isabella (Italiano)", "it-IT", "Female", "Voce femminile calda, espressiva e naturale per romanzi"),
        VoicePreset("it-IT-DiegoNeural", "Diego (Italiano)", "it-IT", "Male", "Voce maschile profonda e chiara per narrativa e saggistica"),
        VoicePreset("it-IT-ElsaNeural", "Elsa (Italiano)", "it-IT", "Female", "Voce femminile nitida, scorrevole e articolata"),
        VoicePreset("it-IT-GiuseppeMultilingualNeural", "Giuseppe (Italiano Multilingual)", "it-IT", "Male", "Voce maschile naturale multilingua (Italiano / Inglese)"),
        VoicePreset("en-US-AvaMultilingualNeural", "Ava (English US)", "en-US", "Female", "Expressive, warm American storytelling voice"),
        VoicePreset("en-US-AndrewMultilingualNeural", "Andrew (English US)", "en-US", "Male", "Warm, confident American narrator voice"),
        VoicePreset("en-GB-SoniaNeural", "Sonia (English UK)", "en-GB", "Female", "Clear, classic British female voice"),
        VoicePreset("en-GB-RyanNeural", "Ryan (English UK)", "en-GB", "Male", "Natural British male narrator voice")
    )

    @JvmStatic
    external fun nativeInitSession(cacheDir: String, voice: String, rate: String): String

    @JvmStatic
    external fun nativeCancelPendingQueue(): String

    @JvmStatic
    external fun nativeCheckHealth(timeoutMs: Long): String

    @JvmStatic
    external fun nativeQuickPing(timeoutMs: Long): String

    @JvmStatic
    external fun nativeGetEnglishVoicePresets(): String

    @JvmStatic
    external fun nativeFetchEnglishVoices(): String

    @JvmStatic
    external fun nativeLoadEpub(epubPath: String): String

    @JvmStatic
    external fun nativeLoadEpubBook(epubPath: String, coverOutputDir: String): String

    @JvmStatic
    external fun nativeFetchBookMetadataOnline(
        title: String,
        author: String,
        isbn: String,
        coverOutputDir: String
    ): String

    @JvmStatic
    external fun nativeGetCacheStats(): String

    @JvmStatic
    external fun nativeClearCache(): String

    @JvmStatic
    external fun nativePruneCache(maxBytes: Long): String

    @JvmStatic
    external fun nativeChunkParagraph(paragraphJson: String, sentencesPerChunk: Int): String

    @JvmStatic
    external fun nativeSynthesizeWithPrebuffer(
        paragraphJson: String,
        chunksJson: String,
        activeIdx: Int
    ): String

    @JvmStatic
    external fun nativeSynthesizeWithCrossParagraphPrebuffer(
        paragraphJson: String,
        chunksJson: String,
        activeIdx: Int,
        nextParagraphJson: String
    ): String

    @JvmStatic
    external fun nativeClickToReadResolve(
        paragraphJson: String,
        charOffset: Int,
        sentencesPerChunk: Int
    ): String

    @JvmStatic
    external fun nativeActiveWordAtMs(timingsJson: String, positionMs: Long): String

    private inline fun <reified T> decodeResponse(rawJson: String): Result<T> {
        return runCatching {
            val envelope = json.decodeFromString<BridgeResponse<T>>(rawJson)
            if (envelope.ok && envelope.data != null) {
                envelope.data
            } else {
                throw IllegalStateException(envelope.error ?: "Unknown native bridge error")
            }
        }
    }

    suspend fun initSession(
        cacheDir: String,
        voice: String = "en-US-AriaNeural",
        rate: String = "+0%"
    ): Result<Boolean> = withContext(Dispatchers.IO) {
        if (!isNativeLoaded) return@withContext Result.success(true)
        decodeResponse(nativeInitSession(cacheDir, voice, rate))
    }

    suspend fun cancelPendingQueue(): Result<Long> = withContext(Dispatchers.IO) {
        if (!isNativeLoaded) return@withContext Result.success(1L)
        decodeResponse(nativeCancelPendingQueue())
    }

    suspend fun checkHealth(timeoutMs: Long = 5000L): Result<EdgeHealthReport> =
        withContext(Dispatchers.IO) {
            if (!isNativeLoaded) {
                return@withContext Result.failure(IllegalStateException("Native library libpagina_core.so not loaded"))
            }
            decodeResponse(nativeCheckHealth(timeoutMs))
        }

    suspend fun quickPing(timeoutMs: Long = 4000L): Result<EdgeHealthReport> =
        withContext(Dispatchers.IO) {
            if (!isNativeLoaded) {
                return@withContext Result.failure(IllegalStateException("Native library libpagina_core.so not loaded"))
            }
            decodeResponse(nativeQuickPing(timeoutMs))
        }

    fun getEnglishVoicePresets(): Result<List<VoicePreset>> {
        if (!isNativeLoaded) return Result.success(fallbackPresets)
        return decodeResponse(nativeGetEnglishVoicePresets())
    }

    suspend fun fetchEnglishVoices(): Result<List<VoicePreset>> =
        withContext(Dispatchers.IO) {
            if (!isNativeLoaded) return@withContext Result.success(fallbackPresets)
            decodeResponse<List<EdgeVoiceDto>>(nativeFetchEnglishVoices()).map { dtoList ->
                dtoList.map { it.toVoicePreset() }
            }
        }

    suspend fun loadEpub(epubPath: String): Result<List<ChapterData>> =
        withContext(Dispatchers.IO) {
            if (!isNativeLoaded) {
                return@withContext Result.failure(IllegalStateException("Native library libpagina_core.so not loaded"))
            }
            decodeResponse(nativeLoadEpub(epubPath))
        }

    suspend fun loadEpubBook(epubPath: String, coverOutputDir: String = ""): Result<EpubBookData> =
        withContext(Dispatchers.IO) {
            if (!isNativeLoaded) {
                return@withContext Result.failure(IllegalStateException("Native library libpagina_core.so not loaded"))
            }
            decodeResponse(nativeLoadEpubBook(epubPath, coverOutputDir))
        }

    suspend fun fetchBookMetadataOnline(
        title: String,
        author: String = "",
        isbn: String = "",
        coverOutputDir: String = ""
    ): Result<OnlineBookMetadata> = withContext(Dispatchers.IO) {
        if (isNativeLoaded) {
            return@withContext decodeResponse(
                nativeFetchBookMetadataOnline(title, author, isbn, coverOutputDir)
            )
        }
        fetchBookMetadataFallbackKotlin(title, author, isbn, coverOutputDir)
    }

    private fun fetchBookMetadataFallbackKotlin(
        title: String,
        author: String,
        isbn: String,
        coverOutputDir: String
    ): Result<OnlineBookMetadata> = runCatching {
        val rawQuery = when {
            isbn.isNotBlank() -> "isbn:${isbn.trim()}"
            author.isNotBlank() && !author.equals("Unknown Author", ignoreCase = true) ->
                "intitle:${title.trim()} inauthor:${author.trim()}"
            else -> "intitle:${title.trim()}"
        }
        val encoded = java.net.URLEncoder.encode(rawQuery, "UTF-8")
        val url = java.net.URL("https://www.googleapis.com/books/v1/volumes?q=$encoded&maxResults=2&printType=books")
        val conn = (url.openConnection() as java.net.HttpURLConnection).apply {
            connectTimeout = 7000
            readTimeout = 7000
            requestMethod = "GET"
        }
        val body = conn.inputStream.bufferedReader().use { it.readText() }
        val root = kotlinx.serialization.json.Json.parseToJsonElement(body).let {
            it as? kotlinx.serialization.json.JsonObject
        } ?: throw IllegalStateException("Empty Google Books response")
        val items = root["items"] as? kotlinx.serialization.json.JsonArray
            ?: throw IllegalStateException("No online metadata found for '$title'")
        val firstObj = items.firstOrNull() as? kotlinx.serialization.json.JsonObject
            ?: throw IllegalStateException("No book volume found")
        val vol = firstObj["volumeInfo"] as? kotlinx.serialization.json.JsonObject
            ?: throw IllegalStateException("Missing volumeInfo")

        fun kotlinx.serialization.json.JsonObject.str(key: String): String? =
            (this[key] as? kotlinx.serialization.json.JsonPrimitive)?.content

        val resolvedTitle = vol.str("title")
        val resolvedAuthor = (vol["authors"] as? kotlinx.serialization.json.JsonArray)
            ?.mapNotNull { (it as? kotlinx.serialization.json.JsonPrimitive)?.content }
            ?.joinToString(", ")
        val resolvedDesc = vol.str("description")
            ?.replace(Regex("<[^>]*>"), "")
            ?.trim()
        val publisher = vol.str("publisher")
        val publishedDate = vol.str("publishedDate")
        val pageCount = vol.str("pageCount")?.toIntOrNull()
        val categories = (vol["categories"] as? kotlinx.serialization.json.JsonArray)
            ?.mapNotNull { (it as? kotlinx.serialization.json.JsonPrimitive)?.content }
            ?: emptyList()

        val imageLinks = vol["imageLinks"] as? kotlinx.serialization.json.JsonObject
        val rawCover = imageLinks?.str("extraLarge")
            ?: imageLinks?.str("large")
            ?: imageLinks?.str("medium")
            ?: imageLinks?.str("thumbnail")
            ?: imageLinks?.str("smallThumbnail")
        val coverUrl = rawCover
            ?.replace("http://", "https://")
            ?.replace("&edge=curl", "")
            ?.replace("zoom=1", "zoom=2")

        var savedCoverPath: String? = null
        if (coverUrl != null && coverOutputDir.isNotBlank()) {
            runCatching {
                val outDir = java.io.File(coverOutputDir).apply { mkdirs() }
                val file = java.io.File(outDir, "cover_online_${(title + author).hashCode().toUInt()}.jpg")
                java.net.URL(coverUrl).openStream().use { input ->
                    val bytes = input.readBytes()
                    if (bytes.size >= 512) {
                        file.writeBytes(bytes)
                        savedCoverPath = file.absolutePath
                    }
                }
            }
        }

        OnlineBookMetadata(
            title = resolvedTitle,
            author = resolvedAuthor,
            description = resolvedDesc,
            publisher = publisher,
            publishedDate = publishedDate,
            isbn = isbn.takeIf { it.isNotBlank() },
            categories = categories,
            pageCount = pageCount,
            coverUrl = coverUrl,
            coverImagePath = savedCoverPath,
            sourceProvider = "Google Books"
        )
    }

    suspend fun getCacheStats(): Result<TtsCacheStats> = withContext(Dispatchers.IO) {
        if (!isNativeLoaded) return@withContext Result.success(TtsCacheStats())
        decodeResponse(nativeGetCacheStats())
    }

    suspend fun clearCache(): Result<Boolean> = withContext(Dispatchers.IO) {
        if (!isNativeLoaded) return@withContext Result.success(true)
        decodeResponse(nativeClearCache())
    }

    suspend fun pruneCache(maxBytes: Long = 150L * 1024L * 1024L): Result<Int> =
        withContext(Dispatchers.IO) {
            if (!isNativeLoaded) return@withContext Result.success(0)
            decodeResponse(nativePruneCache(maxBytes))
        }

    suspend fun chunkParagraph(
        paragraph: ParagraphData,
        sentencesPerChunk: Int = 4
    ): Result<List<SentenceChunk>> = withContext(Dispatchers.IO) {
        val paraJson = json.encodeToString(paragraph)
        decodeResponse(nativeChunkParagraph(paraJson, sentencesPerChunk))
    }

    suspend fun synthesizeWithPrebuffer(
        paragraph: ParagraphData,
        chunks: List<SentenceChunk>,
        activeIdx: Int,
        nextParagraph: ParagraphData? = null
    ): Result<SynthesizedChunkDto> = withContext(Dispatchers.IO) {
        val paraJson = json.encodeToString(paragraph)
        val chunksJson = json.encodeToString(chunks)
        if (nextParagraph != null) {
            val nextParaJson = json.encodeToString(nextParagraph)
            decodeResponse(
                nativeSynthesizeWithCrossParagraphPrebuffer(
                    paraJson,
                    chunksJson,
                    activeIdx,
                    nextParaJson
                )
            )
        } else {
            decodeResponse(nativeSynthesizeWithPrebuffer(paraJson, chunksJson, activeIdx))
        }
    }

    suspend fun clickToReadResolve(
        paragraph: ParagraphData,
        charOffset: Int,
        sentencesPerChunk: Int = 4
    ): Result<ClickToReadDto> = withContext(Dispatchers.IO) {
        val paraJson = json.encodeToString(paragraph)
        decodeResponse(nativeClickToReadResolve(paraJson, charOffset, sentencesPerChunk))
    }

    fun activeWordAtMs(
        timings: List<WordAudioTiming>,
        positionMs: Long
    ): WordAudioTiming? {
        if (timings.isEmpty()) return null
        var low = 0
        var high = timings.lastIndex
        while (low <= high) {
            val mid = (low + high) ushr 1
            val item = timings[mid]
            when {
                positionMs < item.audioOffsetMs -> high = mid - 1
                positionMs > item.audioEndMs -> low = mid + 1
                else -> return item
            }
        }
        return if (low > 0) timings[low - 1] else timings.first()
    }
}
