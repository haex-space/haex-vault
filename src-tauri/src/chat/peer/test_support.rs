//! Vaults that chat with each other in one process (spec 072 T003, research R19): each a device of
//! its own vault (`Member::genesis`), with the Nostr-Relays of the tests on its card, contacts with
//! each other's cards, and optionally a second own device. Delivery runs over a Nostr-Relay in the
//! process ([`mock_relay`]).
//! Modelled on `extensions::peers::test_support`.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::delivery::{now_ms, Delivery};
use super::session::{Me, Session};
use super::{messages, send};
use crate::contacts::card::format::SignedCard;
use crate::contacts::card::{own_card, received};
use crate::contacts::model::{ContactInput, Head};
use crate::contacts::service::{Caller, ContactsService};
use crate::sync::servers::{self, ServerConfig};
use crate::sync::test_support::Member;
use crate::vault_gate::VaultDb;

/// Every message of a conversation, for the checks.
const ALL: messages::Page = messages::Page {
    before: None,
    limit: 100_000,
};

/// How long a test waits for something that should happen.
pub(crate) const LIMIT: Duration = Duration::from_secs(20);

/// One device of a vault in the chat tests.
pub(crate) struct ChatDevice {
    pub member: Member,
    pub db: VaultDb,
    pub me: Me,
    pub session: Arc<Session>,
    token: Mutex<CancellationToken>,
}

impl ChatDevice {
    fn of(member: Member) -> Self {
        let db = member.device.vault_db();
        let me = Me::new(
            member.vault,
            member.keys.clone(),
            member.device.db().device_id().to_string(),
        );
        Self {
            session: Session::new(me.clone()),
            member,
            db,
            me,
            token: Mutex::new(CancellationToken::new()),
        }
    }

    /// A first device of a fresh vault whose card names `relays`.
    pub(crate) fn genesis(relays: &[String]) -> Self {
        let device = Self::of(Member::genesis());
        let config = ServerConfig::only(relays.to_vec(), Vec::new());
        device
            .db
            .write_blocking(move |tx| {
                servers::write(tx, &config)?;
                Ok(own_card::refresh(tx)?)
            })
            .expect("relays and card");
        device
    }

    /// A second device of this vault, listed and with this vault's data; the card is issued again
    /// so it names both devices.
    pub(crate) fn link(&self) -> Self {
        let linked = Member::join(&self.member);
        self.member.add(&linked);
        self.db
            .write_blocking(|tx| Ok(own_card::refresh(tx)?))
            .expect("card");
        linked.device.pull_from(&self.member.device);
        Self::of(linked)
    }

    /// This device's vault card.
    pub(crate) fn card(&self) -> SignedCard {
        self.db
            .read_blocking(|r| Ok(own_card::stored(r)?))
            .expect("read")
            .expect("own card")
    }

    /// Stores `other`'s current card on a contact of this vault (a new one the first time).
    pub(crate) async fn learn(&self, other: &ChatDevice) -> String {
        self.learn_card(other.card()).await
    }

    /// Stores `card` on the contact of its vault, created the first time; answers the contact id.
    pub(crate) async fn learn_card(&self, card: SignedCard) -> String {
        let vault = card.card.vault;
        let known = self
            .db
            .read_blocking(move |r| Ok(received::stored(r, &vault)?))
            .expect("read");
        let contact_id = match known {
            Some((contact_id, _)) => contact_id,
            None => {
                ContactsService::new(self.db.clone())
                    .create(
                        &Caller::User,
                        &[],
                        ContactInput {
                            head: Head {
                                given_name: Some("Kontakt".to_owned()),
                                ..Head::default()
                            },
                            values: vec![],
                            groups: vec![],
                            targets: vec![],
                        },
                    )
                    .await
                    .expect("contact")
                    .id
            }
        };
        let id = contact_id.clone();
        self.db
            .write_blocking(move |tx| Ok(received::replace(tx, &id, &card)?))
            .expect("card");
        contact_id
    }

    /// Connects this device's delivery and runs it until [`Self::stop`] (the vault opens).
    pub(crate) async fn start(&self) {
        let relays = self
            .db
            .read_blocking(|r| Ok(super::targets::listening_relays(r)?))
            .expect("relays");
        let token = CancellationToken::new();
        let old = std::mem::replace(
            &mut *self.token.lock().unwrap_or_else(PoisonError::into_inner),
            token.clone(),
        );
        old.cancel();
        let delivery = Delivery::connect(self.db.clone(), self.me.clone(), relays, token)
            .await
            .expect("connect");
        tokio::spawn(delivery.run(Arc::clone(&self.session)));
    }

    /// Ends this device's delivery (the vault closes); [`Self::start`] opens it again.
    pub(crate) fn stop(&self) {
        self.token
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .cancel();
    }

    /// The direct conversation with `other`.
    pub(crate) fn thread_with(&self, other: &ChatDevice) -> Uuid {
        super::ids::direct_thread_id(&self.me.vault_hex, &other.me.vault_hex)
    }

    /// Opens the conversation with `other` and queues `text`; answers the message id.
    pub(crate) fn send(&self, other: &ChatDevice, text: &str) -> Uuid {
        let (me, own, peer) = (
            self.me.clone(),
            self.me.vault_hex.clone(),
            other.me.vault_hex.clone(),
        );
        let text = text.to_owned();
        let event_id = super::envelope::new_event_id();
        let id = self
            .db
            .write_blocking(move |tx| {
                let thread = super::store::open_direct(tx, &own, &peer, now_ms())?;
                Ok(send::message(tx, &me, thread, &event_id, &text, now_ms())?)
            })
            .expect("send");
        self.session.kick.notify_one();
        id
    }

    /// The messages of the conversation with `other`.
    pub(crate) fn messages_with(&self, other: &ChatDevice) -> Vec<messages::PeerMessage> {
        let (own, thread) = (self.me.vault_hex.clone(), self.thread_with(other));
        self.db
            .read_blocking(move |r| Ok(messages::list(r, &own, thread, ALL)?))
            .expect("messages")
    }

    /// Waits until `done` holds for the messages with `other`; panics after [`LIMIT`].
    pub(crate) async fn wait_for(
        &self,
        other: &ChatDevice,
        what: &str,
        done: impl Fn(&[messages::PeerMessage]) -> bool,
    ) -> Vec<messages::PeerMessage> {
        self.wait_within(LIMIT, other, what, done).await
    }

    /// [`Self::wait_for`] with its own time limit.
    pub(crate) async fn wait_within(
        &self,
        limit: Duration,
        other: &ChatDevice,
        what: &str,
        done: impl Fn(&[messages::PeerMessage]) -> bool,
    ) -> Vec<messages::PeerMessage> {
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            let found = self.messages_with(other);
            if done(&found) {
                return found;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for {what} on {}: {}",
                self.me.device_hex(),
                summary(&found)
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

impl Drop for ChatDevice {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The messages for a failure: all of a few, counts by direction and state of many.
fn summary(found: &[messages::PeerMessage]) -> String {
    if found.len() <= 10 {
        return format!("{found:?}");
    }
    let mut counts = std::collections::BTreeMap::new();
    for message in found {
        *counts
            .entry((message.own, format!("{:?}", message.delivery)))
            .or_insert(0) += 1;
    }
    format!("{} messages, (own, state) → count: {counts:?}", found.len())
}

/// Two vaults that are contacts of each other, both with `relays` on their card.
pub(crate) async fn two_vaults(relays: &[String]) -> (ChatDevice, ChatDevice) {
    let anna = ChatDevice::genesis(relays);
    let ben = ChatDevice::genesis(relays);
    anna.learn(&ben).await;
    ben.learn(&anna).await;
    (anna, ben)
}

/// A Nostr-Relay in the process and its URL. Unlike `MockRelay` (60 events a minute per
/// connection, like a strict public Nostr-Relay) it takes the load of the soak test.
pub(crate) async fn mock_relay() -> (nostr_sdk::local_relay::LocalRelay, String) {
    use nostr_sdk::local_relay::{LocalRelayBuilder, RateLimit};

    let relay = LocalRelayBuilder::default()
        .rate_limit(RateLimit {
            max_reqs: 500,
            notes_per_minute: 1_000_000,
        })
        .messages_per_minute(1_000_000)
        .build();
    relay.run().await.expect("test relay");
    let url = relay.url().await.to_string();
    (relay, url)
}
