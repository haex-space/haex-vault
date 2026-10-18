package com.haex.holzi

import android.os.Bundle
import android.system.Os
import android.view.KeyEvent
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // Spec 043: Rust and SQLite put temporary files under TMPDIR and fall back to
    // /data/local/tmp, which an app cannot write. Set before super.onCreate starts the Rust side,
    // while no other thread reads the environment.
    Os.setenv("TMPDIR", cacheDir.absolutePath, true)
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  // WebView can consume an injected KEYCODE_BACK before Android dispatches it through the
  // OnBackPressedDispatcher. Forward it there so Tauri's app back-button listener also handles
  // hardware back presses consistently with the system gesture.
  override fun dispatchKeyEvent(event: KeyEvent): Boolean {
    if (event.keyCode == KeyEvent.KEYCODE_BACK && event.action == KeyEvent.ACTION_DOWN) {
      onBackPressedDispatcher.onBackPressed()
      return true
    }
    return super.dispatchKeyEvent(event)
  }
}
