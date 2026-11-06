package com.blinc

import android.test.ActivityInstrumentationTestCase2
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.EditorInfo
import com.blinc.example.MainActivity

@Suppress("DEPRECATION")
class NativeImeTests : ActivityInstrumentationTestCase2<MainActivity>(MainActivity::class.java) {
    companion object {
        @JvmStatic external fun nativeFocus(): Boolean
        @JvmStatic external fun nativeText(): String
        @JvmStatic external fun nativeChanges(): Long
        @JvmStatic external fun nativeEmojiCovered(): Int
    }

    private fun eventually(check: () -> Boolean) {
        val deadline = System.nanoTime() + 10_000_000_000L
        while (!check() && System.nanoTime() < deadline) Thread.sleep(25)
        assertTrue(check())
    }

    private fun findEditor(view: View): BlincImeEditor? {
        if (view is BlincImeEditor) return view
        if (view is ViewGroup) for (index in 0 until view.childCount) {
            findEditor(view.getChildAt(index))?.let { return it }
        }
        return null
    }

    fun testCandidatesReachTheGpuDocumentOnce() {
        val host = activity
        eventually { nativeFocus() }
        var editor: BlincImeEditor? = null
        eventually {
            instrumentation.runOnMainSync { editor = findEditor(host.window.decorView) }
            editor != null
        }
        eventually {
            var served = false
            instrumentation.runOnMainSync {
                val manager = host.getSystemService(android.content.Context.INPUT_METHOD_SERVICE) as android.view.inputmethod.InputMethodManager
                served = manager.isActive(editor) && manager.isAcceptingText
            }
            served
        }
        lateinit var connection: android.view.inputmethod.InputConnection
        eventually {
            var connected = false
            instrumentation.runOnMainSync {
                ImeTestService.active?.currentInputConnection?.let { connection = it; connected = true }
            }
            connected
        }
        instrumentation.runOnMainSync {
            assertTrue(connection.commitText("A🦊", 1))
        }
        eventually { nativeText() == "A🦊" && nativeChanges() == 1L }
        val changes = nativeChanges()
        assertEquals("Emoji probe: 2=missing font, 3=missing glyph, 4=raster failure, 5=not colour", 1, nativeEmojiCovered())
        instrumentation.runOnMainSync { assertTrue(connection.setComposingText("ni", 1)) }
        eventually { nativeText() == "A🦊ni" }
        assertEquals(changes, nativeChanges())
        instrumentation.runOnMainSync { assertTrue(connection.setComposingText("nihao", 1)) }
        eventually { nativeText() == "A🦊nihao" }
        assertEquals(changes, nativeChanges())
        instrumentation.runOnMainSync { assertTrue(connection.commitText("你好", 1)) }
        eventually { nativeText() == "A🦊你好" && nativeChanges() == changes + 1 }
        Thread.sleep(250)
        val file = java.io.File(instrumentation.targetContext.getExternalFilesDir(null), "ime-native.png")
        file.outputStream().use { instrumentation.uiAutomation.takeScreenshot().compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        val bounds = IntArray(2)
        instrumentation.runOnMainSync { editor!!.getLocationOnScreen(bounds) }
        java.io.File(file.parentFile, "ime-native-bounds.json").writeText(org.json.JSONArray(listOf(bounds[0], bounds[1], bounds[0] + editor!!.width, bounds[1] + editor!!.height)).toString())
        instrumentation.runOnMainSync {
            connection.sendKeyEvent(android.view.KeyEvent(android.view.KeyEvent.ACTION_DOWN, android.view.KeyEvent.KEYCODE_DEL))
            connection.sendKeyEvent(android.view.KeyEvent(android.view.KeyEvent.ACTION_UP, android.view.KeyEvent.KEYCODE_DEL))
        }
        eventually { nativeText() == "A🦊你" && nativeChanges() == changes + 2 }
    }
}
