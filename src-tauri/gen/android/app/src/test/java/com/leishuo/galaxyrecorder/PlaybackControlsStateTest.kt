package com.leishuo.galaxyrecorder
import org.junit.Assert.*
import org.junit.Test
class PlaybackControlsStateTest {
 @Test fun repeatedTapsAlternateEvenWhilePaused(){val state=PlaybackControlsState();state.tap();assertFalse(state.visible);state.timeout(false);assertFalse(state.visible);state.tap();assertTrue(state.visible);state.tap();assertFalse(state.visible)}
 @Test fun interactionReshowsTheBarAndPlaybackHidesItAfterTimeout(){val state=PlaybackControlsState();state.tap();state.reveal();assertTrue(state.visible);state.timeout(true);assertFalse(state.visible);state.tap();assertTrue(state.visible)}
}
