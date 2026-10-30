//! The bytes of a bundle from the pending slot of the marketplace take the same way through the
//! preview and the install as the bytes of a file (spec 069 US2, FR-010 to FR-012).

use std::sync::Arc;

use ed25519_dalek::SigningKey;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{install_source, preview_source, BundleSource, ExtensionInstallArgs, InstallOrigin};
use crate::error::{HolziError, Result};
use crate::extensions::registry::install::{InstallPreview, Installed};
use crate::files::test_support::PathOpener;
use crate::files::PickedFile;
use crate::marketplace::kinds;
use crate::marketplace::pending::{PendingBundle, PendingSlot};
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::{VaultDb, VaultGate};

fn build(name: &str, version: &str, display: &str, seed: u8) -> Vec<u8> {
    let manifest = haex_bundle::jcs::parse_restricted(&format!(
        r#"{{"name":"{name}","version":"{version}","displayName":"{display}","permissions":{{}}}}"#
    ))
    .unwrap();
    let files = vec![haex_bundle::Entry {
        path: "index.html".into(),
        data: format!("<!doctype html><title>{version}</title>").into_bytes(),
    }];
    haex_bundle::build_archive(files, manifest, &SigningKey::from_bytes(&[seed; 32])).unwrap()
}

struct Rig {
    _dir: tempfile::TempDir,
    _files: tempfile::TempDir,
    files: std::path::PathBuf,
    vault: VaultDb,
    device: Uuid,
    pending: Arc<PendingSlot>,
}

fn rig() -> Rig {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()
        .first()
        .expect("the bootstrap registers this device")
        .vault_device_uuid;
    let files = tempfile::tempdir().unwrap();
    Rig {
        _dir: dir,
        files: files.path().to_path_buf(),
        _files: files,
        vault,
        device,
        pending: Arc::default(),
    }
}

impl Rig {
    /// Puts `bytes` into the slot and returns the source that names them.
    fn downloaded(&self, bytes: &[u8]) -> BundleSource {
        let id = Uuid::new_v4();
        self.pending.put(
            id,
            PendingBundle {
                source_id: "preset-public".into(),
                source_name: "Haex Marketplace".into(),
                slug: "demo".into(),
                version: "1.0.0".into(),
                bytes: Arc::new(bytes.to_vec()),
            },
            crate::marketplace::PENDING_IDLE,
            CancellationToken::new(),
        );
        BundleSource::Marketplace { id: id.to_string() }
    }

    /// Writes `bytes` to a file and returns the source that names it.
    fn picked(&self, bytes: &[u8]) -> BundleSource {
        let path = self.files.join(format!("{}.xt", Uuid::new_v4()));
        std::fs::write(&path, bytes).unwrap();
        BundleSource::File {
            file: PickedFile(path.to_string_lossy().into_owned()),
        }
    }

    fn preview(&self, source: &BundleSource) -> Result<InstallPreview> {
        preview_source(&self.vault, &PathOpener::default(), &self.pending, source)
    }

    fn install(&self, source: &BundleSource, confirmed: bool) -> Result<Installed> {
        install_source(
            &self.vault,
            &PathOpener::default(),
            &self.pending,
            ExtensionInstallArgs {
                source: source.clone(),
                accepted: vec![],
                confirmed,
            },
            self.device,
            1_000,
        )
    }
}

fn reason_of(error: HolziError) -> String {
    match error {
        HolziError::ExtensionInstall { reason } | HolziError::Marketplace { reason } => reason,
        other => panic!("unexpected error {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn the_preview_of_a_downloaded_bundle_equals_the_preview_of_the_same_file() {
    let rig = rig();
    let bytes = build("demo", "1.0.0", "Demo", 1);

    let from_file = rig.preview(&rig.picked(&bytes)).unwrap();
    let mut from_market = rig.preview(&rig.downloaded(&bytes)).unwrap();

    assert!(from_market.signature_valid);
    assert_eq!(
        from_market.origin.take(),
        Some(InstallOrigin {
            source_name: "Haex Marketplace".into()
        })
    );
    assert_eq!(from_market, from_file);
}

#[tokio::test(start_paused = true)]
async fn only_a_marketplace_bundle_has_an_origin() {
    let rig = rig();
    let bytes = build("demo", "1.0.0", "Demo", 1);

    assert_eq!(rig.preview(&rig.picked(&bytes)).unwrap().origin, None);
    assert!(rig
        .preview(&rig.downloaded(&bytes))
        .unwrap()
        .origin
        .is_some());
}

#[tokio::test(start_paused = true)]
async fn the_slot_is_emptied_by_a_successful_install_and_kept_by_a_failed_one() {
    let rig = rig();
    let mut broken = build("demo", "1.0.0", "Demo", 1);
    let last = broken.len() - 30;
    broken[last] ^= 0xff;
    let source = rig.downloaded(&broken);

    assert!(rig.install(&source, false).is_err());
    assert!(
        !rig.pending.is_empty(),
        "a refused bundle stays for another try"
    );

    let good = rig.downloaded(&build("demo", "1.0.0", "Demo", 1));
    rig.install(&good, false).unwrap();
    assert!(rig.pending.is_empty());
    let BundleSource::Marketplace { id } = good else {
        unreachable!()
    };
    assert_eq!(
        reason_of(rig.pending.peek(&id).unwrap_err()),
        kinds::PENDING_MISSING
    );
}

#[tokio::test(start_paused = true)]
async fn an_empty_or_foreign_slot_is_pending_missing_for_preview_and_install() {
    let rig = rig();
    let missing = BundleSource::Marketplace {
        id: Uuid::new_v4().to_string(),
    };
    assert_eq!(
        reason_of(rig.preview(&missing).unwrap_err()),
        kinds::PENDING_MISSING
    );
    assert_eq!(
        reason_of(rig.install(&missing, false).unwrap_err()),
        kinds::PENDING_MISSING
    );

    let other = rig.downloaded(&build("demo", "1.0.0", "Demo", 1));
    assert_eq!(
        reason_of(rig.preview(&missing).unwrap_err()),
        kinds::PENDING_MISSING,
        "a bundle under another id does not answer"
    );
    assert!(rig.preview(&other).is_ok());
}

#[tokio::test(start_paused = true)]
async fn a_bundle_that_fails_its_signature_shows_the_same_error_as_from_a_file() {
    let rig = rig();
    let mut broken = build("demo", "1.0.0", "Demo", 1);
    let last = broken.len() - 30;
    broken[last] ^= 0xff;

    let from_file = rig.preview(&rig.picked(&broken)).unwrap();
    let mut from_market = rig.preview(&rig.downloaded(&broken)).unwrap();
    from_market.origin = None;

    assert!(!from_file.signature_valid);
    assert!(from_file.error.is_some());
    assert_eq!(from_market, from_file);
    assert!(matches!(
        rig.install(&rig.downloaded(&broken), false),
        Err(HolziError::ExtensionInstall { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn a_downgrade_and_a_same_version_replacement_still_need_a_confirmation() {
    let rig = rig();
    rig.install(&rig.downloaded(&build("demo", "2.0.0", "Two", 1)), false)
        .unwrap();

    let older = rig.downloaded(&build("demo", "1.0.0", "One", 1));
    assert!(rig.preview(&older).unwrap().existing.unwrap().is_downgrade);
    assert_eq!(
        reason_of(rig.install(&older, false).unwrap_err()),
        "downgrade_not_confirmed"
    );
    assert!(!rig.pending.is_empty());
    rig.install(&older, true).unwrap();

    let other = rig.downloaded(&build("demo", "1.0.0", "Other", 1));
    assert!(rig.preview(&other).unwrap().replaces_same_version);
    assert_eq!(
        reason_of(rig.install(&other, false).unwrap_err()),
        "replace_not_confirmed"
    );
    rig.install(&other, true).unwrap();
    assert!(rig.pending.is_empty());
}

#[test]
fn the_source_arrives_from_the_webview_as_a_tagged_object() {
    let file: BundleSource = serde_json::from_str(r#"{"kind":"file","file":"/tmp/a.xt"}"#).unwrap();
    assert_eq!(
        file,
        BundleSource::File {
            file: PickedFile("/tmp/a.xt".into())
        }
    );
    let market: BundleSource =
        serde_json::from_str(r#"{"kind":"marketplace","id":"abc"}"#).unwrap();
    assert_eq!(market, BundleSource::Marketplace { id: "abc".into() });
    assert!(serde_json::from_str::<BundleSource>(r#"{"kind":"url","id":"x"}"#).is_err());
    assert!(serde_json::from_str::<BundleSource>(r#""/tmp/a.xt""#).is_err());
}

/// Quickstart M16 (spec 069): every extension of the public marketplace is downloaded the way the
/// app does it (client, size and checksum check) and runs through the install preview of spec 017.
/// It talks to the internet, so it only runs when asked: `cargo test --lib public_marketplace --
/// --ignored --nocapture`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "talks to the public marketplace; quickstart M16"]
async fn every_bundle_of_the_public_marketplace_passes_the_install_preview() {
    use crate::marketplace::app_version;
    use crate::marketplace::catalog::{fetch_catalog, Source};
    use crate::marketplace::client::Http;
    use crate::marketplace::download::download_bundle;
    use crate::marketplace::model::CatalogQuery;
    use crate::marketplace::store::PRESET_URL;

    let source = Source {
        id: "preset-public".into(),
        name: "Haex Marketplace".into(),
        base_url: PRESET_URL.into(),
        auth: None,
        unavailable: None,
    };
    let rig = rig();
    let mut checked = Vec::new();
    let mut failed = Vec::new();
    for page in 1.. {
        let query = CatalogQuery {
            page,
            search: None,
            category: None,
            source_id: None,
        };
        let answer = fetch_catalog(
            Http::default(),
            std::slice::from_ref(&source),
            &query,
            &[],
            &app_version(),
        )
        .await;
        assert!(
            answer.source_errors.is_empty(),
            "{:?}",
            answer.source_errors
        );
        for entry in &answer.entries {
            let Some(version) = entry.latest_compatible.clone() else {
                failed.push(format!("{}: no installable version", entry.slug));
                continue;
            };
            let outcome = async {
                let bytes = download_bundle(
                    Http::default(),
                    &source,
                    &entry.slug,
                    &version,
                    &app_version(),
                    &CancellationToken::new(),
                    &mut |_| {},
                )
                .await?;
                let size = bytes.len();
                let preview = rig.preview(&rig.downloaded(&bytes))?;
                Ok::<_, HolziError>((size, preview))
            }
            .await;
            match outcome {
                Ok((size, preview)) => {
                    println!(
                        "ok   {} {version}: {size} bytes, origin {:?}",
                        entry.slug,
                        preview.origin.map(|o| o.source_name)
                    );
                    checked.push(entry.slug.clone());
                }
                Err(error) => failed.push(format!("{} {version}: {error}", entry.slug)),
            }
        }
        if page >= answer.total_pages {
            break;
        }
    }
    for line in &failed {
        println!("FAIL {line}");
    }
    assert!(failed.is_empty(), "{failed:?}");
    assert!(
        !checked.is_empty(),
        "the public marketplace lists extensions"
    );
}
