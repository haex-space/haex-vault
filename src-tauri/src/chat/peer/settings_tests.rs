use super::*;
use crate::chat::peer::test_support::ChatDevice;

#[test]
fn both_settings_are_on_until_turned_off_and_garbage_reads_as_on() {
    let vault = ChatDevice::genesis(&[]);
    let device: Uuid = vault.me.device_uuid.parse().expect("uuid");
    let (receipts, notify) = vault
        .db
        .read_blocking(move |r| Ok((read_receipts(r)?, notifications(r, device)?)))
        .expect("read");
    assert!(receipts && notify);

    vault
        .db
        .write_blocking(move |tx| {
            set_read_receipts(tx, false)?;
            set_notifications(tx, device, false)?;
            Ok(())
        })
        .expect("write");
    let other = Uuid::new_v4();
    let (receipts, notify, elsewhere) = vault
        .db
        .read_blocking(move |r| {
            Ok((
                read_receipts(r)?,
                notifications(r, device)?,
                notifications(r, other)?,
            ))
        })
        .expect("read");
    assert!(!receipts && !notify);
    assert!(elsewhere, "notifications are per device");

    vault
        .db
        .write_blocking(|tx| {
            preferences::insert_or_update(tx, PrefScope::Vault, KEY_READ_RECEIPTS, "vielleicht")?;
            Ok(())
        })
        .expect("garbage");
    assert!(vault
        .db
        .read_blocking(|r| Ok(read_receipts(r)?))
        .expect("read"));
}
