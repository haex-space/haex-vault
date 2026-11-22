//! Chat key records over the delivery's Nostr client (spec 095 T012, T013): publishing the own
//! record, fetching a peer device's record, and keeping the own one current without the user.
//!
//! The own record goes to the Nostr-Relays on the own card or, when the card names none, to the ones
//! this device listens on (its contacts' ones), where senders publish in that case. It is published
//! again after a new start key and when a relay it should be on did not accept it yet, not on every
//! open, so the relays do not learn when the vault opens.
//!
//! A new start key is made only after a catch-up with at least one connected Nostr-Relay: every
//! opening message waiting there for the current start key is processed before the start key two
//! renewals back is forgotten.

use nostr::event::Event;
use nostr::filter::Filter;

use super::{invalid, Delivery, CONNECT_TIMEOUT};
use crate::chat::peer::ratchet::store::{self, RecordRow};
use crate::chat::peer::ratchet::{account, record, KEY_RECORD_KIND, START_KEY_RENEW_MS};
use crate::chat::peer::targets;
use crate::error::Result;
use crate::sync::presence;

impl Delivery {
    /// Catches up, then makes the account, renews the start key and publishes the own record as
    /// due. Without a connected Nostr-Relay neither runs: a fetch would only wait for its timeout
    /// and find nothing, which must not count as a catch-up.
    pub(super) async fn catch_up_and_keep_record(&self) -> Result<()> {
        if !self.any_relay_connected().await {
            return Ok(());
        }
        self.catch_up().await?;
        self.keep_record().await
    }

    pub(super) async fn any_relay_connected(&self) -> bool {
        self.client
            .relays()
            .await
            .values()
            .any(|relay| relay.status().is_connected())
    }

    /// Makes the account when it is missing, renews the start key after [`START_KEY_RENEW_MS`] and
    /// publishes the own record when it was not accepted since or a relay it belongs on lacks it.
    async fn keep_record(&self) -> Result<()> {
        let (installation, now) = (self.me.installation, self.now());
        let (published_at, published_to) = self
            .db
            .write(move |tx| {
                account::load_or_create(tx, installation, now)?;
                let row = store::account(tx, installation)?.ok_or_else(|| invalid("no account"))?;
                if now - row.start_key_made_at >= START_KEY_RENEW_MS {
                    account::renew_start_key(tx, installation, now)?;
                    return Ok((None, Vec::new()));
                }
                Ok((
                    row.record_published_at,
                    store::record_relays(tx, installation)?,
                ))
            })
            .await?;
        let relays = self.record_relays().await?;
        let missing = relays.iter().any(|relay| !published_to.contains(relay));
        if published_at.is_none() || missing {
            self.publish_own_record().await?;
        }
        Ok(())
    }

    /// Where the own record goes: the relays on the own card, else the listening ones.
    async fn record_relays(&self) -> Result<Vec<String>> {
        self.db
            .read(|r| {
                let own = targets::own_relays(r)?;
                Ok(if own.is_empty() {
                    targets::listening_relays(r)?
                } else {
                    own
                })
            })
            .await
    }

    /// Publishes the own record; answers whether a Nostr-Relay accepted it. Without an account
    /// there is nothing to publish.
    pub(super) async fn publish_own_record(&self) -> Result<bool> {
        let relays = self.record_relays().await?;
        if relays.is_empty() {
            return Ok(false);
        }
        let (me, now) = (self.me.clone(), self.now());
        let event = self
            .db
            .read(move |r| {
                let Some((account, made_at)) = account::load(r, me.installation)? else {
                    return Ok(None);
                };
                Ok(Some(record::own_event(&me, &account, made_at, now)?))
            })
            .await?;
        let Some(event) = event else {
            return Ok(false);
        };
        self.use_relays(&relays).await;
        let accepted: Vec<String> = match self
            .client
            .send_event(&event)
            .to(relays.iter().map(String::as_str))
            .await
        {
            Ok(output) => output.success.keys().map(ToString::to_string).collect(),
            Err(error) => {
                log::warn!("chat: the key record was not published: {error}");
                Vec::new()
            }
        };
        if accepted.is_empty() {
            return Ok(false);
        }
        // A relay URL may come back normalized; note the configured ones that accepted.
        let accepted: Vec<String> = relays
            .into_iter()
            .filter(|relay| accepted.iter().any(|url| same_relay(url, relay)))
            .collect();
        let installation = self.me.installation;
        self.db
            .write(move |tx| {
                Ok(store::set_record_published(
                    tx,
                    installation,
                    now,
                    &accepted,
                )?)
            })
            .await?;
        Ok(true)
    }

    /// Fetches the record of `device` from `relays` and stores the newest one that passes
    /// [`record::accept`]; `None` when none does.
    pub(super) async fn fetch_record(
        &self,
        device: &[u8; 32],
        relays: &[String],
    ) -> Result<Option<RecordRow>> {
        if relays.is_empty() {
            return Ok(None);
        }
        self.use_relays(relays).await;
        let author = presence::public_key(device).map_err(|e| invalid(&e.to_string()))?;
        let filter = Filter::new().kind(KEY_RECORD_KIND).author(author);
        let targets: Vec<(&str, Vec<Filter>)> = relays
            .iter()
            .map(|relay| (relay.as_str(), vec![filter.clone()]))
            .collect();
        let fetched = self
            .client
            .fetch_events(targets)
            .timeout(CONNECT_TIMEOUT * 2)
            .await
            .map_err(|e| invalid(&e.to_string()))?;
        let mut events: Vec<Event> = fetched.into_iter().collect();
        events.sort_by_key(|event| std::cmp::Reverse(event.created_at));
        for event in events {
            let (me, now) = (self.me.clone(), self.now());
            let accepted = self
                .db
                .write(move |tx| Ok(record::accept(tx, &me, &event, now)?))
                .await?;
            match accepted {
                Ok(row) => return Ok(Some(row)),
                Err(refused) => log::info!("chat: refused a key record: {}", refused.code()),
            }
        }
        Ok(None)
    }
}

/// Whether two relay URLs name the same relay (nostr-sdk adds or drops a trailing slash).
fn same_relay(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

#[cfg(test)]
#[path = "delivery_records_tests.rs"]
mod tests;
