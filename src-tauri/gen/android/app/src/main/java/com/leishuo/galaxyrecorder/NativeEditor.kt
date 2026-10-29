package com.leishuo.galaxyrecorder

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.graphics.*
import android.net.Uri
import android.media.MediaMetadataRetriever
import androidx.media3.common.*
import androidx.media3.common.util.UnstableApi
import androidx.media3.effect.CanvasOverlay
import androidx.media3.effect.OverlayEffect
import androidx.media3.transformer.*
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import java.io.File

object ForegroundWork {
    private val active = mutableSetOf<String>()
    @Synchronized fun downloadsActive() = active.any { it.startsWith("download:") }
    @Synchronized fun stopAll(context: Context) { active.clear(); context.stopService(Intent(context, RecorderTransferService::class.java)) }
    @Synchronized fun set(context: Context, key: String, running: Boolean, message: String = "正在传输录像") {
        if (running) active.add(key) else active.remove(key)
        val intent = Intent(context, RecorderTransferService::class.java).putExtra("message", message).putExtra("downloads", active.any { it.startsWith("download:") })
        if (active.isEmpty()) context.stopService(intent) else context.startForegroundService(intent)
    }
}

@UnstableApi
class NativeEditor(private val activity: Activity) {
    private var transformer: Transformer? = null
    private var pending: Invoke? = null
    private var output: File? = null
    private var finished: (() -> Unit)? = null
    fun progress(): Int {
        val holder = ProgressHolder(); val state = transformer?.getProgress(holder)
        return if (state == Transformer.PROGRESS_STATE_AVAILABLE) holder.progress else 0
    }
    fun start(source: Uri, destination: File, data: Map<String, Any?>, invoke: Invoke, onFinished: (() -> Unit)? = null) {
        check(transformer == null && pending == null) { "请等待当前导出完成" }
        destination.parentFile?.mkdirs(); check(!destination.exists()) { "导出文件已存在" }
        val start = ((data["start"] as? Number)?.toDouble() ?: 0.0)
        val end = (data["end"] as? Number)?.toDouble()
        require(start >= 0 && (end == null || end > start)) { "片段时间不正确" }
        val clipping = MediaItem.ClippingConfiguration.Builder().setStartPositionMs((start * 1000).toLong()).apply { if (end != null) setEndPositionMs((end * 1000).toLong()) }.build()
        val item = MediaItem.Builder().setUri(source).setClippingConfiguration(clipping).build()
        val effects = mutableListOf<Effect>()
        @Suppress("UNCHECKED_CAST") val mask = data["mask"] as? Map<String, Number>
        if (mask != null) {
            val x = mask["x"]!!.toFloat(); val y = mask["y"]!!.toFloat(); val w = mask["width"]!!.toFloat(); val h = mask["height"]!!.toFloat()
            val paint = Paint().apply { color=Color.BLACK;alpha=255;isAntiAlias=false;style=Paint.Style.FILL;xfermode=PorterDuffXfermode(PorterDuff.Mode.SRC) }
            val overlay = object : CanvasOverlay(true) {
                override fun onDraw(canvas: Canvas, presentationTimeUs: Long) {
                    canvas.drawColor(Color.TRANSPARENT, PorterDuff.Mode.CLEAR)
                    val rect=MaskPixels.bounds(x.toDouble(),y.toDouble(),w.toDouble(),h.toDouble(),canvas.width,canvas.height)
                    canvas.drawRect(rect.left.toFloat(),rect.top.toFloat(),rect.right.toFloat(),rect.bottom.toFloat(),paint)
                }
            }
            effects.add(OverlayEffect(listOf(overlay)))
        }
        val edited = EditedMediaItem.Builder(item).setEffects(Effects(emptyList(), effects)).build()
        pending = invoke; output = destination; finished = onFinished
        try {
        transformer = Transformer.Builder(activity.applicationContext).setVideoMimeType(MimeTypes.VIDEO_H264).setAudioMimeType(MimeTypes.AUDIO_AAC)
            .addListener(object : Transformer.Listener {
                override fun onCompleted(composition: Composition, result: ExportResult) {
                    fun finish(failure:String?){
                        if(pending!==invoke)return
                        val callback=pending;pending=null;transformer=null;output=null
                        if(failure!=null)destination.delete()
                        ForegroundWork.set(activity,"export",false);finished?.invoke();finished=null
                        if(failure==null)callback?.resolve(JSObject().apply{put("path",destination.path)})else callback?.reject(failure)
                    }
                    if(mask==null)finish(null)else Thread({
                        val failure=try{verifyMask(destination,mask);null}catch(e:Exception){e.message?:"打码校验未通过，未保存此文件"}
                        activity.runOnUiThread{finish(failure)}
                    },"verify-video-mask").start()
                }
                override fun onError(composition: Composition, result: ExportResult, exception: ExportException) {
                    val callback = pending; pending = null; transformer = null; output = null; destination.delete()
                    ForegroundWork.set(activity, "export", false)
                    finished?.invoke(); finished = null
                    callback?.reject("导出失败：${exception.errorCodeName}")
                }
            }).build()
            ForegroundWork.set(activity, "export", true, "正在导出录像片段")
            transformer!!.start(edited, destination.path)
        } catch (exception: Exception) {
            pending = null; transformer?.cancel(); transformer = null; output = null; destination.delete()
            ForegroundWork.set(activity, "export", false)
            finished?.invoke(); finished = null
            throw exception
        }
    }
    private fun verifyMask(file:File,mask:Map<String,Number>){
        val reader=MediaMetadataRetriever()
        try{
            reader.setDataSource(file.path)
            val duration=reader.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull()?:error("无法校验打码视频")
            for(ms in listOf(0L,duration/2,(duration-150).coerceAtLeast(0)).distinct()){
                val frame=reader.getFrameAtTime(ms*1000,MediaMetadataRetriever.OPTION_CLOSEST)?:error("无法校验打码画面")
                try{
                    val pixels=IntArray(frame.width*frame.height);frame.getPixels(pixels,0,frame.width,0,0,frame.width,frame.height)
                    val rect=MaskPixels.bounds(mask["x"]!!.toDouble(),mask["y"]!!.toDouble(),mask["width"]!!.toDouble(),mask["height"]!!.toDouble(),frame.width,frame.height)
                    check(MaskPixels.opaqueBlack(pixels,frame.width,frame.height,rect)){"车架号遮挡校验未通过，未保存此文件，请重试"}
                }finally{frame.recycle()}
            }
        }finally{reader.release()}
    }
    fun cancel() {
        val callback = pending; pending = null; transformer?.cancel(); transformer = null; output?.delete(); output = null
        ForegroundWork.set(activity, "export", false); callback?.reject("导出已取消")
        finished?.invoke(); finished = null
    }
}
