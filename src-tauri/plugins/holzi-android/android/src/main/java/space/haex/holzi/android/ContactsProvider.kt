package space.haex.holzi.android

import android.content.ContentProviderOperation
import android.content.ContentResolver
import android.content.ContentUris
import android.content.Context
import android.net.Uri
import android.provider.ContactsContract
import android.provider.ContactsContract.CommonDataKinds
import android.provider.ContactsContract.Contacts
import android.provider.ContactsContract.Data
import android.provider.ContactsContract.RawContacts
import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject

/**
 * The rows of the phone's address book (spec 051, contracts/android-plugin.md). Only maps rows of
 * the Contacts Provider to JSON and back: which rows change and how is decided in the core
 * (`contacts/phone/`). Mimetypes go by their short name; others are never read nor written
 * (FR-022). Every value goes as text: the data columns store text anyway.
 */
class ContactsProvider(private val context: Context) {
    private val resolver: ContentResolver get() = context.contentResolver

    private class Kind(val mime: String, val columns: Map<String, String>)

    private val kinds = mapOf(
        "StructuredName" to Kind(
            CommonDataKinds.StructuredName.CONTENT_ITEM_TYPE,
            mapOf(
                "display" to CommonDataKinds.StructuredName.DISPLAY_NAME,
                "given" to CommonDataKinds.StructuredName.GIVEN_NAME,
                "family" to CommonDataKinds.StructuredName.FAMILY_NAME,
                "middle" to CommonDataKinds.StructuredName.MIDDLE_NAME,
                "prefix" to CommonDataKinds.StructuredName.PREFIX,
                "suffix" to CommonDataKinds.StructuredName.SUFFIX,
            ),
        ),
        "Nickname" to Kind(
            CommonDataKinds.Nickname.CONTENT_ITEM_TYPE,
            mapOf("name" to CommonDataKinds.Nickname.NAME),
        ),
        "Organization" to Kind(
            CommonDataKinds.Organization.CONTENT_ITEM_TYPE,
            mapOf(
                "company" to CommonDataKinds.Organization.COMPANY,
                "title" to CommonDataKinds.Organization.TITLE,
            ),
        ),
        "Note" to Kind(
            CommonDataKinds.Note.CONTENT_ITEM_TYPE,
            mapOf("note" to CommonDataKinds.Note.NOTE),
        ),
        "Phone" to Kind(
            CommonDataKinds.Phone.CONTENT_ITEM_TYPE,
            mapOf(
                "value" to CommonDataKinds.Phone.NUMBER,
                "type" to CommonDataKinds.Phone.TYPE,
                "label" to CommonDataKinds.Phone.LABEL,
            ),
        ),
        "Email" to Kind(
            CommonDataKinds.Email.CONTENT_ITEM_TYPE,
            mapOf(
                "value" to CommonDataKinds.Email.ADDRESS,
                "type" to CommonDataKinds.Email.TYPE,
                "label" to CommonDataKinds.Email.LABEL,
            ),
        ),
        "Website" to Kind(
            CommonDataKinds.Website.CONTENT_ITEM_TYPE,
            mapOf(
                "value" to CommonDataKinds.Website.URL,
                "type" to CommonDataKinds.Website.TYPE,
                "label" to CommonDataKinds.Website.LABEL,
            ),
        ),
        "SipAddress" to Kind(
            CommonDataKinds.SipAddress.CONTENT_ITEM_TYPE,
            mapOf(
                "value" to CommonDataKinds.SipAddress.SIP_ADDRESS,
                "type" to CommonDataKinds.SipAddress.TYPE,
                "label" to CommonDataKinds.SipAddress.LABEL,
            ),
        ),
        "StructuredPostal" to Kind(
            CommonDataKinds.StructuredPostal.CONTENT_ITEM_TYPE,
            mapOf(
                "formatted" to CommonDataKinds.StructuredPostal.FORMATTED_ADDRESS,
                "street" to CommonDataKinds.StructuredPostal.STREET,
                "pobox" to CommonDataKinds.StructuredPostal.POBOX,
                "neighborhood" to CommonDataKinds.StructuredPostal.NEIGHBORHOOD,
                "city" to CommonDataKinds.StructuredPostal.CITY,
                "region" to CommonDataKinds.StructuredPostal.REGION,
                "postcode" to CommonDataKinds.StructuredPostal.POSTCODE,
                "country" to CommonDataKinds.StructuredPostal.COUNTRY,
                "type" to CommonDataKinds.StructuredPostal.TYPE,
                "label" to CommonDataKinds.StructuredPostal.LABEL,
            ),
        ),
        "Im" to Kind(
            CommonDataKinds.Im.CONTENT_ITEM_TYPE,
            mapOf(
                "data" to CommonDataKinds.Im.DATA,
                "type" to CommonDataKinds.Im.TYPE,
                "label" to CommonDataKinds.Im.LABEL,
                "protocol" to CommonDataKinds.Im.PROTOCOL,
                "customProtocol" to CommonDataKinds.Im.CUSTOM_PROTOCOL,
            ),
        ),
        "Event" to Kind(
            CommonDataKinds.Event.CONTENT_ITEM_TYPE,
            mapOf(
                "startDate" to CommonDataKinds.Event.START_DATE,
                "type" to CommonDataKinds.Event.TYPE,
                "label" to CommonDataKinds.Event.LABEL,
            ),
        ),
        "Relation" to Kind(
            CommonDataKinds.Relation.CONTENT_ITEM_TYPE,
            mapOf(
                "name" to CommonDataKinds.Relation.NAME,
                "type" to CommonDataKinds.Relation.TYPE,
                "label" to CommonDataKinds.Relation.LABEL,
            ),
        ),
    )

    private val shortNames = kinds.entries.associate { (short, kind) -> kind.mime to short }

    /**
     * The accounts that hold contacts, and the local account. Writable are the local account and
     * accounts whose sync adapter uploads changes. Only `READ_CONTACTS` is needed: the accounts come
     * from the raw contacts, not from the account manager.
     */
    fun accounts(): JSONArray {
        val uploading = ContentResolver.getSyncAdapterTypes()
            .filter { it.authority == ContactsContract.AUTHORITY && it.supportsUploading() }
            .map { it.accountType }
            .toSet()
        val seen = linkedSetOf(Pair("", ""))
        resolver.query(
            RawContacts.CONTENT_URI,
            arrayOf(RawContacts.ACCOUNT_TYPE, RawContacts.ACCOUNT_NAME),
            "${RawContacts.DELETED} = 0",
            null,
            null,
        )?.use { cursor ->
            while (cursor.moveToNext()) {
                seen.add(Pair(cursor.getString(0) ?: "", cursor.getString(1) ?: ""))
            }
        }
        val rows = JSONArray()
        for ((type, name) in seen) {
            val row = JSONObject()
            row.put("type", type)
            row.put("name", name)
            row.put("writable", type.isEmpty() || type in uploading)
            row.put("isDefault", false)
            rows.put(row)
        }
        return rows
    }

    /** The raw contacts with an id above `afterId`, deleted ones included, by id. */
    fun index(afterId: Long, limit: Int): JSONArray {
        val uri = RawContacts.CONTENT_URI.buildUpon()
            .appendQueryParameter(ContactsContract.LIMIT_PARAM_KEY, limit.toString())
            .build()
        val rows = JSONArray()
        val aggregates = HashSet<Long>()
        resolver.query(
            uri,
            arrayOf(
                RawContacts._ID,
                RawContacts.VERSION,
                RawContacts.DELETED,
                RawContacts.ACCOUNT_TYPE,
                RawContacts.ACCOUNT_NAME,
                RawContacts.SOURCE_ID,
                RawContacts.CONTACT_ID,
            ),
            "${RawContacts._ID} > ?",
            arrayOf(afterId.toString()),
            "${RawContacts._ID} ASC",
        )?.use { cursor ->
            while (cursor.moveToNext()) {
                val row = JSONObject()
                row.put("id", cursor.getLong(0))
                row.put("version", cursor.getLong(1))
                row.put("deleted", cursor.getInt(2) != 0)
                row.put("accountType", cursor.getString(3) ?: "")
                row.put("accountName", cursor.getString(4) ?: "")
                row.put("sourceId", cursor.getString(5) ?: JSONObject.NULL)
                val contact = if (cursor.isNull(6)) 0L else cursor.getLong(6)
                row.put("contactId", contact)
                // The provider does not answer `RAW_CONTACT_IS_READ_ONLY` to apps; the core takes a
                // raw contact in an account whose sync adapter does not upload for read-only.
                row.put("readOnly", false)
                if (contact != 0L) aggregates.add(contact)
                rows.put(row)
            }
        }
        val lookup = HashMap<Long, Pair<String, Long?>>()
        for (chunk in aggregates.chunked(200)) {
            resolver.query(
                Contacts.CONTENT_URI,
                arrayOf(Contacts._ID, Contacts.LOOKUP_KEY, Contacts.CONTACT_LAST_UPDATED_TIMESTAMP),
                "${Contacts._ID} IN (${chunk.joinToString(",")})",
                null,
                null,
            )?.use { cursor ->
                while (cursor.moveToNext()) {
                    val updated = if (cursor.isNull(2)) null else cursor.getLong(2)
                    lookup[cursor.getLong(0)] = Pair(cursor.getString(1) ?: "", updated)
                }
            }
        }
        for (i in 0 until rows.length()) {
            val row = rows.getJSONObject(i)
            val found = lookup[row.getLong("contactId")]
            row.put("lookupKey", found?.first ?: "")
            row.put("lastUpdated", found?.second ?: JSONObject.NULL)
        }
        return rows
    }

    /** The data rows of the known mimetypes of these raw contacts. */
    fun read(ids: List<Long>): JSONArray {
        val byRaw = LinkedHashMap<Long, JSONArray>()
        ids.forEach { byRaw[it] = JSONArray() }
        if (ids.isEmpty()) return JSONArray()
        val photos = HashSet<Long>()
        val mimes = (kinds.values.map { it.mime } + CommonDataKinds.Photo.CONTENT_ITEM_TYPE)
            .joinToString(",") { "'$it'" }
        resolver.query(
            Data.CONTENT_URI,
            null,
            "${Data.RAW_CONTACT_ID} IN (${ids.joinToString(",")}) AND ${Data.MIMETYPE} IN ($mimes)",
            null,
            "${Data.RAW_CONTACT_ID} ASC, ${Data._ID} ASC",
        )?.use { cursor ->
            val idColumn = cursor.getColumnIndexOrThrow(Data._ID)
            val rawColumn = cursor.getColumnIndexOrThrow(Data.RAW_CONTACT_ID)
            val mimeColumn = cursor.getColumnIndexOrThrow(Data.MIMETYPE)
            while (cursor.moveToNext()) {
                val mime = cursor.getString(mimeColumn)
                if (mime == CommonDataKinds.Photo.CONTENT_ITEM_TYPE) {
                    val file = cursor.getColumnIndex(CommonDataKinds.Photo.PHOTO_FILE_ID)
                    val thumb = cursor.getColumnIndex(CommonDataKinds.Photo.PHOTO)
                    if ((file >= 0 && !cursor.isNull(file)) || (thumb >= 0 && !cursor.isNull(thumb))) {
                        photos.add(cursor.getLong(rawColumn))
                    }
                    continue
                }
                val short = shortNames[mime] ?: continue
                val kind = kinds.getValue(short)
                val fields = JSONObject()
                for ((name, column) in kind.columns) {
                    val index = cursor.getColumnIndex(column)
                    if (index >= 0 && !cursor.isNull(index)) fields.put(name, cursor.getString(index))
                }
                val row = JSONObject()
                row.put("dataId", cursor.getLong(idColumn))
                row.put("mimetype", short)
                row.put("isReadOnly", false)
                row.put("fields", fields)
                byRaw[cursor.getLong(rawColumn)]?.put(row)
            }
        }
        val rows = JSONArray()
        for ((id, data) in byRaw) {
            val row = JSONObject()
            row.put("id", id)
            row.put("data", data)
            row.put("hasPhoto", id in photos)
            rows.put(row)
        }
        return rows
    }

    /**
     * Applies the ops as one batch and answers, per op, the raw contact it touched and that raw
     * contact's version after the batch (research R6), so the core does not take its own writes for
     * changes on the phone. A failing batch changes nothing; its error names no personal data.
     */
    fun apply(ops: JSONArray): JSONObject {
        val dataOwners = ownersOf(ops)
        val batch = ArrayList<ContentProviderOperation>()
        val refIndex = HashMap<Int, Int>()
        val opRaw = arrayOfNulls<Any>(ops.length())
        for (i in 0 until ops.length()) {
            val op = ops.getJSONObject(i)
            val builder = when (op.getString("op")) {
                "insertRaw" -> {
                    refIndex[op.getInt("ref")] = batch.size
                    opRaw[i] = Pair("ref", op.getInt("ref"))
                    ContentProviderOperation.newInsert(RawContacts.CONTENT_URI)
                        .withValue(RawContacts.ACCOUNT_TYPE, op.getString("accountType").ifEmpty { null })
                        .withValue(RawContacts.ACCOUNT_NAME, op.getString("accountName").ifEmpty { null })
                }
                "insertData" -> {
                    val kind = kinds.getValue(op.getString("mimetype"))
                    val raw = op.getJSONObject("raw")
                    val builder = ContentProviderOperation.newInsert(Data.CONTENT_URI)
                        .withValue(Data.MIMETYPE, kind.mime)
                    if (raw.has("ref")) {
                        val ref = raw.getInt("ref")
                        opRaw[i] = Pair("ref", ref)
                        builder.withValueBackReference(Data.RAW_CONTACT_ID, refIndex.getValue(ref))
                    } else {
                        opRaw[i] = raw.getLong("id")
                        builder.withValue(Data.RAW_CONTACT_ID, raw.getLong("id"))
                    }
                    withFields(builder, kind, op.getJSONObject("fields"))
                }
                "updateData" -> {
                    val dataId = op.getLong("dataId")
                    opRaw[i] = dataOwners[dataId]
                    val builder = ContentProviderOperation.newUpdate(Data.CONTENT_URI)
                        .withSelection("${Data._ID} = ?", arrayOf(dataId.toString()))
                    withFields(builder, kinds.getValue(op.getString("mimetype")), op.getJSONObject("fields"))
                }
                "deleteData" -> {
                    val dataId = op.getLong("dataId")
                    opRaw[i] = dataOwners[dataId]
                    ContentProviderOperation.newDelete(Data.CONTENT_URI)
                        .withSelection("${Data._ID} = ?", arrayOf(dataId.toString()))
                }
                "deleteRaw" -> {
                    val id = op.getLong("rawContactId")
                    opRaw[i] = id
                    ContentProviderOperation.newDelete(RawContacts.CONTENT_URI)
                        .withSelection("${RawContacts._ID} = ?", arrayOf(id.toString()))
                }
                else -> throw IllegalArgumentException("unknown op")
            }
            if (batch.isNotEmpty() && batch.size % 100 == 0) builder.withYieldAllowed(true)
            batch.add(builder.build())
        }
        val answer = JSONObject()
        val results = try {
            resolver.applyBatch(ContactsContract.AUTHORITY, batch)
        } catch (e: Exception) {
            answer.put("results", JSONArray())
            answer.put("error", e.javaClass.simpleName)
            return answer
        }
        val rawIds = arrayOfNulls<Long>(ops.length())
        for (i in 0 until ops.length()) {
            rawIds[i] = when (val raw = opRaw[i]) {
                is Long -> raw
                is Pair<*, *> -> results[refIndex.getValue(raw.second as Int)].uri?.let(ContentUris::parseId)
                else -> null
            }
        }
        val versions = versionsOf(rawIds.filterNotNull().toSet())
        val list = JSONArray()
        for (i in 0 until ops.length()) {
            val row = JSONObject()
            val id = rawIds[i]
            row.put("rawContactId", id ?: JSONObject.NULL)
            row.put("version", id?.let { versions[it] } ?: JSONObject.NULL)
            list.put(row)
        }
        answer.put("results", list)
        return answer
    }

    /** The large photo of a raw contact, else the thumbnail of its photo row; base64 JPEG. */
    fun photo(id: Long): String? {
        val raw = ContentUris.withAppendedId(RawContacts.CONTENT_URI, id)
        val display = Uri.withAppendedPath(raw, RawContacts.DisplayPhoto.CONTENT_DIRECTORY)
        try {
            resolver.openAssetFileDescriptor(display, "r")?.use { file ->
                return Base64.encodeToString(file.createInputStream().readBytes(), Base64.NO_WRAP)
            }
        } catch (e: Exception) {
            // No large photo; the thumbnail below.
        }
        resolver.query(
            Data.CONTENT_URI,
            arrayOf(CommonDataKinds.Photo.PHOTO),
            "${Data.RAW_CONTACT_ID} = ? AND ${Data.MIMETYPE} = ?",
            arrayOf(id.toString(), CommonDataKinds.Photo.CONTENT_ITEM_TYPE),
            null,
        )?.use { cursor ->
            if (cursor.moveToFirst() && !cursor.isNull(0)) {
                return Base64.encodeToString(cursor.getBlob(0), Base64.NO_WRAP)
            }
        }
        return null
    }

    private fun withFields(
        builder: ContentProviderOperation.Builder,
        kind: Kind,
        fields: JSONObject,
    ): ContentProviderOperation.Builder {
        for (name in fields.keys()) {
            val column = kind.columns[name] ?: continue
            if (fields.isNull(name)) builder.withValue(column, null)
            else builder.withValue(column, fields.getString(name))
        }
        return builder
    }

    private fun ownersOf(ops: JSONArray): Map<Long, Long> {
        val ids = (0 until ops.length())
            .map { ops.getJSONObject(it) }
            .filter { it.has("dataId") }
            .map { it.getLong("dataId") }
        val owners = HashMap<Long, Long>()
        for (chunk in ids.chunked(200)) {
            resolver.query(
                Data.CONTENT_URI,
                arrayOf(Data._ID, Data.RAW_CONTACT_ID),
                "${Data._ID} IN (${chunk.joinToString(",")})",
                null,
                null,
            )?.use { cursor ->
                while (cursor.moveToNext()) owners[cursor.getLong(0)] = cursor.getLong(1)
            }
        }
        return owners
    }

    private fun versionsOf(ids: Set<Long>): Map<Long, Long> {
        val versions = HashMap<Long, Long>()
        for (chunk in ids.chunked(200)) {
            resolver.query(
                RawContacts.CONTENT_URI,
                arrayOf(RawContacts._ID, RawContacts.VERSION),
                "${RawContacts._ID} IN (${chunk.joinToString(",")})",
                null,
                null,
            )?.use { cursor ->
                while (cursor.moveToNext()) versions[cursor.getLong(0)] = cursor.getLong(1)
            }
        }
        return versions
    }
}
