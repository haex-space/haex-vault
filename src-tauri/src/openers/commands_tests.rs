use std::path::Path;
use std::sync::Mutex;

use serde_json::Value;

use super::*;
use crate::extensions::registry::install::install;
use crate::extensions::registry::remove::set_enabled;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::VaultGate;

const LINK: &str = "mailto:ben@example.org?subject=Hallo";
const PATIENT: Duration = Duration::from_secs(60);

#[derive(Default)]
struct Silent(Mutex<Vec<String>>);

impl Emit for Silent {
    fn emit(&self, event: &str, _payload: Value) {
        self.0.lock().unwrap().push(event.to_owned());
    }
}

struct Setup {
    _dir: tempfile::TempDir,
    db: VaultDb,
    host: Arc<ExtensionHost>,
    device: Uuid,
}

fn setup() -> Setup {
    let (dir, db) = open_test_vault();
    let db = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let device = db
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    Setup {
        _dir: dir,
        db,
        host: Arc::new(ExtensionHost::default()),
        device,
    }
}

impl Setup {
    fn install(&self, bundle: &str) -> Uuid {
        let bytes = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(bundle),
        )
        .unwrap();
        install(&self.db, &bytes, vec![], false, self.device, 1)
            .unwrap()
            .ids
            .extension_id
    }

    fn open(&self, extension: Uuid, link: &str) -> Result<()> {
        open_mail(
            &self.db,
            &self.host,
            Arc::new(Silent::default()),
            self.device,
            extension,
            link,
            PATIENT,
        )
    }
}

#[test]
fn a_registered_extension_gets_the_fields_of_the_link() {
    let s = setup();
    let opener = s.install("extension_e2e/mail-opener.xt");
    s.open(opener, LINK).unwrap();
    let parked = s.host.openers.take(opener).unwrap();
    assert_eq!(parked.to, vec!["ben@example.org".to_owned()]);
    assert_eq!(parked.subject, "Hallo");
}

#[test]
fn a_refused_link_parks_nothing() {
    let s = setup();
    let opener = s.install("extension_e2e/mail-opener.xt");
    for link in [
        "mailto:ben@example.org?subject=a%0d%0aBcc:x@example.org",
        "https://example.org",
        "mailto:no-at-sign",
    ] {
        assert!(
            matches!(
                s.open(opener, link),
                Err(HolziError::OpenerLinkInvalid { .. })
            ),
            "{link}"
        );
        assert_eq!(s.host.openers.take(opener), None, "{link}");
    }
}

#[test]
fn an_extension_that_did_not_register_is_not_available() {
    let s = setup();
    let plain = s.install("extension_bundles/good-notes-like.xt");
    assert!(matches!(
        s.open(plain, LINK),
        Err(HolziError::OpenerNotAvailable)
    ));
    assert_eq!(s.host.openers.take(plain), None);
}

#[test]
fn an_unknown_or_disabled_extension_is_not_available() {
    let s = setup();
    assert!(matches!(
        s.open(Uuid::new_v4(), LINK),
        Err(HolziError::OpenerNotAvailable)
    ));
    let opener = s.install("extension_e2e/mail-opener.xt");
    set_enabled(&s.db, opener, false, 2).unwrap();
    assert!(matches!(
        s.open(opener, LINK),
        Err(HolziError::OpenerNotAvailable)
    ));
    assert_eq!(s.host.openers.take(opener), None);
}

#[test]
fn the_link_is_read_again_when_it_is_opened() {
    let s = setup();
    let opener = s.install("extension_e2e/mail-opener.xt");
    assert_eq!(opener_mail_check(LINK.to_owned()).unwrap().subject, "Hallo");
    // A window that checked a fine link and then asks to open another is not believed.
    assert!(s
        .open(opener, "mailto:ben@example.org?subject=a%0a")
        .is_err());
    assert_eq!(s.host.openers.take(opener), None);
}
