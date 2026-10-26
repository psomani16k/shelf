package com.psomani16k.shelf

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.Settings
import androidx.activity.result.ActivityResult
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import app.tauri.PermissionState
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

/** What the Rust side can't do on Android: storage access, a folder picker, and the system bar insets. */
@TauriPlugin(
  permissions = [Permission(strings = [Manifest.permission.READ_EXTERNAL_STORAGE], alias = "storage")]
)
class ShelfPlugin(private val activity: Activity) : Plugin(activity) {

  @Command
  fun storageGranted(invoke: Invoke) {
    invoke.resolve(JSObject().put("granted", granted()))
  }

  /** Android 11+ has no dialog for all-files access: it opens a settings page and we check on return. */
  @Command
  fun requestStorage(invoke: Invoke) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
      val intent = Intent(
        Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
        Uri.parse("package:${activity.packageName}"),
      )
      startActivityForResult(invoke, intent, "storageSettingsResult")
    } else {
      requestPermissionForAlias("storage", invoke, "storagePermissionResult")
    }
  }

  @ActivityCallback
  fun storageSettingsResult(invoke: Invoke, result: ActivityResult) {
    invoke.resolve(JSObject().put("granted", granted()))
  }

  @PermissionCallback
  fun storagePermissionResult(invoke: Invoke) {
    invoke.resolve(JSObject().put("granted", granted()))
  }

  private fun granted(): Boolean =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) Environment.isExternalStorageManager()
    else getPermissionState("storage") == PermissionState.GRANTED

  @Command
  fun pickFolder(invoke: Invoke) {
    startActivityForResult(invoke, Intent(Intent.ACTION_OPEN_DOCUMENT_TREE), "pickFolderResult")
  }

  @ActivityCallback
  fun pickFolderResult(invoke: Invoke, result: ActivityResult) {
    val uri = result.data?.data
    if (result.resultCode != Activity.RESULT_OK || uri == null) {
      invoke.reject("No folder chosen")
      return
    }
    val path = treeUriToPath(uri)
    if (path == null) {
      invoke.reject("Only folders on this device's storage can be used")
      return
    }
    invoke.resolve(JSObject().put("path", path))
  }

  /** `primary:Books/Sci` → `/storage/emulated/0/Books/Sci`; `1234-ABCD:Books` → `/storage/1234-ABCD/Books`. */
  private fun treeUriToPath(uri: Uri): String? {
    if (uri.authority != "com.android.externalstorage.documents") return null
    val docId = DocumentsContract.getTreeDocumentId(uri)
    val parts = docId.split(":", limit = 2)
    val volume = parts[0]
    val rel = parts.getOrElse(1) { "" }
    val root =
      if (volume == "primary") Environment.getExternalStorageDirectory().absolutePath
      else "/storage/$volume"
    return if (rel.isEmpty()) root else "$root/$rel"
  }

  /** Status bar, navigation bar and display cutout, in CSS pixels. */
  @Command
  fun systemInsets(invoke: Invoke) {
    val insets = ViewCompat.getRootWindowInsets(activity.window.decorView)
      ?.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
    val density = activity.resources.displayMetrics.density.toDouble()
    val result = JSObject()
    result.put("top", (insets?.top ?: 0) / density)
    result.put("right", (insets?.right ?: 0) / density)
    result.put("bottom", (insets?.bottom ?: 0) / density)
    result.put("left", (insets?.left ?: 0) / density)
    invoke.resolve(result)
  }
}
