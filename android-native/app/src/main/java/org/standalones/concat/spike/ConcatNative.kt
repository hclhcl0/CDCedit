package app.cdcedit.editor.spike

import android.content.Context
import android.view.Surface

/**
 * The two JNI entry points UniFFI cannot carry (src/crates/concat-mobile/src/jni.rs).
 * Loading the library here, through System.loadLibrary, is also what runs it
 * once before JNA opens the same .so for the UniFFI calls.
 */
object ConcatNative {
    private var loaded = false

    init {
        try {
            System.loadLibrary("concat_mobile")
            loaded = true
        } catch (_: Throwable) {
        }
    }

    /** Logcat, and the JavaVM + Context that cpal's AAudio host reads. */
    @JvmStatic fun init(context: Context) {
        if (loaded) {
            try { nativeInit(context) } catch (_: Throwable) {}
        }
    }

    /** The Surface's ANativeWindow with one reference acquired; 0 for none. */
    @JvmStatic fun windowFromSurface(surface: Surface): Long {
        return if (loaded) {
            try { nativeWindowFromSurface(surface) } catch (_: Throwable) { 0L }
        } else 0L
    }

    @JvmStatic private external fun nativeInit(context: Context)
    @JvmStatic private external fun nativeWindowFromSurface(surface: Surface): Long
}
