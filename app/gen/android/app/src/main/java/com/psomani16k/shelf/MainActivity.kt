package com.psomani16k.shelf

import android.os.Bundle
import android.view.ActionMode
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  // Wrapping `callback` and calling super wouldn't do: on null, DecorView builds the bar from its own copy.
  override fun onWindowStartingActionMode(callback: ActionMode.Callback, type: Int): ActionMode? {
    if (!SelectionMenu.hidden || type != ActionMode.TYPE_FLOATING) {
      return super.onWindowStartingActionMode(callback, type)
    }
    val mode = SilentActionMode(this, callback)
    return if (callback.onCreateActionMode(mode, mode.menu)) mode else null
  }
}
