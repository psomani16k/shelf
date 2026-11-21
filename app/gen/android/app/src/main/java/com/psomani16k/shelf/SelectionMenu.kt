package com.psomani16k.shelf

import android.app.Activity
import android.view.ActionMode
import android.view.Menu
import android.view.MenuInflater
import android.view.View
import android.widget.PopupMenu

/** Whether the WebView's floating Copy/Share bar is hidden. The reader sets it through `ShelfPlugin`. */
object SelectionMenu {
  @Volatile var hidden = false
}

/**
 * Stands in for the floating bar while it's hidden: the WebView keeps its selection and handles,
 * and whatever it puts in the menu is never shown. Ended by the WebView, like the real one.
 */
class SilentActionMode(
  private val activity: Activity,
  private val callback: ActionMode.Callback,
) : ActionMode() {
  private val menu: Menu = PopupMenu(activity, activity.window.decorView).menu
  private var finished = false

  init {
    type = ActionMode.TYPE_FLOATING
  }

  override fun setTitle(title: CharSequence?) {}
  override fun setTitle(resId: Int) {}
  override fun setSubtitle(subtitle: CharSequence?) {}
  override fun setSubtitle(resId: Int) {}
  override fun setCustomView(view: View?) {}
  override fun invalidate() {}

  override fun finish() {
    if (finished) return
    finished = true
    callback.onDestroyActionMode(this)
  }

  override fun getMenu(): Menu = menu
  override fun getTitle(): CharSequence? = null
  override fun getSubtitle(): CharSequence? = null
  override fun getCustomView(): View? = null
  override fun getMenuInflater(): MenuInflater = MenuInflater(activity)
}
