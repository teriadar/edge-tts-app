package com.pagina.reader.player

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.core.content.ContextCompat
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.MediaSession
import com.pagina.reader.MainActivity
import com.pagina.reader.bridge.PaginaNativeBridge
import com.pagina.reader.bridge.SynthesizedChunkDto
import com.pagina.reader.bridge.WordAudioTiming
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import java.io.File

@OptIn(UnstableApi::class)
class EdgeExoPlayerController(
    private val context: Context,
    private val scope: CoroutineScope,
    private val onActiveWordChanged: (WordAudioTiming?) -> Unit,
    private val onPlayingStateChanged: (Boolean) -> Unit,
    private val onChunkPlaybackCompleted: () -> Unit,
    private val onSkipNext: () -> Unit = {},
    private val onSkipPrevious: () -> Unit = {},
    private val onPlayRequestedFromSession: () -> Unit = {},
    private val onPauseRequestedFromSession: () -> Unit = {}
) {
    private val speechAudioAttributes: AudioAttributes = AudioAttributes.Builder()
        .setUsage(C.USAGE_MEDIA)
        .setContentType(C.AUDIO_CONTENT_TYPE_SPEECH)
        .build()

    private val exoPlayer: ExoPlayer = ExoPlayer.Builder(context)
        .setAudioAttributes(speechAudioAttributes,  true)
        .setHandleAudioBecomingNoisy(true)
        .setWakeMode(C.WAKE_MODE_NETWORK)
        .setSeekBackIncrementMs(15_000L)
        .setSeekForwardIncrementMs(15_000L)
        .build()

    private val forwardingPlayer: Player = object : ForwardingPlayer(exoPlayer) {
        override fun getAvailableCommands(): Player.Commands {
            return super.getAvailableCommands()
                .buildUpon()
                .add(Player.COMMAND_SEEK_TO_NEXT)
                .add(Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM)
                .add(Player.COMMAND_SEEK_TO_PREVIOUS)
                .add(Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM)
                .add(Player.COMMAND_SEEK_BACK)
                .add(Player.COMMAND_SEEK_FORWARD)
                .build()
        }

        override fun isCommandAvailable(command: Int): Boolean {
            return when (command) {
                Player.COMMAND_PLAY_PAUSE,
                Player.COMMAND_SEEK_TO_NEXT,
                Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM,
                Player.COMMAND_SEEK_TO_PREVIOUS,
                Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM,
                Player.COMMAND_SEEK_BACK,
                Player.COMMAND_SEEK_FORWARD -> true
                else -> super.isCommandAvailable(command)
            }
        }

        override fun hasNextMediaItem(): Boolean = true

        override fun hasPreviousMediaItem(): Boolean = true

        override fun play() {
            if (canResume()) {
                resume()
            } else {
                onPlayRequestedFromSession()
            }
        }

        override fun pause() {
            this@EdgeExoPlayerController.pause()
            onPauseRequestedFromSession()
        }

        override fun seekToNext() {
            onSkipNext()
        }

        override fun seekToNextMediaItem() {
            onSkipNext()
        }

        override fun seekToPrevious() {
            if (exoPlayer.currentPosition > 3_500L) {
                exoPlayer.seekTo(0L)
                syncActiveWordAtCurrentPosition()
            } else {
                onSkipPrevious()
            }
        }

        override fun seekToPreviousMediaItem() {
            onSkipPrevious()
        }

        override fun seekBack() {
            super.seekBack()
            syncActiveWordAtCurrentPosition()
        }

        override fun seekForward() {
            super.seekForward()
            syncActiveWordAtCurrentPosition()
        }

        override fun getMediaMetadata(): MediaMetadata {
            return buildMediaMetadata()
        }
    }

    private var mediaSession: MediaSession? = null
    private var currentTimings: List<WordAudioTiming> = emptyList()
    private var positionTickerJob: Job? = null
    private var currentPlaybackSpeed: Float = 1.0f
    private var currentBookTitle: String = "Pagina Reader"
    private var currentBookAuthor: String = "Edge-TTS Neural"
    private var currentBookCoverBytes: ByteArray? = null
    private var chunkCompletionDispatched: Boolean = false

    init {
        exoPlayer.addListener(object : Player.Listener {
            override fun onIsPlayingChanged(isPlaying: Boolean) {
                if (isPlaying) {
                    ensurePlaybackServiceStarted()
                    startPositionTicker()
                    onPlayingStateChanged(true)
                } else {
                    stopPositionTicker()
                    syncActiveWordAtCurrentPosition()
                    onPlayingStateChanged(false)
                }
            }

            override fun onPositionDiscontinuity(
                oldPosition: Player.PositionInfo,
                newPosition: Player.PositionInfo,
                reason: Int
            ) {
                if (reason == Player.DISCONTINUITY_REASON_SEEK ||
                    reason == Player.DISCONTINUITY_REASON_SEEK_ADJUSTMENT
                ) {
                    syncActiveWordAtCurrentPosition()
                }
            }

            override fun onPlaybackStateChanged(playbackState: Int) {
                if (playbackState == Player.STATE_ENDED && !chunkCompletionDispatched && exoPlayer.playWhenReady) {
                    chunkCompletionDispatched = true
                    stopPositionTicker()
                    onChunkPlaybackCompleted()
                }
            }
        })

        initMediaSession()
    }

    private fun initMediaSession() {
        runCatching {
            PaginaPlaybackService.activeMediaSession?.release()
            val launchIntent = Intent(context, MainActivity::class.java).apply {
                flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
            }
            val pendingIntent = PendingIntent.getActivity(
                context,
                0,
                launchIntent,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )
            val session = MediaSession.Builder(context, forwardingPlayer)
                .setId("pagina_media_session")
                .setSessionActivity(pendingIntent)
                .build()
            mediaSession = session
            PaginaPlaybackService.registerSession(session)
        }
    }

    private fun ensurePlaybackServiceStarted() {
        runCatching {
            mediaSession?.let { PaginaPlaybackService.registerSession(it) }
            val serviceIntent = Intent(context, PaginaPlaybackService::class.java)
            ContextCompat.startForegroundService(context, serviceIntent)
        }
    }

    fun updateBookMetadata(title: String, author: String, coverImagePath: String?) {
        currentBookTitle = title.ifBlank { "Pagina Reader" }
        currentBookAuthor = author.ifBlank { "Edge-TTS Neural" }
        currentBookCoverBytes = runCatching {
            coverImagePath?.let { File(it) }?.takeIf { it.exists() && it.length() in 1..(5 * 1024 * 1024) }?.readBytes()
        }.getOrNull()
    }

    private fun buildMediaMetadata(): MediaMetadata {
        val builder = MediaMetadata.Builder()
            .setTitle(currentBookTitle)
            .setArtist(currentBookAuthor)
            .setAlbumTitle("Pagina Reader")
        val coverBytes = currentBookCoverBytes
        if (coverBytes != null) {
            builder.setArtworkData(coverBytes, MediaMetadata.PICTURE_TYPE_FRONT_COVER)
        }
        return builder.build()
    }

    fun setPlaybackSpeed(speed: Float) {
        val clamped = speed.coerceIn(0.5f, 2.5f)
        currentPlaybackSpeed = clamped
        exoPlayer.setPlaybackSpeed(clamped)
    }

    fun seekByDeltaMs(deltaMs: Long) {
        if (exoPlayer.mediaItemCount == 0) return
        val duration = exoPlayer.duration.takeIf { it > 0L } ?: Long.MAX_VALUE
        val targetPos = (exoPlayer.currentPosition + deltaMs).coerceIn(0L, duration)
        exoPlayer.seekTo(targetPos)
        syncActiveWordAtCurrentPosition()
    }

    fun playChunk(chunk: SynthesizedChunkDto, initialSeekWordId: Int? = null) {
        chunkCompletionDispatched = false
        currentTimings = chunk.wordTimings

        val mp3File = chunk.audioFilePath?.let { File(it) }?.takeIf { it.exists() }
            ?: run {
                val fallbackDir = File(context.cacheDir, "pagina_tts_cache").apply { mkdirs() }
                File(fallbackDir, "${chunk.cacheKeySha256}.mp3").apply {
                    if (!exists()) {
                        writeBytes(chunk.toMp3ByteArray())
                    }
                }
            }

        val mediaItem = MediaItem.Builder()
            .setUri(Uri.fromFile(mp3File))
            .setMediaMetadata(buildMediaMetadata())
            .build()

        ensurePlaybackServiceStarted()
        exoPlayer.setMediaItem(mediaItem)
        exoPlayer.setPlaybackSpeed(currentPlaybackSpeed)
        exoPlayer.prepare()

        if (initialSeekWordId != null && currentTimings.isNotEmpty()) {
            val targetTiming = currentTimings.firstOrNull { it.wordId == initialSeekWordId }
                ?: currentTimings.firstOrNull { it.wordId >= initialSeekWordId }
                ?: currentTimings.last()
            exoPlayer.seekTo(targetTiming.audioStartMs)
            onActiveWordChanged(targetTiming)
        } else if (currentTimings.isNotEmpty()) {
            onActiveWordChanged(currentTimings.first())
        }

        exoPlayer.playWhenReady = true
    }

    fun canResume(): Boolean {
        return currentTimings.isNotEmpty() &&
            (exoPlayer.playbackState == Player.STATE_READY || exoPlayer.playbackState == Player.STATE_BUFFERING) &&
            exoPlayer.mediaItemCount > 0
    }

    fun resume() {
        if (canResume()) {
            ensurePlaybackServiceStarted()
            exoPlayer.setPlaybackSpeed(currentPlaybackSpeed)
            exoPlayer.play()
        }
    }

    fun pause() {
        syncActiveWordAtCurrentPosition()
        exoPlayer.pause()
    }

    fun pauseForTransition(clearActiveWord: Boolean = false) {
        chunkCompletionDispatched = true
        stopPositionTicker()
        exoPlayer.pause()
        currentTimings = emptyList()
        if (clearActiveWord) {
            onActiveWordChanged(null)
        }
    }

    fun stop(clearActiveWord: Boolean = true) {
        chunkCompletionDispatched = true
        stopPositionTicker()
        exoPlayer.stop()
        exoPlayer.clearMediaItems()
        currentTimings = emptyList()
        if (clearActiveWord) {
            onActiveWordChanged(null)
        }
        PaginaPlaybackService.stopService(context)
    }

    fun release() {
        stopPositionTicker()
        runCatching {
            if (PaginaPlaybackService.activeMediaSession == mediaSession) {
                PaginaPlaybackService.registerSession(null)
            }
            mediaSession?.release()
            mediaSession = null
        }
        exoPlayer.release()
    }

    private fun syncActiveWordAtCurrentPosition() {
        if (currentTimings.isNotEmpty()) {
            val posMs = exoPlayer.currentPosition.coerceAtLeast(0L)
            val activeWord = PaginaNativeBridge.activeWordAtMs(currentTimings, posMs)
            if (activeWord != null) {
                onActiveWordChanged(activeWord)
            }
        }
    }

    private fun startPositionTicker() {
        positionTickerJob?.cancel()
        positionTickerJob = scope.launch(Dispatchers.Main) {
            while (isActive && exoPlayer.isPlaying) {
                val posMs = exoPlayer.currentPosition.coerceAtLeast(0L)
                val activeWord = PaginaNativeBridge.activeWordAtMs(currentTimings, posMs)
                onActiveWordChanged(activeWord)
                delay(20L)
            }
        }
    }

    private fun stopPositionTicker() {
        positionTickerJob?.cancel()
        positionTickerJob = null
    }
}
