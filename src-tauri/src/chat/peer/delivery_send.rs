//! Sending on device sessions (spec 095 T020, contracts/nostr-chat-ratchet.md "Sending"): every
//! outbox event is encrypted on the session with each receiving device and wrapped for it.
//!
//! Per device: a wrap stored for the row is published again as it is. Otherwise the session is
//! opened from the device's chat key record when there is none (fetched from its Nostr-Relays when
//! no usable one is stored), and encrypting, saving the session and storing the wrap happen in one
//! write transaction ([`seal`]). A device without a usable record gets nothing, never a wrap under
//! its device key alone; it is tried again with the next attempt at the row. After a reset from a
//! device its record is fetched again before a session is opened with it (`refetch_record`).
//!
//! A resend (T031) first fetches the record of each device on whose session nothing was ever
//! received; when the record changed, the sessions that never received go and a new one is opened.
//!
//! A reset (T030) is wrapped here too, once, to the Nostr-Relays of the device's vault.
//!
//! This is the only caller of [`wrap::wrap`] outside tests (`wrap_tests.rs` checks that).

use haex_crdt::CrdtTransaction;
use nostr::event::Event;
use vodozemac::Curve25519PublicKey;

use super::{invalid, Delivery, Published};
use crate::chat::peer::outbox::{self, Entry, FailReason};
use crate::chat::peer::ratchet::wire::{self, Plaintext};
use crate::chat::peer::ratchet::{reset, session, store, RATCHET_KIND, RESET_KIND};
use crate::chat::peer::session::Me;
use crate::chat::peer::targets::{self, Target};
use crate::chat::peer::wrap;
use crate::error::{HaexError, Result};
use crate::storage::query::Query;
use crate::sync::keys::hex;

/// The fixed reason of an attempt that found no device of a vault with a usable record.
pub(crate) const WAITING_FOR_KEYS: &str = "waitingForKeys";

/// What one attempt did for one device.
enum ForDevice {
    /// A Nostr-Relay accepted its wrap, now or earlier.
    Accepted,
    /// No session and no usable record: left out until the next attempt.
    NoRecord,
    /// The record could not be fetched; keep waiting for keys and retain the diagnostic.
    RecordUnavailable(String),
    /// Its wrap was not accepted; the reason.
    NotAccepted(String),
}

impl Delivery {
    /// Publishes `entry` to every device of `target`. The vault is done when a wrap was accepted
    /// for at least one device and for every device that has a session or a usable record.
    pub(super) async fn publish_to_devices(&self, entry: &Entry, target: &Target) -> Published {
        let mut accepted = false;
        let mut missing = false;
        let mut waiting_for_record = false;
        let mut error = String::from(WAITING_FOR_KEYS);
        for device in &target.devices {
            match self.publish_to_device(entry, device, &target.relays).await {
                Ok(ForDevice::Accepted) => accepted = true,
                Ok(ForDevice::NoRecord) => {}
                Ok(ForDevice::RecordUnavailable(reason)) => {
                    waiting_for_record = true;
                    error = reason;
                }
                Ok(ForDevice::NotAccepted(reason)) => {
                    missing = true;
                    error = reason;
                }
                Err(HaexError::InvalidInput { reason }) if reason == "chat.error.tooLarge" => {
                    return Published::Failed(FailReason::TooLarge);
                }
                Err(failure) => {
                    missing = true;
                    error = failure.to_string();
                }
            }
        }
        if accepted && !missing && !waiting_for_record {
            Published::Accepted
        } else {
            Published::NotYet {
                error,
                partial_accepted: accepted,
                none_accepted: missing && !accepted,
            }
        }
    }

    async fn publish_to_device(
        &self,
        entry: &Entry,
        device: &[u8; 32],
        relays: &[String],
    ) -> Result<ForDevice> {
        let device_hex = hex(device);
        let (outbox_id, for_device) = (entry.id.clone(), device_hex.clone());
        let stored = self
            .db
            .read(move |r| Ok(outbox::sealed(r, &outbox_id, &for_device)?))
            .await?;
        let event = match stored {
            Some(sealed) if sealed.accepted_at.is_some() => return Ok(ForDevice::Accepted),
            Some(sealed) => sealed.wrap,
            None => {
                if self.needs_record(&device_hex).await? {
                    match self.fetch_record(device, relays).await {
                        Ok(Some(_)) => {}
                        Ok(None) => return Ok(ForDevice::NoRecord),
                        Err(error) => {
                            return Ok(ForDevice::RecordUnavailable(error.to_string()));
                        }
                    }
                }
                let (me, entry, device, now) =
                    (self.me.clone(), entry.clone(), *device, self.now());
                let sealed = self
                    .db
                    .write(move |tx| Ok(seal(tx, &me, &entry, &device, now)?))
                    .await?;
                let Some(event) = sealed else {
                    return Ok(ForDevice::NoRecord);
                };
                event
            }
        };
        let reason = match self
            .client
            .send_event(&event)
            .to(relays.iter().map(String::as_str))
            .await
        {
            Ok(output) if !output.success.is_empty() => None,
            Ok(output) => Some(
                output
                    .failed
                    .values()
                    .next()
                    .cloned()
                    .unwrap_or_else(|| String::from("no Nostr-Relay accepted")),
            ),
            Err(error) => Some(error.to_string()),
        };
        if let Some(reason) = reason {
            return Ok(ForDevice::NotAccepted(reason));
        }
        let (outbox_id, now) = (entry.id.clone(), self.now());
        self.db
            .write(move |tx| {
                Ok(outbox::mark_sealed_accepted(
                    tx,
                    &outbox_id,
                    &device_hex,
                    now,
                )?)
            })
            .await?;
        Ok(ForDevice::Accepted)
    }

    /// Whether the record of `device` (hex) must be fetched before sealing for it: there is no
    /// session, and no usable stored record or a reset asked for a fresh one.
    async fn needs_record(&self, device: &str) -> Result<bool> {
        let (installation, device, now) = (self.me.installation, device.to_owned(), self.now());
        self.db
            .read(move |r| {
                Ok(store::sessions(r, installation, &device)?.is_empty()
                    && (store::peer(r, installation, &device)?.refetch_record
                        || usable_record(r, &device, now)?.is_none()))
            })
            .await
    }

    /// Before a resend of `entry`: fetches the record of every device of its vaults on whose
    /// session to send on nothing was ever received; when the record's keys changed, the sessions
    /// that never received are deleted, so the attempt opens a new one from the fetched record.
    pub(super) async fn refresh_quiet_records(
        &self,
        entry: &Entry,
        refreshed_devices: &mut std::collections::HashSet<[u8; 32]>,
    ) -> Result<()> {
        for vault in &entry.pending {
            let (me, for_vault) = (self.me.clone(), vault.clone());
            let target = self
                .db
                .read(move |r| Ok(targets::of(r, &me, &for_vault)?))
                .await?;
            for device in &target.devices {
                let (installation, device_hex) = (self.me.installation, hex(device));
                let for_device = device_hex.clone();
                let before = self
                    .db
                    .read(move |r| {
                        let quiet = store::sessions(r, installation, &for_device)?
                            .first()
                            .is_some_and(|row| row.last_received_at.is_none());
                        Ok(quiet.then(|| store::record(r, &for_device)).transpose()?)
                    })
                    .await?;
                let Some(before) = before else {
                    continue;
                };
                if !refreshed_devices.insert(*device) {
                    continue;
                }
                // Without the record the resend goes on the session it has.
                let after = match self.fetch_record(device, &target.relays).await {
                    Ok(Some(after)) => after,
                    Ok(None) => continue,
                    Err(error) => {
                        log::info!("chat: no record fetched before a resend: {error}");
                        continue;
                    }
                };
                let changed = before.is_none_or(|before| {
                    before.identity_key != after.identity_key || before.start_key != after.start_key
                });
                if changed {
                    self.db
                        .write(move |tx| {
                            for row in store::sessions(tx, installation, &device_hex)? {
                                if row.last_received_at.is_none() {
                                    store::delete_session(
                                        tx,
                                        installation,
                                        &device_hex,
                                        &row.session_id,
                                    )?;
                                }
                            }
                            Ok(())
                        })
                        .await?;
                }
            }
        }
        Ok(())
    }

    /// Sends `device` a reset, once, to the Nostr-Relays of its vault; when none accepts it, the
    /// next undecryptable event from the device may send one again.
    pub(super) async fn send_reset(&self, device: [u8; 32]) -> Result<()> {
        let me = self.me.clone();
        let relays = self
            .db
            .read(move |r| {
                Ok(match targets::vault_of_device(r, &me, &device)? {
                    Some(vault) => targets::of(r, &me, &vault)?.relays,
                    None => Vec::new(),
                })
            })
            .await?;
        let mut accepted = false;
        if !relays.is_empty() {
            self.use_relays(&relays).await;
            let event = wrap::wrap(
                RESET_KIND,
                &wire::reset_content(),
                &self.me.keys.device_secret,
                &device,
                self.now(),
            )?;
            accepted = matches!(
                self.client
                    .send_event(&event)
                    .to(relays.iter().map(String::as_str))
                    .await,
                Ok(output) if !output.success.is_empty()
            );
        }
        if !accepted {
            let me = self.me.clone();
            self.db
                .write(move |tx| Ok(reset::not_sent(tx, &me, &device)?))
                .await?;
        }
        Ok(())
    }
}

/// Encrypts the event of `entry` for `device` on its session, opened from the stored record when
/// there is none, wraps it and stores the wrap with the row; answers the wrap, or `None` when there
/// is neither a session nor a usable record (a record a reset asked to fetch again is not usable
/// before that). Runs in one write transaction, so the advanced session and the stored wrap are
/// saved together or not at all. A wrap stored already is answered as it is.
pub(crate) fn seal(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    entry: &Entry,
    device: &[u8; 32],
    now: i64,
) -> Result<Option<Event>> {
    let device_hex = hex(device);
    if let Some(sealed) = outbox::sealed(tx, &entry.id, &device_hex)? {
        return Ok(Some(sealed.wrap));
    }
    if store::sessions(tx, me.installation, &device_hex)?.is_empty() {
        if store::peer(tx, me.installation, &device_hex)?.refetch_record {
            return Ok(None);
        }
        let Some((identity, start)) = usable_record(tx, &device_hex, now)? else {
            return Ok(None);
        };
        session::open_outbound(tx, me, &device_hex, identity, start, now)?;
    }
    let plaintext = Plaintext {
        kind: entry.rumor.kind,
        content: entry.rumor.content.clone(),
    }
    .encode()?;
    let Some(rumor) = session::encrypt(tx, me, &device_hex, plaintext.as_bytes(), now)? else {
        return Ok(None);
    };
    let event = wrap::wrap(
        RATCHET_KIND,
        &rumor.encode()?,
        &me.keys.device_secret,
        device,
        now,
    )?;
    outbox::store_sealed(tx, &entry.id, &device_hex, &event)?;
    Ok(Some(event))
}

/// The keys of the stored record of `device` (hex) while it is valid at `now`.
fn usable_record(
    q: &mut impl Query,
    device: &str,
    now: i64,
) -> Result<Option<(Curve25519PublicKey, Curve25519PublicKey)>> {
    let Some(record) = store::record(q, device)?.filter(|record| record.valid_until > now) else {
        return Ok(None);
    };
    let key = |text: &str| {
        Curve25519PublicKey::from_base64(text)
            .map_err(|_| invalid("a stored record key is invalid"))
    };
    Ok(Some((key(&record.identity_key)?, key(&record.start_key)?)))
}
