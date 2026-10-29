package com.leishuo.galaxyrecorder

internal enum class PlaybackGesture { SEEK, BRIGHTNESS, VOLUME }
internal object PlaybackGestures {
    fun mode(dx:Float,dy:Float,startX:Float,width:Float,threshold:Float):PlaybackGesture? {
        if(maxOf(kotlin.math.abs(dx),kotlin.math.abs(dy))<threshold)return null
        return if(kotlin.math.abs(dx)>=kotlin.math.abs(dy))PlaybackGesture.SEEK else if(startX<width/2)PlaybackGesture.BRIGHTNESS else PlaybackGesture.VOLUME
    }
    fun seek(start:Long,dx:Float,width:Float,duration:Long):Long = (start+dx/width.coerceAtLeast(1f)*minOf(duration,120000L)).toLong().coerceIn(0L,duration.coerceAtLeast(0L))
    fun level(start:Float,dy:Float,height:Float,minimum:Float=0f):Float = (start-dy/(height*.7f).coerceAtLeast(1f)).coerceIn(minimum,1f)
}
