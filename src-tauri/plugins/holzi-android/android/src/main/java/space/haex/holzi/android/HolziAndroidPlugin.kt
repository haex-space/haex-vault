package space.haex.holzi.android

import android.Manifest
import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.net.Network
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.OpenableColumns
import android.provider.Settings
import android.view.WindowManager
import android.webkit.WebView
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Channel
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class UriArgs {
    lateinit var uri: String
}

@InvokeArg
class SecureArgs {
    var enabled: Boolean = true
}

@InvokeArg
class InsetsArgs {
    lateinit var channel: Channel
}

@InvokeArg
class NetworkArgs {
    lateinit var channel: Channel
}

/**
 * holzi's Android platform code (spec 043, ADR-0010); the core calls it directly. The core asks for
 * the microphone (FR-029) and, before API 30, the storage (spec 044 FR-039) through the plugin's
 * own `checkPermissions`/`requestPermissions`.
 */
@TauriPlugin(
    permissions = [
        Permission(strings = [Manifest.permission.RECORD_AUDIO], alias = "microphone"),
        Permission(
            strings = [
                Manifest.permission.READ_EXTERNAL_STORAGE,
                Manifest.permission.WRITE_EXTERNAL_STORAGE,
            ],
            alias = "storage",
        ),
    ]
)
class HolziAndroidPlugin(private val activity: Activity) : Plugin(activity) {
    @Volatile private var insetsChannel: Channel? = null
    @Volatile private var lastInsets: JSObject? = null

    /**
     * The window is drawn edge to edge, so the page must keep its controls clear of the system bars,
     * the display cutout and the keyboard itself (FR-011, FR-012). The space they take goes to the
     * core in CSS pixels; the default handling of the web view still runs.
     */
    override fun load(webView: WebView) {
        super.load(webView)
        ViewCompat.setOnApplyWindowInsetsListener(webView) { view, insets ->
            val bars = insets.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
            )
            val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
            val density = view.resources.displayMetrics.density
            val next = JSObject()
            next.put("top", bars.top / density)
            next.put("right", bars.right / density)
            next.put("bottom", bars.bottom / density)
            next.put("left", bars.left / density)
            next.put("keyboard", maxOf(0, ime.bottom - bars.bottom) / density)
            lastInsets = next
            insetsChannel?.send(next)
            ViewCompat.onApplyWindowInsets(view, insets)
        }
        ViewCompat.requestApplyInsets(webView)
    }

    /** Sends the insets now and on every change to the core's channel. */
    @Command
    fun watchInsets(invoke: Invoke) {
        val args = invoke.parseArgs(InsetsArgs::class.java)
        insetsChannel = args.channel
        lastInsets?.let { args.channel.send(it) }
        invoke.resolve()
    }

    /**
     * Tells the core when the device moves to another network (FR-019, research R5): iroh cannot
     * see that from native code on Android. The network the device has when this is called is no
     * change; a later default network that differs from the last one is.
     */
    @Command
    fun watchNetwork(invoke: Invoke) {
        val args = invoke.parseArgs(NetworkArgs::class.java)
        val connectivity =
            activity.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
        val current = connectivity.activeNetwork
        connectivity.registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
            @Volatile private var last: Network? = current

            override fun onAvailable(network: Network) {
                if (network == last) return
                last = network
                args.channel.send(JSObject())
            }
        })
        invoke.resolve()
    }

    /**
     * The name the person gave the phone, else its model (research R10); the host name is
     * "localhost" on Android.
     */
    @Command
    fun deviceName(invoke: Invoke) {
        val result = JSObject()
        val named = Settings.Global.getString(activity.contentResolver, Settings.Global.DEVICE_NAME)
        result.put("name", if (named.isNullOrBlank()) Build.MODEL else named)
        invoke.resolve(result)
    }

    /** The name a document provider shows for a chosen file (contract picked-file.md). */
    @Command
    fun displayName(invoke: Invoke) {
        val args = invoke.parseArgs(UriArgs::class.java)
        val result = JSObject()
        try {
            activity.contentResolver
                .query(Uri.parse(args.uri), arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
                ?.use { cursor ->
                    if (cursor.moveToFirst() && !cursor.isNull(0)) {
                        result.put("name", cursor.getString(0))
                    }
                }
        } catch (e: Exception) {
            // No name then; the core falls back to the last part of the address.
        }
        invoke.resolve(result)
    }

    /**
     * Screen capture protection (FR-011a): no screenshots or recordings of the window, and an empty
     * preview in the recent apps view (API 33+), while a vault is open.
     */
    @Command
    fun setSecure(invoke: Invoke) {
        val args = invoke.parseArgs(SecureArgs::class.java)
        activity.runOnUiThread {
            if (args.enabled) {
                activity.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
            } else {
                activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                activity.setRecentsScreenshotEnabled(!args.enabled)
            }
            invoke.resolve()
        }
    }

    /**
     * Whether holzi may read and write the shared storage (spec 044 FR-039): "Zugriff auf alle
     * Dateien" from API 30 on, the storage permissions before.
     */
    @Command
    fun allFilesAccess(invoke: Invoke) {
        val granted = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Environment.isExternalStorageManager()
        } else {
            listOf(
                Manifest.permission.READ_EXTERNAL_STORAGE,
                Manifest.permission.WRITE_EXTERNAL_STORAGE,
            ).all { activity.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED }
        }
        val result = JSObject()
        result.put("granted", granted)
        invoke.resolve(result)
    }

    /**
     * Opens the system's page for "Zugriff auf alle Dateien" (API 30 on): holzi's own page, else
     * the list of all apps where a system has no page per app. The person comes back by themselves;
     * the window checks again then (research R11). Before API 30 nothing opens (`opened` false) and
     * the core asks for the storage permission instead.
     */
    @Command
    fun requestAllFilesAccess(invoke: Invoke) {
        val result = JSObject()
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            result.put("opened", false)
            invoke.resolve(result)
            return
        }
        activity.runOnUiThread {
            try {
                activity.startActivity(
                    Intent(
                        Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                        Uri.parse("package:${activity.packageName}"),
                    )
                )
            } catch (e: ActivityNotFoundException) {
                activity.startActivity(Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION))
            }
            result.put("opened", true)
            invoke.resolve(result)
        }
    }
}
