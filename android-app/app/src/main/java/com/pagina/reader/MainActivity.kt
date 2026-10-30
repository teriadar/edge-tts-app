package com.pagina.reader

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.animation.Crossfade
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.core.content.ContextCompat
import com.pagina.reader.ui.ReaderViewModel
import com.pagina.reader.ui.screens.LibraryScreen
import com.pagina.reader.ui.screens.ReaderScreen
import com.pagina.reader.ui.theme.PaginaReaderTheme

class MainActivity : ComponentActivity() {

    private val viewModel: ReaderViewModel by viewModels()

    private val notificationPermissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { _ -> }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        requestNotificationPermissionIfNeeded()

        setContent {
            val state by viewModel.uiState.collectAsState()

            PaginaReaderTheme {
                BackHandler(enabled = state.activeBook != null) {
                    viewModel.closeReader()
                }

                Crossfade(
                    targetState = state.activeBook != null,
                    label = "screen_navigation"
                ) { inReader ->
                    if (inReader) {
                        ReaderScreen(
                            state = state,
                            onBackToLibrary = viewModel::closeReader,
                            onTogglePlayPause = viewModel::togglePlayPause,
                            onWordTapped = viewModel::onWordTapped,
                            onSelectVoice = viewModel::selectVoice,
                            onSelectRate = viewModel::selectRate,
                            onSelectPitch = viewModel::selectPitch,
                            onAdjustFontSize = viewModel::updateFontSize,
                            onRefreshHealth = { viewModel.refreshEdgeHealth(fullCheck = true) },
                            onScrollPositionChanged = viewModel::onScrollPositionChanged,
                            onJumpToHeading = viewModel::onJumpToHeading
                        )
                    } else {
                        LibraryScreen(
                            state = state,
                            onSearchQueryChange = viewModel::setSearchQuery,
                            onOpenBook = viewModel::openBook,
                            onImportEpubUri = viewModel::importEpubFromUri,
                            onRefreshHealth = { viewModel.refreshEdgeHealth(fullCheck = true) },
                            onFetchBookMetadata = viewModel::fetchOnlineMetadataForBook,
                            onEnrichAllMetadata = viewModel::enrichMissingBookMetadata
                        )
                    }
                }
            }
        }
    }

    private fun requestNotificationPermissionIfNeeded() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            val granted = ContextCompat.checkSelfPermission(
                this,
                Manifest.permission.POST_NOTIFICATIONS
            ) == PackageManager.PERMISSION_GRANTED
            if (!granted) {
                notificationPermissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
            }
        }
    }
}
