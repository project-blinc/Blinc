package com.blinc

import android.test.ActivityInstrumentationTestCase2
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.BaseInputConnection

@Suppress("DEPRECATION")
class ImeTests : ActivityInstrumentationTestCase2<ImeActivity>(ImeActivity::class.java) {
    fun testCompositionCandidatesAndUtf16Selection() {
        val host = activity
        instrumentation.runOnMainSync {
            val editor = host.editor
            val connection = editor.onCreateInputConnection(EditorInfo())!!
            assertTrue(connection.setComposingText("ni", 1))
            assertEquals("A🦊ni", editor.text.toString())
            assertEquals("[3,5]", host.edits.last().getJSONArray("composing").toString())
            assertEquals("[5,5]", host.edits.last().getJSONArray("selection").toString())
            // A delayed render snapshot must not clear the native composing span.
            editor.receive(host.snapshot())
            assertEquals(3, BaseInputConnection.getComposingSpanStart(editor.editableText))
            assertTrue(connection.setComposingText("nihao", 1))
            assertTrue(connection.commitText("你好", 1))
            assertEquals("A🦊你好", editor.text.toString())
            assertTrue(host.edits.last().isNull("composing"))
            assertEquals(3, host.edits.size)
            assertTrue(connection.setComposingText("ma", 1))
            assertTrue(connection.finishComposingText())
            assertEquals("A🦊你好ma", host.edits.last().getString("text"))
            assertTrue(host.edits.last().isNull("composing"))
            connection.sendKeyEvent(android.view.KeyEvent(android.view.KeyEvent.ACTION_DOWN, android.view.KeyEvent.KEYCODE_DEL))
            connection.sendKeyEvent(android.view.KeyEvent(android.view.KeyEvent.ACTION_UP, android.view.KeyEvent.KEYCODE_DEL))
            assertEquals("A🦊你好m", editor.text.toString())
        }
    }

    fun testBatchReplacementRollbackAndRetiredConnection() {
        val host = activity
        instrumentation.runOnMainSync {
            val editor = host.editor
            val connection = editor.onCreateInputConnection(EditorInfo())!!
            assertTrue(connection.beginBatchEdit())
            assertTrue(connection.setSelection(1, 3))
            assertTrue(connection.setComposingText("ni", 1))
            assertTrue(connection.commitText("你", 1))
            assertEquals(0, host.edits.size)
            connection.endBatchEdit()
            assertEquals(1, host.edits.size)
            assertEquals("A你", host.edits.last().getString("text"))
            // The document can reject an edit and acknowledge its sequence.
            editor.receive(host.snapshot(text = "A🦊", sequence = 1))
            assertEquals("A🦊", editor.text.toString())
            assertTrue(connection.deleteSurroundingTextInCodePoints(1, 0))
            assertEquals("A", editor.text.toString())
            editor.deactivate()
            assertFalse(connection.commitText("stale", 1))
            assertEquals("A", editor.text.toString())
        }
    }

    fun testReturnCommitsMarkedTextBeforeSubmitting() {
        val host = activity
        instrumentation.runOnMainSync {
            val editor = host.editor
            val connection = editor.onCreateInputConnection(EditorInfo())!!
            assertTrue(connection.setComposingText("ni", 1))
            connection.performEditorAction(EditorInfo.IME_ACTION_DONE)
            assertTrue(host.edits.last().isNull("composing"))
            assertFalse(host.edits.last().getBoolean("submit"))
            connection.performEditorAction(EditorInfo.IME_ACTION_DONE)
            assertTrue(host.edits.last().getBoolean("submit"))
            assertEquals("A🦊ni", host.edits.last().getString("text"))
        }
    }
}
