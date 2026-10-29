package com.leishuo.galaxyrecorder
import org.junit.Test
import org.junit.Assert.*
class PlaybackGesturesTest {
    @Test fun jitterDoesNotStartAControlGesture(){assertNull(PlaybackGestures.mode(5f,9f,20f,400f,12f))}
    @Test fun horizontalMovementSeeksOnEitherSide(){assertEquals(PlaybackGesture.SEEK,PlaybackGestures.mode(-40f,15f,350f,400f,12f))}
    @Test fun verticalMovementUsesTheInitialTouchSide(){assertEquals(PlaybackGesture.BRIGHTNESS,PlaybackGestures.mode(3f,-40f,30f,400f,12f));assertEquals(PlaybackGesture.VOLUME,PlaybackGestures.mode(3f,40f,300f,400f,12f))}
    @Test fun seekIsRelativeBoundedAndLimitedToTwoMinutes(){assertEquals(360000L,PlaybackGestures.seek(300000L,200f,400f,3600000L));assertEquals(0L,PlaybackGestures.seek(5000L,-400f,400f,12000L));assertEquals(12000L,PlaybackGestures.seek(5000L,400f,400f,12000L))}
    @Test fun levelsAreBoundedAndBrightnessCannotBeBlack(){assertEquals(1f,PlaybackGestures.level(.5f,-140f,400f),.0001f);assertEquals(0f,PlaybackGestures.level(.5f,400f,400f),.0001f);assertEquals(.05f,PlaybackGestures.level(.5f,400f,400f,.05f),.0001f)}
}
