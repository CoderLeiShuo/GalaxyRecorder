package com.leishuo.galaxyrecorder
import org.junit.Assert.*
import org.junit.Test
class MaskPixelsTest {
 @Test fun extendsTheRightCornerAndRoundsOutwards(){val r=MaskPixels.bounds(.62,0.0,.38,.09,1920,1080);assertEquals(1920,r.right);assertEquals(0,r.top);assertTrue(r.left<=1190);assertTrue(r.bottom>=98)}
 @Test fun detectsEvenOneReadablePixelInsideTheExport(){val r=PixelMask(0,0,20,10);val pixels=IntArray(200){0xff000000.toInt()};assertTrue(MaskPixels.opaqueBlack(pixels,20,10,r));pixels[5*20+12]=0xffffffff.toInt();assertFalse(MaskPixels.opaqueBlack(pixels,20,10,r))}
 @Test fun rejectsTranslucentMasksAndAllowsSmallCodecNoise(){val r=PixelMask(0,0,20,10);assertFalse(MaskPixels.opaqueBlack(IntArray(200){0xff666666.toInt()},20,10,r));assertTrue(MaskPixels.opaqueBlack(IntArray(200){0xff111111.toInt()},20,10,r))}
 @Test fun acceptsFloatRoundingButRejectsInvalidRegions(){assertEquals(1920,MaskPixels.bounds(.62f.toDouble(),0.0,.38f.toDouble(),.09f.toDouble(),1920,1080).right);try{MaskPixels.bounds(.9,0.0,.4,.1,1920,1080);fail()}catch(_:IllegalArgumentException){}}
}
