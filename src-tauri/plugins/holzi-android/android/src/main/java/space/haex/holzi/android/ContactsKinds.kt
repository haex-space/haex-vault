package space.haex.holzi.android

import android.provider.ContactsContract.CommonDataKinds

/** A mimetype of the address book: its full name and its columns by the names the core uses. */
internal class Kind(val mime: String, val columns: Map<String, String>)

/**
 * The mimetypes the core reads and writes, by their short name (spec 051,
 * contracts/android-plugin.md). Others are never read nor written (FR-022).
 */
internal val contactKinds = mapOf(
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
    "GroupMembership" to Kind(
        CommonDataKinds.GroupMembership.CONTENT_ITEM_TYPE,
        mapOf("groupId" to CommonDataKinds.GroupMembership.GROUP_ROW_ID),
    ),
)
