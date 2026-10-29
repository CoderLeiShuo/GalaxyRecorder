package com.leishuo.galaxyrecorder
internal class PlaybackControlsState {
 var visible=true
  private set
 fun tap(){visible=!visible}
 fun reveal(){visible=true}
 fun timeout(playing:Boolean){if(playing)visible=false}
}
