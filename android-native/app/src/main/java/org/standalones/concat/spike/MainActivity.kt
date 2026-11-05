package app.cdcedit.editor.spike

import android.net.Uri
import android.os.Bundle
import android.os.SystemClock
import android.provider.OpenableColumns
import android.util.Log
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.FilterChip
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.concat_mobile.ClipView
import uniffi.concat_mobile.Engine
import uniffi.concat_mobile.Stats

private const val TAG = "concat-spike-ui"

class MainActivity : ComponentActivity() {
    private lateinit var engine: Engine

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val started = SystemClock.elapsedRealtime()
        ConcatNative.init(applicationContext)
        YtDlpBridge.init(applicationContext)
        val loaded = SystemClock.elapsedRealtime()
        engine = Engine(filesDir.absolutePath)
        val opened = SystemClock.elapsedRealtime()
        Log.i(TAG, "library loaded in ${loaded - started} ms, engine open in ${opened - loaded} ms")
        // Made by the app, so a file adb pushes into it is the app's to read.
        Log.i(TAG, "push test media to ${getExternalFilesDir(null)}")
        // For a run driven over adb: --es import <path> [--ez bench true] [--es mode vulkan|gl|cpu]
        val importPath = intent.getStringExtra("import")
        val bench = intent.getBooleanExtra("bench", false)
        val mode = intent.getStringExtra("mode") ?: "vulkan"
        setContent {
            MaterialTheme(colorScheme = darkColorScheme()) {
                Editor(engine, importPath, bench, mode, ::copyIn, File(filesDir, "media"))
            }
        }
    }

    /** A picked document copied into the app's own storage: the engine reads paths. */
    private fun copyIn(uri: Uri): File {
        var name = "picked.mp4"
        contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) name = it.getString(0) ?: name
        }
        val media = File(filesDir, "media").apply { mkdirs() }
        val baseName = name.substringBeforeLast(".")
        val extension = if (name.contains(".")) name.substringAfterLast(".") else "mp4"
        var target = File(media, name)
        var counter = 1
        while (target.exists()) {
            target = File(media, "${baseName}_$counter.$extension")
            counter++
        }
        contentResolver.openInputStream(uri)!!.use { input ->
            target.outputStream().use { input.copyTo(it, 1 shl 20) }
        }
        return target
    }
}

private fun seconds(value: Double) = "%.2f".format(value)

@Composable
private fun Editor(
    engine: Engine,
    importPath: String?,
    bench: Boolean,
    initialMode: String,
    copyIn: (Uri) -> File,
    downloadDir: File,
) {
    val scope = rememberCoroutineScope()
    var clips by remember { mutableStateOf(engine.clips()) }
    var duration by remember { mutableStateOf(engine.duration()) }
    var video by remember { mutableStateOf(engine.video()) }
    var position by remember { mutableStateOf(0.0) }
    var playing by remember { mutableStateOf(false) }
    var stats by remember { mutableStateOf(Stats("", 0u, 0.0, 0.0, 0.0, 0u, 0.0, 0.0, 0.0, 0u, 0.0, "")) }
    var mode by remember { mutableStateOf(initialMode) }
    var message by remember { mutableStateOf("") }
    var scrubbing by remember { mutableStateOf(false) }
    var showLinkDialog by remember { mutableStateOf(false) }

    fun refresh() {
        clips = engine.clips()
        duration = engine.duration()
        video = engine.video()
    }

    suspend fun importFile(file: File) {
        val started = SystemClock.elapsedRealtime()
        try {
            withContext(Dispatchers.IO) { engine.importVideo(file.absolutePath) }
            refresh()
            message = "imported ${file.name} in ${SystemClock.elapsedRealtime() - started} ms"
        } catch (error: Exception) {
            message = "import failed: ${error.message}"
        }
        Log.i(TAG, message)
    }

    val picker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        if (uri != null) scope.launch {
            val started = SystemClock.elapsedRealtime()
            val file = withContext(Dispatchers.IO) { copyIn(uri) }
            Log.i(TAG, "copied ${file.name} (${file.length() / 1024} KB) in ${SystemClock.elapsedRealtime() - started} ms")
            importFile(file)
        }
    }

    // The playhead and the numbers, read from the engine at the display's pace.
    LaunchedEffect(Unit) {
        while (true) {
            if (!scrubbing) position = engine.position()
            playing = engine.isPlaying()
            stats = engine.stats()
            delay(33)
        }
    }

    LaunchedEffect(importPath, bench) {
        if (importPath != null && clips.isEmpty()) importFile(File(importPath))
        if (bench) runBench(engine)
    }

    val context = androidx.compose.ui.platform.LocalContext.current

    Column(
        Modifier.fillMaxSize().background(Color(0xFF111111)).safeDrawingPadding().padding(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        val aspect = if (video.height > 0u) video.width.toFloat() / video.height.toFloat() else 9f / 16f
        Box(
            Modifier.fillMaxWidth().weight(1f).background(Color.Black),
            contentAlignment = Alignment.Center,
        ) {
            // A new SurfaceView for a new renderer: a window that has had a
            // GPU API connected to it cannot be locked by the CPU path.
            key(mode) {
                AndroidView(
                    modifier = Modifier.aspectRatio(aspect, matchHeightConstraintsFirst = aspect < 1f),
                    factory = { context ->
                        SurfaceView(context).apply {
                            holder.addCallback(object : SurfaceHolder.Callback {
                                override fun surfaceCreated(holder: SurfaceHolder) {}
                                override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                                    val window = ConcatNative.windowFromSurface(holder.surface)
                                    if (window == 0L) return
                                    val started = SystemClock.elapsedRealtime()
                                    try {
                                        val described = engine.attachSurface(window.toULong(), width.toUInt(), height.toUInt(), mode)
                                        Log.i(TAG, "attached ${width}x$height in ${SystemClock.elapsedRealtime() - started} ms: $described")
                                    } catch (error: Exception) {
                                        message = "surface: ${error.message}"
                                        Log.e(TAG, message)
                                    }
                                }
                                override fun surfaceDestroyed(holder: SurfaceHolder) {
                                    engine.detachSurface()
                                }
                            })
                        }
                    },
                )
            }
        }

        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = {
                picker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.VideoOnly))
            }) { Text("Import") }
            Button(onClick = { showLinkDialog = true }) { Text("Tải link") }
            Button(onClick = { if (playing) engine.pause() else engine.play() }, enabled = duration > 0) {
                Text(if (playing) "Pause" else "Play")
            }
            Text("${seconds(position)} / ${seconds(duration)} s", color = Color.White)
        }

        if (showLinkDialog) {
            LinkDownloadDialog(
                onDismiss = { showLinkDialog = false },
                onDownloaded = { file ->
                    exportToPublicDownloads(context, file)
                    android.widget.Toast.makeText(
                        context,
                        "Đã tải xong: Lưu vào Download/CDCedit",
                        android.widget.Toast.LENGTH_LONG
                    ).show()
                    scope.launch { importFile(file) }
                },
                downloadDir = downloadDir,
            )
        }

        Slider(
            value = position.toFloat(),
            valueRange = 0f..maxOf(duration.toFloat(), 0.01f),
            onValueChange = {
                scrubbing = true
                position = it.toDouble()
                engine.seek(it.toDouble())
            },
            onValueChangeFinished = { scrubbing = false },
            enabled = duration > 0,
        )

        ClipStrip(clips, position, duration) { engine.seek(it) }

        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for (option in listOf("vulkan", "gl", "cpu")) {
                FilterChip(selected = mode == option, onClick = { mode = option }, label = { Text(option) })
            }
        }
        Text(
            buildString {
                append(stats.renderer.ifEmpty { "no surface" })
                append("\nplay ${"%.1f".format(stats.playFps)} fps, late ${stats.lateFrames}; ")
                append("decode ${"%.1f".format(stats.lastDecodeMs)} ms, draw ${"%.1f".format(stats.lastDrawMs)} ms")
                append("\nscrub last ${"%.0f".format(stats.lastScrubMs)} / mean ${"%.0f".format(stats.meanScrubMs)} / max ${"%.0f".format(stats.maxScrubMs)} ms over ${stats.scrubs}")
                if (stats.lastError.isNotEmpty()) append("\n${stats.lastError}")
                if (message.isNotEmpty()) append("\n$message")
            },
            color = Color(0xFFBBBBBB),
            fontSize = 11.sp,
            lineHeight = 13.sp,
        )
    }
}

@Composable
private fun LinkDownloadDialog(
    onDismiss: () -> Unit,
    onDownloaded: (File) -> Unit,
    downloadDir: File,
) {
    val context = androidx.compose.ui.platform.LocalContext.current
    val scope = rememberCoroutineScope()
    var url by remember { mutableStateOf("") }
    var inspecting by remember { mutableStateOf(false) }
    var downloading by remember { mutableStateOf(false) }
    var updating by remember { mutableStateOf(false) }
    var streamInfo by remember { mutableStateOf<StreamInfo?>(null) }
    var customName by remember { mutableStateOf("") }
    var selectedQuality by remember { mutableStateOf(0) }
    var audioOnly by remember { mutableStateOf(false) }
    var progress by remember { mutableStateOf(0f) }
    var statusText by remember { mutableStateOf("") }

    fun startDownload() {
        downloading = true
        progress = 0f
        statusText = "Bắt đầu tải..."
        scope.launch {
            val result = YtDlpBridge.download(
                url = url.trim(),
                destDir = downloadDir,
                customName = customName.ifBlank { null },
                quality = selectedQuality,
                audioOnly = audioOnly,
                onProgress = { p, msg ->
                    progress = p
                    statusText = msg
                },
            )
            downloading = false
            result.fold(
                onSuccess = { file ->
                    onDownloaded(file)
                    onDismiss()
                },
                onFailure = {
                    statusText = "Lỗi tải: ${it.message}"
                },
            )
        }
    }

    AlertDialog(
        onDismissRequest = { if (!downloading && !updating) onDismiss() },
        title = {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("Tải video từ link")
                TextButton(
                    onClick = {
                        updating = true
                        statusText = "Đang cập nhật yt-dlp từ GitHub..."
                        scope.launch {
                            val res = YtDlpBridge.update(context)
                            updating = false
                            res.fold(
                                onSuccess = { statusText = "Đã cập nhật yt-dlp lên bản mới nhất!" },
                                onFailure = { statusText = "Lỗi cập nhật: ${it.message}" },
                            )
                        }
                    },
                    enabled = !updating && !downloading,
                ) {
                    Text(if (updating) "Đang nâng cấp..." else "🔄 Cập nhật engine", fontSize = 11.sp)
                }
            }
        },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(
                    value = url,
                    onValueChange = {
                        url = it
                        streamInfo = null
                    },
                    label = { Text("YouTube / Facebook / TikTok link") },
                    modifier = Modifier.fillMaxWidth(),
                    enabled = !downloading && !inspecting,
                    singleLine = true,
                )

                if (inspecting) {
                    Text("Đang kiểm tra liên kết...", color = Color.Gray, fontSize = 12.sp)
                    LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
                }

                streamInfo?.let { info ->
                    OutlinedTextField(
                        value = customName,
                        onValueChange = { customName = it },
                        label = { Text("Tên video lưu (bạn có thể sửa cho gọn)") },
                        modifier = Modifier.fillMaxWidth(),
                        enabled = !downloading,
                        singleLine = true,
                    )
                    if (info.duration > 0) {
                        Text("Thời lượng: ${info.duration.toInt()}s", color = Color.Gray, fontSize = 12.sp)
                    }

                    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        listOf(0 to "Best", 1080 to "1080p", 720 to "720p", 480 to "480p").forEach { (q, label) ->
                            FilterChip(
                                selected = !audioOnly && selectedQuality == q,
                                onClick = {
                                    audioOnly = false
                                    selectedQuality = q
                                },
                                label = { Text(label) },
                                enabled = !downloading,
                            )
                        }
                    }
                    FilterChip(
                        selected = audioOnly,
                        onClick = { audioOnly = !audioOnly },
                        label = { Text("Chỉ âm thanh (m4a)") },
                        enabled = !downloading,
                    )
                }

                if (downloading) {
                    Text(statusText.ifEmpty { "Đang tải xuống..." }, fontSize = 12.sp)
                    LinearProgressIndicator(progress = { progress }, modifier = Modifier.fillMaxWidth())
                } else if (statusText.isNotEmpty()) {
                    Text(statusText, color = Color.Yellow, fontSize = 12.sp)
                }
            }
        },
        confirmButton = {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (streamInfo == null) {
                    if (statusText.isNotEmpty()) {
                        Button(
                            onClick = { startDownload() },
                            enabled = url.isNotBlank() && !downloading,
                        ) { Text("Tải trực tiếp") }
                    }
                    Button(
                        onClick = {
                            if (url.isNotBlank()) {
                                inspecting = true
                                statusText = ""
                                scope.launch {
                                    val result = YtDlpBridge.inspect(url.trim())
                                    inspecting = false
                                    result.fold(
                                        onSuccess = {
                                            streamInfo = it
                                            customName = it.title
                                        },
                                        onFailure = { statusText = "Lỗi kiểm tra: ${it.message}" },
                                    )
                                }
                            }
                        },
                        enabled = url.isNotBlank() && !inspecting && !downloading,
                    ) { Text("Kiểm tra") }
                } else {
                    Button(
                        onClick = { startDownload() },
                        enabled = !downloading,
                    ) { Text("Tải về") }
                }
            }
        },
        dismissButton = {
            if (!downloading) {
                TextButton(onClick = onDismiss) { Text("Đóng") }
            }
        },
    )
}

/** The timeline: one block per clip at 48dp a second, the playhead over it. Tap to seek. */
@Composable
private fun ClipStrip(clips: List<ClipView>, position: Double, duration: Double, seek: (Double) -> Unit) {
    val perSecond = 48.dp
    Box(
        Modifier.fillMaxWidth().height(56.dp).background(Color(0xFF1C1C1C)).horizontalScroll(rememberScrollState()),
    ) {
        Box(Modifier.width(perSecond * maxOf(duration.toFloat(), 1f)).fillMaxHeight()) {
            for (clip in clips) {
                Box(
                    Modifier
                        .offset(x = perSecond * clip.start.toFloat(), y = (clip.lane.toInt() * 4).dp)
                        .width(perSecond * clip.duration.toFloat())
                        .height(48.dp)
                        .padding(horizontal = 1.dp)
                        .background(Color(0xFF2F6FDE))
                        .border(1.dp, Color(0xFF9CC0FF)),
                ) {
                    Text(
                        clip.name,
                        color = Color.White,
                        fontSize = 11.sp,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(4.dp),
                    )
                }
            }
            Spacer(
                Modifier.offset(x = perSecond * position.toFloat()).width(2.dp).fillMaxHeight().background(Color.White),
            )
        }
    }
}

/** A scripted run, for numbers without a finger: jumps, a drag, then playback. */
private suspend fun runBench(engine: Engine) {
    delay(1500)
    val duration = engine.duration()
    if (duration <= 0) {
        Log.w("concat-spike-bench", "nothing on the timeline")
        return
    }
    fun report(label: String) {
        val s = engine.stats()
        Log.i(
            "concat-spike-bench",
            "$label: renderer='${s.renderer}' frames=${s.frames} scrubs=${s.scrubs} " +
                "scrub last=${"%.1f".format(s.lastScrubMs)} mean=${"%.1f".format(s.meanScrubMs)} max=${"%.1f".format(s.maxScrubMs)} ms " +
                "decode=${"%.1f".format(s.lastDecodeMs)} draw=${"%.1f".format(s.lastDrawMs)} ms " +
                "playFps=${"%.1f".format(s.playFps)} late=${s.lateFrames} audioPeak=${"%.3f".format(s.audioPeak)} error='${s.lastError}'",
        )
    }
    // Jumps: each seek waited out, the way a tap on the strip lands.
    engine.resetStats()
    delay(200)
    for (i in 0 until 30) {
        engine.seek(duration * ((i * 7) % 30) / 30.0)
        delay(250)
    }
    report("jumps")
    // A drag: a seek every 16 ms, as a finger on the slider sends them.
    engine.resetStats()
    delay(200)
    var at = 0.0
    while (at < minOf(duration, 4.0)) {
        engine.seek(at)
        at += 0.05
        delay(16)
    }
    delay(600)
    report("drag")
    // Playback for six seconds from the start.
    engine.seek(0.0)
    delay(300)
    engine.resetStats()
    delay(100)
    engine.play()
    delay(6000)
    report("play")
    engine.pause()
}

private fun exportToPublicDownloads(context: android.content.Context, sourceFile: File) {
    try {
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.Q) {
            val resolver = context.contentResolver
            val contentValues = android.content.ContentValues().apply {
                put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME, sourceFile.name)
                put(
                    android.provider.MediaStore.MediaColumns.MIME_TYPE,
                    if (sourceFile.extension.lowercase() == "m4a") "audio/mp4" else "video/mp4"
                )
                put(
                    android.provider.MediaStore.MediaColumns.RELATIVE_PATH,
                    "${android.os.Environment.DIRECTORY_DOWNLOADS}/CDCedit"
                )
                put(android.provider.MediaStore.MediaColumns.IS_PENDING, 1)
            }
            val uri = resolver.insert(
                if (sourceFile.extension.lowercase() == "m4a")
                    android.provider.MediaStore.Audio.Media.EXTERNAL_CONTENT_URI
                else
                    android.provider.MediaStore.Video.Media.EXTERNAL_CONTENT_URI,
                contentValues
            )
            if (uri != null) {
                resolver.openOutputStream(uri)?.use { out ->
                    sourceFile.inputStream().use { it.copyTo(out) }
                }
                contentValues.clear()
                contentValues.put(android.provider.MediaStore.MediaColumns.IS_PENDING, 0)
                resolver.update(uri, contentValues, null, null)
            }
        } else {
            val pubDir = File(
                android.os.Environment.getExternalStoragePublicDirectory(android.os.Environment.DIRECTORY_DOWNLOADS),
                "CDCedit"
            ).apply { mkdirs() }
            val dest = File(pubDir, sourceFile.name)
            sourceFile.copyTo(dest, overwrite = true)
            android.media.MediaScannerConnection.scanFile(
                context,
                arrayOf(dest.absolutePath),
                null,
                null
            )
        }
    } catch (e: Exception) {
        Log.e("concat-spike", "Failed to export to public downloads: ${e.message}")
    }
}
