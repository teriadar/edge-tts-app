package com.nuvola.reader.ui.theme

import android.app.Activity
import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.view.WindowCompat

val ChapterTitleExtraSpacing: Dp = 14.dp

data class ReaderTypography(
    val fontSizeSp: Float = 18f,
    val lineHeightMultiplier: Float = 1.55f,
    val paragraphSpacing: Dp = 14.dp,
    val pageHorizontalPadding: Dp = 20.dp,
    val pageVerticalPadding: Dp = 18.dp,
    val textAlign: TextAlign = TextAlign.Start,
    val fontFamily: FontFamily = FontFamily.Serif
) {
    val bodyFontSize: TextUnit get() = fontSizeSp.sp
    val bodyLineHeight: TextUnit get() = (fontSizeSp * lineHeightMultiplier).sp

    fun paragraphStyle(fontColor: Color): TextStyle = TextStyle(
        fontSize = bodyFontSize,
        lineHeight = bodyLineHeight,
        fontFamily = fontFamily,
        fontWeight = FontWeight.Normal,
        color = fontColor
    )

    fun chapterTitleStyle(fontColor: Color): TextStyle = TextStyle(
        fontSize = (fontSizeSp + 8f).sp,
        lineHeight = ((fontSizeSp + 8f) * 1.3f).sp,
        fontFamily = fontFamily,
        fontWeight = FontWeight.ExtraBold,
        letterSpacing = 0.2.sp,
        color = fontColor
    )
}

@Composable
fun NuvolaReaderTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit
) {
    val context = LocalContext.current
    val colorScheme = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> {
            if (darkTheme) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        }
        darkTheme -> darkColorScheme()
        else -> lightColorScheme()
    }

    val view = LocalView.current
    if (!view.isInEditMode) {
        SideEffect {
            val window = (view.context as? Activity)?.window ?: return@SideEffect
            val insetsController = WindowCompat.getInsetsController(window, view)
            insetsController.isAppearanceLightStatusBars = !darkTheme
            insetsController.isAppearanceLightNavigationBars = !darkTheme
        }
    }

    MaterialTheme(
        colorScheme = colorScheme,
        content = content
    )
}
