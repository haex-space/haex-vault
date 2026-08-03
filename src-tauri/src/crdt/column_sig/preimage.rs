pub const DOMAIN_TAG: &str = "haex/space-col-sig/v1";

pub(crate) fn push_field(buf: &mut Vec<u8>, field: &[u8]) {
    let len = u32::try_from(field.len()).expect("field length exceeds u32");
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(field);
}

pub fn build_preimage(
    space_id: &[u8],
    table_name: &[u8],
    row_pks: &[u8],
    column_name: &[u8],
    hlc: &[u8],
    author_did: &[u8],
    value_bytes: &[u8],
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(
        4 + DOMAIN_TAG.len()
            + 4
            + space_id.len()
            + 4
            + table_name.len()
            + 4
            + row_pks.len()
            + 4
            + column_name.len()
            + 4
            + hlc.len()
            + 4
            + author_did.len()
            + 4
            + value_bytes.len(),
    );
    push_field(&mut buf, DOMAIN_TAG.as_bytes());
    push_field(&mut buf, space_id);
    push_field(&mut buf, table_name);
    push_field(&mut buf, row_pks);
    push_field(&mut buf, column_name);
    push_field(&mut buf, hlc);
    push_field(&mut buf, author_did);
    push_field(&mut buf, value_bytes);
    buf
}
