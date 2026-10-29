package com.leishuo.galaxyrecorder
import org.junit.Test
import org.junit.Assert.*
class PanZoomTest {
 @Test fun pinchKeepsThePointUnderTheFingers(){assertEquals(PictureTransform(2f,-50f,-30f),PanZoom.scale(PictureTransform(),2f,50f,30f,400f,200f,2f))}
 @Test fun panningCannotPullThePictureBeyondItsEdges(){assertEquals(PictureTransform(2f,200f,0f),PanZoom.clamp(PictureTransform(2f,900f,-900f),400f,800f,2f))}
 @Test fun returningToOriginalSizeClearsPan(){assertEquals(PictureTransform(),PanZoom.clamp(PictureTransform(1f,900f,-900f),400f,200f,2f))}
 @Test fun scaleIsBounded(){assertEquals(5f,PanZoom.scale(PictureTransform(),99f,0f,0f,400f,200f,2f).zoom)}
}
