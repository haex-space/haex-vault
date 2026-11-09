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

#[test]
fn no_blossom_server_until_chosen_and_the_download_limit_is_per_device() {
    let vault = ChatDevice::genesis(&[]);
    let device: Uuid = vault.me.device_uuid.parse().expect("uuid");
    let (server, limit) = vault
        .db
        .read_blocking(move |r| Ok((blossom_server(r)?, auto_limit(r, device)?)))
        .expect("read");
    assert_eq!(server, None, "FR-038: none until the user picks one");
    assert_eq!(limit, DEFAULT_AUTO_LIMIT);

    vault
        .db
        .write_blocking(move |tx| {
            set_blossom_server(tx, Some(" https://blossom.example/ "))?;
            set_auto_limit(tx, device, 5_000_000)?;
            Ok(())
        })
        .expect("write");
    let other = Uuid::new_v4();
    let (server, limit, elsewhere) = vault
        .db
        .read_blocking(move |r| {
            Ok((
                blossom_server(r)?,
                auto_limit(r, device)?,
                auto_limit(r, other)?,
            ))
        })
        .expect("read");
    assert_eq!(server.as_deref(), Some("https://blossom.example/"));
    assert_eq!(limit, 5_000_000);
    assert_eq!(elsewhere, DEFAULT_AUTO_LIMIT);

    vault
        .db
        .write_blocking(|tx| Ok(set_blossom_server(tx, None)?))
        .expect("write");
    let server = vault
        .db
        .read_blocking(|r| Ok(blossom_server(r)?))
        .expect("read");
    assert_eq!(server, None, "cleared again");
}
