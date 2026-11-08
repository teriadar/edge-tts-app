package com.nuvola.reader.player

import android.content.Intent
import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.DefaultMediaNotificationProvider
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import com.nuvola.reader.R

@OptIn(UnstableApi::class)
class NuvolaPlaybackService : MediaSessionService() {

    companion object {
        @Volatile
        var activeMediaSession: MediaSession? = null
            private set

        @Volatile
        private var instance: NuvolaPlaybackService? = null

        fun registerSession(session: MediaSession?) {
            activeMediaSession = session
            if (session != null) {
                instance?.attachSessionIfNeeded(session)
            }
        }

        fun stopService(context: android.content.Context) {
            runCatching {
                instance?.stopForeground(STOP_FOREGROUND_REMOVE)
                instance?.stopSelf()
                context.stopService(Intent(context, NuvolaPlaybackService::class.java))
            }
        }
    }

    override fun onCreate() {
        super.onCreate()
        instance = this
        val notificationProvider = DefaultMediaNotificationProvider.Builder(applicationContext)
            .build()
            .apply {
                setSmallIcon(R.mipmap.ic_launcher)
            }
        setMediaNotificationProvider(notificationProvider)
        activeMediaSession?.let { attachSessionIfNeeded(it) }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        activeMediaSession?.let { attachSessionIfNeeded(it) }
        return super.onStartCommand(intent, flags, startId)
    }

    private fun attachSessionIfNeeded(session: MediaSession) {
        runCatching {
            if (!sessions.contains(session)) {
                addSession(session)
            }
        }
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? {
        return activeMediaSession
    }

    override fun onTaskRemoved(rootIntent: Intent?) {
        val player = activeMediaSession?.player
        if (player == null || !player.playWhenReady || player.mediaItemCount == 0) {
            stopSelf()
        }
    }

    override fun onDestroy() {
        if (instance === this) {
            instance = null
        }
        super.onDestroy()
    }
}
