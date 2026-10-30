package com.pagina.reader.data

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import com.pagina.reader.ui.BookItem
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.io.File
import java.security.MessageDigest

@Serializable
data class PersistedAppState(
    val fontSizeSp: Float = 18f,
    val selectedVoice: String = "it-IT-IsabellaNeural",
    val selectedRate: String = "+0%",
    val selectedPitch: String = "+0Hz",
    val books: List<BookItem> = emptyList()
)

class PaginaStorageRepository(private val context: Context) {

    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
        prettyPrint = true
    }

    private val stateFile: File
        get() = File(context.filesDir, "pagina_reader_state.json")

    val booksDir: File
        get() = File(context.filesDir, "books").apply { mkdirs() }

    val coversDir: File
        get() = File(context.filesDir, "covers").apply { mkdirs() }

    suspend fun loadState(): PersistedAppState? = withContext(Dispatchers.IO) {
        runCatching {
            if (!stateFile.exists()) return@withContext null
            val content = stateFile.readText()
            json.decodeFromString<PersistedAppState>(content)
        }.getOrNull()
    }

    suspend fun saveState(state: PersistedAppState) = withContext(Dispatchers.IO) {
        runCatching {
            val tmpFile = File(context.filesDir, "pagina_reader_state.json.tmp")
            tmpFile.writeText(json.encodeToString(state))
            if (!tmpFile.renameTo(stateFile)) {
                stateFile.writeText(json.encodeToString(state))
                tmpFile.delete()
            }
        }
    }

    suspend fun copyEpubUriToInternalStorage(uri: Uri): Result<File> = withContext(Dispatchers.IO) {
        runCatching {
            val displayName = resolveDisplayName(uri) ?: "imported_${System.currentTimeMillis()}.epub"
            val sanitizedName = displayName.replace(Regex("[^a-zA-Z0-9._-]"), "_")
            val finalFileName = if (sanitizedName.lowercase().endsWith(".epub")) {
                sanitizedName
            } else {
                "$sanitizedName.epub"
            }

            val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
                ?: throw IllegalStateException("Cannot open input stream for URI: $uri")

            if (bytes.isEmpty()) {
                throw IllegalStateException("Selected EPUB file is empty")
            }

            val hashPrefix = sha256ShortHex(bytes)
            val targetFile = File(booksDir, "${hashPrefix}_$finalFileName")
            targetFile.writeBytes(bytes)
            targetFile
        }
    }

    private fun resolveDisplayName(uri: Uri): String? {
        return runCatching {
            context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
                ?.use { cursor ->
                    if (cursor.moveToFirst()) {
                        val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                        if (idx >= 0) cursor.getString(idx) else null
                    } else null
                }
        }.getOrNull() ?: uri.lastPathSegment
    }

    private fun sha256ShortHex(bytes: ByteArray): String {
        val digest = MessageDigest.getInstance("SHA-256").digest(bytes)
        return digest.take(6).joinToString("") { "%02x".format(it) }
    }
}
