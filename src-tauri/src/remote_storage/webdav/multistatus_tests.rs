//! `PROPFIND` answers as Nextcloud (SabreDAV) and Apache `mod_dav` send them (spec 050 research
//! R2): prefixes and namespaces, hrefs as paths and as full addresses, percent-encoded names,
//! entities, weak ETags, and entries outside the requested path dropped.

use super::multistatus::{parse, Entry};
use crate::remote_storage::StorageError;

const NEXTCLOUD: &str = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns" xmlns:oc="http://owncloud.org/ns">
 <d:response>
  <d:href>/remote.php/dav/files/anna/Fotos/</d:href>
  <d:propstat>
   <d:prop><d:resourcetype><d:collection/></d:resourcetype>
    <d:getlastmodified>Mon, 12 Oct 2026 10:00:00 GMT</d:getlastmodified>
    <d:getetag>"6721a"</d:getetag></d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
  <d:propstat>
   <d:prop><d:getcontentlength/></d:prop>
   <d:status>HTTP/1.1 404 Not Found</d:status>
  </d:propstat>
 </d:response>
 <d:response>
  <d:href>/remote.php/dav/files/anna/Fotos/a%20b.jpg</d:href>
  <d:propstat>
   <d:prop><d:resourcetype/><d:getcontentlength>12</d:getcontentlength>
    <d:getlastmodified>Mon, 12 Oct 2026 10:01:00 GMT</d:getlastmodified>
    <d:getetag>"abc123"</d:getetag><oc:href>/not/this</oc:href></d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
 <d:response>
  <d:href>/remote.php/dav/files/anna/Fotos/Urlaub%20%26%20Meer/</d:href>
  <d:propstat>
   <d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
</d:multistatus>"#;

const APACHE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:ns0="DAV:">
<D:response xmlns:lp1="DAV:" xmlns:lp2="http://apache.org/dav/props/">
<D:href>https://u123.your-storagebox.de/holzi/Q%26A.txt</D:href>
<D:propstat>
<D:prop>
<lp1:resourcetype/>
<lp1:getcontentlength>7</lp1:getcontentlength>
<lp1:getlastmodified>Mon, 12 Oct 2026 09:00:00 GMT</lp1:getlastmodified>
<lp1:getetag>W/"7-5e1"</lp1:getetag>
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
</D:response>
<D:response>
<D:href>/elsewhere/secret.txt</D:href>
<D:propstat><D:prop><lp1:resourcetype xmlns:lp1="DAV:"/></D:prop>
<D:status>HTTP/1.1 200 OK</D:status></D:propstat>
</D:response>
</D:multistatus>"#;

#[test]
fn a_nextcloud_answer_reads_as_entries_of_the_folder() {
    let entries = parse(NEXTCLOUD, "/remote.php/dav/files/anna/Fotos/").expect("parse");
    assert_eq!(
        entries,
        [
            Entry {
                name: String::new(),
                is_dir: true,
                size: 0,
                modified: Some("Mon, 12 Oct 2026 10:00:00 GMT".to_owned()),
                etag: Some("6721a".to_owned()),
                quota_used: None,
                quota_available: None,
            },
            Entry {
                name: "a b.jpg".to_owned(),
                is_dir: false,
                size: 12,
                modified: Some("Mon, 12 Oct 2026 10:01:00 GMT".to_owned()),
                etag: Some("abc123".to_owned()),
                quota_used: None,
                quota_available: None,
            },
            Entry {
                name: "Urlaub & Meer".to_owned(),
                is_dir: true,
                size: 0,
                modified: None,
                etag: None,
                quota_used: None,
                quota_available: None,
            },
        ]
    );
}

#[test]
fn an_apache_answer_with_full_addresses_and_weak_etags_reads_too() {
    let entries = parse(APACHE, "/holzi/").expect("parse");
    assert_eq!(entries.len(), 1, "the entry outside the folder is dropped");
    assert_eq!(entries[0].name, "Q&A.txt");
    assert_eq!(entries[0].size, 7);
    assert_eq!(entries[0].etag.as_deref(), Some("7-5e1"));
    assert!(!entries[0].is_dir);
}

#[test]
fn entities_in_an_href_are_resolved() {
    let xml = r#"<d:multistatus xmlns:d="DAV:"><d:response>
        <d:href>/dav/Tom&amp;Jerry&#46;txt</d:href>
        <d:propstat><d:prop><d:getcontentlength>1</d:getcontentlength></d:prop>
        <d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#;
    let entries = parse(xml, "/dav/").expect("parse");
    assert_eq!(entries[0].name, "Tom&Jerry.txt");
}

#[test]
fn an_answer_that_does_not_parse_is_a_protocol_error() {
    assert_eq!(
        parse("<d:multistatus xmlns:d=\"DAV:\"></d:wrong>", "/").err(),
        Some(StorageError::Protocol)
    );
}

#[test]
fn the_quota_of_a_folder_reads_as_used_and_available() {
    let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:"><d:response><d:href>/remote.php/dav/files/anna/</d:href>
<d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype>
<d:quota-used-bytes>1230</d:quota-used-bytes><d:quota-available-bytes>8770</d:quota-available-bytes>
</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#;
    let entries = parse(xml, "/remote.php/dav/files/anna/").expect("parse");
    assert_eq!(entries[0].quota_used, Some(1230));
    assert_eq!(entries[0].quota_available, Some(8770));
}

#[test]
fn an_unlimited_quota_keeps_its_negative_mark() {
    let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:"><d:response><d:href>/dav/</d:href>
<d:propstat><d:prop><d:quota-used-bytes>5</d:quota-used-bytes>
<d:quota-available-bytes>-3</d:quota-available-bytes></d:prop>
<d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#;
    let entries = parse(xml, "/dav/").expect("parse");
    assert_eq!(entries[0].quota_available, Some(-3));
}
