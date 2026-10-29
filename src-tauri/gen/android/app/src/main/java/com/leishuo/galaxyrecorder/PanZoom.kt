package com.leishuo.galaxyrecorder

internal data class PictureTransform(val zoom:Float=1f,val x:Float=0f,val y:Float=0f)
internal object PanZoom {
    fun clamp(value:PictureTransform,width:Float,height:Float,aspect:Float):PictureTransform {
        val zoom=value.zoom.coerceIn(1f,5f);val pictureWidth=minOf(width,height*aspect)*zoom;val pictureHeight=pictureWidth/aspect
        val dx=((pictureWidth-width)/2).coerceAtLeast(0f);val dy=((pictureHeight-height)/2).coerceAtLeast(0f)
        return PictureTransform(zoom,if(dx>0)value.x.coerceIn(-dx,dx)else 0f,if(dy>0)value.y.coerceIn(-dy,dy)else 0f)
    }
    fun scale(value:PictureTransform,ratio:Float,x:Float,y:Float,width:Float,height:Float,aspect:Float):PictureTransform {
        val zoom=(value.zoom*ratio).coerceIn(1f,5f);val factor=zoom/value.zoom
        return clamp(PictureTransform(zoom,x-(x-value.x)*factor,y-(y-value.y)*factor),width,height,aspect)
    }
}
