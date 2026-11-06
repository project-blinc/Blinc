package com.blinc

import android.app.Activity
import android.os.Bundle
import org.json.JSONObject

class ImeActivity : Activity() {
    internal lateinit var editor: BlincImeEditor
    val edits = mutableListOf<JSONObject>()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        editor = BlincImeEditor(this) { edits.add(JSONObject(it)); true }
        setContentView(editor)
        editor.receive(snapshot())
        editor.requestFocus()
    }

    fun snapshot(text: String = "A🦊", sequence: Long = 0, session: String = "100"): JSONObject =
        JSONObject().put("id", "42").put("session", session).put("sequence", sequence)
            .put("text", text).put("selection", org.json.JSONArray(listOf(text.length, text.length)))
            .put("composing", JSONObject.NULL).put("multiline", false).put("secure", false)
            .put("x", 20).put("y", 20).put("width", 300).put("height", 48).put("fontSize", 16)
}

// The protocol tests supply a recording dispatcher to the actual native editor.
internal object BlincNativeBridge {
    fun nativeDispatchImeEdit(json: String): Boolean = false
}
