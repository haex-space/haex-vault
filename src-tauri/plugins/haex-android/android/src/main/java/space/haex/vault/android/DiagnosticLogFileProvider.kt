package space.haex.vault.android

import androidx.core.content.FileProvider

/**
 * The file provider for the copy of the diagnostic log (spec 098). It is a class of its own, so
 * its entry in the manifest does not collide with the app's own `FileProvider`, and it exposes only
 * the cache folder `diagnostic-log` (`res/xml/diagnostic_log_paths.xml`).
 */
class DiagnosticLogFileProvider : FileProvider() {
    companion object {
        /** The folder in the app's cache that holds the copy; the core writes it there. */
        const val FOLDER = "diagnostic-log"
        const val AUTHORITY_SUFFIX = "diagnosticlog"
    }
}
