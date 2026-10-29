package com.leishuo.galaxyrecorder
import android.graphics.*
import android.graphics.drawable.Drawable

enum class PlaybackIcon { PLAY, PAUSE, EXIT, MORE }
class PlaybackIconDrawable(private val icon:PlaybackIcon):Drawable(){
 private val paint=Paint(Paint.ANTI_ALIAS_FLAG).apply{color=Color.WHITE;strokeWidth=2f;strokeCap=Paint.Cap.ROUND;strokeJoin=Paint.Join.ROUND}
 override fun draw(canvas:Canvas){
  val save=canvas.save();canvas.translate(bounds.left.toFloat(),bounds.top.toFloat());canvas.scale(bounds.width()/24f,bounds.height()/24f)
  paint.style=Paint.Style.FILL
  when(icon){
   PlaybackIcon.PLAY -> canvas.drawPath(Path().apply{moveTo(7f,4f);lineTo(20f,12f);lineTo(7f,20f);close()},paint)
   PlaybackIcon.PAUSE -> {canvas.drawRoundRect(5f,4f,10f,20f,1f,1f,paint);canvas.drawRoundRect(14f,4f,19f,20f,1f,1f,paint)}
   PlaybackIcon.MORE -> for(y in listOf(5f,12f,19f))canvas.drawCircle(12f,y,1.6f,paint)
   PlaybackIcon.EXIT -> {paint.style=Paint.Style.STROKE;canvas.drawPath(Path().apply{moveTo(3f,9f);lineTo(9f,9f);lineTo(9f,3f);moveTo(15f,3f);lineTo(15f,9f);lineTo(21f,9f);moveTo(3f,15f);lineTo(9f,15f);lineTo(9f,21f);moveTo(15f,21f);lineTo(15f,15f);lineTo(21f,15f)},paint)}
  };canvas.restoreToCount(save)
 }
 override fun setAlpha(alpha:Int){paint.alpha=alpha;invalidateSelf()}
 override fun setColorFilter(filter:ColorFilter?){paint.colorFilter=filter;invalidateSelf()}
 @Deprecated("Deprecated in Java") override fun getOpacity()=PixelFormat.TRANSLUCENT
}
