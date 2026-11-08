package com.nuvola.reader.ui

import android.app.Application
import android.net.Uri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.nuvola.reader.bridge.ChapterData
import com.nuvola.reader.bridge.EdgeHealthReport
import com.nuvola.reader.bridge.NuvolaNativeBridge
import com.nuvola.reader.bridge.ParagraphData
import com.nuvola.reader.bridge.ParagraphKind
import com.nuvola.reader.bridge.SentenceChunk
import com.nuvola.reader.bridge.ServiceStatus
import com.nuvola.reader.bridge.TtsCacheStats
import com.nuvola.reader.bridge.VoicePreset
import com.nuvola.reader.bridge.WordAudioTiming
import com.nuvola.reader.data.NuvolaStorageRepository
import com.nuvola.reader.data.PersistedAppState
import com.nuvola.reader.player.EdgeExoPlayerController
import com.nuvola.reader.ui.theme.ReaderTypography
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.serialization.Serializable
import java.io.File

@Serializable
data class BookItem(
    val id: String,
    val title: String,
    val author: String,
    val epubPath: String,
    val coverImagePath: String? = null,
    val description: String? = null,
    val publisher: String? = null,
    val publishedDate: String? = null,
    val isbn: String? = null,
    val categories: List<String> = emptyList(),
    val pageCount: Int? = null,
    val metadataSource: String? = null,
    val language: String = "en",
    val progress: Float = 0f,
    val lastChapterIndex: Int = 0,
    val lastParagraphIndex: Int = 0,
    val lastChunkIndex: Int = 0,
    val lastWordId: Int? = null,
    val lastScrollOffset: Int = 0
)

data class ReaderUiState(
    val typography: ReaderTypography = ReaderTypography(),

    val healthReport: EdgeHealthReport? = null,
    val isCheckingHealth: Boolean = false,
    val cacheStats: TtsCacheStats = TtsCacheStats(),

    val voicePresets: List<VoicePreset> = emptyList(),
    val selectedVoice: String = "it-IT-IsabellaNeural",
    val selectedRate: String = "+0%",
    val selectedPitch: String = "+0Hz",

    val books: List<BookItem> = emptyList(),
    val fetchingMetadataBookIds: Set<String> = emptySet(),
    val searchQuery: String = "",
    val activeBook: BookItem? = null,
    val paragraphs: List<ParagraphData> = emptyList(),
    val currentParagraphIndex: Int = 0,

    val currentParagraphChunks: List<SentenceChunk> = emptyList(),
    val currentChunkIndex: Int = 0,
    val isSynthesizing: Boolean = false,
    val isPlaying: Boolean = false,
    val activeWordTiming: WordAudioTiming? = null,
    val showReaderChrome: Boolean = true,
    val errorMessage: String? = null
) {
    val currentParagraph: ParagraphData?
        get() = paragraphs.getOrNull(currentParagraphIndex)

    val nextParagraphInBook: ParagraphData?
        get() = paragraphs.drop(currentParagraphIndex + 1).firstOrNull {
            it.kind != ParagraphKind.Footnote
        }

    val bookProgress: Float
        get() {
            val total = paragraphs.size
            if (total <= 1) return 0f
            return (currentParagraphIndex.toFloat() / (total - 1).toFloat()).coerceIn(0f, 1f)
        }
}

class ReaderViewModel(application: Application) : AndroidViewModel(application) {

    private val _uiState = MutableStateFlow(ReaderUiState())
    val uiState: StateFlow<ReaderUiState> = _uiState.asStateFlow()

    private val storageRepository = NuvolaStorageRepository(application.applicationContext)

    private val cacheDirPath: String by lazy {
        File(application.cacheDir, "nuvola_tts_cache").apply { mkdirs() }.absolutePath
    }

    private var activeSynthesisJob: Job? = null

    private fun cancelActiveSynthesis() {
        activeSynthesisJob?.cancel()
        activeSynthesisJob = null
    }

    private val playerController = EdgeExoPlayerController(
        context = application.applicationContext,
        scope = viewModelScope,
        onActiveWordChanged = { timing ->
            _uiState.update { it.copy(activeWordTiming = timing) }
        },
        onPlayingStateChanged = { playing ->
            val wasPlaying = _uiState.value.isPlaying
            _uiState.update { it.copy(isPlaying = playing) }
            if (wasPlaying && !playing) {
                updateActiveBookProgress()
            }
        },
        onChunkPlaybackCompleted = {
            advanceToNextChunkOrParagraph()
        },
        onSkipNext = {
            skipToNextChunkOrParagraph()
        },
        onSkipPrevious = {
            skipToPreviousChunkOrParagraph()
        },
        onPlayRequestedFromSession = {
            val s = _uiState.value
            if (!s.isPlaying && !s.isSynthesizing) {
                startOrResumeParagraphPlayback(
                    paragraphIndex = s.currentParagraphIndex,
                    chunkIndex = s.currentChunkIndex,
                    seekWordId = s.activeWordTiming?.wordId ?: s.activeBook?.lastWordId
                )
            }
        },
        onPauseRequestedFromSession = {
            cancelActiveSynthesis()
            _uiState.update { it.copy(isSynthesizing = false, isPlaying = false) }
            updateActiveBookProgress()
        }
    )

    init {
        initializeEngineAndPresets()
    }

    private fun curatedVoicePresets(): List<VoicePreset> = listOf(
        VoicePreset("en-US-AvaMultilingualNeural", "Ava (US)", "en-US", "Female", "Expressive, warm American storytelling voice"),
        VoicePreset("en-US-AndrewMultilingualNeural", "Andrew (US)", "en-US", "Male", "Warm, confident American narrator voice"),
        VoicePreset("en-GB-SoniaNeural", "Sonia (UK)", "en-GB", "Female", "Clear, classic British female voice"),
        VoicePreset("en-GB-RyanNeural", "Ryan (UK)", "en-GB", "Male", "Natural British male narrator voice"),
        VoicePreset("it-IT-IsabellaNeural", "Isabella", "it-IT", "Female", "Voce femminile calda, espressiva e naturale per romanzi"),
        VoicePreset("it-IT-DiegoNeural", "Diego", "it-IT", "Male", "Voce maschile profonda e chiara per narrativa e saggistica"),
        VoicePreset("it-IT-ElsaNeural", "Elsa", "it-IT", "Female", "Voce femminile nitida, scorrevole e articolata"),
        VoicePreset("it-IT-GiuseppeMultilingualNeural", "Giuseppe", "it-IT", "Male", "Voce maschile naturale multilingua (Italiano / Inglese)")
    )

    private fun sanitizeBookDescription(raw: String?): String? {
        if (raw.isNullOrBlank()) return null
        val decodedEntities = raw
            .replace(Regex("&#x([0-9a-fA-F]+);")) { m ->
                val code = m.groupValues[1].toIntOrNull(16) ?: 32
                if (code == 0xA0) " " else code.toChar().toString()
            }
            .replace(Regex("&#([0-9]+);")) { m ->
                val code = m.groupValues[1].toIntOrNull() ?: 32
                if (code == 160) " " else code.toChar().toString()
            }
            .replace("&nbsp;", " ")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace('\u00A0', ' ')
            .replace(Regex("([a-zàèéìòù])([.!?])([A-ZÀÈÉÌÒÙ])"), "$1$2 $3")

        return decodedEntities
            .lines()
            .joinToString("\n") { line -> line.trim().replace(Regex("\\s+"), " ") }
            .replace(Regex("\n{3,}"), "\n\n")
            .trim()
            .takeIf { it.isNotEmpty() }
    }

    private fun initializeEngineAndPresets() {
        val presets = curatedVoicePresets()
        val presetIds = presets.map { it.shortName }.toSet()
        val fallbackVoice = presets.first().shortName

        viewModelScope.launch {
            val persisted = storageRepository.loadState()
            val resolvedFontSize = persisted?.fontSizeSp?.coerceIn(13f, 30f) ?: 18f
            val resolvedVoice = persisted?.selectedVoice?.takeIf { it in presetIds } ?: fallbackVoice
            val resolvedRate = persisted?.selectedRate?.takeIf { it.isNotBlank() } ?: "+0%"
            val resolvedPitch = persisted?.selectedPitch?.takeIf { it.isNotBlank() } ?: "+0Hz"
            val resolvedBooks = persisted?.books.orEmpty().map { book ->
                book.copy(description = sanitizeBookDescription(book.description))
            }

            _uiState.update {
                it.copy(
                    typography = it.typography.copy(fontSizeSp = resolvedFontSize),
                    voicePresets = presets,
                    selectedVoice = resolvedVoice,
                    selectedRate = resolvedRate,
                    selectedPitch = resolvedPitch,
                    books = resolvedBooks
                )
            }

            playerController.setPlaybackSpeed(rateToSpeedFactor(resolvedRate))

            NuvolaNativeBridge.initSession(
                cacheDir = cacheDirPath,
                voice = resolvedVoice,
                rate = "+0%|pitch=$resolvedPitch"
            )
            NuvolaNativeBridge.pruneCache(maxBytes = 150L * 1024L * 1024L)
            refreshCacheStats()
            refreshEdgeHealth(fullCheck = true)
            persistCurrentState()
            enrichMissingBookMetadata()
        }
    }

    private fun rateToSpeedFactor(rate: String): Float {
        val cleaned = rate.trim().removeSuffix("%").removePrefix("+")
        val pct = cleaned.toFloatOrNull() ?: 0f
        return (1.0f + (pct / 100f)).coerceIn(0.5f, 2.5f)
    }

    private fun persistCurrentState() {
        val s = _uiState.value
        viewModelScope.launch {
            storageRepository.saveState(
                PersistedAppState(
                    fontSizeSp = s.typography.fontSizeSp,
                    selectedVoice = s.selectedVoice,
                    selectedRate = s.selectedRate,
                    selectedPitch = s.selectedPitch,
                    books = s.books
                )
            )
        }
    }

    fun refreshCacheStats() {
        viewModelScope.launch {
            NuvolaNativeBridge.getCacheStats().onSuccess { stats ->
                _uiState.update { it.copy(cacheStats = stats) }
            }
        }
    }

    fun clearTtsCache() {
        viewModelScope.launch {
            NuvolaNativeBridge.clearCache().onSuccess {
                refreshCacheStats()
            }
        }
    }

    fun dismissError() {
        _uiState.update { it.copy(errorMessage = null) }
    }

    fun refreshEdgeHealth(fullCheck: Boolean = false) {
        viewModelScope.launch {
            _uiState.update { it.copy(isCheckingHealth = true) }
            val result = if (fullCheck) {
                NuvolaNativeBridge.checkHealth(5000L)
            } else {
                NuvolaNativeBridge.quickPing(4000L)
            }
            result.onSuccess { report ->
                _uiState.update { it.copy(healthReport = report, isCheckingHealth = false) }
            }.onFailure { err ->
                _uiState.update {
                    it.copy(
                        isCheckingHealth = false,
                        healthReport = EdgeHealthReport(
                            status = ServiceStatus.Offline,
                            isOperational = false,
                            voicesApiReachable = false,
                            websocketHandshakeOk = false,
                            message = "Edge-TTS unreachable: ${err.message}"
                        )
                    )
                }
            }
        }
    }

    fun setSearchQuery(query: String) {
        _uiState.update { it.copy(searchQuery = query) }
    }

    fun toggleReaderChrome() {
        _uiState.update { it.copy(showReaderChrome = !it.showReaderChrome) }
    }

    fun updateFontSize(deltaSp: Float) {
        _uiState.update {
            val nextSize = (it.typography.fontSizeSp + deltaSp).coerceIn(13f, 30f)
            it.copy(typography = it.typography.copy(fontSizeSp = nextSize))
        }
        persistCurrentState()
    }

    fun selectVoice(voiceShortName: String) {
        val stateBefore = _uiState.value
        if (stateBefore.selectedVoice == voiceShortName) return
        val wasPlaying = stateBefore.isPlaying || stateBefore.isSynthesizing
        val savedParagraphIdx = stateBefore.currentParagraphIndex
        val savedChunkIdx = stateBefore.currentChunkIndex
        val savedWordTiming = stateBefore.activeWordTiming
        val currentPitch = stateBefore.selectedPitch

        cancelActiveSynthesis()
        viewModelScope.launch {
            playerController.pauseForTransition(clearActiveWord = false)
            _uiState.update {
                it.copy(
                    selectedVoice = voiceShortName,
                    activeWordTiming = savedWordTiming,
                    isSynthesizing = false
                )
            }
            NuvolaNativeBridge.initSession(
                cacheDir = cacheDirPath,
                voice = voiceShortName,
                rate = "+0%|pitch=$currentPitch"
            )
            persistCurrentState()
            if (wasPlaying) {
                startOrResumeParagraphPlayback(
                    paragraphIndex = savedParagraphIdx,
                    chunkIndex = savedChunkIdx,
                    seekWordId = savedWordTiming?.wordId
                )
            }
        }
    }

    fun selectPitch(pitch: String) {
        val stateBefore = _uiState.value
        if (stateBefore.selectedPitch == pitch) return
        val wasPlaying = stateBefore.isPlaying || stateBefore.isSynthesizing
        val savedParagraphIdx = stateBefore.currentParagraphIndex
        val savedChunkIdx = stateBefore.currentChunkIndex
        val savedWordTiming = stateBefore.activeWordTiming
        val currentVoice = stateBefore.selectedVoice

        cancelActiveSynthesis()
        viewModelScope.launch {
            playerController.pauseForTransition(clearActiveWord = false)
            _uiState.update {
                it.copy(
                    selectedPitch = pitch,
                    activeWordTiming = savedWordTiming,
                    isSynthesizing = false
                )
            }
            NuvolaNativeBridge.initSession(
                cacheDir = cacheDirPath,
                voice = currentVoice,
                rate = "+0%|pitch=$pitch"
            )
            persistCurrentState()
            if (wasPlaying) {
                startOrResumeParagraphPlayback(
                    paragraphIndex = savedParagraphIdx,
                    chunkIndex = savedChunkIdx,
                    seekWordId = savedWordTiming?.wordId
                )
            }
        }
    }

    fun selectRate(rate: String) {
        _uiState.update { it.copy(selectedRate = rate) }
        playerController.setPlaybackSpeed(rateToSpeedFactor(rate))
        persistCurrentState()
    }

    fun importEpubFromUri(uri: Uri) {
        viewModelScope.launch {
            storageRepository.copyEpubUriToInternalStorage(uri)
                .onSuccess { copiedFile ->
                    importAndOpenEpub(
                        epubPath = copiedFile.absolutePath,
                        fallbackTitle = copiedFile.nameWithoutExtension,
                        author = "Unknown Author"
                    )
                }
                .onFailure { err ->
                    _uiState.update {
                        it.copy(errorMessage = "Could not import EPUB URI: ${err.message}")
                    }
                }
        }
    }

    private fun flattenBookParagraphs(chapters: List<ChapterData>): List<ParagraphData> {
        var globalParaId = 0
        return chapters.flatMap { chapter ->
            chapter.paragraphs.map { para ->
                para.copy(id = globalParaId++)
            }
        }
    }

    private val bookParagraphsCache = mutableMapOf<String, List<ParagraphData>>()

    fun importAndOpenEpub(
        epubPath: String,
        fallbackTitle: String = "Imported Book",
        author: String = "Unknown Author"
    ) {
        viewModelScope.launch {
            NuvolaNativeBridge.loadEpubBook(
                epubPath = epubPath,
                coverOutputDir = storageRepository.coversDir.absolutePath
            ).onSuccess { epubBook ->
                val resolvedTitle = epubBook.title.takeIf { it.isNotBlank() } ?: fallbackTitle
                val resolvedAuthor = epubBook.author.takeIf { it.isNotBlank() && it != "Unknown Author" } ?: author
                val allParagraphs = flattenBookParagraphs(epubBook.chapters)
                bookParagraphsCache[epubPath] = allParagraphs
                val newBook = BookItem(
                    id = epubPath.hashCode().toString(),
                    title = resolvedTitle,
                    author = resolvedAuthor,
                    epubPath = epubPath,
                    coverImagePath = epubBook.coverImagePath,
                    description = sanitizeBookDescription(epubBook.description),
                    publisher = epubBook.publisher,
                    publishedDate = epubBook.publishedDate,
                    isbn = epubBook.isbn,
                    metadataSource = "EPUB OPF",
                    language = epubBook.language
                )
                _uiState.update { state ->
                    val updatedBooks = (listOf(newBook) + state.books.filterNot { it.id == newBook.id })
                    state.copy(
                        books = updatedBooks,
                        errorMessage = null
                    )
                }
                persistCurrentState()

                if (newBook.coverImagePath.isNullOrBlank() || newBook.description.isNullOrBlank()) {
                    fetchOnlineMetadataForBook(newBook)
                }
            }.onFailure { err ->
                _uiState.update { it.copy(errorMessage = "Could not open EPUB: ${err.message}") }
            }
        }
    }

    fun enrichMissingBookMetadata() {
        val currentBooks = _uiState.value.books
        currentBooks.filter { book ->
            val coverMissing = book.coverImagePath.isNullOrBlank() || !File(book.coverImagePath).exists()
            coverMissing || book.description.isNullOrBlank()
        }.forEach { book ->
            fetchOnlineMetadataForBook(book)
        }
    }

    fun fetchOnlineMetadataForBook(book: BookItem, overwriteExistingCover: Boolean = false) {
        if (_uiState.value.fetchingMetadataBookIds.contains(book.id)) return
        _uiState.update { it.copy(fetchingMetadataBookIds = it.fetchingMetadataBookIds + book.id) }

        viewModelScope.launch {
            NuvolaNativeBridge.fetchBookMetadataOnline(
                title = book.title,
                author = book.author,
                isbn = book.isbn.orEmpty(),
                coverOutputDir = storageRepository.coversDir.absolutePath
            ).onSuccess { online ->
                _uiState.update { state ->
                    val updatedBooks = state.books.map { existing ->
                        if (existing.id != book.id) return@map existing

                        val existingCoverValid = !existing.coverImagePath.isNullOrBlank() &&
                            File(existing.coverImagePath).exists()
                        val resolvedCover = when {
                            overwriteExistingCover && !online.coverImagePath.isNullOrBlank() -> online.coverImagePath
                            existingCoverValid -> existing.coverImagePath
                            !online.coverImagePath.isNullOrBlank() -> online.coverImagePath
                            else -> existing.coverImagePath
                        }

                        existing.copy(
                            author = if (existing.author == "Unknown Author" && !online.author.isNullOrBlank()) {
                                online.author
                            } else {
                                existing.author
                            },
                            coverImagePath = resolvedCover,
                            description = sanitizeBookDescription(online.description)
                                ?: sanitizeBookDescription(existing.description),
                            publisher = online.publisher?.takeIf { it.isNotBlank() } ?: existing.publisher,
                            publishedDate = online.publishedDate?.takeIf { it.isNotBlank() } ?: existing.publishedDate,
                            isbn = online.isbn?.takeIf { it.isNotBlank() } ?: existing.isbn,
                            categories = if (online.categories.isNotEmpty()) online.categories else existing.categories,
                            pageCount = online.pageCount ?: existing.pageCount,
                            metadataSource = online.sourceProvider,
                            language = online.language?.takeIf { it.isNotBlank() } ?: existing.language
                        )
                    }
                    val updatedActive = state.activeBook?.let { active ->
                        updatedBooks.find { it.id == active.id } ?: active
                    }
                    if (updatedActive != null && updatedActive.id == book.id) {
                        playerController.updateBookMetadata(
                            title = updatedActive.title,
                            author = updatedActive.author,
                            coverImagePath = updatedActive.coverImagePath
                        )
                    }
                    state.copy(
                        books = updatedBooks,
                        activeBook = updatedActive,
                        fetchingMetadataBookIds = state.fetchingMetadataBookIds - book.id
                    )
                }
                persistCurrentState()
            }.onFailure {
                _uiState.update { state ->
                    state.copy(fetchingMetadataBookIds = state.fetchingMetadataBookIds - book.id)
                }
            }
        }
    }

    fun removeBook(bookId: String) {
        _uiState.update { state ->
            val removed = state.books.find { it.id == bookId }
            if (removed != null) {
                bookParagraphsCache.remove(removed.epubPath)
            }
            state.copy(books = state.books.filterNot { it.id == bookId })
        }
        persistCurrentState()
    }

    private fun tokenizeParagraphWordsFallback(text: String): List<com.nuvola.reader.bridge.WordSpan> {
        val result = mutableListOf<com.nuvola.reader.bridge.WordSpan>()
        val tokenRegex = Regex("[\\p{L}\\p{N}]+(?:['’\\-][\\p{L}\\p{N}]+)*")
        var nextId = 0
        for (match in tokenRegex.findAll(text)) {
            result.add(
                com.nuvola.reader.bridge.WordSpan(
                    id = nextId++,
                    charStart = match.range.first,
                    charEnd = match.range.last + 1,
                    text = match.value
                )
            )
        }
        return result
    }

    private fun buildBookmarkWordTiming(
        paragraph: ParagraphData?,
        wordId: Int?,
        fallbackToFirstWord: Boolean = false
    ): WordAudioTiming? {
        if (paragraph == null) return null
        val words = paragraph.words.ifEmpty { tokenizeParagraphWordsFallback(paragraph.text) }
        if (words.isEmpty()) return null
        val span = when {
            wordId != null -> words.find { it.id == wordId } ?: words.firstOrNull { it.id >= wordId } ?: words.last()
            fallbackToFirstWord -> words.first()
            else -> return null
        }
        return WordAudioTiming(
            wordId = span.id,
            paragraphId = paragraph.id,
            text = span.text,
            charStart = span.charStart,
            charEnd = span.charEnd,
            audioOffsetMs = 0L,
            audioDurationMs = 0L,
            audioEndMs = 0L
        )
    }

    private fun activateBookWithBookmark(book: BookItem, allParagraphs: List<ParagraphData>) {

        val latestBook = _uiState.value.books.find { it.id == book.id } ?: book
        val maxPara = (allParagraphs.size - 1).coerceAtLeast(0)
        val pIdx = latestBook.lastParagraphIndex.coerceIn(0, maxPara)
        val activePara = allParagraphs.getOrNull(pIdx)
        val restoredWordTiming = buildBookmarkWordTiming(
            paragraph = activePara,
            wordId = latestBook.lastWordId,
            fallbackToFirstWord = pIdx > 0 || latestBook.lastWordId != null
        )

        playerController.updateBookMetadata(
            title = latestBook.title,
            author = latestBook.author,
            coverImagePath = latestBook.coverImagePath
        )

        _uiState.update {
            it.copy(
                activeBook = latestBook,
                paragraphs = allParagraphs,
                currentParagraphIndex = pIdx,
                currentParagraphChunks = emptyList(),
                currentChunkIndex = latestBook.lastChunkIndex.coerceAtLeast(0),
                activeWordTiming = restoredWordTiming,
                errorMessage = null
            )
        }
    }

    fun openBook(book: BookItem) {
        val cachedParagraphs = bookParagraphsCache[book.epubPath]
        if (cachedParagraphs != null) {
            activateBookWithBookmark(book, cachedParagraphs)
            return
        }
        viewModelScope.launch {
            NuvolaNativeBridge.loadEpubBook(
                epubPath = book.epubPath,
                coverOutputDir = storageRepository.coversDir.absolutePath
            ).onSuccess { epubBook ->
                val allParagraphs = flattenBookParagraphs(epubBook.chapters)
                bookParagraphsCache[book.epubPath] = allParagraphs
                activateBookWithBookmark(book, allParagraphs)
            }.onFailure { err ->
                _uiState.update { it.copy(errorMessage = "Could not open book: ${err.message}") }
            }
        }
    }

    fun onScrollPositionChanged(firstVisibleParagraphIndex: Int, scrollOffsetPx: Int) {
        val state = _uiState.value
        val active = state.activeBook ?: return
        if (state.paragraphs.isEmpty()) return

        val clampedIdx = firstVisibleParagraphIndex.coerceIn(0, state.paragraphs.lastIndex)
        val total = state.paragraphs.size
        val newParagraphIdx = if (state.isPlaying || state.isSynthesizing) {
            state.currentParagraphIndex
        } else {
            clampedIdx
        }
        val progress = if (total <= 1) {
            0f
        } else {
            (newParagraphIdx.toFloat() / (total - 1).toFloat()).coerceIn(0f, 1f)
        }

        val paragraphChangedWhileIdle = !state.isPlaying &&
            !state.isSynthesizing &&
            clampedIdx != state.currentParagraphIndex

        val updatedBook = active.copy(
            progress = progress,
            lastParagraphIndex = newParagraphIdx,
            lastChunkIndex = if (paragraphChangedWhileIdle) 0 else state.currentChunkIndex,
            lastWordId = if (paragraphChangedWhileIdle) null else (state.activeWordTiming?.wordId ?: active.lastWordId),
            lastScrollOffset = if (newParagraphIdx == clampedIdx) scrollOffsetPx.coerceAtLeast(0) else 0
        )

        _uiState.update { s ->
            s.copy(
                currentParagraphIndex = newParagraphIdx,
                currentChunkIndex = if (paragraphChangedWhileIdle) 0 else s.currentChunkIndex,
                activeWordTiming = if (paragraphChangedWhileIdle) null else s.activeWordTiming,
                activeBook = updatedBook,
                books = s.books.map { if (it.id == updatedBook.id) updatedBook else it }
            )
        }
        persistCurrentState()
    }

    fun onJumpToHeading(headingParagraphIndex: Int) {
        val state = _uiState.value
        val active = state.activeBook ?: return
        if (state.paragraphs.isEmpty()) return

        val targetIdx = headingParagraphIndex.coerceIn(0, state.paragraphs.lastIndex)
        val wasPlaying = state.isPlaying || state.isSynthesizing
        val total = state.paragraphs.size
        val progress = if (total <= 1) {
            0f
        } else {
            (targetIdx.toFloat() / (total - 1).toFloat()).coerceIn(0f, 1f)
        }

        val updatedBook = active.copy(
            progress = progress,
            lastParagraphIndex = targetIdx,
            lastChunkIndex = 0,
            lastWordId = null,
            lastScrollOffset = 0
        )

        _uiState.update { s ->
            s.copy(
                currentParagraphIndex = targetIdx,
                currentChunkIndex = 0,
                currentParagraphChunks = emptyList(),
                activeWordTiming = null,
                activeBook = updatedBook,
                books = s.books.map { if (it.id == updatedBook.id) updatedBook else it }
            )
        }
        persistCurrentState()

        if (wasPlaying) {
            cancelActiveSynthesis()
            playerController.pauseForTransition(clearActiveWord = true)
            startOrResumeParagraphPlayback(
                paragraphIndex = targetIdx,
                chunkIndex = 0,
                seekWordId = null
            )
        }
    }

    fun closeReader() {
        cancelActiveSynthesis()
        updateActiveBookProgress()
        playerController.stop()
        viewModelScope.launch { NuvolaNativeBridge.cancelPendingQueue() }
        _uiState.update {
            it.copy(
                activeBook = null,
                activeWordTiming = null,
                isPlaying = false,
                isSynthesizing = false
            )
        }
        persistCurrentState()
    }

    fun togglePlayPause() {
        val state = _uiState.value
        if (state.isPlaying || state.isSynthesizing) {
            cancelActiveSynthesis()
            playerController.pause()
            _uiState.update { it.copy(isSynthesizing = false, isPlaying = false) }
            updateActiveBookProgress()
            return
        }
        if (playerController.canResume()) {
            playerController.resume()
            return
        }
        startOrResumeParagraphPlayback(
            paragraphIndex = state.currentParagraphIndex,
            chunkIndex = state.currentChunkIndex,
            seekWordId = state.activeWordTiming?.wordId ?: state.activeBook?.lastWordId
        )
    }

    fun onWordTapped(paragraphIndex: Int, charOffset: Int) {
        val paragraphs = _uiState.value.paragraphs
        val paragraph = paragraphs.getOrNull(paragraphIndex) ?: return
        val nextParagraph = paragraphs.getOrNull(paragraphIndex + 1)

        cancelActiveSynthesis()
        activeSynthesisJob = viewModelScope.launch {
            playerController.pauseForTransition(clearActiveWord = false)
            _uiState.update {
                it.copy(
                    currentParagraphIndex = paragraphIndex,
                    isSynthesizing = true,
                    errorMessage = null
                )
            }

            NuvolaNativeBridge.clickToReadResolve(paragraph, charOffset, sentencesPerChunk = 4)
                .onSuccess { clickDto ->
                    val initialTiming = buildBookmarkWordTiming(paragraph, clickDto.tappedWord.id)
                    _uiState.update {
                        it.copy(
                            currentParagraphChunks = clickDto.chunks,
                            currentChunkIndex = clickDto.targetChunkIndex,
                            activeWordTiming = initialTiming
                        )
                    }
                    updateActiveBookProgress(explicitScrollOffset = 0)
                    synthesizeAndPlayChunk(
                        paragraph = paragraph,
                        chunks = clickDto.chunks,
                        chunkIndex = clickDto.targetChunkIndex,
                        seekWordId = clickDto.tappedWord.id,
                        nextParagraph = nextParagraph
                    )
                }
                .onFailure { err ->
                    if (err is CancellationException) throw err
                    _uiState.update {
                        it.copy(isSynthesizing = false, errorMessage = err.message)
                    }
                }
        }
    }

    private fun startOrResumeParagraphPlayback(
        paragraphIndex: Int,
        chunkIndex: Int,
        seekWordId: Int?
    ) {
        val paragraphs = _uiState.value.paragraphs
        val paragraph = paragraphs.getOrNull(paragraphIndex) ?: return
        val nextParagraph = paragraphs.getOrNull(paragraphIndex + 1)

        cancelActiveSynthesis()
        activeSynthesisJob = viewModelScope.launch {
            _uiState.update {
                it.copy(
                    currentParagraphIndex = paragraphIndex,
                    isSynthesizing = true,
                    errorMessage = null
                )
            }

            val chunksResult = NuvolaNativeBridge.chunkParagraph(paragraph, sentencesPerChunk = 4)
            chunksResult.onSuccess { chunks ->
                if (chunks.isEmpty()) {

                    val nextNonEmptyIdx = (paragraphIndex + 1 until paragraphs.size)
                        .firstOrNull { idx -> paragraphs[idx].kind != ParagraphKind.Footnote && paragraphs[idx].text.any { it.isLetterOrDigit() } }
                    if (nextNonEmptyIdx != null) {
                        startOrResumeParagraphPlayback(
                            paragraphIndex = nextNonEmptyIdx,
                            chunkIndex = 0,
                            seekWordId = null
                        )
                    } else {
                        _uiState.update { it.copy(isSynthesizing = false, isPlaying = false) }
                    }
                    return@onSuccess
                }

                val wordChunkIdx = if (seekWordId != null) {
                    chunks.indexOfFirst { seekWordId in it.wordIds }.takeIf { it >= 0 }
                } else {
                    null
                }
                val validChunkIdx = (wordChunkIdx ?: chunkIndex).coerceIn(0, chunks.size - 1)
                _uiState.update {
                    it.copy(
                        currentParagraphChunks = chunks,
                        currentChunkIndex = validChunkIdx
                    )
                }
                synthesizeAndPlayChunk(
                    paragraph = paragraph,
                    chunks = chunks,
                    chunkIndex = validChunkIdx,
                    seekWordId = seekWordId,
                    nextParagraph = nextParagraph
                )
            }.onFailure { err ->
                if (err is CancellationException) throw err
                _uiState.update { it.copy(isSynthesizing = false, errorMessage = err.message) }
            }
        }
    }

    private suspend fun synthesizeAndPlayChunk(
        paragraph: ParagraphData,
        chunks: List<SentenceChunk>,
        chunkIndex: Int,
        seekWordId: Int?,
        nextParagraph: ParagraphData?
    ) {
        NuvolaNativeBridge.synthesizeWithPrebuffer(
            paragraph = paragraph,
            chunks = chunks,
            activeIdx = chunkIndex,
            nextParagraph = nextParagraph
        ).onSuccess { synthDto ->
            _uiState.update { it.copy(isSynthesizing = false) }
            playerController.playChunk(synthDto, initialSeekWordId = seekWordId)
            updateActiveBookProgress(explicitScrollOffset = 0)
            refreshCacheStats()
        }.onFailure { err ->
            if (err is CancellationException) throw err
            _uiState.update {
                it.copy(
                    isSynthesizing = false,
                    isPlaying = false,
                    errorMessage = "TTS Error: ${err.message}"
                )
            }
        }
    }

    private fun advanceToNextChunkOrParagraph() {
        val state = _uiState.value
        val chunks = state.currentParagraphChunks

        if (state.currentChunkIndex + 1 < chunks.size) {
            val nextChunkIdx = state.currentChunkIndex + 1
            val paragraph = state.currentParagraph ?: return
            val nextParagraph = state.nextParagraphInBook
            cancelActiveSynthesis()
            _uiState.update { it.copy(currentChunkIndex = nextChunkIdx, isSynthesizing = true) }
            activeSynthesisJob = viewModelScope.launch {
                synthesizeAndPlayChunk(
                    paragraph = paragraph,
                    chunks = chunks,
                    chunkIndex = nextChunkIdx,
                    seekWordId = null,
                    nextParagraph = nextParagraph
                )
            }
        } else {
            val nextParaIdx = (state.currentParagraphIndex + 1 until state.paragraphs.size)
                .firstOrNull { idx ->
                    state.paragraphs[idx].kind != ParagraphKind.Footnote &&
                        state.paragraphs[idx].text.any { it.isLetterOrDigit() }
                }
            if (nextParaIdx != null) {
                startOrResumeParagraphPlayback(
                    paragraphIndex = nextParaIdx,
                    chunkIndex = 0,
                    seekWordId = null
                )
            } else {
                _uiState.update { it.copy(isPlaying = false, isSynthesizing = false, activeWordTiming = null) }
                updateActiveBookProgress()
            }
        }
    }

    fun skipToNextChunkOrParagraph() {
        cancelActiveSynthesis()
        playerController.pauseForTransition(clearActiveWord = false)
        advanceToNextChunkOrParagraph()
    }

    fun skipToPreviousChunkOrParagraph() {
        val state = _uiState.value
        if (state.paragraphs.isEmpty()) return
        cancelActiveSynthesis()
        playerController.pauseForTransition(clearActiveWord = false)

        if (state.currentChunkIndex > 0 && state.currentParagraphChunks.isNotEmpty()) {
            val prevChunkIdx = state.currentChunkIndex - 1
            val paragraph = state.currentParagraph ?: return
            val nextParagraph = state.nextParagraphInBook
            _uiState.update { it.copy(currentChunkIndex = prevChunkIdx, isSynthesizing = true) }
            activeSynthesisJob = viewModelScope.launch {
                synthesizeAndPlayChunk(
                    paragraph = paragraph,
                    chunks = state.currentParagraphChunks,
                    chunkIndex = prevChunkIdx,
                    seekWordId = null,
                    nextParagraph = nextParagraph
                )
            }
        } else {
            val prevParaIdx = (state.currentParagraphIndex - 1 downTo 0)
                .firstOrNull { idx ->
                    state.paragraphs[idx].kind != ParagraphKind.Footnote &&
                        state.paragraphs[idx].text.any { it.isLetterOrDigit() }
                }
                ?: 0
            startOrResumeParagraphPlayback(
                paragraphIndex = prevParaIdx,
                chunkIndex = Int.MAX_VALUE,
                seekWordId = null
            )
        }
    }

    fun seekByDeltaMs(deltaMs: Long) {
        playerController.seekByDeltaMs(deltaMs)
    }

    private fun updateActiveBookProgress(explicitScrollOffset: Int? = null) {
        val state = _uiState.value
        val active = state.activeBook ?: return
        val progress = state.bookProgress
        val updated = active.copy(
            progress = progress,
            lastChapterIndex = 0,
            lastParagraphIndex = state.currentParagraphIndex,
            lastChunkIndex = state.currentChunkIndex,
            lastWordId = state.activeWordTiming?.wordId ?: active.lastWordId,
            lastScrollOffset = explicitScrollOffset ?: active.lastScrollOffset
        )
        _uiState.update { s ->
            s.copy(
                activeBook = updated,
                books = s.books.map { if (it.id == updated.id) updated else it }
            )
        }
        persistCurrentState()
    }

    override fun onCleared() {
        updateActiveBookProgress()
        playerController.release()
        super.onCleared()
    }
}
