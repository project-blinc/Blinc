/**
 * Blinc Native Bridge for Android
 *
 * Kotlin implementation for handling native calls from Rust.
 * Register handlers for each namespace/function, then Rust can call
 * them via native_call("namespace", "function", args).
 *
 * Usage:
 * ```kotlin
 * // In Application.onCreate()
 * BlincNativeBridge.registerDefaults(context)
 *
 * // Or register custom handlers
 * BlincNativeBridge.register("myapi", "my_function") { args ->
 *     // args is JSONArray
 *     "result"
 * }
 * ```
 */

package com.blinc

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.BatteryManager
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.view.inputmethod.InputMethodManager
import androidx.core.content.getSystemService
import org.json.JSONArray
import org.json.JSONObject
import java.util.Locale
import java.util.TimeZone

object BlincNativeBridge {

    // Handler type: (args: JSONArray) -> Any?
    private val handlers = mutableMapOf<String, MutableMap<String, (JSONArray) -> Any?>>()

    // Application context for system services
    private var appContext: Context? = null

    // Activity reference (when initialized from an Activity) — required for
    // anything that needs a window/decor view, e.g. soft keyboard show/hide.
    // The application context returned by `Context.applicationContext` is NOT
    // an Activity, so storing the original `Context` here lets us recover
    // the Activity when callers pass one in.
    private var activityRef: java.lang.ref.WeakReference<android.app.Activity>? = null

    // Last IME inset (in logical pixels) we pushed to Rust. Used by the
    // window-insets listener to skip duplicate dispatches when nothing
    // about the keyboard has actually changed.
    private var lastDispatchedImeInsetPx: Int = -1

    // Last system-bar safe-area insets (in logical pixels) pushed to Rust.
    // The single `setOnApplyWindowInsetsListener` on the decor view fires
    // for every inset type, so we share it between the IME path and the
    // notch / status-bar / nav-bar / gesture-bar path and dedupe each
    // stream independently.
    private var lastDispatchedSafeAreaTopPx: Int = -1
    private var lastDispatchedSafeAreaRightPx: Int = -1
    private var lastDispatchedSafeAreaBottomPx: Int = -1
    private var lastDispatchedSafeAreaLeftPx: Int = -1

    /**
     * Initialize with application context
     */
    fun init(context: Context) {
        appContext = context.applicationContext
        if (context is android.app.Activity) {
            activityRef = java.lang.ref.WeakReference(context)
            attachWindowInsetsListener(context)
        }
    }

    private fun currentActivity(): android.app.Activity? = activityRef?.get()

    /**
     * Wire up a shared window-insets listener on the activity's decor view.
     *
     * A single [View.setOnApplyWindowInsetsListener] callback carries every
     * kind of inset (IME, status bar, navigation bar, gesture bar, display
     * cutout, ...). We dispatch two streams from it:
     *
     *  1. **Soft-keyboard / IME inset** → mirrors iOS'
     *     `UIKeyboardWillChangeFrameNotification`. Whenever the keyboard
     *     bottom inset changes (show, hide, hardware-keyboard attach,
     *     split-keyboard mode, IME swap, ...) the listener computes the
     *     bottom inset in **logical pixels** and pushes it via
     *     [nativeDispatchKeyboardInset].
     *
     *  2. **System-bar safe-area insets** → mirrors iOS'
     *     `UIWindow.safeAreaInsets`. Whenever the status bar / nav bar /
     *     notch cutout / gesture bar insets change (rotation, split-screen,
     *     PiP exit, immersive-mode toggle, display cutout mode, ...) the
     *     listener converts the four edges to logical pixels and pushes
     *     them via [nativeDispatchSafeArea].
     *
     * Both streams dedupe against their respective `lastDispatched*Px`
     * fields so unrelated inset changes don't thrash the Rust side.
     *
     * Implementation notes:
     * - `WindowInsets.Type.ime()` and `WindowInsets.Type.systemBars()`
     *   both require API 30+. On older devices we fall back to the
     *   `systemWindowInset*` accessors (deprecated but functional on
     *   API 24–29) plus a global-layout listener for the IME path.
     */
    private fun attachWindowInsetsListener(activity: android.app.Activity) {
        val decorView = activity.window?.decorView ?: return
        val density = activity.resources.displayMetrics.density.coerceAtLeast(0.001f)

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            // Modern path (API 30+): WindowInsets.Type reports per-type
            // insets in physical pixels. The listener fires for every
            // animation frame as the keyboard slides in / out, and
            // again on rotation / split-screen transitions.
            decorView.setOnApplyWindowInsetsListener { v, insets ->
                // --- IME ---
                val imeBottomPx = insets.getInsets(android.view.WindowInsets.Type.ime()).bottom
                val imeLogicalPx = (imeBottomPx.toFloat() / density).toInt()
                if (imeLogicalPx != lastDispatchedImeInsetPx) {
                    lastDispatchedImeInsetPx = imeLogicalPx
                    try {
                        nativeDispatchKeyboardInset(imeLogicalPx)
                    } catch (e: UnsatisfiedLinkError) {
                        // Native side hasn't loaded the symbol yet — most
                        // likely because the user app isn't using
                        // blinc_app::android::AndroidApp::run. Skip
                        // silently; the inset just won't propagate.
                    }
                }

                // --- System bars (status / nav / cutout / gesture) ---
                val sys = insets.getInsets(android.view.WindowInsets.Type.systemBars())
                val cutout = insets.getInsets(android.view.WindowInsets.Type.displayCutout())
                // Merge system bars with the display cutout — the cutout
                // can extend past the status bar on landscape phones
                // with a camera notch.
                val topLogical = (maxOf(sys.top, cutout.top).toFloat() / density).toInt()
                val rightLogical = (maxOf(sys.right, cutout.right).toFloat() / density).toInt()
                val bottomLogical = (maxOf(sys.bottom, cutout.bottom).toFloat() / density).toInt()
                val leftLogical = (maxOf(sys.left, cutout.left).toFloat() / density).toInt()
                if (topLogical != lastDispatchedSafeAreaTopPx
                    || rightLogical != lastDispatchedSafeAreaRightPx
                    || bottomLogical != lastDispatchedSafeAreaBottomPx
                    || leftLogical != lastDispatchedSafeAreaLeftPx
                ) {
                    lastDispatchedSafeAreaTopPx = topLogical
                    lastDispatchedSafeAreaRightPx = rightLogical
                    lastDispatchedSafeAreaBottomPx = bottomLogical
                    lastDispatchedSafeAreaLeftPx = leftLogical
                    try {
                        nativeDispatchSafeArea(topLogical, rightLogical, bottomLogical, leftLogical)
                    } catch (e: UnsatisfiedLinkError) {
                        // see IME branch
                    }
                }

                v.onApplyWindowInsets(insets)
            }
            // Force an initial dispatch so we have a baseline (otherwise
            // the very first frame after activity launch sees a stale
            // sentinel and doesn't update until the next inset change).
            decorView.requestApplyInsets()
        } else {
            // Legacy path (API 24-29): use the deprecated
            // `systemWindowInset*` accessors for safe area, and a
            // global-layout listener with visible-display-frame diff
            // for the IME path. The legacy IME path catches show /
            // hide but not the per-frame animation steps.
            decorView.setOnApplyWindowInsetsListener { v, insets ->
                @Suppress("DEPRECATION")
                val topLogical = (insets.systemWindowInsetTop.toFloat() / density).toInt()
                @Suppress("DEPRECATION")
                val rightLogical = (insets.systemWindowInsetRight.toFloat() / density).toInt()
                @Suppress("DEPRECATION")
                val bottomLogical = (insets.systemWindowInsetBottom.toFloat() / density).toInt()
                @Suppress("DEPRECATION")
                val leftLogical = (insets.systemWindowInsetLeft.toFloat() / density).toInt()
                if (topLogical != lastDispatchedSafeAreaTopPx
                    || rightLogical != lastDispatchedSafeAreaRightPx
                    || bottomLogical != lastDispatchedSafeAreaBottomPx
                    || leftLogical != lastDispatchedSafeAreaLeftPx
                ) {
                    lastDispatchedSafeAreaTopPx = topLogical
                    lastDispatchedSafeAreaRightPx = rightLogical
                    lastDispatchedSafeAreaBottomPx = bottomLogical
                    lastDispatchedSafeAreaLeftPx = leftLogical
                    try {
                        nativeDispatchSafeArea(topLogical, rightLogical, bottomLogical, leftLogical)
                    } catch (e: UnsatisfiedLinkError) {
                        // see modern branch
                    }
                }
                v.onApplyWindowInsets(insets)
            }
            decorView.requestApplyInsets()

            val rect = android.graphics.Rect()
            decorView.viewTreeObserver.addOnGlobalLayoutListener {
                decorView.getWindowVisibleDisplayFrame(rect)
                val screenHeight = decorView.rootView.height
                val keyboardPx = (screenHeight - rect.bottom).coerceAtLeast(0)
                val logicalPx = (keyboardPx.toFloat() / density).toInt()
                if (logicalPx != lastDispatchedImeInsetPx) {
                    lastDispatchedImeInsetPx = logicalPx
                    try {
                        nativeDispatchKeyboardInset(logicalPx)
                    } catch (e: UnsatisfiedLinkError) {
                        // see modern branch
                    }
                }
            }
        }
    }

    /**
     * Register a native function handler
     *
     * @param namespace The namespace (e.g., "device", "haptics")
     * @param name The function name
     * @param handler Handler that receives JSON args and returns a result
     */
    fun register(namespace: String, name: String, handler: (JSONArray) -> Any?) {
        handlers.getOrPut(namespace) { mutableMapOf() }[name] = handler
    }

    /**
     * Convenience: Register a no-arg function returning String
     */
    fun registerString(namespace: String, name: String, handler: () -> String) {
        register(namespace, name) { handler() }
    }

    /**
     * Convenience: Register a no-arg void function
     */
    fun registerVoid(namespace: String, name: String, handler: () -> Unit) {
        register(namespace, name) { handler(); null }
    }

    /**
     * Called from JNI to execute a registered function
     *
     * @param namespace The namespace
     * @param name The function name
     * @param argsJson JSON-encoded arguments array
     * @return JSON-encoded result or error
     */
    @JvmStatic
    fun callNative(namespace: String, name: String, argsJson: String): String {
        return try {
            val nsHandlers = handlers[namespace]
                ?: return errorJson("NotRegistered", "Namespace '$namespace' not found")

            val handler = nsHandlers[name]
                ?: return errorJson("NotRegistered", "Function '$namespace.$name' not found")

            val args = JSONArray(argsJson)
            val result = handler(args)

            successJson(result)
        } catch (e: Exception) {
            errorJson("PlatformError", e.message ?: "Unknown error")
        }
    }

    /**
     * Register default handlers for common functionality
     */
    fun registerDefaults(context: Context) {
        init(context)
        val ctx = context.applicationContext

        // =====================================================================
        // Device namespace
        // =====================================================================

        registerString("device", "get_battery_level") {
            val bm = ctx.getSystemService<BatteryManager>()
            bm?.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY)?.toString() ?: "0"
        }

        registerString("device", "get_model") {
            Build.MODEL
        }

        registerString("device", "get_os_version") {
            Build.VERSION.RELEASE
        }

        register("device", "is_low_power_mode") {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
                val pm = ctx.getSystemService(Context.POWER_SERVICE) as? android.os.PowerManager
                pm?.isPowerSaveMode ?: false
            } else {
                false
            }
        }

        register("device", "has_notch") {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                // Check for display cutout
                // This requires a window, return false as default
                false
            } else {
                false
            }
        }

        registerString("device", "get_locale") {
            Locale.getDefault().toString()
        }

        registerString("device", "get_timezone") {
            TimeZone.getDefault().id
        }

        // =====================================================================
        // Haptics namespace
        // =====================================================================

        register("haptics", "vibrate") { args ->
            val durationMs = args.optLong(0, 100)
            vibrate(ctx, durationMs)
            null
        }

        register("haptics", "impact") { args ->
            val style = args.optInt(0, 1)
            val amplitude = when (style) {
                0 -> 50   // light
                2 -> 255  // heavy
                else -> 128 // medium
            }
            vibrateWithAmplitude(ctx, 10, amplitude)
            null
        }

        registerVoid("haptics", "selection") {
            vibrateWithAmplitude(ctx, 5, 50)
        }

        registerVoid("haptics", "success") {
            vibrateWithAmplitude(ctx, 30, 200)
        }

        registerVoid("haptics", "warning") {
            vibrateWithAmplitude(ctx, 50, 150)
        }

        registerVoid("haptics", "error") {
            vibrateWithAmplitude(ctx, 100, 255)
        }

        // =====================================================================
        // Clipboard namespace
        // =====================================================================

        register("clipboard", "copy") { args ->
            val text = args.optString(0, "")
            val clipboard = ctx.getSystemService<ClipboardManager>()
            clipboard?.setPrimaryClip(ClipData.newPlainText("Blinc", text))
            null
        }

        registerString("clipboard", "paste") {
            val clipboard = ctx.getSystemService<ClipboardManager>()
            clipboard?.primaryClip?.getItemAt(0)?.text?.toString() ?: ""
        }

        register("clipboard", "has_content") {
            val clipboard = ctx.getSystemService<ClipboardManager>()
            clipboard?.hasPrimaryClip() ?: false
        }

        registerVoid("clipboard", "clear") {
            val clipboard = ctx.getSystemService<ClipboardManager>()
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                clipboard?.clearPrimaryClip()
            }
        }

        // =====================================================================
        // Text-edit context menu namespace
        // =====================================================================
        //
        // Mirrors the iOS `edit_menu` namespace. Rust text-editable
        // widgets call into this from their double-tap handlers to
        // show a native Android contextual menu (Cut / Copy / Paste /
        // Select All) over the focused selection.
        //
        // Android's equivalent of iOS `UIMenuController` is
        // `ActionMode` started against the activity's content view.
        // The action callbacks are routed back into Rust by
        // synthesizing the same Cmd+key codes the desktop runner
        // uses for the corresponding shortcuts:
        //
        //   Cut        → key code 88 (Cmd+X)
        //   Copy       → key code 67 (Cmd+C)
        //   Paste      → key code 86 (Cmd+V)
        //   Select All → key code 65 (Cmd+A)
        //
        // Each Blinc text-editable widget already handles those
        // shortcut codes, so the menu plugs into the existing
        // copy/cut/paste paths once the dispatch is wired up.
        //
        // Bitmask layout matches `text_edit::edit_menu_actions`:
        //   bit 0 = CUT
        //   bit 1 = COPY
        //   bit 2 = PASTE
        //   bit 3 = SELECT_ALL

        register("edit_menu", "show") { args ->
            val anchorX = (args.optDouble(0, 0.0)).toFloat()
            val anchorY = (args.optDouble(1, 0.0)).toFloat()
            val selX = (args.optDouble(2, anchorX.toDouble())).toFloat()
            val selY = (args.optDouble(3, anchorY.toDouble())).toFloat()
            val selW = (args.optDouble(4, 0.0)).toFloat()
            val selH = (args.optDouble(5, 24.0)).toFloat()
            val actions = args.optInt(6, 0)
            val activity = currentActivity()
            activity?.runOnUiThread {
                BlincEditMenuHelper.show(activity, anchorX, anchorY, selX, selY, selW, selH, actions)
            }
            null
        }

        registerVoid("edit_menu", "hide") {
            val activity = currentActivity()
            activity?.runOnUiThread {
                BlincEditMenuHelper.hide()
            }
        }

        // =====================================================================
        // Keyboard namespace
        // =====================================================================

        register("keyboard", "sync") { args ->
            val json = args.getString(0)
            currentActivity()?.let { activity ->
                activity.runOnUiThread { BlincIme.synchronize(activity, json) }
            }
            null
        }

        register("keyboard", "show") { _ ->
            // The Application context cannot show the soft keyboard — we
            // need an Activity for the decor view + input method service.
            // The Activity is stashed by `init(context)` when called with
            // an Activity (see MainActivity.onCreate), kept as a
            // WeakReference so we don't leak it.
            val activity = currentActivity()
            activity?.runOnUiThread {
                if (BlincIme.show()) return@runOnUiThread
                val imm = activity.getSystemService(Context.INPUT_METHOD_SERVICE)
                    as? InputMethodManager
                val view = activity.window?.decorView
                if (view != null && imm != null) {
                    // NativeActivity's decor view is not focusable by default,
                    // so the IMM ignores `showSoftInput` against it. Force
                    // it focusable, take focus, then request the keyboard.
                    view.isFocusable = true
                    view.isFocusableInTouchMode = true
                    view.requestFocus()
                    imm.showSoftInput(view, InputMethodManager.SHOW_FORCED)
                }
            }
            null
        }

        register("keyboard", "hide") { _ ->
            val activity = currentActivity()
            activity?.runOnUiThread {
                val imm = activity.getSystemService(Context.INPUT_METHOD_SERVICE)
                    as? InputMethodManager
                BlincIme.hide()
                val token = activity.window?.decorView?.windowToken
                imm?.hideSoftInputFromWindow(token, 0)
            }
            null
        }

        // =====================================================================
        // Camera namespace
        // =====================================================================

        register("camera", "preview_start") { args ->
            val width = args.optInt(0, 640)
            val height = args.optInt(1, 480)
            val fps = args.optInt(2, 30)
            val facing = args.optInt(3, 0) // 0=front, 1=back
            val streamId = args.optLong(4, 0)

            startCameraPreview(ctx, width, height, fps, facing, streamId)
            null
        }

        registerVoid("camera", "preview_stop") {
            stopCameraPreview()
        }

        // =====================================================================
        // Audio recording namespace
        // =====================================================================

        register("audio", "record_start") { args ->
            val sampleRate = args.optInt(0, 44100)
            val channels = args.optInt(1, 1)
            val streamId = args.optLong(2, 0)

            startAudioRecording(ctx, sampleRate, channels, streamId)
            null
        }

        registerVoid("audio", "record_stop") {
            stopAudioRecording()
        }

        // =====================================================================
        // App namespace
        // =====================================================================

        registerString("app", "get_version") {
            try {
                ctx.packageManager.getPackageInfo(ctx.packageName, 0).versionName ?: "1.0"
            } catch (e: Exception) {
                "1.0"
            }
        }

        registerString("app", "get_build_number") {
            try {
                val info = ctx.packageManager.getPackageInfo(ctx.packageName, 0)
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                    info.longVersionCode.toString()
                } else {
                    @Suppress("DEPRECATION")
                    info.versionCode.toString()
                }
            } catch (e: Exception) {
                "1"
            }
        }

        registerString("app", "get_bundle_id") {
            ctx.packageName
        }

        register("app", "open_url") { args ->
            val url = args.optString(0, "")
            try {
                val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url))
                intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                ctx.startActivity(intent)
                true
            } catch (e: Exception) {
                false
            }
        }

        register("app", "share_text") { args ->
            val text = args.optString(0, "")
            val intent = Intent(Intent.ACTION_SEND).apply {
                type = "text/plain"
                putExtra(Intent.EXTRA_TEXT, text)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            ctx.startActivity(Intent.createChooser(intent, "Share").apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            })
            null
        }
    }

    // =========================================================================
    // Helper functions
    // =========================================================================

    private fun successJson(value: Any?): String {
        val obj = JSONObject()
        obj.put("success", true)
        when (value) {
            null -> obj.put("value", JSONObject.NULL)
            is String -> obj.put("value", value)
            is Boolean -> obj.put("value", value)
            is Int -> obj.put("value", value)
            is Long -> obj.put("value", value)
            is Float -> obj.put("value", value)
            is Double -> obj.put("value", value)
            is ByteArray -> obj.put("value", android.util.Base64.encodeToString(value, android.util.Base64.NO_WRAP))
            else -> obj.put("value", value.toString())
        }
        return obj.toString()
    }

    private fun errorJson(type: String, message: String): String {
        val obj = JSONObject()
        obj.put("success", false)
        obj.put("errorType", type)
        obj.put("errorMessage", message)
        return obj.toString()
    }

    private fun vibrate(context: Context, durationMs: Long) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val vm = context.getSystemService<VibratorManager>()
            vm?.defaultVibrator?.vibrate(
                VibrationEffect.createOneShot(durationMs, VibrationEffect.DEFAULT_AMPLITUDE)
            )
        } else {
            @Suppress("DEPRECATION")
            val v = context.getSystemService<Vibrator>()
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                v?.vibrate(VibrationEffect.createOneShot(durationMs, VibrationEffect.DEFAULT_AMPLITUDE))
            } else {
                @Suppress("DEPRECATION")
                v?.vibrate(durationMs)
            }
        }
    }

    private fun vibrateWithAmplitude(context: Context, durationMs: Long, amplitude: Int) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                val vm = context.getSystemService<VibratorManager>()
                vm?.defaultVibrator?.vibrate(VibrationEffect.createOneShot(durationMs, amplitude))
            } else {
                @Suppress("DEPRECATION")
                val v = context.getSystemService<Vibrator>()
                v?.vibrate(VibrationEffect.createOneShot(durationMs, amplitude))
            }
        } else {
            vibrate(context, durationMs)
        }
    }

    // =========================================================================
    // Camera preview
    // =========================================================================

    private var cameraStreamId: Long = 0
    private var isCameraRunning = false

    /**
     * Start camera preview and stream RGBA frames to Rust via JNI.
     *
     * Uses Camera2 API. Each frame is converted to RGBA and sent via
     * nativeDispatchStreamData(streamId, rgbaBytes).
     */
    private fun startCameraPreview(
        context: Context,
        width: Int, height: Int, fps: Int, facing: Int, streamId: Long
    ) {
        cameraStreamId = streamId
        isCameraRunning = true

        // Camera2 implementation requires android.hardware.camera2 imports
        // and a background HandlerThread. This is a template — users should
        // adapt to their specific camera requirements.
        //
        // The key integration point:
        // 1. Open CameraDevice for the requested facing
        // 2. Create ImageReader with ImageFormat.YUV_420_888
        // 3. In OnImageAvailableListener, convert YUV → RGBA
        // 4. Call: nativeDispatchStreamData(streamId, rgbaBytes)
        //
        // Example conversion (simplified):
        // val image = reader.acquireLatestImage()
        // val rgba = yuvToRgba(image)  // convert planes to RGBA
        // nativeDispatchStreamData(cameraStreamId, rgba)
        // image.close()

        android.util.Log.i("BlincNativeBridge", "Camera preview started: ${width}x${height} @ ${fps}fps, stream=$streamId")
    }

    private fun stopCameraPreview() {
        isCameraRunning = false
        android.util.Log.i("BlincNativeBridge", "Camera preview stopped")
    }

    // =========================================================================
    // Audio recording
    // =========================================================================

    private var audioStreamId: Long = 0
    private var isAudioRecording = false
    private var audioRecordThread: Thread? = null

    /**
     * Start audio recording and stream PCM samples to Rust.
     *
     * Uses AudioRecord API. PCM float samples are sent as raw bytes via
     * nativeDispatchStreamData(streamId, pcmBytes).
     */
    private fun startAudioRecording(
        context: Context,
        sampleRate: Int, channels: Int, streamId: Long
    ) {
        audioStreamId = streamId
        isAudioRecording = true

        val channelConfig = if (channels == 1)
            android.media.AudioFormat.CHANNEL_IN_MONO
        else
            android.media.AudioFormat.CHANNEL_IN_STEREO

        val bufferSize = android.media.AudioRecord.getMinBufferSize(
            sampleRate, channelConfig, android.media.AudioFormat.ENCODING_PCM_FLOAT
        )

        audioRecordThread = Thread {
            try {
                val recorder = android.media.AudioRecord(
                    android.media.MediaRecorder.AudioSource.MIC,
                    sampleRate, channelConfig,
                    android.media.AudioFormat.ENCODING_PCM_FLOAT,
                    bufferSize
                )
                recorder.startRecording()

                val buffer = FloatArray(bufferSize / 4)
                while (isAudioRecording) {
                    val read = recorder.read(buffer, 0, buffer.size, android.media.AudioRecord.READ_BLOCKING)
                    if (read > 0) {
                        // Convert float array to byte array (little-endian)
                        val bytes = ByteArray(read * 4)
                        val bb = java.nio.ByteBuffer.wrap(bytes).order(java.nio.ByteOrder.LITTLE_ENDIAN)
                        for (i in 0 until read) {
                            bb.putFloat(buffer[i])
                        }
                        nativeDispatchStreamData(audioStreamId, bytes)
                    }
                }

                recorder.stop()
                recorder.release()
            } catch (e: Exception) {
                android.util.Log.e("BlincNativeBridge", "Audio recording error: ${e.message}")
            }
        }
        audioRecordThread?.start()
    }

    private fun stopAudioRecording() {
        isAudioRecording = false
        audioRecordThread?.join(1000)
        audioRecordThread = null
    }

    // JNI bridge for stream data
    @JvmStatic
    external fun nativeDispatchStreamData(streamId: Long, data: ByteArray)

    // JNI bridge for soft-keyboard inset updates.
    //
    // Called from `attachWindowInsetsListener` whenever
    // `WindowInsets.Type.ime().bottom` changes. The Rust runtime
    // (`Java_com_blinc_BlincNativeBridge_nativeDispatchKeyboardInset` in
    // `crates/blinc_app/src/android.rs`) stores the value in a global
    // atomic that the `android_main` poll loop reads on every tick to
    // drive the "scroll focused text input above the keyboard" behavior.
    //
    // The Kotlin side already converts the raw physical-pixel value
    // from `WindowInsets` into LOGICAL pixels by dividing by the
    // display density, so the Rust side gets a value directly comparable
    // to `WindowedContext.height`.
    @JvmStatic
    external fun nativeDispatchKeyboardInset(insetLogicalPx: Int)

    @JvmStatic
    external fun nativeDispatchImeEdit(json: String): Boolean

    // JNI bridge for system-bar safe-area inset updates.
    //
    // Called from `attachWindowInsetsListener` whenever the status bar,
    // navigation bar, notch cutout, or gesture bar inset changes. The
    // Rust runtime (`Java_com_blinc_BlincNativeBridge_nativeDispatchSafeArea`
    // in `crates/blinc_app/src/android.rs`) stores the four values in
    // global atomics; the `android_main` poll loop copies them into
    // `WindowedContext.safe_area` when any edge changes.
    //
    // All four values are logical pixels (already divided by display
    // density), matching `WindowedContext.width` / `height`. The tuple
    // order is `(top, right, bottom, left)` — the same order the blinc
    // `Window::safe_area_insets` trait method uses.
    @JvmStatic
    external fun nativeDispatchSafeArea(top: Int, right: Int, bottom: Int, left: Int)

    // JNI bridge for synthesized key-down events with modifier flags.
    //
    // Called from `BlincEditMenuHelper.onActionItemClicked` when the
    // user picks Cut / Copy / Paste / Select All from the native edit
    // menu. The Rust handler
    // (`Java_com_blinc_BlincNativeBridge_nativeDispatchKeyDownWithModifiers`
    // in `crates/blinc_app/src/android.rs`) queues the event and the
    // android_main poll loop dispatches it through `tree.broadcast_key_event`
    // on the next tick — which lands in the existing Cmd-shortcut
    // branch of every Blinc text-editable widget's `on_key_down`
    // handler.
    //
    // `modifiers` bitmask: shift=0x01, ctrl=0x02, alt=0x04, meta=0x08.
    // The edit menu callbacks always set the meta bit so the dispatch
    // routes through the Cmd-shortcut path.
    @JvmStatic
    external fun nativeDispatchKeyDownWithModifiers(keyCode: Int, modifiers: Int)
}

// =============================================================================
// Edit menu helper
// =============================================================================

/**
 * Native Android contextual edit menu (Cut / Copy / Paste / Select All)
 * shown over the focused Blinc text-editable widget on double-tap.
 *
 * Mirrors the iOS `BlincEditMenuHelper`. Uses
 * [android.view.ActionMode] (the framework-level equivalent of iOS's
 * UIMenuController) anchored at the position the Rust side passed in
 * via `edit_menu.show`.
 *
 * Action callbacks need to be wired through to Rust via the same
 * shortcut-key dispatch path Blinc's text-editable widgets already
 * use for Cmd+X / Cmd+C / Cmd+V / Cmd+A. That requires a JNI export
 * for `handleKeyDownWithModifiers` (or similar) which doesn't exist
 * yet — until then the menu shows on screen so the user gets visual
 * feedback that the gesture registered, and the action dispatch is
 * a TODO.
 */
object BlincEditMenuHelper {
    private var currentActionMode: android.view.ActionMode? = null

    fun show(
        activity: android.app.Activity,
        anchorX: Float,
        anchorY: Float,
        @Suppress("UNUSED_PARAMETER") selectionX: Float,
        @Suppress("UNUSED_PARAMETER") selectionY: Float,
        @Suppress("UNUSED_PARAMETER") selectionWidth: Float,
        @Suppress("UNUSED_PARAMETER") selectionHeight: Float,
        actions: Int,
    ) {
        // Dismiss any existing menu first.
        hide()

        val rootView = activity.window?.decorView?.rootView ?: return
        val callback = object : android.view.ActionMode.Callback {
            override fun onCreateActionMode(mode: android.view.ActionMode, menu: android.view.Menu): Boolean {
                if (actions and 0x01 != 0) {
                    menu.add(0, android.R.id.cut, 0, android.R.string.cut)
                }
                if (actions and 0x02 != 0) {
                    menu.add(0, android.R.id.copy, 1, android.R.string.copy)
                }
                if (actions and 0x04 != 0) {
                    menu.add(0, android.R.id.paste, 2, android.R.string.paste)
                }
                if (actions and 0x08 != 0) {
                    menu.add(0, android.R.id.selectAll, 3, android.R.string.selectAll)
                }
                return true
            }

            override fun onPrepareActionMode(mode: android.view.ActionMode, menu: android.view.Menu): Boolean = false

            override fun onActionItemClicked(mode: android.view.ActionMode, item: android.view.MenuItem): Boolean {
                // Dispatch the matching Cmd+key into Rust via the
                // JNI key-down-with-modifiers export. The Blinc
                // text-editable widgets already handle these
                // shortcut codes in their `on_key_down` handlers,
                // so the menu plugs into the same code path the
                // hardware-keyboard shortcuts use on every platform.
                //
                // We always set the meta (Cmd) modifier bit (0x08)
                // since the widget handlers gate clipboard ops
                // behind `ctx.meta`.
                val keyCode = when (item.itemId) {
                    android.R.id.cut -> 88        // Cmd+X
                    android.R.id.copy -> 67       // Cmd+C
                    android.R.id.paste -> 86      // Cmd+V
                    android.R.id.selectAll -> 65  // Cmd+A
                    else -> -1
                }
                if (keyCode >= 0) {
                    try {
                        // Fully qualified — this method lives on the
                        // top-level `BlincNativeBridge` object, not
                        // on `BlincEditMenuHelper`. Calling it
                        // unqualified from inside the helper trips
                        // an Unresolved reference at the Kotlin
                        // compile step.
                        BlincNativeBridge.nativeDispatchKeyDownWithModifiers(keyCode, 0x08)
                    } catch (e: UnsatisfiedLinkError) {
                        // Native side not loaded — silently ignore so
                        // the menu still dismisses cleanly.
                    }
                }
                mode.finish()
                return true
            }

            override fun onDestroyActionMode(mode: android.view.ActionMode) {
                if (currentActionMode === mode) {
                    currentActionMode = null
                }
            }
        }

        currentActionMode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            // API 23+: use the floating action mode anchored to a
            // rect in the root view's coordinate space, which is
            // closer to the iOS UIMenuController behavior.
            rootView.startActionMode(
                object : android.view.ActionMode.Callback2() {
                    override fun onCreateActionMode(mode: android.view.ActionMode, menu: android.view.Menu) =
                        callback.onCreateActionMode(mode, menu)
                    override fun onPrepareActionMode(mode: android.view.ActionMode, menu: android.view.Menu) =
                        callback.onPrepareActionMode(mode, menu)
                    override fun onActionItemClicked(mode: android.view.ActionMode, item: android.view.MenuItem) =
                        callback.onActionItemClicked(mode, item)
                    override fun onDestroyActionMode(mode: android.view.ActionMode) =
                        callback.onDestroyActionMode(mode)
                    override fun onGetContentRect(
                        mode: android.view.ActionMode,
                        view: android.view.View,
                        outRect: android.graphics.Rect,
                    ) {
                        // Anchor the menu over the tap point. Convert
                        // logical pixels to physical pixels using the
                        // display density (the Rust side passes
                        // logical px / DIP).
                        val density = activity.resources.displayMetrics.density
                        val px = (anchorX * density).toInt()
                        val py = (anchorY * density).toInt()
                        outRect.set(px, py, px + 1, py + 1)
                    }
                },
                android.view.ActionMode.TYPE_FLOATING,
            )
        } else {
            // API < 23: legacy primary action mode (sticks to the top
            // of the screen). Less ideal but functional.
            rootView.startActionMode(callback)
        }
    }

    fun hide() {
        currentActionMode?.finish()
        currentActionMode = null
    }
}
