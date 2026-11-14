package space.haex.holzi.android

import android.app.Activity
import android.content.Intent
import android.content.pm.ShortcutInfo
import android.content.pm.ShortcutManager
import android.graphics.drawable.Icon
import android.net.Uri
import android.os.Build

/**
 * The pinned shortcut of a pass on the home screen (spec 092, FR-038, `contracts/android-intents.md`).
 * The shortcut opens the app with `haex-vault://pass/<id>` and holds nothing else: the id is an
 * opaque UUID, the label is what the person typed, and neither a code nor the title is in it. The
 * core checks the id and the label too; this refuses anything that is not a UUID or has no label.
 */
class PassShortcut(private val activity: Activity) {
    /** Whether the launcher can pin shortcuts (API 26 and above, and the launcher agrees). */
    fun supported(): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return false
        val manager = activity.getSystemService(ShortcutManager::class.java) ?: return false
        return manager.isRequestPinShortcutSupported
    }

    /** Asks the launcher to pin the shortcut; the system asks the person to confirm. */
    fun pin(id: String, label: String): Boolean {
        if (!UUID_FORM.matches(id)) return false
        val name = label.filter { !it.isISOControl() }.trim().take(MAX_LABEL)
        if (name.isEmpty() || !supported()) return false
        val manager = activity.getSystemService(ShortcutManager::class.java) ?: return false
        val open = Intent(Intent.ACTION_VIEW, Uri.parse("$LINK_PREFIX${id.lowercase()}"))
            .setPackage(activity.packageName)
        val info = ShortcutInfo.Builder(activity, "pass-${id.lowercase()}")
            .setShortLabel(name)
            .setLongLabel(name)
            .setIcon(Icon.createWithResource(activity, activity.applicationInfo.icon))
            .setIntent(open)
            .build()
        return manager.requestPinShortcut(info, null)
    }

    private companion object {
        const val LINK_PREFIX = "haex-vault://pass/"
        const val MAX_LABEL = 40
        val UUID_FORM = Regex(
            "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
        )
    }
}
