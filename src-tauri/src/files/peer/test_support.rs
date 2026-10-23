//! Devices in one process for the tests of file access (spec 056 T026): each a device of a vault
//! (`Member`) on a loopback endpoint without relays, serving `haex-vault-files/1` over a temporary
//! folder that stands for its file system, with holzi's own places in a folder of their own.

// These tests insert devices and cards directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use haex_crdt::rusqlite::params;
use iroh::RelayMode;

use crate::contacts::card::own_card;
use crate::contacts::service::{Caller as ContactsCaller, CardInto, ContactsService};
use crate::files::local::{OwnPlaces, Place};
use crate::files::peer::client::{PeerLink, PeerLinks};
use crate::files::peer::handshake::DialError;
use crate::files::peer::serve::{FilesProtocol, ServeContext};
use crate::files::share::store::{self, NewFileShare};
use crate::files::share::{FileShareItemKind, FileShareLevel};
use crate::files::transfer::local::Removal;
use crate::files::SourceRef;
use crate::remote_storage::StorageError;
use crate::sync::endpoint::{NodeConfig, Protocols, SyncNode};
use crate::sync::test_support::Member;
use crate::sync::wire::FILES_ALPN;
use crate::vault_gate::VaultDb;

/// One device of a vault, serving files.
pub(crate) struct Device {
    pub member: Member,
    pub db: VaultDb,
    pub ctx: Arc<ServeContext>,
    pub node: Arc<SyncNode>,
    /// This device's `vault_device_uuid`.
    pub uuid: String,
    files: tempfile::TempDir,
}

impl Device {
    /// `member`'s device, serving `haex-vault-files/1`.
    pub(crate) async fn of(member: Member) -> Self {
        let db = member.device.vault_db();
        let uuid = member.device.db().device_id().to_string();
        {
            let uuid = uuid.clone();
            db.write_blocking(move |tx| {
                tx.execute(
                    "INSERT OR IGNORE INTO known_devices \
                     (installation_uuid, vault_device_uuid, alias, first_seen) \
                     VALUES (?1, ?2, NULL, 0)",
                    params![uuid::Uuid::new_v4().to_string(), uuid],
                )?;
                Ok(())
            })
            .expect("known device");
        }
        let files = tempfile::tempdir().expect("files");
        let root = files.path().canonicalize().expect("root");
        std::fs::create_dir_all(root.join("holzi-own")).expect("own place");
        let own = OwnPlaces::new(vec![root.join("holzi-own")]);
        let mut ctx = ServeContext::new(
            member.keys.clone(),
            member.vault,
            db.clone(),
            own,
            vec![Place {
                name: "home",
                path: root.clone(),
            }],
            uuid.clone(),
        );
        // Tests never move anything into the user's trash.
        ctx.removal = Removal::Permanent;
        let ctx = Arc::new(ctx);
        let handler_ctx = Arc::clone(&ctx);
        let protocols = Protocols::default().with(FILES_ALPN, move |tasks| {
            FilesProtocol::new(handler_ctx, tasks)
        });
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
        Self {
            member,
            db,
            ctx,
            node,
            uuid,
            files,
        }
    }

    /// The folder that stands for this device's files (canonical).
    pub(crate) fn root(&self) -> PathBuf {
        self.files.path().canonicalize().expect("root")
    }

    /// holzi's own place on this device.
    pub(crate) fn own_place(&self) -> PathBuf {
        self.root().join("holzi-own")
    }

    /// Writes `body` to `relative` below [`Device::root`].
    pub(crate) fn write(&self, relative: &str, body: &[u8]) -> PathBuf {
        let path = self.root().join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(&path, body).expect("write");
        path
    }

    /// Dials `other` on `haex-vault-files/1`.
    pub(crate) async fn link_to(&self, other: &Device) -> Result<PeerLink, DialError> {
        PeerLink::open(&self.node, &self.ctx.local(), other.node.addr()).await
    }

    /// Shares `path` (a folder or file below the root) of this device as `name`; returns its id.
    pub(crate) fn share(&self, path: &Path, item_kind: FileShareItemKind, name: &str) -> String {
        let (device, path, name) = (
            self.uuid.clone(),
            path.to_string_lossy().into_owned(),
            name.to_owned(),
        );
        self.db
            .write_blocking(move |tx| {
                Ok(store::create_share(
                    tx,
                    &NewFileShare {
                        vault_device_uuid: &device,
                        source: &SourceRef::Device,
                        path: &path,
                        item_kind,
                        name: &name,
                    },
                    "2026-10-23T10:00:00.000Z",
                )?)
            })
            .expect("share")
    }

    /// Gives `vault` `level` in `share`.
    pub(crate) fn grant(&self, share: &str, vault: [u8; 32], level: FileShareLevel) {
        let (share, vault) = (share.to_owned(), crate::sync::keys::hex(&vault));
        self.db
            .write_blocking(move |tx| {
                Ok(store::save_grant(
                    tx,
                    &share,
                    &vault,
                    level,
                    "2026-10-23T10:01:00.000Z",
                )?)
            })
            .expect("grant");
    }

    /// Takes `vault`'s grant in `share` away.
    pub(crate) fn revoke(&self, share: &str, vault: [u8; 32]) {
        let (share, vault) = (share.to_owned(), crate::sync::keys::hex(&vault));
        self.db
            .write_blocking(move |tx| {
                Ok(store::delete_grant(
                    tx,
                    &share,
                    &vault,
                    "2026-10-23T10:02:00.000Z",
                )?)
            })
            .expect("revoke");
    }
}

/// A main device of a new vault and a linked device of it.
pub(crate) async fn own_pair() -> (Device, Device) {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    (Device::of(main).await, Device::of(linked).await)
}

/// `of`'s card, made now, stored on a new contact in `on`.
pub(crate) async fn store_card(of: &Device, on: &Device) {
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
        .card_accept(&ContactsCaller::User, text, CardInto::New)
        .await
        .expect("accept the card");
}

/// One link for every target.
pub(crate) struct Fixed(pub Arc<PeerLink>);

#[async_trait]
impl PeerLinks for Fixed {
    async fn link(&self, _target: &str) -> Result<Arc<PeerLink>, StorageError> {
        Ok(Arc::clone(&self.0))
    }
}
