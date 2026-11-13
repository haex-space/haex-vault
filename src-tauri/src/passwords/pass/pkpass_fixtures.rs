//! Pass files built in code for the tests of the reader and of the import (spec 092, T063): a ZIP
//! with `pass.json` and pictures, throwaway data only; no real pass is in the repository.

use std::io::Write;

use serde_json::{json, Value};

/// A PNG header naming a 64 by 64 picture, then throwaway bytes: enough for the reader, which reads
/// only the header.
pub(super) const PNG: &[u8] =
    b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x40\x00\x00\x00\x40throwaway-picture";

pub(super) fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        writer.start_file(*name, options).expect("start");
        writer.write_all(data).expect("write");
    }
    writer.finish().expect("finish").into_inner()
}

pub(super) fn file(pass: &Value, extra: &[(&str, &[u8])]) -> Vec<u8> {
    let json = serde_json::to_vec(pass).expect("json");
    let mut entries: Vec<(&str, &[u8])> = vec![("pass.json", &json)];
    entries.extend_from_slice(extra);
    zip(&entries)
}

pub(super) fn train() -> Value {
    json!({
        "formatVersion": 1,
        "passTypeIdentifier": "pass.example.rail",
        "serialNumber": "S-1",
        "organizationName": "Example Rail",
        "description": "Train ticket",
        "logoText": "ICE",
        "relevantDate": "2026-11-14T08:30:00+01:00",
        "expirationDate": "2026-11-14T23:59:00+01:00",
        "webServiceURL": "https://updates.example.org/passes",
        "authenticationToken": "token-token-token-1",
        "backgroundColor": "rgb(1,2,3)",
        "barcodes": [
            { "format": "PKBarcodeFormatAztec", "message": "TICKET-DATA-1", "altText": "Ticket" },
            { "format": "PKBarcodeFormatQR", "message": "https://example.org/t/1" }
        ],
        "boardingPass": {
            "transitType": "PKTransitTypeTrain",
            "primaryFields": [
                { "key": "from", "label": "From", "value": "Berlin" },
                { "key": "to", "label": "To", "value": "München" }
            ],
            "auxiliaryFields": [
                { "key": "seat", "label": "Seat", "value": "42", "changeMessage": "Seat is %@" }
            ],
            "backFields": [
                { "key": "terms", "label": "Terms", "value": "Be on time." }
            ]
        }
    })
}
