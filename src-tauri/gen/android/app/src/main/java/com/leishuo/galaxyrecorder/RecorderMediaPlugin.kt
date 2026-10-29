package com.leishuo.galaxyrecorder

import android.Manifest
import android.app.Activity
import android.content.*
import android.content.pm.ActivityInfo
import android.graphics.*
import android.media.MediaMetadataRetriever
import android.media.AudioManager
import android.net.*
import android.net.wifi.*
import android.os.Build
import android.provider.MediaStore
import android.view.*
import android.webkit.WebView
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.Button
import android.widget.ImageButton
import android.widget.TextView
import android.widget.SeekBar
import android.os.Handler
import android.os.Looper
import androidx.activity.OnBackPressedCallback
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.activity.result.ActivityResult
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.FileProvider
import androidx.documentfile.provider.DocumentFile
import androidx.media3.common.*
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.rtsp.RtspMediaSource
import app.tauri.PermissionState
import app.tauri.annotation.*
import app.tauri.plugin.*
import com.google.zxing.*
import com.google.zxing.common.HybridBinarizer
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import org.json.JSONObject
import java.io.*
import java.util.concurrent.Executors

@InvokeArg class MediaArgs { var operation: String = ""; var payload: Map<String, Any?> = emptyMap() }

@TauriPlugin(permissions = [
    Permission(alias = "wifi", strings = [Manifest.permission.ACCESS_FINE_LOCATION]),
    Permission(alias = "nearby", strings = [Manifest.permission.NEARBY_WIFI_DEVICES]),
    Permission(alias = "storage", strings = [Manifest.permission.WRITE_EXTERNAL_STORAGE]),
    Permission(alias = "notifications", strings = [Manifest.permission.POST_NOTIFICATIONS])
])
@UnstableApi
class RecorderMediaPlugin(private val activity: Activity) : Plugin(activity) {
    private val workers = Executors.newFixedThreadPool(2)
    private var webView: WebView? = null
    private var player: ExoPlayer? = null
    private val mediaVolume by lazy { MediaVolume(activity) }
    private var previousVolumeStream:Int?=null
    private var host: FrameLayout? = null
    private var texture: TextureView? = null
    private var error: String? = null
    private var picture = PictureTransform()
    private var surfaceVisible=true
    private var pendingAction:String?=null
    private var fullscreenStatus:TextView?=null
    private var gestureHint:TextView?=null
    private var previousBrightness:Float?=null
    private val hideGestureHint=Runnable { gestureHint?.visibility=View.GONE }
    private var dimensions = 16f / 9f
    private var previousOrientation: Int? = null
    private var fullscreen = false
    private var lastBounds: Map<String, Any?> = emptyMap()
    private var fullscreenControls: LinearLayout? = null
    private val controlsState=PlaybackControlsState()
    private var backCallback: OnBackPressedCallback? = null
    private val ui = Handler(Looper.getMainLooper())
    private val hideControls = Runnable { controlsState.timeout(player?.playWhenReady==true);fullscreenControls?.visibility=if(controlsState.visible)View.VISIBLE else View.GONE }
    private var updateControls: Runnable? = null
    private var notificationRequested = false
    private val editor = NativeEditor(activity)
    private var wifiCallback: ConnectivityManager.NetworkCallback? = null
    override fun load(webView: WebView) {
        this.webView = webView
        webView.settings.setSupportZoom(false)
        webView.settings.builtInZoomControls = false
        webView.settings.displayZoomControls = false
        webView.setOnLongClickListener { true };webView.isHapticFeedbackEnabled=false
        RecorderTransferService.cancelDownloads = { trigger("transferAction", json("action" to "cancelAll")) }
    }
    private fun json(vararg pairs: Pair<String, Any?>) = JSObject().apply { pairs.forEach { put(it.first, it.second ?: JSONObject.NULL) } }
    private fun Map<String, Any?>.text(key: String) = this[key] as? String ?: ""
    private fun Map<String, Any?>.number(key: String, default: Double = 0.0) = (this[key] as? Number)?.toDouble() ?: default
    private fun privateFile(path: String): File = MediaFiles.resolve(activity.dataDir, activity.filesDir, path)
    private val savedPreferences by lazy { activity.getSharedPreferences("recorder_saved_files",Context.MODE_PRIVATE) }
    @Synchronized private fun registered(value:String)=savedPreferences.getStringSet("uris",emptySet())!!.contains(value)
    @Synchronized private fun registerSaved(value:String) {
        val entries=savedPreferences.getStringSet("uris",emptySet())!!.toMutableSet();entries.add(value)
        check(savedPreferences.edit().putStringSet("uris",entries).commit()) { "保存文件索引失败" }
    }
    @Synchronized private fun forgetSaved(value:String) {
        val entries=savedPreferences.getStringSet("uris",emptySet())!!.toMutableSet();entries.remove(value);savedPreferences.edit().putStringSet("uris",entries).commit()
    }
    private fun ownedUri(value:String):Uri {
        val uri=Uri.parse(value)
        if(uri.scheme=="content") { require(registered(value)){"文件不在已保存录像中"};return uri }
        val file=File(if(uri.scheme=="file")uri.path!! else value).canonicalFile
        if(registered(file.path))return Uri.fromFile(file)
        return Uri.fromFile(privateFile(file.path))
    }
    private fun shareUri(value:String):Uri {
        val uri=ownedUri(value)
        return if(uri.scheme=="file")FileProvider.getUriForFile(activity,activity.packageName+".fileprovider",File(uri.path!!))else uri
    }
    private fun savedInfo(value:String):JSObject {
        try {
            val uri=ownedUri(value)
            if(uri.scheme=="file") {val file=File(uri.path!!);return json("path" to value,"exists" to file.isFile,"size" to file.length(),"name" to file.name)}
            activity.contentResolver.query(uri,arrayOf(android.provider.OpenableColumns.DISPLAY_NAME,android.provider.OpenableColumns.SIZE),null,null,null).use { cursor ->
                if(cursor!=null && cursor.moveToFirst())return json("path" to value,"exists" to true,"name" to cursor.getString(0),"size" to if(cursor.isNull(1))0 else cursor.getLong(1))
            }
        } catch (_:Exception) {}
        return json("path" to value,"exists" to false)
    }
    private fun deleteSaved(value:String) {
        val uri=ownedUri(value)
        if(uri.scheme=="file"){check(File(uri.path!!).delete()){ "文件删除失败" };android.media.MediaScannerConnection.scanFile(activity,arrayOf(uri.path!!),arrayOf("video/mp4"),null)}
        else check(activity.contentResolver.delete(uri,null,null)>0){ "相册文件删除失败" }
        forgetSaved(value)
    }
    private fun renameSaved(value:String,name:String):JSObject {
        require(name.isNotBlank() && name.length<=140 && '/' !in name && '\\' !in name)
        val uri=ownedUri(value)
        val next=if(uri.scheme=="file") {
            val file=File(uri.path!!);val target=File(file.parentFile,name);require(!target.exists()){ "同名文件已存在" };check(file.renameTo(target)){ "重命名失败" };target.canonicalPath
        } else if(android.provider.DocumentsContract.isDocumentUri(activity,uri)) {
            val document=DocumentFile.fromSingleUri(activity,uri)?:error("文件不可用");check(document.renameTo(name)){ "重命名失败" };document.uri.toString()
        } else {
            check(activity.contentResolver.update(uri,ContentValues().apply{put(MediaStore.Video.Media.DISPLAY_NAME,name)},null,null)>0){ "相册文件重命名失败" };uri.toString()
        }
        registerSaved(next);if(next!=value)forgetSaved(value)
        if(uri.scheme=="file")android.media.MediaScannerConnection.scanFile(activity,arrayOf(uri.path!!,next),arrayOf("video/mp4","video/mp4"),null)
        return json("uri" to next)
    }
    private fun source(value: String): Uri {
        val uri = Uri.parse(value)
        if (uri.scheme == null || uri.scheme == "file" || uri.scheme == "content") return ownedUri(value)
        require(uri.scheme in listOf("http", "rtsp") && (uri.host == "127.0.0.1" || uri.host?.let { host -> InetAddressesCompat.privateIpv4(host) } == true)) { "只允许访问记录仪和本地媒体" }
        return uri
    }
    private fun background(invoke: Invoke, action: () -> JSObject) { workers.execute { try { invoke.resolve(action()) } catch (e: Exception) { invoke.reject(e.message ?: "媒体操作失败") } } }

    @Command fun execute(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(MediaArgs::class.java)
            val data = args.payload
            when (args.operation) {
                "environment" -> invoke.resolve(json("manufacturer" to Build.MANUFACTURER, "model" to Build.MODEL, "android" to Build.VERSION.RELEASE, "api" to Build.VERSION.SDK_INT))
                "chooseDirectory" -> startActivityForResult(invoke, Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION), "directoryResult")
                "open" -> activity.runOnUiThread { try { open(data); invoke.resolve(json("volume" to mediaVolume.read(),"volumeSteps" to mediaVolume.steps())) } catch (e: Exception) { invoke.reject(e.message) } }
                "bounds" -> activity.runOnUiThread { bounds(data); invoke.resolve(json()) }
                "control" -> activity.runOnUiThread {
                    try {
                    val value = data.number("value"); val p = player
                    when (data.text("action")) {
                        "play" -> p?.play(); "pause" -> p?.pause(); "stop" -> stopPlayback(); "seek" -> p?.seekTo((value.coerceAtLeast(0.0) * 1000).toLong())
                        "volume" -> mediaVolume.set(value.coerceIn(0.0,1.0).toFloat())
                        "speed" -> p?.setPlaybackSpeed(value.coerceIn(0.25, 4.0).toFloat())
                        "visible" -> { surfaceVisible=value>0;host?.visibility=if(surfaceVisible)View.VISIBLE else View.INVISIBLE }
                        "fullscreen" -> setFullscreen(value > 0)
                    }; invoke.resolve(json())
                    } catch (e: Exception) { invoke.reject(e.message ?: "播放控制失败") }
                }
                "status" -> activity.runOnUiThread { invoke.resolve(json("position" to ((player?.currentPosition ?: 0) / 1000.0), "duration" to player?.duration?.takeIf { it > 0 }?.div(1000.0), "playing" to (player?.isPlaying == true), "buffering" to (player?.playbackState == Player.STATE_BUFFERING), "error" to error, "exportProgress" to editor.progress(), "fullscreen" to fullscreen, "volume" to mediaVolume.read(), "volumeSteps" to mediaVolume.steps(), "action" to pendingAction)); pendingAction=null }
                "close" -> activity.runOnUiThread { close(); invoke.resolve(json()) }
                "metadata" -> background(invoke) {
                    val retriever = MediaMetadataRetriever()
                    try {
                        val uri = source(data.text("source")); if (uri.scheme == "file" || uri.scheme == "content") retriever.setDataSource(activity,uri) else retriever.setDataSource(uri.toString(), emptyMap())
                        val bitmap = if (Build.VERSION.SDK_INT >= 27) retriever.getScaledFrameAtTime(1000000, MediaMetadataRetriever.OPTION_CLOSEST_SYNC, 480, 270) else retriever.getFrameAtTime(1000000, MediaMetadataRetriever.OPTION_CLOSEST_SYNC)
                        val thumb = bitmap?.let { frame -> val out = ByteArrayOutputStream(); frame.compress(Bitmap.CompressFormat.JPEG, 75, out); frame.recycle(); "data:image/jpeg;base64," + android.util.Base64.encodeToString(out.toByteArray(), android.util.Base64.NO_WRAP) }
                        json("duration" to retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toDoubleOrNull()?.div(1000), "thumbnail" to thumb)
                    } finally { retriever.release() }
                }
                "inspectSavedMany" -> background(invoke) {
                    val paths=data["paths"] as? List<*> ?: emptyList<Any>()
                    json("items" to org.json.JSONArray().apply{paths.forEach{value->put(savedInfo(value as? String ?: ""))}})
                }
                "deleteSaved" -> background(invoke) {deleteSaved(data.text("path"));json()}
                "renameSaved" -> background(invoke) {renameSaved(data.text("path"),data.text("name"))}
                "save" -> {
                    if (Build.VERSION.SDK_INT <= 28 && data.text("mode") == "gallery" && getPermissionState("storage") != PermissionState.GRANTED) { requestPermissionForAlias("storage", invoke, "permissionResult"); return }
                    background(invoke) { save(data) }
                }
                "share" -> {
                    val uri = shareUri(data.text("path"))
                    activity.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("video/mp4").putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION), "分享原始录像")); invoke.resolve(json())
                }
                "shareMany" -> {
                    val paths = data["paths"] as? List<*> ?: error("没有选择录像")
                    require(paths.size in 1..100)
                    val uris = ArrayList(paths.map { value -> shareUri(value as? String ?: error("录像路径无效")) })
                    val intent = Intent(if (uris.size == 1) Intent.ACTION_SEND else Intent.ACTION_SEND_MULTIPLE).setType(if (data["original"] == true) "application/octet-stream" else "video/mp4").addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    if (uris.size == 1) intent.putExtra(Intent.EXTRA_STREAM, uris[0]) else intent.putParcelableArrayListExtra(Intent.EXTRA_STREAM, uris)
                    intent.clipData = ClipData.newUri(activity.contentResolver, "录像", uris[0]).apply { uris.drop(1).forEach { addItem(ClipData.Item(it)) } }
                    activity.startActivity(Intent.createChooser(intent, if (data["original"] == true) "以原文件分享" else "分享录像")); invoke.resolve(json())
                }
                "saveLogs" -> startActivityForResult(invoke, Intent(Intent.ACTION_CREATE_DOCUMENT).setType("text/plain").addCategory(Intent.CATEGORY_OPENABLE).putExtra(Intent.EXTRA_TITLE, "吉行影记日志.txt"), "logsResult")
                "shareLogs" -> {
                    val file = File(activity.cacheDir, "diagnostics/吉行影记日志.txt"); file.parentFile?.mkdirs(); file.writeText(data.text("text").take(1024 * 1024))
                    val uri = FileProvider.getUriForFile(activity, activity.packageName + ".fileprovider", file)
                    activity.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION).apply { clipData = ClipData.newUri(activity.contentResolver,"运行日志",uri) }, "分享运行日志")); invoke.resolve(json())
                }
                "export" -> activity.runOnUiThread {
                    var restore: (() -> Unit)? = null
                    try {
                        if (data["background"] == true) {
                            val p = player; val item = p?.currentMediaItem; val position = p?.currentPosition ?: 0L; val playing = p?.playWhenReady == true
                            if (p != null && item != null) {
                                p.stop(); p.clearMediaItems(); var restored = false
                                restore = { if (!restored && player === p) {
                                    restored = true
                                    val rtsp = item.localConfiguration?.uri?.scheme == "rtsp"
                                    if (rtsp) p.setMediaSource(RtspMediaSource.Factory().setForceUseRtpTcp(true).createMediaSource(item)) else p.setMediaItem(item)
                                    p.prepare(); if (!rtsp) p.seekTo(position); p.playWhenReady = playing
                                } }
                            }
                        } else close()
                        editor.start(source(data.text("source")), privateFile(data.text("output")), data, invoke, restore)
                    } catch (e: Exception) { restore?.invoke(); invoke.reject(e.message) }
                }
                "cancelExport" -> activity.runOnUiThread { editor.cancel(); invoke.resolve(json()) }
                "scan" -> startActivityForResult(invoke, ScanOptions().setDesiredBarcodeFormats(ScanOptions.QR_CODE).setPrompt("扫描车机热点二维码").setBeepEnabled(false).setOrientationLocked(false).createScanIntent(activity), "scanResult")
                "scanImage" -> startActivityForResult(invoke, Intent(Intent.ACTION_OPEN_DOCUMENT).setType("image/*").addCategory(Intent.CATEGORY_OPENABLE), "imageResult")
                "openWifi" -> { activity.startActivity(Intent(android.provider.Settings.ACTION_WIFI_SETTINGS)); invoke.resolve(json()) }
                "releaseWifi" -> { releaseWifi(); invoke.resolve(json()) }
                "connectWifi", "searchRecorderWifi" -> {
                    val alias = if (Build.VERSION.SDK_INT >= 33) "nearby" else "wifi"
                    if (getPermissionState(alias) != PermissionState.GRANTED) { requestPermissionForAlias(alias, invoke, "permissionResult"); return }
                    connectWifi(data, invoke)
                }
                "transfer" -> {
                    if (data["active"] == true && Build.VERSION.SDK_INT >= 33 && !notificationRequested && getPermissionState("notifications") != PermissionState.GRANTED) {
                        notificationRequested = true; requestPermissionForAlias("notifications", invoke, "permissionResult"); return
                    }
                    ForegroundWork.set(activity, data.text("key").ifBlank { "download" }, data["active"] == true, data.text("message"))
                    invoke.resolve(json())
                }
                else -> invoke.reject("不支持的媒体操作")
            }
        } catch (e: Exception) { invoke.reject(e.message ?: "媒体操作失败") }
    }
    @PermissionCallback fun permissionResult(invoke: Invoke) {
        val args = invoke.parseArgs(MediaArgs::class.java)
        if (args.operation == "transfer") {
            // Foreground work can continue even when notifications are declined.
            ForegroundWork.set(activity, args.payload.text("key").ifBlank { "download" }, args.payload["active"] == true, args.payload.text("message"))
            invoke.resolve(json()); return
        }
        val alias = if (args.operation == "save") "storage" else if (Build.VERSION.SDK_INT >= 33) "nearby" else "wifi"
        if (getPermissionState(alias) == PermissionState.GRANTED) execute(invoke) else invoke.reject("未获得所需系统权限")
    }
    @ActivityCallback fun directoryResult(invoke: Invoke, result: ActivityResult) {
        try {
            val uri = result.data?.data?.takeIf { result.resultCode == Activity.RESULT_OK }
            if (uri == null) { invoke.resolve(json()); return }
            activity.contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
            invoke.resolve(json("uri" to uri.toString(), "label" to (DocumentFile.fromTreeUri(activity, uri)?.name ?: "已授权保存目录")))
        } catch (e: Exception) { invoke.reject(e.message) }
    }
    @ActivityCallback fun scanResult(invoke: Invoke, result: ActivityResult) { invoke.resolve(json("text" to ScanContract().parseResult(result.resultCode, result.data).contents)) }
    @ActivityCallback fun logsResult(invoke: Invoke, result: ActivityResult) {
        val uri = result.data?.data?.takeIf { result.resultCode == Activity.RESULT_OK }
        if (uri == null) { invoke.resolve(json("cancelled" to true)); return }
        val text = invoke.parseArgs(MediaArgs::class.java).payload.text("text").take(1024 * 1024)
        background(invoke) { activity.contentResolver.openOutputStream(uri,"w").use { requireNotNull(it).write(text.toByteArray(Charsets.UTF_8)) }; json() }
    }
    @ActivityCallback fun imageResult(invoke: Invoke, result: ActivityResult) {
        val uri = result.data?.data?.takeIf { result.resultCode == Activity.RESULT_OK }
        if (uri == null) { invoke.resolve(json()); return }
        background(invoke) {
            val options = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            activity.contentResolver.openInputStream(uri).use { BitmapFactory.decodeStream(it, null, options) }
            options.inSampleSize = (maxOf(options.outWidth, options.outHeight) / 1600).coerceAtLeast(1); options.inJustDecodeBounds = false
            var bitmap = activity.contentResolver.openInputStream(uri).use { BitmapFactory.decodeStream(it, null, options) } ?: error("无法读取图片")
            try {
                repeat(4) {
                    val pixels = IntArray(bitmap.width * bitmap.height); bitmap.getPixels(pixels, 0, bitmap.width, 0, 0, bitmap.width, bitmap.height)
                    try { return@background json("text" to MultiFormatReader().decode(BinaryBitmap(HybridBinarizer(RGBLuminanceSource(bitmap.width, bitmap.height, pixels))), mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE))).text) } catch (_: NotFoundException) { }
                    val rotated = Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, Matrix().apply { postRotate(90f) }, true); if (rotated !== bitmap) bitmap.recycle(); bitmap = rotated
                }; error("图片中没有识别到二维码")
            } finally { bitmap.recycle() }
        }
    }
    private fun open(data: Map<String, Any?>) {
        close(); error = null; picture=PictureTransform();surfaceVisible=true; dimensions = 16f / 9f
        previousOrientation = activity.requestedOrientation
        previousVolumeStream=activity.volumeControlStream;activity.volumeControlStream=AudioManager.STREAM_MUSIC
        val uri = source(data.text("source")); val root = activity.findViewById<ViewGroup>(android.R.id.content)
        host = object : FrameLayout(activity) {
            override fun dispatchTouchEvent(event: MotionEvent): Boolean {
                val visible = clipBounds
                if (visible != null && !visible.contains(event.x.toInt(), event.y.toInt())) return false
                return super.dispatchTouchEvent(event)
            }
        }.apply { setBackgroundColor(Color.BLACK); clipChildren = true; keepScreenOn = true }
        texture = TextureView(activity); host!!.addView(texture); root.addView(host)
        var gestureMode:PlaybackGesture?=null
        var gestureBlocked=false
        var startX=0f;var startY=0f;var startPosition=0L;var startVolume=1f;var startBrightness=1f;var targetPosition=0L
        val detector = ScaleGestureDetector(activity, object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
            override fun onScaleBegin(detector:ScaleGestureDetector):Boolean {gestureBlocked=true;gestureMode=null;return fullscreen}
            override fun onScale(detector: ScaleGestureDetector): Boolean {
                if(!fullscreen)return false
                host?.let { picture=PanZoom.scale(picture,detector.scaleFactor,detector.focusX-it.width/2,detector.focusY-it.height/2,it.width.toFloat(),it.height.toFloat(),dimensions);fit() };return true
            }
        })
        val gestures=GestureDetector(activity,object:GestureDetector.SimpleOnGestureListener(){
            override fun onDown(event:MotionEvent):Boolean {
                gestureMode=null;gestureBlocked=false;startX=event.x;startY=event.y;startPosition=player?.currentPosition?:0L;targetPosition=startPosition;startVolume=mediaVolume.read();startBrightness=currentBrightness();return true
            }
            override fun onSingleTapConfirmed(event:MotionEvent):Boolean {
                if(fullscreen&&!gestureBlocked){controlsState.tap();fullscreenControls?.visibility=if(controlsState.visible)View.VISIBLE else View.GONE;ui.removeCallbacks(hideControls);ui.postDelayed(hideControls,3000)}
                return true
            }
            override fun onDoubleTap(event:MotionEvent):Boolean {if(fullscreen){gestureBlocked=true;gestureMode=null;picture=PictureTransform();fit()};return true}
            override fun onScroll(first:MotionEvent?,current:MotionEvent,dx:Float,dy:Float):Boolean {
                if(!fullscreen||detector.isInProgress||current.pointerCount!=1||gestureBlocked)return false
                if(picture.zoom>1f){gestureMode=null;picture=picture.copy(x=picture.x-dx,y=picture.y-dy);fit();return true}
                val h=host?:return false;val moveX=current.x-startX;val moveY=current.y-startY
                if(gestureMode==null)gestureMode=PlaybackGestures.mode(moveX,moveY,startX,h.width.toFloat(),12*activity.resources.displayMetrics.density)
                when(gestureMode){
                    PlaybackGesture.SEEK -> player?.let{p->if(p.duration>0 && p.isCurrentMediaItemSeekable){targetPosition=PlaybackGestures.seek(startPosition,moveX,h.width.toFloat(),p.duration);showGestureHint("进度 ${timeLabel(targetPosition)} / ${timeLabel(p.duration)}")}}
                    PlaybackGesture.BRIGHTNESS -> {val value=PlaybackGestures.level(startBrightness,moveY,h.height.toFloat(),.05f);activity.window.attributes=activity.window.attributes.apply{screenBrightness=value};showGestureHint("亮度 ${(value*100).toInt()}%")}
                    PlaybackGesture.VOLUME -> {val value=PlaybackGestures.level(startVolume,moveY,h.height.toFloat());val actual=mediaVolume.set(value);showGestureHint("音量 ${kotlin.math.round(actual*100).toInt()}%")}
                    null -> return false
                };return true
            }
        })
        host!!.setOnTouchListener { _, event ->
            if(fullscreen){
                detector.onTouchEvent(event);gestures.onTouchEvent(event)
                if(event.actionMasked==MotionEvent.ACTION_POINTER_DOWN){gestureBlocked=true;gestureMode=null}
                if(event.actionMasked==MotionEvent.ACTION_UP||event.actionMasked==MotionEvent.ACTION_CANCEL){
                    if(event.actionMasked==MotionEvent.ACTION_UP&&!gestureBlocked&&gestureMode==PlaybackGesture.SEEK&&player?.isCurrentMediaItemSeekable==true)player?.seekTo(targetPosition)
                    val interacted=gestureMode!=null||gestureBlocked;gestureMode=null;ui.postDelayed(hideGestureHint,800);if(interacted)revealFullscreenControls()
                }
            }
            true
        }
        host!!.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> fit() }
        player = ExoPlayer.Builder(activity).build().apply {
            volume=1f
            setAudioAttributes(AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MOVIE).build(),true)
            setVideoTextureView(texture)
            addListener(object : Player.Listener {
                override fun onVideoSizeChanged(size: VideoSize) { if (size.height > 0) dimensions = size.width.toFloat() / size.height; fit() }
                override fun onPlayerError(e: PlaybackException) { error = "无法解码或读取此录像：${e.errorCodeName}" }
            })
            val item = MediaItem.fromUri(uri)
            if (uri.scheme == "rtsp") setMediaSource(RtspMediaSource.Factory().setForceUseRtpTcp(true).createMediaSource(item)) else setMediaItem(item)
            prepare(); playWhenReady = true
        }
        bounds(data)
    }
    private fun bounds(data: Map<String, Any?>) {
        if (fullscreen) return
        lastBounds = data
        val scale = data.number("scale", activity.resources.displayMetrics.density.toDouble())
        val root = activity.findViewById<ViewGroup>(android.R.id.content); val webLocation = IntArray(2); val rootLocation = IntArray(2)
        webView?.getLocationInWindow(webLocation); root.getLocationInWindow(rootLocation)
        host?.layoutParams = FrameLayout.LayoutParams((data.number("width") * scale).toInt().coerceAtLeast(1), (data.number("height") * scale).toInt().coerceAtLeast(1)).apply {
            leftMargin = (data.number("left") * scale).toInt() + webLocation[0] - rootLocation[0]; topMargin = (data.number("top") * scale).toInt() + webLocation[1] - rootLocation[1]
        }
        val clip = Rect((data.number("clipLeft") * scale).toInt(), (data.number("clipTop") * scale).toInt(), (data.number("clipRight", data.number("width")) * scale).toInt(), (data.number("clipBottom", data.number("height")) * scale).toInt())
        host?.clipBounds = clip; host?.visibility = if (clip.isEmpty || !surfaceVisible) View.INVISIBLE else View.VISIBLE
        host?.post { fit() }
    }
    private fun fit() {
        val h=host?:return
        picture=PanZoom.clamp(picture,h.width.toFloat(),h.height.toFloat(),dimensions)
        val width=minOf(h.width.toFloat(),h.height*dimensions)*picture.zoom;val height=width/dimensions
        texture?.layoutParams=FrameLayout.LayoutParams(width.toInt().coerceAtLeast(1),height.toInt().coerceAtLeast(1),Gravity.CENTER)
        texture?.translationX=picture.x;texture?.translationY=picture.y
    }

    private fun stopPlayback(){player?.let{it.pause();if(it.isCurrentMediaItemSeekable)it.seekTo(0)}}
    private fun currentBrightness():Float {
        val value=activity.window.attributes.screenBrightness
        return if(value>=0)value.coerceIn(.05f,1f)else(android.provider.Settings.System.getInt(activity.contentResolver,android.provider.Settings.System.SCREEN_BRIGHTNESS,128)/255f).coerceIn(.05f,1f)
    }
    private fun timeLabel(milliseconds:Long):String {val seconds=milliseconds.coerceAtLeast(0)/1000;return if(seconds>=3600)"%d:%02d:%02d".format(seconds/3600,seconds/60%60,seconds%60)else "%02d:%02d".format(seconds/60,seconds%60)}
    private fun revealFullscreenControls(){if(!fullscreen)return;controlsState.reveal();fullscreenControls?.visibility=View.VISIBLE;ui.removeCallbacks(hideControls);ui.postDelayed(hideControls,3000)}
    private fun showGestureHint(text:String){gestureHint?.text=text;gestureHint?.visibility=View.VISIBLE;ui.removeCallbacks(hideGestureHint);ui.removeCallbacks(hideControls)}

    private fun setFullscreen(enabled: Boolean) {
        if (fullscreen == enabled || host == null) return
        fullscreen = enabled
        val h = host!!
        val bars = WindowCompat.getInsetsController(activity.window, activity.window.decorView)
        if (enabled) {
            activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
            bars.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            bars.hide(WindowInsetsCompat.Type.systemBars())
            h.clipBounds = null; h.visibility = if(surfaceVisible)View.VISIBLE else View.INVISIBLE
            h.layoutParams = FrameLayout.LayoutParams(-1, -1)
            previousBrightness=activity.window.attributes.screenBrightness
            val density=activity.resources.displayMetrics.density
            fun dp(value:Int)=(value*density).toInt()
            controlsState.reveal()
            val controls = LinearLayout(activity).apply { orientation=LinearLayout.HORIZONTAL;gravity=Gravity.CENTER_VERTICAL;setPadding(dp(16),dp(8),dp(16),dp(8));setBackgroundColor(0xbb000000.toInt()) }
            fun button(label:String,icon:PlaybackIcon,action:()->Unit)=ImageButton(activity).apply{
                contentDescription=label;setImageDrawable(PlaybackIconDrawable(icon));setBackgroundColor(Color.TRANSPARENT);setPadding(dp(9),dp(9),dp(9),dp(9));layoutParams=LinearLayout.LayoutParams(dp(44),dp(44))
                setOnClickListener{action();revealFullscreenControls()}
            }
            val play=button("暂停",PlaybackIcon.PAUSE){player?.let{if(it.playWhenReady)it.pause()else it.play()}}
            controls.addView(play)
            fun timestamp()=TextView(activity).apply{setTextColor(Color.WHITE);textSize=12f;gravity=Gravity.CENTER;minWidth=dp(43)}
            val currentTime=timestamp();val endTime=timestamp()
            val seek=SeekBar(activity).apply{max=1000;contentDescription="全屏播放进度"}
            controls.addView(currentTime);controls.addView(seek,LinearLayout.LayoutParams(0,dp(40),1f));controls.addView(endTime)
            controls.addView(button("录像文件操作",PlaybackIcon.MORE){pendingAction="menu"})
            controls.addView(button("退出全屏",PlaybackIcon.EXIT){setFullscreen(false)})
            var dragging=false
            seek.setOnSeekBarChangeListener(object:SeekBar.OnSeekBarChangeListener{
                override fun onProgressChanged(bar:SeekBar,value:Int,fromUser:Boolean){if(fromUser)currentTime.text=timeLabel((player?.duration?.coerceAtLeast(0)?:0)*value/1000)}
                override fun onStartTrackingTouch(bar:SeekBar){dragging=true;ui.removeCallbacks(hideControls)}
                override fun onStopTrackingTouch(bar:SeekBar){player?.let{if(it.duration>0)it.seekTo(it.duration*bar.progress/1000)};dragging=false;revealFullscreenControls()}
            })
            h.addView(controls,FrameLayout.LayoutParams(-1,-2,Gravity.BOTTOM));fullscreenControls=controls
            gestureHint=TextView(activity).apply{setTextColor(Color.WHITE);textSize=18f;setPadding(dp(22),dp(14),dp(22),dp(14));setBackgroundColor(0xdd142132.toInt());gravity=Gravity.CENTER;visibility=View.GONE}
            h.addView(gestureHint,FrameLayout.LayoutParams(-2,-2,Gravity.CENTER))
            fullscreenStatus=TextView(activity).apply{setTextColor(Color.WHITE);setPadding(18,12,18,12);gravity=Gravity.CENTER;setBackgroundColor(0x66000000);setOnClickListener{if(error!=null)pendingAction="retry"}}
            h.addView(fullscreenStatus,FrameLayout.LayoutParams(-2,-2,Gravity.CENTER))
            updateControls = object : Runnable { override fun run() {
                if (!fullscreen) return
                fullscreenStatus?.text=error?.let{"$it\n轻触重试"}?:"正在缓冲…"
                fullscreenStatus?.visibility=if(error!=null || player?.playbackState==Player.STATE_BUFFERING)View.VISIBLE else View.GONE
                val p=player
                val playing=p?.playWhenReady==true
                play.contentDescription=if(playing)"暂停" else "播放"
                play.setImageDrawable(PlaybackIconDrawable(if(playing)PlaybackIcon.PAUSE else PlaybackIcon.PLAY))
                seek.isEnabled=p!=null&&p.duration>0&&p.isCurrentMediaItemSeekable
                if(!dragging){currentTime.text=timeLabel(p?.currentPosition?:0);if(p!=null&&p.duration>0)seek.progress=(p.currentPosition*1000/p.duration).toInt()}
                endTime.text=timeLabel(p?.duration?:0)
                ui.postDelayed(this, 300)
            } }; ui.post(updateControls!!); ui.postDelayed(hideControls, 3000)
            backCallback = object : OnBackPressedCallback(true) { override fun handleOnBackPressed() { setFullscreen(false) } }.also { (activity as? AppCompatActivity)?.onBackPressedDispatcher?.addCallback(it) }
        } else {
            previousBrightness?.let{value->activity.window.attributes=activity.window.attributes.apply{screenBrightness=value}};previousBrightness=null
            ui.removeCallbacks(hideGestureHint);gestureHint?.let{h.removeView(it)};gestureHint=null
            bars.show(WindowInsetsCompat.Type.systemBars())
            activity.requestedOrientation = previousOrientation ?: ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
            ui.removeCallbacks(hideControls); updateControls?.let { ui.removeCallbacks(it) }; updateControls = null
            fullscreenControls?.let { h.removeView(it) }; fullscreenControls = null
            fullscreenStatus?.let{h.removeView(it)};fullscreenStatus=null;picture=PictureTransform()
            backCallback?.remove(); backCallback = null
            bounds(lastBounds)
        }
        h.post { fit() }
    }
    private fun close() { previousVolumeStream?.let{activity.volumeControlStream=it};previousVolumeStream=null;setFullscreen(false); player?.release(); player = null; host?.let { (it.parent as? ViewGroup)?.removeView(it) }; host = null; texture = null; previousOrientation?.let { activity.requestedOrientation = it }; previousOrientation = null }
    override fun onPause(activity: AppCompatActivity) { player?.pause() }
    override fun onDestroy(activity: AppCompatActivity) { close(); if (!ForegroundWork.downloadsActive()) { editor.cancel(); workers.shutdownNow() } }
    private fun save(data: Map<String, Any?>): JSObject {
        val input = ownedUri(data.text("path")); val name = File(data.text("name")).name.ifBlank { "行车录像.mp4" }
        val resolver = activity.contentResolver
        val fingerprint=if(input.scheme=="file")File(input.path!!).let{"${it.length()}:${it.lastModified()}"}else input.toString()
        val publication="published:"+data.text("path")+"|"+data.text("mode")+"|"+data.text("directory")+"|"+name+"|"+fingerprint
        savedPreferences.getString(publication,null)?.let{cached->if(savedInfo(cached).optBoolean("exists"))return json("uri" to cached)}
        val uri = if (data.text("mode") == "folder") {
            val tree = Uri.parse(data.text("directory")); require(resolver.persistedUriPermissions.any { it.uri == tree && it.isWritePermission }) { "保存目录授权已失效，请重新选择" }
            DocumentFile.fromTreeUri(activity, tree)?.createFile("video/mp4", name)?.uri ?: error("无法在目录中创建录像")
        } else if (Build.VERSION.SDK_INT >= 29) {
            resolver.insert(MediaStore.Video.Media.EXTERNAL_CONTENT_URI, ContentValues().apply { put(MediaStore.Video.Media.DISPLAY_NAME, name); put(MediaStore.Video.Media.MIME_TYPE, "video/mp4"); put(MediaStore.Video.Media.RELATIVE_PATH, "Movies/GalaxyRecorder"); put(MediaStore.Video.Media.IS_PENDING, 1) }) ?: error("无法创建相册录像")
        } else {
            val folder = File(android.os.Environment.getExternalStoragePublicDirectory(android.os.Environment.DIRECTORY_MOVIES), "GalaxyRecorder"); check(folder.exists() || folder.mkdirs())
            val target = File(folder, java.util.UUID.randomUUID().toString() + "_" + name)
            try {resolver.openInputStream(input).use {source->requireNotNull(source);target.outputStream().use{source.copyTo(it)}};registerSaved(target.canonicalPath);check(savedPreferences.edit().putString(publication,target.canonicalPath).commit())}catch(e:Exception){target.delete();throw e}
            android.media.MediaScannerConnection.scanFile(activity, arrayOf(target.path), arrayOf("video/mp4"), null); return json("uri" to target.canonicalPath)
        }
        try {
            resolver.openOutputStream(uri, "w").use { output -> requireNotNull(output); resolver.openInputStream(input).use { source -> requireNotNull(source).copyTo(output) } }
            if (data.text("mode") != "folder" && Build.VERSION.SDK_INT >= 29) resolver.update(uri, ContentValues().apply { put(MediaStore.Video.Media.IS_PENDING, 0) }, null, null)
        registerSaved(uri.toString())
        check(savedPreferences.edit().putString(publication,uri.toString()).commit()){ "保存文件索引失败" }
        } catch (e: Exception) { resolver.delete(uri, null, null); throw e }
        return json("uri" to uri.toString())
    }
    private fun releaseWifi() {
        val manager=activity.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
        val requested=RecorderWifiSelection.requested
        RecorderWifiSelection.requested=null
        wifiCallback?.let { try {manager.unregisterNetworkCallback(it)} catch (_:Exception) {} };wifiCallback=null
        if(requested!=null && manager.boundNetworkForProcess==requested)manager.bindProcessToNetwork(null)
    }
    @Suppress("DEPRECATION")
    private fun legacyRecorderScan(data:Map<String,Any?>,invoke:Invoke){
        val wifi=activity.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        var finished=false
        lateinit var receiver:BroadcastReceiver
        fun finish(){if(!finished){finished=true;try{activity.unregisterReceiver(receiver)}catch(_:Exception){}}}
        fun choose(){
            if(finished)return
            val names=wifi.scanResults.map{it.SSID}.filter{it.matches(Regex("[Gg][Ee][Ee][Ll][Yy]_.*"))}.distinct()
            finish()
            if(names.isEmpty()){invoke.reject("未找到记录仪 Wi-Fi，请靠近车辆并开启记录仪，或在系统设置中连接");return}
            android.app.AlertDialog.Builder(activity).setTitle("选择你的记录仪").setItems(names.toTypedArray()){_,index->connectWifi(data+mapOf("ssid" to names[index],"search" to false),invoke)}.setNegativeButton("取消"){_,_->invoke.reject("连接已取消")}.setOnCancelListener{invoke.reject("连接已取消")}.show()
        }
        receiver=object:BroadcastReceiver(){override fun onReceive(context:Context,intent:Intent){choose()}}
        activity.registerReceiver(receiver,IntentFilter(WifiManager.SCAN_RESULTS_AVAILABLE_ACTION))
        if(!wifi.startScan()){choose();return}
        ui.postDelayed({choose()},8000)
    }
    private fun connectWifi(data: Map<String, Any?>, invoke: Invoke) {
        if(Looper.myLooper()!=Looper.getMainLooper()){activity.runOnUiThread{try{connectWifi(data,invoke)}catch(e:Exception){invoke.reject(e.message)}};return}
        val searching=data["search"]==true
        val ssid = data.text("ssid"); val password = data.text("password"); require(searching || ssid.isNotBlank() && ssid.length <= 32)
        require(password.isEmpty() || password.length in 8..63){"Wi-Fi 密码需要 8–63 个字符"}
        if (Build.VERSION.SDK_INT >= 29) {
            val manager = activity.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
            releaseWifi()
            val specifier = WifiNetworkSpecifier.Builder().apply {
                if(searching)setSsidPattern(android.os.PatternMatcher("[Gg][Ee][Ee][Ll][Yy]_.*",android.os.PatternMatcher.PATTERN_ADVANCED_GLOB))else setSsid(ssid)
                if (password.isNotEmpty()) setWpa2Passphrase(password)
            }.build()
            val request = NetworkRequest.Builder().addTransportType(NetworkCapabilities.TRANSPORT_WIFI).removeCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET).setNetworkSpecifier(specifier).build()
            val callback = object : ConnectivityManager.NetworkCallback() {
                private var resolved = false
                @Synchronized private fun ready(network: Network, properties: LinkProperties?) {
                    if (resolved || wifiCallback !== this || properties?.linkAddresses?.none { RecorderAddresses.local(it.address) } != false) return
                    RecorderWifiSelection.requested=network
                    resolved=true
                    if (manager.bindProcessToNetwork(network)) invoke.resolve(json("connected" to true)) else invoke.reject("未能连接热点，请重试")
                }
                @Synchronized private fun failed(message:String) { if(wifiCallback===this && !resolved){resolved=true;invoke.reject(message)} }
                override fun onAvailable(network: Network) {
                    if(wifiCallback!==this)return
                    RecorderWifiSelection.requested=network
                    ready(network,manager.getLinkProperties(network))
                    ui.postDelayed({ failed("热点尚未准备好，请稍后重试连接") },15000)
                }
                override fun onLinkPropertiesChanged(network:Network,properties:LinkProperties) {ready(network,properties)}
                override fun onLost(network: Network) { if(RecorderWifiSelection.requested==network)RecorderWifiSelection.requested=null;failed("车机热点已断开，请重新连接") }
                override fun onUnavailable() { failed("热点连接未完成，请在系统 Wi-Fi 中确认") }
            }; wifiCallback = callback; manager.requestNetwork(request, callback, 30000)
        } else {
            if(searching){legacyRecorderScan(data,invoke);return}
            @Suppress("DEPRECATION") val config = WifiConfiguration().apply { SSID = "\"${ssid.replace("\"", "\\\"")}\""; if (password.isEmpty()) allowedKeyManagement.set(WifiConfiguration.KeyMgmt.NONE) else preSharedKey = "\"${password.replace("\"", "\\\"")}\"" }
            @Suppress("DEPRECATION") val wifi = activity.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
            @Suppress("DEPRECATION") val id = wifi.addNetwork(config)
            @Suppress("DEPRECATION") check(id >= 0 && wifi.enableNetwork(id, true)) { "请在系统 Wi-Fi 中连接热点" }
            invoke.resolve(json("connected" to true))
        }
    }
}

private object InetAddressesCompat {
    fun privateIpv4(host: String): Boolean {
        val p = host.split('.').map { it.toIntOrNull() ?: return false }; if (p.size != 4 || p.any { it !in 0..255 }) return false
        return host in listOf("193.168.0.1","192.72.1.1") || p[0] == 10 || p[0] == 192 && p[1] == 168 || p[0] == 172 && p[1] in 16..31
    }
}
