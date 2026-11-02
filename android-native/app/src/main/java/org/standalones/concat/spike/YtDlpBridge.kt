package app.cdcedit.editor.spike

import android.content.Context
import android.util.Log
import com.yausername.ffmpeg.FFmpeg
import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
import com.yausername.youtubedl_android.mapper.VideoInfo
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private const val TAG = "concat-ytdlp"

data class StreamInfo(
    val title: String,
    val duration: Double,
    val thumbnail: String,
    val fileSize: Long,
    val heights: List<Int>,
)

object YtDlpBridge {
    @Volatile
    private var initialized = false

    fun init(context: Context) {
        if (initialized) return
        try {
            YoutubeDL.getInstance().init(context)
            FFmpeg.getInstance().init(context)
            initialized = true
            Log.i(TAG, "youtubedl-android and ffmpeg initialized successfully")

            CoroutineScope(Dispatchers.IO).launch {
                try {
                    val status = YoutubeDL.getInstance().updateYoutubeDL(context)
                    Log.i(TAG, "yt-dlp auto-update: $status")
                } catch (e: Exception) {
                    Log.w(TAG, "yt-dlp background update failed: ${e.message}")
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to initialize youtubedl-android", e)
        }
    }

    suspend fun update(context: Context): Result<String> = withContext(Dispatchers.IO) {
        try {
            val status = YoutubeDL.getInstance().updateYoutubeDL(context)
            Result.success(status?.name ?: "DONE")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to update yt-dlp", e)
            Result.failure(e)
        }
    }

    suspend fun inspect(url: String): Result<StreamInfo> = withContext(Dispatchers.IO) {
        try {
            val cleanUrl = url.trim()
            val isYouTube = cleanUrl.contains("youtube.com") || cleanUrl.contains("youtu.be")
            val request = YoutubeDLRequest(cleanUrl).apply {
                addOption("--no-check-certificates")
                addOption("--no-warnings")
                if (isYouTube) {
                    addOption("--extractor-args", "youtube:player_client=android,web")
                } else {
                    addOption("--user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                }
                addOption("--socket-timeout", "20")
            }
            val info: VideoInfo = try {
                YoutubeDL.getInstance().getInfo(request)
            } catch (_: Exception) {
                // Fallback to simple getInfo
                YoutubeDL.getInstance().getInfo(cleanUrl)
            }
            val heights = info.formats?.mapNotNull {
                try { it.height?.toInt() } catch (_: Exception) { null }
            }?.filter { it > 0 }?.distinct()?.sorted() ?: emptyList()
            val durationSec = try {
                info.duration.toDouble()
            } catch (_: Exception) {
                0.0
            }
            val rawTitle = info.title ?: "Media Video"
            val stream = StreamInfo(
                title = cleanTitle(rawTitle),
                duration = durationSec,
                thumbnail = info.thumbnail ?: "",
                fileSize = info.fileSize ?: 0L,
                heights = heights,
            )
            Result.success(stream)
        } catch (e: Exception) {
            Log.e(TAG, "Failed to inspect $url", e)
            val msg = e.message ?: e.cause?.message ?: "Không thể đọc dữ liệu video"
            Result.failure(Exception(msg, e))
        }
    }

    fun cleanTitle(raw: String): String {
        val noTags = raw.replace(Regex("#[\\w\\p{L}]+"), "")
        val sanitized = noTags.replace(Regex("[\\\\/:*?\"<>|]"), "")
            .replace(Regex("\\s+"), " ")
            .trim()
            .trim('.', ' ', '-')
        return if (sanitized.isNotBlank()) sanitized.take(70) else "video_${System.currentTimeMillis() % 10000}"
    }

    suspend fun download(
        url: String,
        destDir: File,
        customName: String? = null,
        quality: Int = 0,
        audioOnly: Boolean = false,
        onProgress: (Float, String) -> Unit,
    ): Result<File> = withContext(Dispatchers.IO) {
        try {
            if (!destDir.exists()) destDir.mkdirs()

            val cleanUrl = url.trim()
            val isYouTube = cleanUrl.contains("youtube.com") || cleanUrl.contains("youtu.be")
            val outputName = if (!customName.isNullOrBlank()) {
                cleanTitle(customName)
            } else {
                "%(title).70B"
            }
            val request = YoutubeDLRequest(cleanUrl).apply {
                addOption("--no-mtime")
                addOption("--no-check-certificates")
                if (isYouTube) {
                    addOption("--extractor-args", "youtube:player_client=android,web")
                } else {
                    addOption("--user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                }
                addOption("-o", "${destDir.absolutePath}/$outputName.%(ext)s")
                if (audioOnly) {
                    addOption("-f", "bestaudio/best")
                    addOption("--extract-audio")
                    addOption("--audio-format", "m4a")
                } else {
                    val format = if (quality == 0) {
                        "bestvideo+bestaudio/best"
                    } else {
                        "bestvideo[height<=$quality]+bestaudio/best[height<=$quality]/best"
                    }
                    addOption("-f", format)
                    addOption("--merge-output-format", "mp4")
                }
            }

            val beforeFiles = destDir.listFiles()?.toSet() ?: emptySet()

            YoutubeDL.getInstance().execute(request) { progress, etaInSeconds, line ->
                val msg = if (etaInSeconds > 0) "${progress.toInt()}% · ETA ${etaInSeconds}s" else "${progress.toInt()}%"
                onProgress(progress / 100f, msg)
            }

            val afterFiles = destDir.listFiles()?.toSet() ?: emptySet()
            val newFiles = afterFiles - beforeFiles
            val downloadedFile = newFiles.firstOrNull { it.isFile && (it.extension == "mp4" || it.extension == "m4a" || it.extension == "mkv" || it.extension == "webm") }
                ?: destDir.listFiles()?.filter { it.isFile }?.maxByOrNull { it.lastModified() }

            if (downloadedFile != null && downloadedFile.exists()) {
                Result.success(downloadedFile)
            } else {
                Result.failure(IllegalStateException("No downloaded media file found in $destDir"))
            }
        } catch (e: Exception) {
            Log.e(TAG, "Download failed for $url", e)
            Result.failure(e)
        }
    }
}
