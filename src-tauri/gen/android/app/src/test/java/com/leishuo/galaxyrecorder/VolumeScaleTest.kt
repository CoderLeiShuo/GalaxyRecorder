package com.leishuo.galaxyrecorder
import org.junit.Assert.*
import org.junit.Test
class VolumeScaleTest {
 @Test fun usesTheActualDeviceSteps(){assertEquals(5,VolumeScale.index(1f/3,0,15));assertEquals(1f/3,VolumeScale.fraction(5,0,15),.0001f);assertEquals(10,VolumeScale.index(.5f,0,20))}
 @Test fun respectsBoundsAndNonzeroMinimum(){assertEquals(0,VolumeScale.index(-1f,0,15));assertEquals(15,VolumeScale.index(2f,0,15));assertEquals(4,VolumeScale.index(.5f,1,7));assertEquals(.5f,VolumeScale.fraction(4,1,7),.0001f)}
 @Test fun fixedLevelDevicesDoNotDivideByZero(){assertEquals(0f,VolumeScale.fraction(7,7,7),0f);assertEquals(7,VolumeScale.index(.4f,7,7))}
}
