package com.blinc.{{project_name_snake}}

import android.app.NativeActivity
import android.os.Bundle

/**
 * Main Activity for {{project_name}}
 *
 * This activity loads the Rust library and delegates to the native code.
 * The actual UI is rendered by Blinc via the native library.
 */
class MainActivity : NativeActivity() {
    companion object {
        init {
            // Load the Rust library
            System.loadLibrary("{{project_name_snake}}")
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
    }
}
