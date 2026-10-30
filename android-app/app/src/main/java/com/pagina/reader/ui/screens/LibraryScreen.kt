package com.pagina.reader.ui.screens

import android.graphics.BitmapFactory
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.MenuBook
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.pagina.reader.R
import com.pagina.reader.bridge.EdgeHealthReport
import com.pagina.reader.bridge.ServiceStatus
import com.pagina.reader.ui.BookItem
import com.pagina.reader.ui.ReaderUiState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import java.io.File

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LibraryScreen(
    state: ReaderUiState,
    onSearchQueryChange: (String) -> Unit,
    onOpenBook: (BookItem) -> Unit,
    onImportEpubUri: (Uri) -> Unit,
    onRefreshHealth: () -> Unit,
    onFetchBookMetadata: (BookItem, Boolean) -> Unit = { _, _ -> },
    onEnrichAllMetadata: () -> Unit = {}
) {
    var searching by remember { mutableStateOf(false) }
    var showSettingsCloud by remember { mutableStateOf(false) }
    var showHealthDialog by remember { mutableStateOf(false) }
    var showPermissionsInfoDialog by remember { mutableStateOf(false) }
    var selectedBookForInfoId by remember { mutableStateOf<String?>(null) }
    val searchFocusRequester = remember { FocusRequester() }
    val keyboardController = LocalSoftwareKeyboardController.current

    LaunchedEffect(searching) {
        if (searching) {
            delay(80)
            runCatching { searchFocusRequester.requestFocus() }
            keyboardController?.show()
        } else {
            keyboardController?.hide()
        }
    }

    val epubPickerLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocument()
    ) { uri: Uri? ->
        if (uri != null) {
            onImportEpubUri(uri)
        }
    }

    val filteredBooks = remember(state.books, state.searchQuery) {
        if (state.searchQuery.isBlank()) {
            state.books
        } else {
            val q = state.searchQuery.trim().lowercase()
            state.books.filter {
                it.title.lowercase().contains(q) || it.author.lowercase().contains(q)
            }
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    AnimatedContent(
                        targetState = searching,
                        transitionSpec = {
                            (fadeIn(tween(180)) + scaleIn(initialScale = 0.95f, animationSpec = tween(180)))
                                .togetherWith(fadeOut(tween(140)))
                        },
                        label = "search_bar_transition"
                    ) { isSearching ->
                        if (isSearching) {
                            Surface(
                                shape = CircleShape,
                                color = MaterialTheme.colorScheme.surfaceContainerHigh,
                                tonalElevation = 3.dp,
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(end = 12.dp)
                                    .height(46.dp)
                            ) {
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    modifier = Modifier
                                        .fillMaxSize()
                                        .padding(start = 16.dp, end = 6.dp)
                                ) {
                                    Icon(
                                        imageVector = Icons.Default.Search,
                                        contentDescription = null,
                                        tint = MaterialTheme.colorScheme.primary,
                                        modifier = Modifier.size(20.dp)
                                    )
                                    Spacer(Modifier.width(10.dp))
                                    Box(
                                        contentAlignment = Alignment.CenterStart,
                                        modifier = Modifier.weight(1f)
                                    ) {
                                        if (state.searchQuery.isEmpty()) {
                                            Text(
                                                text = "Search by title or author...",
                                                style = MaterialTheme.typography.bodyMedium,
                                                color = MaterialTheme.colorScheme.onSurfaceVariant
                                            )
                                        }
                                        BasicTextField(
                                            value = state.searchQuery,
                                            onValueChange = onSearchQueryChange,
                                            singleLine = true,
                                            textStyle = MaterialTheme.typography.bodyMedium.copy(
                                                color = MaterialTheme.colorScheme.onSurface
                                            ),
                                            cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                                            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                                            keyboardActions = KeyboardActions(
                                                onSearch = { keyboardController?.hide() }
                                            ),
                                            modifier = Modifier
                                                .fillMaxWidth()
                                                .focusRequester(searchFocusRequester)
                                        )
                                    }
                                    Surface(
                                        onClick = {
                                            if (state.searchQuery.isNotEmpty()) {
                                                onSearchQueryChange("")
                                            } else {
                                                searching = false
                                            }
                                        },
                                        shape = CircleShape,
                                        color = MaterialTheme.colorScheme.surfaceContainerHighest,
                                        modifier = Modifier.size(32.dp)
                                    ) {
                                        Box(contentAlignment = Alignment.Center) {
                                            Icon(
                                                imageVector = Icons.Default.Close,
                                                contentDescription = "Close search",
                                                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                                                modifier = Modifier.size(16.dp)
                                            )
                                        }
                                    }
                                }
                            }
                        } else {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(12.dp)
                            ) {
                                Image(
                                    painter = painterResource(id = R.drawable.ic_pagina_logo),
                                    contentDescription = "Pagina Logo",
                                    modifier = Modifier.size(38.dp)
                                )
                                Column {
                                    Text("Pagina Reader", style = MaterialTheme.typography.titleLarge)
                                    Text(
                                        "Listen your books everywhere",
                                        style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant
                                    )
                                }
                            }
                        }
                    }
                }
            )
        },
        floatingActionButton = {
            FloatingActionButton(
                onClick = { showSettingsCloud = !showSettingsCloud },
                shape = CircleShape
            ) {
                Icon(
                    imageVector = if (showSettingsCloud) Icons.Default.Close else Icons.Default.Menu,
                    contentDescription = "Pagina Hub"
                )
            }
        }
    ) { innerPadding ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
        ) {
            if (filteredBooks.isEmpty()) {
                Box(
                    modifier = Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center
                ) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Image(
                            painter = painterResource(id = R.drawable.ic_pagina_logo),
                            contentDescription = "Pagina Logo",
                            modifier = Modifier.size(72.dp)
                        )
                        Spacer(Modifier.height(14.dp))
                        Text(
                            text = "No books in library yet",
                            style = MaterialTheme.typography.titleMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                        Spacer(Modifier.height(4.dp))
                        Text(
                            text = "Import an EPUB book to start reading and listening",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                }
            } else {
                LazyVerticalGrid(
                    columns = GridCells.Adaptive(minSize = 156.dp),
                    contentPadding = PaddingValues(
                        start = 12.dp,
                        end = 12.dp,
                        top = 12.dp,
                        bottom = 88.dp
                    ),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                    modifier = Modifier.fillMaxSize()
                ) {
                    items(filteredBooks, key = { it.id }) { book ->
                        PaginaBookGridCard(
                            book = book,
                            isFetchingMetadata = state.fetchingMetadataBookIds.contains(book.id),
                            onTap = { onOpenBook(book) },
                            onRead = { onOpenBook(book) },
                            onInfo = { selectedBookForInfoId = book.id }
                        )
                    }
                }
            }

            if (showSettingsCloud) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .background(Color.Black.copy(alpha = 0.22f))
                        .clickable(
                            interactionSource = remember { MutableInteractionSource() },
                            indication = null,
                            onClick = { showSettingsCloud = false }
                        )
                )
            }

            AnimatedVisibility(
                visible = showSettingsCloud,
                enter = fadeIn(tween(180)) +
                    scaleIn(
                        initialScale = 0.85f,
                        transformOrigin = TransformOrigin(1f, 1f),
                        animationSpec = tween(200)
                    ) +
                    slideInVertically(initialOffsetY = { it / 6 }, animationSpec = tween(200)),
                exit = fadeOut(tween(140)) +
                    scaleOut(
                        targetScale = 0.85f,
                        transformOrigin = TransformOrigin(1f, 1f),
                        animationSpec = tween(150)
                    ) +
                    slideOutVertically(targetOffsetY = { it / 6 }, animationSpec = tween(150)),
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .padding(end = 16.dp, bottom = 84.dp)
            ) {
                SettingsCloudBubble(
                    state = state,
                    searching = searching,
                    onImportEpub = {
                        showSettingsCloud = false
                        epubPickerLauncher.launch(
                            arrayOf("application/epub+zip", "application/octet-stream", "*/*")
                        )
                    },
                    onToggleSearch = {
                        searching = !searching
                        if (!searching) onSearchQueryChange("")
                        showSettingsCloud = false
                    },
                    onEnrichAllMetadata = {
                        showSettingsCloud = false
                        onEnrichAllMetadata()
                    },
                    onOpenHealthDetails = {
                        showSettingsCloud = false
                        showHealthDialog = true
                    },
                    onOpenPermissionsInfo = {
                        showSettingsCloud = false
                        showPermissionsInfoDialog = true
                    }
                )
            }
        }
    }

    val selectedBookForInfo = remember(state.books, selectedBookForInfoId) {
        selectedBookForInfoId?.let { id -> state.books.find { it.id == id } }
    }
    if (selectedBookForInfo != null) {
        BookMetadataDetailsDialog(
            book = selectedBookForInfo,
            isFetching = state.fetchingMetadataBookIds.contains(selectedBookForInfo.id),
            onRefreshMetadata = { onFetchBookMetadata(selectedBookForInfo, false) },
            onReadBook = {
                val target = selectedBookForInfo
                selectedBookForInfoId = null
                onOpenBook(target)
            },
            onDismiss = { selectedBookForInfoId = null }
        )
    }

    if (showHealthDialog) {
        EdgeHealthDetailsDialog(
            report = state.healthReport,
            isChecking = state.isCheckingHealth,
            onRefresh = onRefreshHealth,
            onDismiss = { showHealthDialog = false }
        )
    }

    if (showPermissionsInfoDialog) {
        AppPermissionsInfoDialog(
            onDismiss = { showPermissionsInfoDialog = false }
        )
    }
}

@Composable
private fun rememberCoverBitmap(coverImagePath: String?): ImageBitmap? {
    val bitmap by produceState<ImageBitmap?>(initialValue = null, key1 = coverImagePath) {
        value = if (coverImagePath.isNullOrBlank()) {
            null
        } else {
            withContext(Dispatchers.IO) {
                runCatching {
                    val file = File(coverImagePath)
                    if (file.exists() && file.length() > 0L) {
                        BitmapFactory.decodeFile(file.absolutePath)?.asImageBitmap()
                    } else {
                        null
                    }
                }.getOrNull()
            }
        }
    }
    return bitmap
}

@Composable
private fun SettingsCloudBubble(
    state: ReaderUiState,
    searching: Boolean,
    onImportEpub: () -> Unit,
    onToggleSearch: () -> Unit,
    onEnrichAllMetadata: () -> Unit,
    onOpenHealthDetails: () -> Unit,
    onOpenPermissionsInfo: () -> Unit
) {
    Surface(
        shape = RoundedCornerShape(26.dp),
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        tonalElevation = 6.dp,
        shadowElevation = 10.dp,
        modifier = Modifier.widthIn(min = 260.dp, max = 320.dp)
    ) {
        Column(
            modifier = Modifier.padding(horizontal = 18.dp, vertical = 16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)
        ) {

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp)
                ) {
                    Text(
                        text = "Pagina Hub",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold
                    )
                    Surface(
                        shape = CircleShape,
                        color = MaterialTheme.colorScheme.surfaceContainerHighest,
                        modifier = Modifier
                            .size(26.dp)
                            .clip(CircleShape)
                            .clickable(onClick = onOpenPermissionsInfo)
                    ) {
                        Box(contentAlignment = Alignment.Center) {
                            Icon(
                                imageVector = Icons.Default.Info,
                                contentDescription = null,
                                tint = MaterialTheme.colorScheme.primary,
                                modifier = Modifier.size(16.dp)
                            )
                        }
                    }
                }
                EdgeHealthBadge(
                    report = state.healthReport,
                    isChecking = state.isCheckingHealth,
                    onClick = onOpenHealthDetails
                )
            }

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                Button(
                    onClick = onImportEpub,
                    modifier = Modifier.weight(1f),
                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 10.dp)
                ) {
                    Icon(
                        imageVector = Icons.Default.Add,
                        contentDescription = null,
                        modifier = Modifier.size(18.dp)
                    )
                    Spacer(Modifier.width(6.dp))
                    Text("Import EPUB", maxLines = 1)
                }

                FilledTonalButton(
                    onClick = onToggleSearch,
                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 10.dp)
                ) {
                    Icon(
                        imageVector = if (searching) Icons.Default.Close else Icons.Default.Search,
                        contentDescription = null,
                        modifier = Modifier.size(18.dp)
                    )
                    Spacer(Modifier.width(4.dp))
                    Text(if (searching) "Clear" else "Search", maxLines = 1)
                }
            }

            OutlinedButton(
                onClick = onEnrichAllMetadata,
                modifier = Modifier.fillMaxWidth(),
                contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp)
            ) {
                Icon(
                    imageVector = Icons.Default.Refresh,
                    contentDescription = null,
                    modifier = Modifier.size(16.dp)
                )
                Spacer(Modifier.width(6.dp))
                Text(
                    text = if (state.fetchingMetadataBookIds.isNotEmpty()) {
                        "Fetching Covers (${state.fetchingMetadataBookIds.size})..."
                    } else {
                        "Fetch Missing Covers & Info"
                    },
                    maxLines = 1
                )
            }
        }
    }
}

@Composable
private fun PaginaBookGridCard(
    book: BookItem,
    isFetchingMetadata: Boolean,
    onTap: () -> Unit,
    onRead: () -> Unit,
    onInfo: () -> Unit
) {
    val colors = MaterialTheme.colorScheme
    val coverBitmap = rememberCoverBitmap(book.coverImagePath)

    Surface(
        color = colors.surfaceContainer,
        shape = RoundedCornerShape(12.dp),
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .clickable(onClick = onTap)
    ) {
        Column(modifier = Modifier.padding(10.dp)) {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .aspectRatio(0.72f)
                    .clip(RoundedCornerShape(8.dp))
                    .background(colors.surfaceContainerHighest)
            ) {
                if (coverBitmap != null) {
                    Image(
                        bitmap = coverBitmap,
                        contentDescription = book.title,
                        contentScale = ContentScale.Crop,
                        modifier = Modifier.fillMaxSize()
                    )
                } else {
                    Icon(
                        imageVector = Icons.AutoMirrored.Filled.MenuBook,
                        contentDescription = null,
                        tint = colors.onSurfaceVariant,
                        modifier = Modifier
                            .size(32.dp)
                            .align(Alignment.Center)
                    )
                }

                if (isFetchingMetadata) {
                    CircularProgressIndicator(
                        strokeWidth = 2.dp,
                        modifier = Modifier
                            .size(22.dp)
                            .align(Alignment.TopStart)
                            .padding(6.dp)
                    )
                }

                Surface(
                    shape = CircleShape,
                    color = colors.surface.copy(alpha = 0.78f),
                    modifier = Modifier
                        .align(Alignment.TopEnd)
                        .padding(6.dp)
                        .size(28.dp)
                        .clip(CircleShape)
                        .clickable(onClick = onInfo)
                ) {
                    Box(contentAlignment = Alignment.Center) {
                        Icon(
                            imageVector = Icons.Default.Info,
                            contentDescription = "Book Metadata & Synopsis",
                            tint = colors.onSurface,
                            modifier = Modifier.size(16.dp)
                        )
                    }
                }

                FilledTonalIconButton(
                    onClick = onRead,
                    modifier = Modifier
                        .align(Alignment.BottomEnd)
                        .padding(6.dp)
                        .size(34.dp)
                ) {
                    Icon(
                        imageVector = Icons.Default.PlayArrow,
                        contentDescription = "Listen book",
                        modifier = Modifier.size(18.dp)
                    )
                }
            }
            Spacer(Modifier.height(8.dp))
            Text(
                text = book.title,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.Medium
            )
            Spacer(Modifier.height(2.dp))
            Text(
                text = book.author,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                style = MaterialTheme.typography.bodySmall,
                color = colors.onSurfaceVariant
            )
            Spacer(Modifier.height(6.dp))
            LinearProgressIndicator(
                progress = { book.progress },
                modifier = Modifier
                    .fillMaxWidth()
                    .height(3.dp)
                    .clip(RoundedCornerShape(2.dp))
            )
        }
    }
}

@Composable
fun EdgeHealthBadge(
    report: EdgeHealthReport?,
    isChecking: Boolean,
    onClick: () -> Unit
) {
    val isHealthy = !isChecking && report?.status == ServiceStatus.Healthy
    val dotColor = when {
        isChecking -> Color(0xFF9E9E9E)
        report == null -> Color(0xFF9E9E9E)
        report.status == ServiceStatus.Healthy -> Color(0xFF2ECC71)
        report.status == ServiceStatus.Degraded -> Color(0xFFF9A825)
        else -> Color(0xFFC62828)
    }

    val infiniteTransition = rememberInfiniteTransition(label = "edgeHealthPulse")
    val pulseScale by infiniteTransition.animateFloat(
        initialValue = 1f,
        targetValue = 2.2f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 1400, easing = FastOutSlowInEasing),
            repeatMode = RepeatMode.Restart
        ),
        label = "pulseHaloScale"
    )
    val pulseAlpha by infiniteTransition.animateFloat(
        initialValue = 0.65f,
        targetValue = 0f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 1400, easing = FastOutSlowInEasing),
            repeatMode = RepeatMode.Restart
        ),
        label = "pulseHaloAlpha"
    )
    val coreScale by infiniteTransition.animateFloat(
        initialValue = 0.92f,
        targetValue = 1.12f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 700, easing = FastOutSlowInEasing),
            repeatMode = RepeatMode.Reverse
        ),
        label = "pulseCoreScale"
    )

    val label = when {
        isChecking -> "Checking..."
        report == null -> "Edge TTS"
        report.status == ServiceStatus.Healthy -> "Edge ${report.wsHandshakeLatencyMs ?: "--"}ms"
        report.status == ServiceStatus.Degraded -> "Degraded"
        else -> "Offline"
    }

    Surface(
        shape = RoundedCornerShape(50),
        color = MaterialTheme.colorScheme.surfaceContainerHighest,
        modifier = Modifier
            .clip(RoundedCornerShape(50))
            .clickable(onClick = onClick)
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp)
        ) {
            Box(
                contentAlignment = Alignment.Center,
                modifier = Modifier.size(12.dp)
            ) {
                if (isHealthy) {
                    Box(
                        modifier = Modifier
                            .size(8.dp)
                            .graphicsLayer {
                                scaleX = pulseScale
                                scaleY = pulseScale
                                alpha = pulseAlpha
                            }
                            .clip(CircleShape)
                            .background(dotColor)
                    )
                }
                Box(
                    modifier = Modifier
                        .size(8.dp)
                        .graphicsLayer {
                            val s = if (isHealthy) coreScale else 1f
                            scaleX = s
                            scaleY = s
                        }
                        .clip(CircleShape)
                        .background(dotColor)
                )
            }
            Spacer(Modifier.width(6.dp))
            Text(
                text = label,
                style = MaterialTheme.typography.labelSmall
            )
        }
    }
}

@Composable
private fun EdgeHealthDetailsDialog(
    report: EdgeHealthReport?,
    isChecking: Boolean,
    onRefresh: () -> Unit,
    onDismiss: () -> Unit
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Edge-TTS Service Health") },
        text = {
            if (isChecking) {
                Text("Checking connection and Sec-MS-GEC v2 handshake with Microsoft Edge...")
            } else if (report == null) {
                Text("No health check run yet.")
            } else {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("Status: ${report.status}", fontWeight = FontWeight.Bold)
                    Text("WSS Handshake: ${if (report.websocketHandshakeOk) "OK (${report.wsHandshakeLatencyMs ?: 0} ms)" else "Failed"}")
                    Text("Voices API: ${if (report.voicesApiReachable) "OK (${report.httpLatencyMs ?: 0} ms)" else "Unreachable"}")
                    Text("App Voices: 8 Neural (4 Italiano · 4 English)")
                    if (report.availableVoicesCount > 0) {
                        Text("Edge Server Catalog: ${report.availableVoicesCount} voices online")
                    }
                    Text("Sec-MS-GEC Token: v${report.secMsGecVersion}")
                    Spacer(Modifier.height(4.dp))
                    Text(
                        text = report.message.replace(Regex(", \\d+ English voices"), ""),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onRefresh) {
                Icon(Icons.Default.Refresh, contentDescription = null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(4.dp))
                Text("Re-check")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Close")
            }
        }
    )
}

@Composable
private fun BookMetadataDetailsDialog(
    book: BookItem,
    isFetching: Boolean,
    onRefreshMetadata: () -> Unit,
    onReadBook: () -> Unit,
    onDismiss: () -> Unit
) {
    val coverBitmap = rememberCoverBitmap(book.coverImagePath)
    val colors = MaterialTheme.colorScheme

    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Text(
                text = book.title,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis
            )
        },
        text = {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(max = 420.dp)
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(10.dp)
            ) {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalAlignment = Alignment.Top
                ) {
                    Box(
                        modifier = Modifier
                            .width(88.dp)
                            .aspectRatio(0.72f)
                            .clip(RoundedCornerShape(8.dp))
                            .background(colors.surfaceContainerHighest),
                        contentAlignment = Alignment.Center
                    ) {
                        if (coverBitmap != null) {
                            Image(
                                bitmap = coverBitmap,
                                contentDescription = book.title,
                                contentScale = ContentScale.Crop,
                                modifier = Modifier.fillMaxSize()
                            )
                        } else {
                            Icon(
                                imageVector = Icons.AutoMirrored.Filled.MenuBook,
                                contentDescription = null,
                                tint = colors.onSurfaceVariant,
                                modifier = Modifier.size(28.dp)
                            )
                        }
                    }

                    Column(
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                        modifier = Modifier.weight(1f)
                    ) {
                        Text(
                            text = book.author,
                            style = MaterialTheme.typography.bodyMedium,
                            fontWeight = FontWeight.Medium
                        )
                        if (!book.publisher.isNullOrBlank()) {
                            Text(
                                text = "Publisher: ${book.publisher}",
                                style = MaterialTheme.typography.bodySmall,
                                color = colors.onSurfaceVariant
                            )
                        }
                        if (!book.publishedDate.isNullOrBlank()) {
                            Text(
                                text = "Published: ${book.publishedDate}",
                                style = MaterialTheme.typography.bodySmall,
                                color = colors.onSurfaceVariant
                            )
                        }
                        if (book.pageCount != null && book.pageCount > 0) {
                            Text(
                                text = "Pages: ${book.pageCount}",
                                style = MaterialTheme.typography.bodySmall,
                                color = colors.onSurfaceVariant
                            )
                        }
                        if (!book.isbn.isNullOrBlank()) {
                            Text(
                                text = "ISBN: ${book.isbn}",
                                style = MaterialTheme.typography.bodySmall,
                                color = colors.onSurfaceVariant
                            )
                        }
                        if (!book.metadataSource.isNullOrBlank()) {
                            Text(
                                text = "Source: ${book.metadataSource}",
                                style = MaterialTheme.typography.labelSmall,
                                color = colors.primary
                            )
                        }
                    }
                }

                if (book.categories.isNotEmpty()) {
                    Text(
                        text = book.categories.joinToString(" · "),
                        style = MaterialTheme.typography.labelMedium,
                        color = colors.secondary
                    )
                }

                HorizontalDivider()

                Text(
                    text = if (!book.description.isNullOrBlank()) {
                        book.description
                    } else if (isFetching) {
                        "Fetching synopsis and high-resolution cover from online catalogs..."
                    } else {
                        "No description available yet. You can fetch missing covers and info from Pagina Hub."
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = colors.onSurfaceVariant
                )
            }
        },
        confirmButton = {
            Button(onClick = onReadBook) {
                Icon(Icons.Default.PlayArrow, contentDescription = null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(4.dp))
                Text("Read / Listen")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Close")
            }
        }
    )
}

@Composable
private fun AppPermissionsInfoDialog(
    onDismiss: () -> Unit
) {
    val permissions = remember {
        listOf(
            "INTERNET & ACCESS_NETWORK_STATE" to
                "Required to establish the WebSocket connection to the Edge-TTS service, check network availability before synthesis requests, and fetch online book covers/metadata.",
            "FOREGROUND_SERVICE & FOREGROUND_SERVICE_MEDIA_PLAYBACK" to
                "Required to run the background media playback service so the operating system does not terminate the process while the app is in the background.",
            "WAKE_LOCK" to
                "Allows keeping the CPU awake during audio playback and background speech synthesis while the screen is off.",
            "POST_NOTIFICATIONS" to
                "Required on Android 13+ (API level 33+) to display the persistent media notification with playback controls."
        )
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Text(
                text = "Required Android Permissions",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold
            )
        },
        text = {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(max = 400.dp)
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp)
            ) {
                Text(
                    text = "To enable background audio playback, WebSocket streaming, and network status management, Pagina Reader uses the following system permissions:",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )

                permissions.forEach { (permTitle, permDesc) ->
                    Surface(
                        shape = RoundedCornerShape(10.dp),
                        color = MaterialTheme.colorScheme.surfaceContainerHighest,
                        modifier = Modifier.fillMaxWidth()
                    ) {
                        Column(
                            modifier = Modifier.padding(10.dp),
                            verticalArrangement = Arrangement.spacedBy(4.dp)
                        ) {
                            Text(
                                text = permTitle,
                                style = MaterialTheme.typography.labelMedium,
                                fontWeight = FontWeight.Bold,
                                color = MaterialTheme.colorScheme.primary
                            )
                            Text(
                                text = permDesc,
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurface
                            )
                        }
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) {
                Text("Close")
            }
        }
    )
}
