package com.leishuo.galaxyrecorder
import kotlin.math.floor
import kotlin.math.ceil

data class PixelMask(val left:Int,val top:Int,val right:Int,val bottom:Int)
object MaskPixels {
 fun bounds(x:Double,y:Double,width:Double,height:Double,frameWidth:Int,frameHeight:Int):PixelMask {
  require(listOf(x,y,width,height).all{it.isFinite()}&&x>=0&&y>=0&&width>0&&height>0&&x+width<=1.000001&&y+height<=1.000001)
  require(frameWidth>0&&frameHeight>0)
  return PixelMask((floor(x*frameWidth).toInt()-2).coerceAtLeast(0),(floor(y*frameHeight).toInt()-2).coerceAtLeast(0),(ceil((x+width)*frameWidth).toInt()+2).coerceAtMost(frameWidth),(ceil((y+height)*frameHeight).toInt()+2).coerceAtMost(frameHeight))
 }
 fun opaqueBlack(pixels:IntArray,width:Int,height:Int,mask:PixelMask):Boolean {
  val inset=minOf(3,(mask.right-mask.left-1)/2,(mask.bottom-mask.top-1)/2).coerceAtLeast(0)
  for(y in mask.top+inset until mask.bottom-inset)for(x in mask.left+inset until mask.right-inset){
   val pixel=pixels[y*width+x];if((pixel ushr 16 and 255)>48||(pixel ushr 8 and 255)>48||(pixel and 255)>48)return false
  };return true
 }
}
