package com.leishuo.galaxyrecorder

import android.content.Context
import android.media.AudioManager
import android.os.Build
import kotlin.math.roundToInt

internal object VolumeScale {
 fun fraction(index:Int,min:Int,max:Int):Float=if(max<=min)0f else ((index-min).toFloat()/(max-min)).coerceIn(0f,1f)
 fun index(fraction:Float,min:Int,max:Int):Int=(min+fraction.coerceIn(0f,1f)*(max-min).coerceAtLeast(0)).roundToInt().coerceIn(min,max.coerceAtLeast(min))
}
internal class MediaVolume(context:Context){
 private val manager=context.applicationContext.getSystemService(Context.AUDIO_SERVICE) as AudioManager
 private val stream=AudioManager.STREAM_MUSIC
 private fun minimum()=if(Build.VERSION.SDK_INT>=28)manager.getStreamMinVolume(stream)else 0
 fun steps()=(manager.getStreamMaxVolume(stream)-minimum()).coerceAtLeast(1)
 fun read():Float=if(manager.isStreamMute(stream))0f else VolumeScale.fraction(manager.getStreamVolume(stream),minimum(),manager.getStreamMaxVolume(stream))
 fun set(value:Float):Float {
  val target=VolumeScale.index(value,minimum(),manager.getStreamMaxVolume(stream))
  if(value>0&&manager.isStreamMute(stream))manager.adjustStreamVolume(stream,AudioManager.ADJUST_UNMUTE,0)
  manager.setStreamVolume(stream,target,AudioManager.FLAG_SHOW_UI)
  return read()
 }
}
