package com.blinc

import android.inputmethodservice.InputMethodService
import android.view.View
import android.view.inputmethod.EditorInfo

/** A deterministic system IME for the native editing contract tests. */
class ImeTestService : InputMethodService() {
    companion object { var active: ImeTestService? = null }
    override fun onStartInput(info: EditorInfo?, restarting: Boolean) {
        super.onStartInput(info, restarting)
        active = this
    }
    override fun onEvaluateFullscreenMode(): Boolean = false
    override fun onCreateInputView(): View = View(this).apply { minimumHeight = (48 * resources.displayMetrics.density).toInt() }
    override fun onDestroy() {
        if (active === this) active = null
        super.onDestroy()
    }
}
