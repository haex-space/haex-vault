package space.haex.holzi.android

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.net.Uri
import android.os.Parcelable
import android.provider.OpenableColumns
import app.tauri.plugin.Channel
import app.tauri.plugin.JSObject
import java.io.File
import java.util.UUID
import java.util.concurrent.Executors

/**
 * Files another app shares to holzi (spec 092, FR-037, `contracts/android-intents.md`). The read
 * permission of a shared address is transient, so each stream is copied at once into the app's own
 * cache directory and the copy is reported to the core, which holds it until a vault is open. A
 * share that arrives before the core listens (a cold start by a share) is buffered here. Nothing is
 * read or imported here, and a stream over the limit is not kept. The copying runs on one worker of
 * its own, one job after the other (the sweep before the first copy), so a slow provider cannot
 * block the UI thread; the permission of a shared address lasts as long as the activity does.
 */
class SharedFilesIntake(private val activity: Activity) {
    @Volatile private var channel: Channel? = null
    private val buffered = ArrayList<JSObject>()
    private val worker = Executors.newSingleThreadExecutor { task ->
        Thread(task, "holzi-shared-files").also { it.isDaemon = true }
    }

    private val directory: File get() = File(activity.cacheDir, DIRECTORY)

    /** Removes the copies of an earlier process; the person shares again if one was never used. */
    fun sweep() {
        worker.execute { directory.listFiles()?.forEach { it.delete() } }
    }

    /**
     * Takes the files of a share intent (`SEND`, `SEND_MULTIPLE`); any other intent is ignored. The
     * addresses are read here (cheap); the copying is done by the worker.
     */
    fun handle(intent: Intent?) {
        if (intent == null) return
        if (intent.action != Intent.ACTION_SEND && intent.action != Intent.ACTION_SEND_MULTIPLE) return
        val type = intent.type ?: ""
        val uris = streams(intent)
        // A restart of the process must not take the same share again.
        intent.action = Intent.ACTION_MAIN
        intent.removeExtra(Intent.EXTRA_STREAM)
        intent.clipData = null
        if (uris.isEmpty()) return
        worker.execute {
            for (uri in uris) {
                copy(uri, type)?.let(::report)
            }
        }
    }

    /** Starts reporting to the core's channel; what came before is reported now. */
    fun watch(target: Channel) {
        val waiting = synchronized(buffered) {
            channel = target
            ArrayList(buffered).also { buffered.clear() }
        }
        waiting.forEach { target.send(it) }
    }

    private fun report(file: JSObject) {
        val target = synchronized(buffered) {
            channel ?: run {
                buffered.add(file)
                null
            }
        }
        target?.send(file)
    }

    /**
     * The addresses a share intent carries. The sender is any app, so nothing is trusted: an extra
     * that is no address is skipped (it must not crash the start), and an address is taken only if
     * [readable] says so.
     */
    @Suppress("DEPRECATION")
    private fun streams(intent: Intent): List<Uri> {
        val found = LinkedHashSet<Uri>()
        try {
            if (intent.action == Intent.ACTION_SEND) {
                (intent.getParcelableExtra<Parcelable>(Intent.EXTRA_STREAM) as? Uri)?.let(found::add)
            } else {
                intent.getParcelableArrayListExtra<Parcelable>(Intent.EXTRA_STREAM)
                    ?.filterIsInstance<Uri>()
                    ?.let(found::addAll)
            }
            val clip: ClipData? = intent.clipData
            if (clip != null) {
                for (index in 0 until clip.itemCount) clip.getItemAt(index).uri?.let(found::add)
            }
        } catch (e: RuntimeException) {
            // A malformed extra (a class this app cannot read): the share is dropped, not the app.
            return emptyList()
        }
        return found.filter(::readable).take(MAX_FILES)
    }

    /**
     * Whether holzi may open `uri` for the sender. The read permission of a shared address belongs
     * to the sender's grant, but this app opens it with its own rights: a `file:` address would read
     * any file the app can (its vault, its settings) and an address of the app's own provider would
     * read its cache, so only `content:` addresses of other apps are copied.
     */
    private fun readable(uri: Uri): Boolean {
        if (uri.scheme != "content") return false
        val authority = uri.authority ?: return false
        val own = activity.packageName
        return authority != own && !authority.startsWith("$own.")
    }

    /** The copy of one stream as the report the core reads, or `null` if it could not be kept. */
    private fun copy(uri: Uri, intentType: String): JSObject? {
        val name = displayName(uri) ?: uri.lastPathSegment ?: "file"
        val mime = activity.contentResolver.getType(uri) ?: intentType
        directory.mkdirs()
        // The name on disk is only an id plus the ending, never the sender's name.
        val ending = name.substringAfterLast('.', "").lowercase().filter { it.isLetterOrDigit() }.take(8)
        val target = File(directory, UUID.randomUUID().toString() + if (ending.isEmpty()) "" else ".$ending")
        try {
            val copied = activity.contentResolver.openInputStream(uri)?.use { input ->
                target.outputStream().use { output ->
                    val buffer = ByteArray(BUFFER)
                    var total = 0L
                    while (true) {
                        val read = input.read(buffer)
                        if (read < 0) break
                        total += read
                        // Over the limit: stop reading, the partial copy is removed below.
                        if (total > MAX_BYTES) return@use false
                        output.write(buffer, 0, read)
                    }
                    true
                }
            } ?: false
            if (!copied) {
                target.delete()
                return null
            }
        } catch (e: Exception) {
            target.delete()
            return null
        }
        val report = JSObject()
        report.put("path", target.absolutePath)
        report.put("name", name)
        report.put("mime", mime)
        return report
    }

    private fun displayName(uri: Uri): String? = try {
        activity.contentResolver
            .query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor ->
                if (cursor.moveToFirst() && !cursor.isNull(0)) cursor.getString(0) else null
            }
    } catch (e: Exception) {
        null
    }

    private companion object {
        const val DIRECTORY = "shared-files"
        /** As an attachment (spec 034 FR-020); the core checks again. */
        const val MAX_BYTES = 25L * 1024 * 1024
        const val MAX_FILES = 20
        const val BUFFER = 64 * 1024
    }
}
