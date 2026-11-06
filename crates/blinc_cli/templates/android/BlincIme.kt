package com.blinc

import android.app.Activity
import android.content.Context
import android.graphics.Canvas
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputConnectionWrapper
import android.view.inputmethod.InputMethodManager
import android.view.inputmethod.TextAttribute
import android.widget.EditText
import android.widget.FrameLayout
import org.json.JSONArray
import org.json.JSONObject
import java.lang.ref.WeakReference

/** Keeps the system keyboard's composing document alongside the GPU editor. */
internal object BlincIme {
    private var current = WeakReference<BlincImeEditor>(null)

    fun synchronize(activity: Activity, json: String) {
        val value = if (json == "null") null else JSONObject(json)
        var editor = current.get()
        if (editor != null && (value == null || editor.session != value.getString("session") || editor.context !== activity)) {
            editor.deactivate(hideKeyboard = value == null)
            (editor.parent as? ViewGroup)?.removeView(editor)
            editor = null
        }
        if (value == null) { current = WeakReference(null); return }
        if (editor == null) {
            editor = BlincImeEditor(activity) { BlincNativeBridge.nativeDispatchImeEdit(it) }
            activity.addContentView(editor, FrameLayout.LayoutParams(1, 1))
            current = WeakReference(editor)
        }
        editor.receive(value)
        if (!editor.hasFocus()) {
            editor.requestFocus()
            editor.post { editor.showKeyboard() }
        }
    }

    fun show(): Boolean = current.get()?.let { it.showKeyboard(); true } ?: false
    fun hide() { current.get()?.deactivate() }
}

internal class BlincImeEditor(context: Context, private val dispatch: (String) -> Boolean) : EditText(context) {
    var session = ""
        private set
    private var node = ""
    private var sequence = 0L
    private var applying = true
    private var depth = 0
    private var active = true
    private var last = ""
    private var multiline = false
    private var pendingSubmit = false

    init {
        background = null
        isCursorVisible = false
        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        setPadding(0, 0, 0, 0)
        isFocusableInTouchMode = true
        addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(s: Editable?) { publish() }
        })
        setOnEditorActionListener { _, _, _ ->
            val composing = BaseInputConnection.getComposingSpanStart(editableText) >= 0
            mutate { BaseInputConnection.removeComposingSpans(editableText); true }
            if (!composing) publish(submit = true)
            true
        }
        applying = false
    }

    // Touches and drawing belong to the GPU; the native editor supplies IME geometry.
    override fun onDraw(canvas: Canvas) {}
    override fun dispatchTouchEvent(event: MotionEvent): Boolean = false

    fun deactivate(hideKeyboard: Boolean = true) {
        active = false
        applying = true
        clearFocus()
        if (hideKeyboard) (context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager)
            ?.hideSoftInputFromWindow(windowToken, 0)
    }

    fun showKeyboard() {
        if (active) (context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager)
            ?.showSoftInput(this, InputMethodManager.SHOW_IMPLICIT)
    }

    fun receive(value: JSONObject) {
        val incomingSession = value.getString("session")
        if (!active || (session == incomingSession && value.getLong("sequence") < sequence)) return
        applying = true
        try {
            val fresh = session != incomingSession
            session = incomingSession
            node = value.getString("id")
            sequence = value.getLong("sequence")
            multiline = value.getBoolean("multiline")
            val secure = value.getBoolean("secure")
            val kind = InputType.TYPE_CLASS_TEXT or when {
                secure -> InputType.TYPE_TEXT_VARIATION_PASSWORD
                else -> 0
            } or (if (multiline) InputType.TYPE_TEXT_FLAG_MULTI_LINE else 0) or
                (if (value.optBoolean("autocorrect", false) && !secure) InputType.TYPE_TEXT_FLAG_AUTO_CORRECT else 0)
            if (inputType != kind) inputType = kind
            imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI or if (multiline) EditorInfo.IME_ACTION_NONE else EditorInfo.IME_ACTION_DONE
            val density = resources.displayMetrics.density
            setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, (value.getDouble("fontSize") * density).toFloat())
            x = (value.getDouble("x") * density).toFloat()
            y = (value.getDouble("y") * density).toFloat()
            layoutParams = layoutParams.apply {
                width = (value.getDouble("width") * density).toInt().coerceAtLeast(1)
                height = (value.getDouble("height") * density).toInt().coerceAtLeast(1)
            }
            if (value.isNull("composing")) BaseInputConnection.removeComposingSpans(editableText)
            val incoming = value.getString("text")
            if (text.toString() != incoming) setText(incoming)
            val selection = value.getJSONArray("selection")
            val start = selection.getInt(0).coerceIn(0, length())
            val end = selection.getInt(1).coerceIn(0, length())
            if (selectionStart != start || selectionEnd != end) setSelection(start, end)
            last = content().toString()
            if (fresh) (context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager)?.restartInput(this)
        } finally { applying = false }
    }

    private fun content(): JSONObject {
        val start = BaseInputConnection.getComposingSpanStart(editableText)
        val end = BaseInputConnection.getComposingSpanEnd(editableText)
        return JSONObject().put("text", text.toString())
            .put("selection", JSONArray(listOf(selectionStart.coerceAtLeast(0), selectionEnd.coerceAtLeast(0))))
            .put("composing", if (start >= 0 && end > start) JSONArray(listOf(start, end)) else JSONObject.NULL)
    }

    private fun publish(submit: Boolean = false) {
        if (applying || !active || session.isEmpty()) return
        pendingSubmit = pendingSubmit || submit
        if (depth > 0) return
        val value = content()
        val key = value.toString()
        if (!pendingSubmit && key == last) return
        value.put("id", node).put("session", session).put("sequence", sequence + 1).put("submit", pendingSubmit)
        if (dispatch(value.toString())) {
            sequence += 1
            last = key
            pendingSubmit = false
        }
    }

    private inline fun mutate(block: () -> Boolean): Boolean {
        if (!active) return false
        depth += 1
        return try { block() } finally { depth -= 1; publish() }
    }

    override fun onSelectionChanged(selStart: Int, selEnd: Int) {
        super.onSelectionChanged(selStart, selEnd)
        publish()
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean = mutate {
        if (!multiline && event.keyCode == KeyEvent.KEYCODE_ENTER) {
            if (event.action == KeyEvent.ACTION_DOWN) onEditorAction(EditorInfo.IME_ACTION_DONE)
            true
        } else super.dispatchKeyEvent(event)
    }

    override fun onCreateInputConnection(info: EditorInfo): InputConnection? {
        val target = super.onCreateInputConnection(info) ?: return null
        val connectionSession = session
        return object : InputConnectionWrapper(target, false) {
            private inline fun change(block: () -> Boolean): Boolean =
                if (connectionSession != session) false else mutate(block)
            override fun beginBatchEdit(): Boolean {
                if (!active || connectionSession != session) return false
                depth += 1
                return super.beginBatchEdit()
            }
            override fun endBatchEdit(): Boolean {
                if (!active || connectionSession != session) return false
                return try { super.endBatchEdit() } finally { depth = (depth - 1).coerceAtLeast(0); publish() }
            }
            override fun setComposingText(text: CharSequence?, cursor: Int): Boolean = change { super.setComposingText(text, cursor) }
            override fun setComposingRegion(start: Int, end: Int): Boolean = change { super.setComposingRegion(start, end) }
            override fun finishComposingText(): Boolean = change { super.finishComposingText() }
            override fun commitText(text: CharSequence?, cursor: Int): Boolean = change { super.commitText(text, cursor) }
            override fun setSelection(start: Int, end: Int): Boolean = change { super.setSelection(start, end) }
            override fun deleteSurroundingText(before: Int, after: Int): Boolean = change { super.deleteSurroundingText(before, after) }
            override fun deleteSurroundingTextInCodePoints(before: Int, after: Int): Boolean = change { super.deleteSurroundingTextInCodePoints(before, after) }
            override fun sendKeyEvent(event: KeyEvent): Boolean = change { this@BlincImeEditor.dispatchKeyEvent(event) }
            override fun performEditorAction(action: Int): Boolean = change { super.performEditorAction(action) }
            override fun closeConnection() {
                change { super.finishComposingText(); super.closeConnection(); true }
            }
            override fun commitText(text: CharSequence, cursor: Int, attribute: TextAttribute?): Boolean = change { super.commitText(text, cursor, attribute) }
            override fun setComposingText(text: CharSequence, cursor: Int, attribute: TextAttribute?): Boolean = change { super.setComposingText(text, cursor, attribute) }
            override fun replaceText(start: Int, end: Int, text: CharSequence, cursor: Int, attribute: TextAttribute?): Boolean = change { super.replaceText(start, end, text, cursor, attribute) }
        }
    }
}
