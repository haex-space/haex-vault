use super::*;

#[test]
fn dates_of_listings_and_heads_become_milliseconds() {
    assert_eq!(
        parse_iso_ms("2026-10-06T10:00:00.000Z"),
        Some(1_791_280_800_000)
    );
    assert_eq!(
        parse_iso_ms("2026-10-06T10:00:00Z"),
        Some(1_791_280_800_000)
    );
    assert_eq!(
        parse_http_date_ms("Tue, 06 Oct 2026 10:00:00 GMT"),
        Some(1_791_280_800_000)
    );
    assert_eq!(parse_iso_ms("gestern"), None);
    assert_eq!(parse_http_date_ms("Tue, 06 Okt 2026 10:00:00 GMT"), None);
}
