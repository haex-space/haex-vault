//! Vaults in one process for the tests of channels (spec 057 T021): each a device of its own vault
//! (`Member::genesis`) on a loopback endpoint without relays, with the `haex-vault-ext-peer/1` handler,
//! an extension host whose window is recorded, and the same test extension installed.
//! [`open_test_channel`] opens a channel without invitation or lobby, which only tests can.
//!
//! Without relays a join code names no way to the host; [`Vault::learn`] tells a vault where
//! another one listens, as the relay of the code would. The relay path itself is checked in the
//! E2E scene, which has a relay.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use haex_crdt::rusqlite::params;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::RelayMode;
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use super::admission::{verify_incoming, PeerContext, PeerProtocol, PURPOSE};
use super::channel::{self, Channel, Frames, Opening, Origin};
use super::lobby::{self, Visibility};
use super::ExtensionRef;
use crate::contacts::card::own_card;
use crate::contacts::service::{Caller, CardInto, ContactsService};
use crate::extensions::bridge::dispatch::Emit;
use crate::extensions::bridge::events::FRAME_EVENT;
use crate::extensions::fake_desktop::FakeDesktop;
use crate::extensions::host::ExtensionHost;
use crate::extensions::ids::{extension_id, ExtensionName, PublicKey};
use crate::extensions::permissions::store::{put, NewPermission};
use crate::extensions::permissions::VAULT_WIDE;
use crate::sync::endpoint::{NodeConfig, NodeTasks, Protocols, SyncNode};
use crate::sync::foreign_auth;
use crate::sync::test_support::Member;
use crate::sync::wire::EXT_PEER_ALPN;
use crate::vault_gate::VaultDb;

/// The publisher key of the test extensions.
pub(crate) const KEY: &str = "abababababababababababababababababababababababababababababababab";
/// The test extension every vault has.
pub(crate) const NAME: &str = "peer-probe";
/// How long a test waits for something that should happen.
pub(crate) const LIMIT: Duration = Duration::from_secs(20);

/// holzi's window as the tests see it: every event, with when it arrived.
pub(crate) struct Window {
    events: Mutex<Vec<(Instant, String, Value)>>,
    count: watch::Sender<usize>,
}

impl Emit for Window {
    fn emit(&self, event: &str, payload: Value) {
        let len = {
            let mut events = self.events.lock().unwrap_or_else(PoisonError::into_inner);
            events.push((Instant::now(), event.to_owned(), payload));
            events.len()
        };
        self.count.send_replace(len);
    }
}

impl Window {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::default(),
            count: watch::channel(0).0,
        })
    }

    /// The frame events of `event_type`: `(arrival, frame, data)`.
    pub(crate) fn frame_events(&self, event_type: &str) -> Vec<(Instant, String, Value)> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(_, event, payload)| event == FRAME_EVENT && payload["type"] == event_type)
            .map(|(at, _, payload)| {
                (
                    *at,
                    payload["frame"].as_str().unwrap_or_default().to_owned(),
                    payload["data"].clone(),
                )
            })
            .collect()
    }

    /// The events holzi's own window got under `event`, in order.
    pub(crate) fn window_events(&self, event: &str) -> Vec<Value> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(_, name, _)| name == event)
            .map(|(_, _, payload)| payload.clone())
            .collect()
    }

    /// Every frame event, of any type.
    pub(crate) fn all_frame_events(&self) -> usize {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(_, event, _)| event == FRAME_EVENT)
            .count()
    }

    /// Waits until `done` holds, at most [`LIMIT`].
    pub(crate) async fn wait_until(&self, done: impl Fn(&Self) -> bool) {
        let mut count = self.count.subscribe();
        tokio::time::timeout(LIMIT, async {
            while !done(self) {
                if count.changed().await.is_err() {
                    return;
                }
            }
        })
        .await
        .expect("the window heard it in time");
    }
}

/// How a vault answers incoming connections on `haex-vault-ext-peer/1`.
pub(crate) enum Admit {
    /// The real handler, with this wait for the admission message.
    Real(Duration),
    /// A channel for the test extension right after the exchange.
    AtOnce,
}

/// One device of its own vault, ready for channels.
pub(crate) struct Vault {
    pub member: Member,
    pub db: VaultDb,
    pub host: Arc<ExtensionHost>,
    pub window: Arc<Window>,
    /// The notifications this vault's system was asked to show.
    pub desktop: Arc<FakeDesktop>,
    pub node: Arc<SyncNode>,
    pub ctx: Arc<PeerContext>,
    /// The test extension, installed here.
    pub extension_id: Uuid,
    /// Channels the [`Admit::AtOnce`] handler opened.
    opened: tokio::sync::Mutex<mpsc::UnboundedReceiver<Arc<Channel>>>,
}

impl Vault {
    /// A new vault with the test extension and a frame of it open.
    pub(crate) async fn start(admit: Admit) -> Self {
        Self::of(Member::genesis(), admit).await
    }

    /// `member`'s device with the test extension and a frame of it open.
    pub(crate) async fn of(member: Member, admit: Admit) -> Self {
        let db = member.device.vault_db();
        let host = Arc::new(ExtensionHost::default());
        let window = Window::new();
        let desktop = Arc::new(FakeDesktop::default());
        host.set_desktop(desktop.clone());
        let ctx = Arc::new(PeerContext {
            keys: member.keys.clone(),
            vault: member.vault,
            db: db.clone(),
            frames: Frames::new(window.clone(), &host),
        });
        let extension_id = install(&db, NAME);
        let (opened_tx, opened) = mpsc::unbounded_channel();
        let handler_ctx = Arc::clone(&ctx);
        let protocols = match admit {
            Admit::Real(limit) => Protocols::default().with(EXT_PEER_ALPN, move |tasks| {
                PeerProtocol::new(handler_ctx, tasks).with_admission_limit(limit)
            }),
            Admit::AtOnce => Protocols::default().with(EXT_PEER_ALPN, move |tasks| OpenAtOnce {
                ctx: handler_ctx,
                tasks,
                extension_id,
                opened: opened_tx,
            }),
        };
        let node = SyncNode::bind_with(
            Arc::clone(&member.device.replica),
            member.keys.clone(),
            member.vault,
            NodeConfig {
                relay_mode: RelayMode::Disabled,
                bind_addr: Some((std::net::Ipv4Addr::LOCALHOST, 0).into()),
                session: None,
            },
            Arc::new(|_| {}),
            protocols,
        )
        .await
        .expect("bind");
        let node = Arc::new(node);
        host.peers.attach(Arc::clone(&ctx), &node);
        let vault = Self {
            member,
            db,
            host,
            window,
            desktop,
            node,
            ctx,
            extension_id,
            opened: tokio::sync::Mutex::new(opened),
        };
        vault.open_frame(extension_id);
        vault
    }

    /// Closes every open frame of the test extension, as when its tab was closed.
    pub(crate) fn close_frames(&self) {
        for session in self.host.frames.of_extension(self.extension_id) {
            self.host.frames.close(&session.frame);
        }
    }

    /// Opens a frame of `extension_id`; returns its id.
    pub(crate) fn open_frame(&self, extension_id: Uuid) -> String {
        self.host
            .frames
            .open(extension_id, Uuid::new_v4(), &Uuid::new_v4().to_string())
            .frame
            .clone()
    }

    /// The vault's identity.
    pub(crate) fn vault(&self) -> [u8; 32] {
        self.member.vault
    }

    /// Tells this vault where `other` listens, as the relay of a join code would.
    pub(crate) async fn learn(&self, other: &Vault) {
        self.node.note_presence(other.node.addr()).await;
    }

    /// Opens the lobby `lobby_id` of the test extension; returns its join code.
    pub(crate) fn open_lobby(&self, lobby_id: &str, visibility: Visibility) -> String {
        lobby::open(
            &self.host.peers,
            self.extension_id,
            probe(),
            lobby_id,
            visibility,
        )
        .expect("open a lobby")
    }

    /// Joins the lobby of `code` with the test extension as `name`.
    pub(crate) async fn join(
        &self,
        code: &str,
        name: &str,
    ) -> Result<super::join::Joined, super::PeerError> {
        super::join::join(
            &self.host.peers,
            self.extension_id,
            probe(),
            code,
            name,
            &serde_json::json!({ "skin": name }),
        )
        .await
    }

    /// The next channel the [`Admit::AtOnce`] handler opened.
    pub(crate) async fn accepted(&self) -> Arc<Channel> {
        tokio::time::timeout(LIMIT, self.opened.lock().await.recv())
            .await
            .expect("a channel in time")
            .expect("the handler runs")
    }

    /// Dials `other` and runs the exchange as the dialing side; the connection and the first
    /// stream are the caller's.
    pub(crate) async fn dial(
        &self,
        other: &Vault,
    ) -> (
        Connection,
        iroh::endpoint::SendStream,
        iroh::endpoint::RecvStream,
        Result<foreign_auth::Verified, foreign_auth::ForeignAuthError>,
    ) {
        let connection = self
            .node
            .dial_alpn(other.node.addr(), EXT_PEER_ALPN)
            .await
            .expect("dial");
        let (mut send, mut recv) = connection.open_bi().await.expect("stream");
        let remote = *connection.remote_id().as_bytes();
        let verified = foreign_auth::dial(
            &mut send,
            &mut recv,
            &self.ctx.local(self.host.peers.seen()),
            PURPOSE,
            remote,
        )
        .await;
        (connection, send, recv, verified)
    }

    /// Grants the test extension `peers`/`connect`/`*`, remembered for the vault.
    pub(crate) fn grant_peers(&self, status: &str) {
        self.grant("peers", "connect", status);
    }

    /// Grants the test extension `kind`/`action` on `*`, remembered for the vault.
    pub(crate) fn grant(&self, kind: &'static str, action: &'static str, status: &str) {
        let extension_id = self.extension_id;
        let status = status.to_owned();
        self.db
            .write_blocking(move |tx| {
                put(
                    tx,
                    extension_id,
                    &NewPermission {
                        kind,
                        action,
                        target: "*",
                        status: &status,
                        declared: true,
                        vault_device_uuid: VAULT_WIDE,
                    },
                    1,
                )
                .map_err(haex_crdt::Error::from)
            })
            .expect("grant");
    }

    pub(crate) async fn shutdown(&self) {
        self.node.shutdown().await;
    }
}

/// The test extension as it is named on the wire.
pub(crate) fn probe() -> ExtensionRef {
    ExtensionRef {
        public_key: KEY.to_owned(),
        name: NAME.to_owned(),
    }
}

/// Installs an enabled extension of [`KEY`] and `name` in `db`; returns its id.
pub(crate) fn install(db: &VaultDb, name: &str) -> Uuid {
    let id = extension_id(
        &PublicKey::parse(KEY).expect("key"),
        &ExtensionName::parse(name).expect("name"),
    );
    let name = name.to_owned();
    db.write_blocking(move |tx| {
        tx.execute(
            "INSERT INTO extensions (id, public_key, name, enabled, state, installed_at, \
             updated_at) VALUES (?1, ?2, ?3, 1, 'installed', 1, 1)",
            params![id.to_string(), KEY, name],
        )?;
        Ok(())
    })
    .expect("install");
    id
}

/// Each vault stores the other's card on a new contact; returns the contact ids `(b on a, a on b)`.
pub(crate) async fn exchange_cards(a: &Vault, b: &Vault) -> (String, String) {
    (store_card(b, a).await, store_card(a, b).await)
}

/// `of`'s card, made now, stored on a new contact in `on`.
pub(crate) async fn store_card(of: &Vault, on: &Vault) -> String {
    of.db
        .write_blocking(|tx| own_card::refresh(tx).map_err(haex_crdt::Error::from))
        .expect("refresh the card");
    let text = of
        .db
        .read_blocking(|q| own_card::read(q).map_err(haex_crdt::Error::from))
        .expect("read the card")
        .text
        .expect("a main device has a card");
    ContactsService::new(on.db.clone())
        .card_accept(&Caller::User, text, CardInto::New)
        .await
        .expect("accept the card")
        .id
}

/// Opens a channel of the test extension from `a` to `b`, which must answer with
/// [`Admit::AtOnce`]: `(a's side, b's side)`.
pub(crate) async fn open_test_channel(a: &Vault, b: &Vault) -> (Arc<Channel>, Arc<Channel>) {
    let (connection, send, recv, verified) = a.dial(b).await;
    let opening = Opening {
        extension_id: a.extension_id,
        remote: verified.expect("verified"),
        origin: Origin::Test,
        connection,
        send,
        recv,
    };
    let near = channel::open(opening, &a.ctx.frames, &a.node.tasks().tracker).expect("open");
    (near, b.accepted().await)
}

/// The test handler of [`Admit::AtOnce`].
#[derive(Clone)]
struct OpenAtOnce {
    ctx: Arc<PeerContext>,
    tasks: NodeTasks,
    extension_id: Uuid,
    opened: mpsc::UnboundedSender<Arc<Channel>>,
}

impl std::fmt::Debug for OpenAtOnce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAtOnce").finish_non_exhaustive()
    }
}

impl ProtocolHandler for OpenAtOnce {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let this = self.clone();
        self.tasks.tracker.spawn(async move {
            let Some((remote, send, recv)) = verify_incoming(&this.ctx, &connection).await else {
                connection.close(0u32.into(), b"");
                return;
            };
            let opening = Opening {
                extension_id: this.extension_id,
                remote,
                origin: Origin::Test,
                connection,
                send,
                recv,
            };
            if let Ok(channel) = channel::open(opening, &this.ctx.frames, &this.tasks.tracker) {
                let _ = this.opened.send(channel);
            }
        });
        Ok(())
    }
}
