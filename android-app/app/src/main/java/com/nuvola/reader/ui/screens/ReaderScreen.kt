package com.nuvola.reader.ui.screens

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.ClickableText
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.FormatListBulleted
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.RecordVoiceOver
import androidx.compose.material.icons.filled.Remove
import androidx.compose.material.icons.filled.TextFields
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.nuvola.reader.bridge.ParagraphData
import com.nuvola.reader.bridge.ParagraphKind
import com.nuvola.reader.bridge.WordAudioTiming
import com.nuvola.reader.ui.ReaderUiState
import com.nuvola.reader.ui.theme.ReaderTypography
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.launch
import java.util.Locale
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReaderScreen(
    state: ReaderUiState,
    onBackToLibrary: () -> Unit,
    onTogglePlayPause: () -> Unit,
    onWordTapped: (paragraphIndex: Int, charOffset: Int) -> Unit,
    onSelectVoice: (String) -> Unit,
    onSelectRate: (String) -> Unit,
    onSelectPitch: (String) -> Unit,
    onAdjustFontSize: (Float) -> Unit,
    onRefreshHealth: () -> Unit,
    onScrollPositionChanged: (firstVisibleParagraphIndex: Int, scrollOffsetPx: Int) -> Unit = { _, _ -> },
    onJumpToHeading: (headingParagraphIndex: Int) -> Unit = {}
) {
    val activeBookId = state.activeBook?.id
    val initialParagraphIdx = remember(activeBookId) {
        (state.activeBook?.lastParagraphIndex ?: state.currentParagraphIndex).coerceAtLeast(0)
    }
    val initialScrollOffset = remember(activeBookId) {
        (state.activeBook?.lastScrollOffset ?: 0).coerceAtLeast(0)
    }
    val listState = rememberLazyListState(
        initialFirstVisibleItemIndex = initialParagraphIdx,
        initialFirstVisibleItemScrollOffset = initialScrollOffset
    )
    val coroutineScope = rememberCoroutineScope()
    var bookmarkRestored by remember(activeBookId) { mutableStateOf(false) }
    var lastJumpedHeadingIndex by remember(activeBookId) { mutableIntStateOf(-1) }

    val headingIndices = remember(state.paragraphs) {
        val explicitHeadings = state.paragraphs.mapIndexedNotNull { idx, p ->
            if (p.kind == ParagraphKind.Title || p.kind == ParagraphKind.Subtitle) idx else null
        }
        if (explicitHeadings.isNotEmpty()) {
            explicitHeadings
        } else {
            state.paragraphs.mapIndexedNotNull { idx, p ->
                if (p.chapterOffset == 0) idx else null
            }
        }
    }

    var showVoiceSheet by remember { mutableStateOf(false) }
    var showChaptersDrawer by remember { mutableStateOf(false) }

    BackHandler(enabled = showChaptersDrawer) {
        showChaptersDrawer = false
    }

    val backgroundColor = MaterialTheme.colorScheme.background
    val fontColor = MaterialTheme.colorScheme.onBackground
    val highlightBackground = MaterialTheme.colorScheme.primaryContainer
    val highlightText = MaterialTheme.colorScheme.onPrimaryContainer

    LaunchedEffect(activeBookId, state.paragraphs.size) {
        if (state.paragraphs.isNotEmpty()) {
            val targetIdx = (state.activeBook?.lastParagraphIndex ?: state.currentParagraphIndex)
                .coerceIn(0, state.paragraphs.lastIndex)
            val targetOffset = (state.activeBook?.lastScrollOffset ?: 0).coerceAtLeast(0)
            listState.scrollToItem(targetIdx, targetOffset)
            bookmarkRestored = true
        }
    }

    LaunchedEffect(listState, bookmarkRestored) {
        if (!bookmarkRestored) return@LaunchedEffect
        snapshotFlow {
            Triple(
                listState.isScrollInProgress,
                listState.firstVisibleItemIndex,
                listState.firstVisibleItemScrollOffset
            )
        }
            .filter { (scrolling, _, _) -> !scrolling }
            .distinctUntilChanged()
            .collect { (_, index, offset) ->
                if (state.paragraphs.isNotEmpty()) {
                    lastJumpedHeadingIndex = -1
                    val currentParaVisible = state.activeWordTiming != null &&
                        listState.layoutInfo.visibleItemsInfo.any { it.index == state.currentParagraphIndex }
                    val effectiveIdx = if (currentParaVisible) state.currentParagraphIndex else index
                    val effectiveOffset = if (currentParaVisible) 0 else offset
                    onScrollPositionChanged(effectiveIdx, effectiveOffset)
                }
            }
    }

    LaunchedEffect(state.currentParagraphIndex, state.isPlaying) {
        if ((state.isPlaying || state.isSynthesizing) && state.currentParagraphIndex in state.paragraphs.indices) {
            val isAlreadyVisible = listState.layoutInfo.visibleItemsInfo.any {
                it.index == state.currentParagraphIndex
            }
            if (!isAlreadyVisible) {
                listState.animateScrollToItem(state.currentParagraphIndex)
            }
        }
    }

    Scaffold(
        contentWindowInsets = WindowInsets.safeDrawing,
        containerColor = backgroundColor,
        topBar = {
            AnimatedVisibility(
                visible = state.showReaderChrome,
                enter = slideInVertically { -it } + fadeIn(),
                exit = slideOutVertically { -it } + fadeOut()
            ) {
                TopAppBar(
                    colors = TopAppBarDefaults.topAppBarColors(
                        containerColor = MaterialTheme.colorScheme.surfaceContainer,
                        titleContentColor = MaterialTheme.colorScheme.onSurface,
                        actionIconContentColor = MaterialTheme.colorScheme.onSurface,
                        navigationIconContentColor = MaterialTheme.colorScheme.onSurface
                    ),
                    navigationIcon = {
                        IconButton(
                            onClick = {
                                if (bookmarkRestored && state.paragraphs.isNotEmpty()) {
                                    val currentParaVisible = state.activeWordTiming != null &&
                                        listState.layoutInfo.visibleItemsInfo.any { it.index == state.currentParagraphIndex }
                                    val effectiveIdx = if (currentParaVisible) state.currentParagraphIndex else listState.firstVisibleItemIndex
                                    val effectiveOffset = if (currentParaVisible) 0 else listState.firstVisibleItemScrollOffset
                                    onScrollPositionChanged(effectiveIdx, effectiveOffset)
                                }
                                onBackToLibrary()
                            }
                        ) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                        }
                    },
                    title = {
                        Text(
                            text = state.activeBook?.title ?: "",
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            style = MaterialTheme.typography.titleMedium
                        )
                    },
                    actions = {
                        if (headingIndices.isNotEmpty()) {
                            IconButton(onClick = { showChaptersDrawer = true }) {
                                Icon(
                                    imageVector = Icons.AutoMirrored.Filled.FormatListBulleted,
                                    contentDescription = null
                                )
                            }
                        }
                    }
                )
            }
        },
        bottomBar = {
            AnimatedVisibility(
                visible = state.showReaderChrome,
                enter = slideInVertically { it } + fadeIn(),
                exit = slideOutVertically { it } + fadeOut()
            ) {
                NuvolaReaderBottomBar(
                    state = state,
                    onTogglePlayPause = onTogglePlayPause,
                    onOpenVoiceSheet = { showVoiceSheet = true },
                    onSelectRate = onSelectRate
                )
            }
        }
    ) { innerPadding ->
        if (state.paragraphs.isEmpty()) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(innerPadding),
                contentAlignment = Alignment.Center
            ) {
                Text("No readable content in this book.", color = fontColor)
            }
        } else {
            Box(modifier = Modifier.fillMaxSize()) {
                LazyColumn(
                    state = listState,
                    contentPadding = PaddingValues(
                        start = state.typography.pageHorizontalPadding,
                        end = state.typography.pageHorizontalPadding,
                        top = innerPadding.calculateTopPadding() + state.typography.pageVerticalPadding,
                        bottom = innerPadding.calculateBottomPadding() + state.typography.pageVerticalPadding + 64.dp
                    ),
                    modifier = Modifier.fillMaxSize()
                ) {

                    itemsIndexed(
                        items = state.paragraphs,
                        key = { _, p -> p.id }
                    ) { pIndex, paragraph ->
                        val isCurrentParagraph = pIndex == state.currentParagraphIndex
                        val isActiveParagraph = isCurrentParagraph && (
                            state.isPlaying || state.isSynthesizing || state.activeWordTiming != null
                        )
                        InteractiveParagraphItem(
                            paragraph = paragraph,
                            typography = state.typography,
                            fontColor = fontColor,
                            highlightBackground = highlightBackground,
                            highlightText = highlightText,
                            isActiveParagraph = isActiveParagraph,
                            activeWordTiming = if (isCurrentParagraph) state.activeWordTiming else null,
                            onWordClicked = { charOffset ->
                                onWordTapped(pIndex, charOffset)
                            }
                        )
                        Spacer(modifier = Modifier.height(state.typography.paragraphSpacing))
                    }
                }

                if (headingIndices.isNotEmpty()) {
                    Surface(
                        onClick = {
                            val currentAnchor = max(listState.firstVisibleItemIndex, lastJumpedHeadingIndex)
                            val nextHeadingIdx = headingIndices.firstOrNull { it > currentAnchor }
                                ?: headingIndices.first()
                            lastJumpedHeadingIndex = nextHeadingIdx
                            onJumpToHeading(nextHeadingIdx)
                            coroutineScope.launch {
                                listState.animateScrollToItem(nextHeadingIdx, 0)
                            }
                        },
                        shape = CircleShape,
                        color = MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.92f),
                        contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
                        tonalElevation = 4.dp,
                        shadowElevation = 6.dp,
                        modifier = Modifier
                            .align(Alignment.BottomEnd)
                            .padding(
                                end = 18.dp,
                                bottom = innerPadding.calculateBottomPadding() + 18.dp
                            )
                            .size(46.dp)
                    ) {
                        Box(contentAlignment = Alignment.Center, modifier = Modifier.fillMaxSize()) {
                            Icon(
                                imageVector = Icons.Default.KeyboardArrowDown,
                                contentDescription = null,
                                modifier = Modifier.size(26.dp)
                            )
                        }
                    }
                }
            }
        }
    }

    val currentAnchorParagraphIndex by remember {
        derivedStateOf {
            if (state.isPlaying || state.isSynthesizing) {
                state.currentParagraphIndex
            } else {
                listState.firstVisibleItemIndex
            }
        }
    }

    ChaptersRightSideDrawer(
        visible = showChaptersDrawer,
        bookTitle = state.activeBook?.title ?: "Chapters",
        paragraphs = state.paragraphs,
        headingIndices = headingIndices,
        currentParagraphIndex = currentAnchorParagraphIndex,
        onDismiss = { showChaptersDrawer = false },
        onSelectHeading = { targetIdx ->
            showChaptersDrawer = false
            lastJumpedHeadingIndex = targetIdx
            onJumpToHeading(targetIdx)
            coroutineScope.launch {
                listState.animateScrollToItem(targetIdx, 0)
            }
        }
    )

    if (showVoiceSheet) {
        var selectedLanguageTab by remember(state.selectedVoice) {
            mutableStateOf(if (state.selectedVoice.startsWith("it-", ignoreCase = true)) "it" else "en")
        }
        val filteredVoices = remember(state.voicePresets, selectedLanguageTab) {
            state.voicePresets.filter { preset ->
                preset.locale.startsWith(selectedLanguageTab, ignoreCase = true)
            }
        }
        val tonePresets = remember {
            listOf(
                "+0Hz" to "Natural",
                "-3Hz" to "Warm Book",
                "-6Hz" to "Deep",
                "+3Hz" to "Bright"
            )
        }

        ModalBottomSheet(onDismissRequest = { showVoiceSheet = false }) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .navigationBarsPadding()
                    .padding(horizontal = 20.dp, vertical = 12.dp)
            ) {
                Text("Neural Voices", style = MaterialTheme.typography.titleMedium)
                Spacer(Modifier.height(10.dp))

                Surface(
                    shape = RoundedCornerShape(14.dp),
                    color = MaterialTheme.colorScheme.surfaceContainerHighest,
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(4.dp),
                        horizontalArrangement = Arrangement.spacedBy(6.dp)
                    ) {
                        val tabs = listOf("en" to "English", "it" to "Italiano")
                        tabs.forEach { (langCode, label) ->
                            val active = selectedLanguageTab == langCode
                            Surface(
                                shape = RoundedCornerShape(10.dp),
                                color = if (active) {
                                    MaterialTheme.colorScheme.primaryContainer
                                } else {
                                    Color.Transparent
                                },
                                modifier = Modifier
                                    .weight(1f)
                                    .clickable { selectedLanguageTab = langCode }
                            ) {
                                Box(
                                    contentAlignment = Alignment.Center,
                                    modifier = Modifier.padding(vertical = 8.dp)
                                ) {
                                    Text(
                                        text = label,
                                        style = MaterialTheme.typography.labelLarge,
                                        fontWeight = if (active) FontWeight.SemiBold else FontWeight.Normal,
                                        color = if (active) {
                                            MaterialTheme.colorScheme.onPrimaryContainer
                                        } else {
                                            MaterialTheme.colorScheme.onSurfaceVariant
                                        }
                                    )
                                }
                            }
                        }
                    }
                }

                Spacer(Modifier.height(12.dp))
                Text(
                    text = "Naturalness",
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                Spacer(Modifier.height(6.dp))
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(6.dp)
                ) {
                    tonePresets.forEach { (pitchValue, label) ->
                        val isActiveTone = state.selectedPitch == pitchValue
                        Surface(
                            shape = RoundedCornerShape(10.dp),
                            color = if (isActiveTone) {
                                MaterialTheme.colorScheme.secondaryContainer
                            } else {
                                MaterialTheme.colorScheme.surfaceContainerHigh
                            },
                            modifier = Modifier
                                .weight(1f)
                                .clickable { onSelectPitch(pitchValue) }
                        ) {
                            Box(
                                contentAlignment = Alignment.Center,
                                modifier = Modifier.padding(vertical = 7.dp, horizontal = 4.dp)
                            ) {
                                Text(
                                    text = label,
                                    maxLines = 1,
                                    style = MaterialTheme.typography.labelSmall,
                                    fontWeight = if (isActiveTone) FontWeight.SemiBold else FontWeight.Normal,
                                    color = if (isActiveTone) {
                                        MaterialTheme.colorScheme.onSecondaryContainer
                                    } else {
                                        MaterialTheme.colorScheme.onSurfaceVariant
                                    }
                                )
                            }
                        }
                    }
                }

                Spacer(Modifier.height(12.dp))
                LazyColumn(modifier = Modifier.height(260.dp)) {
                    itemsIndexed(filteredVoices) { _, voicePreset ->
                        val isSelected = voicePreset.shortName == state.selectedVoice
                        val isNaturalPlus = voicePreset.shortName.contains("Multilingual", ignoreCase = true)
                        val containerColor = if (isSelected) {
                            MaterialTheme.colorScheme.primaryContainer
                        } else {
                            MaterialTheme.colorScheme.surfaceContainerLow
                        }
                        val primaryTextColor = if (isSelected) {
                            MaterialTheme.colorScheme.onPrimaryContainer
                        } else {
                            MaterialTheme.colorScheme.onSurface
                        }
                        val secondaryTextColor = if (isSelected) {
                            MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.85f)
                        } else {
                            MaterialTheme.colorScheme.onSurfaceVariant
                        }
                        Surface(
                            color = containerColor,
                            contentColor = primaryTextColor,
                            shape = RoundedCornerShape(10.dp),
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(vertical = 4.dp)
                                .clickable {
                                    onSelectVoice(voicePreset.shortName)
                                    showVoiceSheet = false
                                }
                        ) {
                            Column(modifier = Modifier.padding(12.dp)) {
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    horizontalArrangement = Arrangement.SpaceBetween,
                                    verticalAlignment = Alignment.CenterVertically
                                ) {
                                    Row(
                                        modifier = Modifier
                                            .weight(1f)
                                            .padding(end = 8.dp),
                                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                                        verticalAlignment = Alignment.CenterVertically
                                    ) {
                                        Text(
                                            text = voicePreset.displayName,
                                            color = primaryTextColor,
                                            fontWeight = FontWeight.SemiBold,
                                            maxLines = 1,
                                            overflow = TextOverflow.Ellipsis
                                        )
                                        if (isNaturalPlus) {
                                            Surface(
                                                shape = RoundedCornerShape(6.dp),
                                                color = Color(0xFF00695C),
                                                contentColor = Color(0xFFE0F2F1)
                                            ) {
                                                Text(
                                                    text = "✦ Natural+",
                                                    style = MaterialTheme.typography.labelSmall,
                                                    fontWeight = FontWeight.Bold,
                                                    maxLines = 1,
                                                    softWrap = false,
                                                    color = Color(0xFFE0F2F1),
                                                    modifier = Modifier.padding(horizontal = 7.dp, vertical = 2.dp)
                                                )
                                            }
                                        }
                                    }
                                    Text(
                                        text = "${voicePreset.locale} · ${voicePreset.gender}",
                                        style = MaterialTheme.typography.labelSmall,
                                        maxLines = 1,
                                        softWrap = false,
                                        color = secondaryTextColor
                                    )
                                }
                                Text(
                                    text = voicePreset.description,
                                    style = MaterialTheme.typography.bodySmall,
                                    color = secondaryTextColor
                                )
                            }
                        }
                    }
                }
                Spacer(Modifier.height(16.dp))
            }
        }
    }

}

@Composable
private fun InteractiveParagraphItem(
    paragraph: ParagraphData,
    typography: ReaderTypography,
    fontColor: Color,
    highlightBackground: Color,
    highlightText: Color,
    isActiveParagraph: Boolean,
    activeWordTiming: WordAudioTiming?,
    onWordClicked: (charOffset: Int) -> Unit
) {
    val highlightRange: IntRange? = remember(paragraph.id, paragraph.text.length, activeWordTiming) {
        if (activeWordTiming != null && paragraph.text.isNotEmpty()) {
            val start = activeWordTiming.charStart.coerceIn(0, paragraph.text.length)
            val end = activeWordTiming.charEnd.coerceIn(start, paragraph.text.length)
            if (start < end) start until end else null
        } else {
            null
        }
    }

    val annotatedText = remember(paragraph.id, paragraph.text, highlightRange, highlightText) {
        buildAnnotatedString {
            append(paragraph.text)
            if (highlightRange != null) {
                addStyle(
                    style = SpanStyle(
                        color = highlightText,
                        fontWeight = FontWeight.Bold
                    ),
                    start = highlightRange.first,
                    end = highlightRange.last + 1
                )
            }
        }
    }

    val baseStyle = typography.paragraphStyle(fontColor)
    val paragraphContainerColor = if (isActiveParagraph) {
        MaterialTheme.colorScheme.secondaryContainer.copy(alpha = 0.32f)
    } else {
        Color.Transparent
    }

    Box(
        modifier = Modifier
            .fillMaxWidth()
            .then(
                if (isActiveParagraph) {
                    Modifier
                        .clip(RoundedCornerShape(10.dp))
                        .background(paragraphContainerColor)
                        .padding(horizontal = 10.dp, vertical = 6.dp)
                } else {
                    Modifier
                }
            )
    ) {
        when (paragraph.kind) {
            com.nuvola.reader.bridge.ParagraphKind.Title -> {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 14.dp, bottom = 6.dp)
                ) {
                    RoundedHighlightClickableText(
                        text = annotatedText,
                        highlightRange = highlightRange,
                        highlightBackground = highlightBackground,
                        style = baseStyle.copy(
                            fontSize = baseStyle.fontSize * 1.38f,
                            lineHeight = baseStyle.lineHeight * 1.22f,
                            fontWeight = FontWeight.Bold,
                            color = MaterialTheme.colorScheme.onBackground
                        ),
                        onClick = onWordClicked
                    )
                }
            }
            com.nuvola.reader.bridge.ParagraphKind.Subtitle -> {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 8.dp, bottom = 2.dp)
                ) {
                    RoundedHighlightClickableText(
                        text = annotatedText,
                        highlightRange = highlightRange,
                        highlightBackground = highlightBackground,
                        style = baseStyle.copy(
                            fontSize = baseStyle.fontSize * 1.15f,
                            lineHeight = baseStyle.lineHeight * 1.15f,
                            fontWeight = FontWeight.SemiBold,
                            color = MaterialTheme.colorScheme.primary
                        ),
                        onClick = onWordClicked
                    )
                }
            }
            com.nuvola.reader.bridge.ParagraphKind.Quote -> {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(vertical = 4.dp)
                ) {
                    Box(
                        modifier = Modifier
                            .width(3.dp)
                            .height(24.dp)
                            .background(
                                color = MaterialTheme.colorScheme.primary.copy(alpha = 0.5f),
                                shape = RoundedCornerShape(2.dp)
                            )
                    )
                    Spacer(modifier = Modifier.width(12.dp))
                    RoundedHighlightClickableText(
                        text = annotatedText,
                        highlightRange = highlightRange,
                        highlightBackground = highlightBackground,
                        style = baseStyle.copy(
                            fontStyle = androidx.compose.ui.text.font.FontStyle.Italic,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        ),
                        modifier = Modifier.weight(1f),
                        onClick = onWordClicked
                    )
                }
            }
            com.nuvola.reader.bridge.ParagraphKind.Footnote -> {
                Surface(
                    shape = RoundedCornerShape(8.dp),
                    color = MaterialTheme.colorScheme.surfaceContainerLow,
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(vertical = 2.dp)
                ) {
                    Column(modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp)) {
                        Text(
                            text = "Note",
                            style = MaterialTheme.typography.labelSmall,
                            fontWeight = FontWeight.SemiBold,
                            color = MaterialTheme.colorScheme.primary.copy(alpha = 0.8f)
                        )
                        Spacer(modifier = Modifier.height(2.dp))
                        RoundedHighlightClickableText(
                            text = annotatedText,
                            highlightRange = highlightRange,
                            highlightBackground = highlightBackground,
                            style = baseStyle.copy(
                                fontSize = baseStyle.fontSize * 0.84f,
                                lineHeight = baseStyle.lineHeight * 0.88f,
                                color = MaterialTheme.colorScheme.onSurfaceVariant
                            ),
                            onClick = onWordClicked
                        )
                    }
                }
            }
            com.nuvola.reader.bridge.ParagraphKind.Body -> {
                RoundedHighlightClickableText(
                    text = annotatedText,
                    highlightRange = highlightRange,
                    highlightBackground = highlightBackground,
                    style = baseStyle,
                    onClick = onWordClicked
                )
            }
        }
    }
}

@Composable
private fun RoundedHighlightClickableText(
    text: AnnotatedString,
    highlightRange: IntRange?,
    highlightBackground: Color,
    style: TextStyle,
    modifier: Modifier = Modifier,
    onClick: (charOffset: Int) -> Unit
) {
    var layoutResult by remember { mutableStateOf<TextLayoutResult?>(null) }

    Text(
        text = text,
        style = style,
        onTextLayout = { layoutResult = it },
        modifier = modifier
            .drawBehind {
                val layout = layoutResult ?: return@drawBehind
                val range = highlightRange ?: return@drawBehind
                val textLen = text.length
                if (textLen == 0) return@drawBehind

                val startIdx = range.first.coerceIn(0, textLen - 1)
                val endIdx = range.last.coerceIn(startIdx, textLen - 1)

                val hPad = 3.5.dp.toPx()
                val vPad = 1.dp.toPx()
                val radius = 6.dp.toPx()

                var currentBox: Rect? = null
                for (i in startIdx..endIdx) {
                    val charBox = layout.getBoundingBox(i)
                    if (charBox.width <= 0f && charBox.height <= 0f) continue
                    val existing = currentBox
                    if (existing == null) {
                        currentBox = charBox
                    } else if (kotlin.math.abs(charBox.top - existing.top) < existing.height * 0.5f) {
                        currentBox = Rect(
                            left = min(existing.left, charBox.left),
                            top = min(existing.top, charBox.top),
                            right = max(existing.right, charBox.right),
                            bottom = max(existing.bottom, charBox.bottom)
                        )
                    } else {
                        drawRoundRect(
                            color = highlightBackground,
                            topLeft = Offset(existing.left - hPad, existing.top - vPad),
                            size = Size(existing.width + hPad * 2f, existing.height + vPad * 2f),
                            cornerRadius = CornerRadius(radius, radius)
                        )
                        currentBox = charBox
                    }
                }

                currentBox?.let { box ->
                    drawRoundRect(
                        color = highlightBackground,
                        topLeft = Offset(box.left - hPad, box.top - vPad),
                        size = Size(box.width + hPad * 2f, box.height + vPad * 2f),
                        cornerRadius = CornerRadius(radius, radius)
                    )
                }
            }
            .pointerInput(text) {
                detectTapGestures { tapOffset ->
                    layoutResult?.getOffsetForPosition(tapOffset)?.let(onClick)
                }
            }
    )
}

@Composable
private fun NuvolaReaderBottomBar(
    state: ReaderUiState,
    onTogglePlayPause: () -> Unit,
    onOpenVoiceSheet: () -> Unit,
    onSelectRate: (String) -> Unit
) {
    val activeVoiceLabel = remember(state.selectedVoice, state.voicePresets) {
        val preset = state.voicePresets.firstOrNull { it.shortName == state.selectedVoice }
        val langTag = state.selectedVoice.substringBefore("-").uppercase(Locale.US)
        val rawName = preset?.displayName?.substringBefore(" (")
            ?: state.selectedVoice.substringAfterLast("-").removeSuffix("MultilingualNeural").removeSuffix("Neural")
        "$rawName ($langTag)"
    }

    Surface(
        color = MaterialTheme.colorScheme.surfaceContainer,
        tonalElevation = 3.dp
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
            modifier = Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
                .padding(horizontal = 16.dp, vertical = 10.dp)
        ) {
            FilterChip(
                selected = false,
                onClick = onOpenVoiceSheet,
                leadingIcon = {
                    Icon(
                        Icons.Default.RecordVoiceOver,
                        contentDescription = null,
                        modifier = Modifier.size(16.dp)
                    )
                },
                label = {
                    Text(
                        text = activeVoiceLabel,
                        maxLines = 1
                    )
                }
            )

            FilledIconButton(
                onClick = onTogglePlayPause,
                modifier = Modifier.size(48.dp)
            ) {
                if (state.isSynthesizing) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(20.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.onPrimary
                    )
                } else {
                    Icon(
                        imageVector = if (state.isPlaying) Icons.Default.Pause else Icons.Default.PlayArrow,
                        contentDescription = if (state.isPlaying) "Pause" else "Play"
                    )
                }
            }

            SpeedStepperControl(
                selectedRate = state.selectedRate,
                onSelectRate = onSelectRate
            )
        }
    }
}

@Composable
private fun SpeedStepperControl(
    selectedRate: String,
    onSelectRate: (String) -> Unit
) {
    val currentPct = remember(selectedRate) {
        selectedRate.trim().removeSuffix("%").removePrefix("+").toIntOrNull() ?: 0
    }
    val speedMultiplier = (1.0f + currentPct / 100f).coerceIn(0.5f, 2.0f)
    val speedLabel = String.format(Locale.US, "%.1fx", speedMultiplier)

    fun formatRatePct(pct: Int): String {
        val clamped = pct.coerceIn(-50, 100)
        return if (clamped >= 0) "+$clamped%" else "$clamped%"
    }

    Surface(
        shape = RoundedCornerShape(50),
        color = MaterialTheme.colorScheme.surfaceContainerHighest,
        tonalElevation = 2.dp
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.padding(horizontal = 4.dp, vertical = 2.dp)
        ) {
            IconButton(
                enabled = currentPct > -50,
                onClick = { onSelectRate(formatRatePct(currentPct - 10)) },
                modifier = Modifier.size(32.dp)
            ) {
                Icon(
                    imageVector = Icons.Default.Remove,
                    contentDescription = "Slower (-0.1x)",
                    modifier = Modifier.size(16.dp)
                )
            }

            Text(
                text = speedLabel,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.SemiBold,
                textAlign = TextAlign.Center,
                color = if (currentPct != 0) {
                    MaterialTheme.colorScheme.primary
                } else {
                    MaterialTheme.colorScheme.onSurface
                },
                modifier = Modifier
                    .clickable { onSelectRate("+0%") }
                    .padding(horizontal = 6.dp)
            )

            IconButton(
                enabled = currentPct < 100,
                onClick = { onSelectRate(formatRatePct(currentPct + 10)) },
                modifier = Modifier.size(32.dp)
            ) {
                Icon(
                    imageVector = Icons.Default.Add,
                    contentDescription = "Faster (+0.1x)",
                    modifier = Modifier.size(16.dp)
                )
            }
        }
    }
}

private data class ChapterTocEntry(
    val paragraphIndex: Int,
    val title: String,
    val isSubchapter: Boolean
)

@Composable
private fun ChaptersRightSideDrawer(
    visible: Boolean,
    bookTitle: String,
    paragraphs: List<ParagraphData>,
    headingIndices: List<Int>,
    currentParagraphIndex: Int,
    onDismiss: () -> Unit,
    onSelectHeading: (Int) -> Unit
) {
    val titleCount = remember(headingIndices, paragraphs) {
        headingIndices.count { idx -> paragraphs.getOrNull(idx)?.kind == ParagraphKind.Title }
    }
    val subtitleCount = remember(headingIndices, paragraphs) {
        headingIndices.count { idx -> paragraphs.getOrNull(idx)?.kind == ParagraphKind.Subtitle }
    }
    val hasMixedHierarchy = titleCount > 1 && subtitleCount > 0

    var showOnlyMainTitles by remember(hasMixedHierarchy) { mutableStateOf(false) }

    val entries = remember(headingIndices, paragraphs, hasMixedHierarchy, showOnlyMainTitles) {
        headingIndices.mapNotNull { idx ->
            val p = paragraphs.getOrNull(idx) ?: return@mapNotNull null
            if (hasMixedHierarchy && showOnlyMainTitles && p.kind == ParagraphKind.Subtitle) {
                return@mapNotNull null
            }
            val cleanText = p.text.replace(Regex("\\s+"), " ").trim()
            if (cleanText.isEmpty()) return@mapNotNull null
            ChapterTocEntry(
                paragraphIndex = idx,
                title = cleanText,
                isSubchapter = hasMixedHierarchy && p.kind == ParagraphKind.Subtitle
            )
        }
    }

    val activeEntryIndex = remember(entries, currentParagraphIndex) {
        val idx = entries.indexOfLast { it.paragraphIndex <= currentParagraphIndex }
        if (idx >= 0) idx else 0
    }

    val tocListState = rememberLazyListState()

    LaunchedEffect(visible, activeEntryIndex, entries.size) {
        if (visible && entries.isNotEmpty() && activeEntryIndex in entries.indices) {
            tocListState.scrollToItem((activeEntryIndex - 2).coerceAtLeast(0))
        }
    }

    Box(modifier = Modifier.fillMaxSize()) {

        AnimatedVisibility(
            visible = visible,
            enter = fadeIn(),
            exit = fadeOut()
        ) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(Color.Black.copy(alpha = 0.45f))
                    .clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        onClick = onDismiss
                    )
            )
        }

        AnimatedVisibility(
            visible = visible,
            enter = slideInHorizontally(initialOffsetX = { fullWidth -> fullWidth }) + fadeIn(),
            exit = slideOutHorizontally(targetOffsetX = { fullWidth -> fullWidth }) + fadeOut(),
            modifier = Modifier.align(Alignment.CenterEnd)
        ) {
            Surface(
                color = MaterialTheme.colorScheme.surfaceContainerLow,
                contentColor = MaterialTheme.colorScheme.onSurface,
                shape = RoundedCornerShape(topStart = 22.dp, bottomStart = 22.dp),
                tonalElevation = 6.dp,
                shadowElevation = 16.dp,
                modifier = Modifier
                    .fillMaxHeight()
                    .widthIn(min = 280.dp, max = 330.dp)
                    .fillMaxWidth(0.84f)
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .statusBarsPadding()
                        .navigationBarsPadding()
                ) {

                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(start = 18.dp, end = 10.dp, top = 14.dp, bottom = 10.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                text = "Chapters",
                                style = MaterialTheme.typography.titleMedium,
                                fontWeight = FontWeight.Bold,
                                color = MaterialTheme.colorScheme.onSurface
                            )
                            Text(
                                text = "${entries.size} sections · $bookTitle",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis
                            )
                        }
                        IconButton(onClick = onDismiss) {
                            Icon(
                                imageVector = Icons.Default.Close,
                                contentDescription = "Close chapters"
                            )
                        }
                    }

                    if (hasMixedHierarchy) {
                        Surface(
                            shape = RoundedCornerShape(12.dp),
                            color = MaterialTheme.colorScheme.surfaceContainerHighest,
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 16.dp, vertical = 4.dp)
                        ) {
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(3.dp),
                                horizontalArrangement = Arrangement.spacedBy(4.dp)
                            ) {
                                val options = listOf(
                                    false to "All (${titleCount + subtitleCount})",
                                    true to "Titles Only ($titleCount)"
                                )
                                options.forEach { (onlyTitles, label) ->
                                    val selected = showOnlyMainTitles == onlyTitles
                                    Surface(
                                        shape = RoundedCornerShape(9.dp),
                                        color = if (selected) {
                                            MaterialTheme.colorScheme.primaryContainer
                                        } else {
                                            Color.Transparent
                                        },
                                        modifier = Modifier
                                            .weight(1f)
                                            .clip(RoundedCornerShape(9.dp))
                                            .clickable { showOnlyMainTitles = onlyTitles }
                                    ) {
                                        Box(
                                            contentAlignment = Alignment.Center,
                                            modifier = Modifier.padding(vertical = 6.dp, horizontal = 6.dp)
                                        ) {
                                            Text(
                                                text = label,
                                                style = MaterialTheme.typography.labelMedium,
                                                fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
                                                maxLines = 1,
                                                color = if (selected) {
                                                    MaterialTheme.colorScheme.onPrimaryContainer
                                                } else {
                                                    MaterialTheme.colorScheme.onSurfaceVariant
                                                }
                                            )
                                        }
                                    }
                                }
                            }
                        }
                        Spacer(Modifier.height(6.dp))
                    }

                    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f))

                    LazyColumn(
                        state = tocListState,
                        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 10.dp),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                        modifier = Modifier.weight(1f)
                    ) {
                        itemsIndexed(
                            items = entries,
                            key = { _, item -> item.paragraphIndex }
                        ) { index, entry ->
                            val isCurrent = index == activeEntryIndex
                            val itemContainerColor = if (isCurrent) {
                                MaterialTheme.colorScheme.primaryContainer
                            } else {
                                Color.Transparent
                            }
                            val itemTextColor = if (isCurrent) {
                                MaterialTheme.colorScheme.onPrimaryContainer
                            } else if (entry.isSubchapter) {
                                MaterialTheme.colorScheme.onSurfaceVariant
                            } else {
                                MaterialTheme.colorScheme.onSurface
                            }

                            Surface(
                                color = itemContainerColor,
                                contentColor = itemTextColor,
                                shape = RoundedCornerShape(10.dp),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clip(RoundedCornerShape(10.dp))
                                    .clickable { onSelectHeading(entry.paragraphIndex) }
                            ) {
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .padding(
                                            start = if (entry.isSubchapter) 24.dp else 12.dp,
                                            end = 12.dp,
                                            top = if (entry.isSubchapter) 8.dp else 11.dp,
                                            bottom = if (entry.isSubchapter) 8.dp else 11.dp
                                        )
                                ) {
                                    Text(
                                        text = if (entry.isSubchapter) "↳" else "${index + 1}.",
                                        style = MaterialTheme.typography.labelSmall,
                                        fontWeight = if (isCurrent) FontWeight.Bold else FontWeight.Medium,
                                        color = if (isCurrent) {
                                            MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.85f)
                                        } else {
                                            MaterialTheme.colorScheme.primary.copy(alpha = 0.75f)
                                        },
                                        modifier = Modifier.width(26.dp)
                                    )
                                    Text(
                                        text = entry.title,
                                        style = if (entry.isSubchapter) {
                                            MaterialTheme.typography.bodySmall
                                        } else {
                                            MaterialTheme.typography.bodyMedium
                                        },
                                        fontWeight = when {
                                            isCurrent -> FontWeight.Bold
                                            entry.isSubchapter -> FontWeight.Normal
                                            else -> FontWeight.SemiBold
                                        },
                                        color = itemTextColor,
                                        maxLines = 2,
                                        overflow = TextOverflow.Ellipsis,
                                        modifier = Modifier.weight(1f)
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
