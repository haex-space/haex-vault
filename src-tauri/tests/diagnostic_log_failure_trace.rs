//! A failing step of the import leaves its cause in the log of the vault (spec 098 SC-002). The
//! app told only "the file is damaged"; the cause stood in a log that release builds did not have.

#[path = "common/kdbx_fixture.rs"]
mod kdbx_fixture;
#[path = "common/legacy_haex_vault_fixture.rs"]
mod legacy_haex_vault_fixture;

use zeroize::Zeroizing;

use haex_lib::diagnostic_log::{self, level::LogLevel};
use haex_lib::passwords::import::{legacy_haex_vault, Credentials};
use legacy_haex_vault_fixture::PASSWORD;

#[test]
fn a_read_that_fails_leaves_its_cause_in_the_log_of_the_vault() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = legacy_haex_vault_fixture::build(dir.path());
    // A blob where the reader expects the title text: the layout is right, the content is not.
    legacy_haex_vault_fixture::open(&source.path)
        .execute("UPDATE haex_passwords_item_details SET title = X'00ff'", [])
        .expect("damage the titles");

    let log_dir = tempfile::tempdir().expect("tempdir");
    let logs = log_dir.path().join("logs");
    log::set_logger(diagnostic_log::logger()).expect("the one logger of this test binary");
    diagnostic_log::attach_folder(&logs, LogLevel::Standard).expect("attach");
    LogLevel::Standard.apply();

    let credentials = Credentials {
        password: Some(Zeroizing::new(PASSWORD.to_string())),
        key_file: None,
    };
    let outcome = legacy_haex_vault::read(&source.path, &credentials);
    diagnostic_log::end();

    assert!(outcome.is_err(), "the damaged file is not read");
    let text = std::fs::read_to_string(logs.join("haex.log")).expect("the log exists");
    let line = text
        .lines()
        .find(|line| line.contains("[WARN]") && line.contains("legacy_haex_vault"))
        .or_else(|| text.lines().find(|line| line.contains("[WARN]")))
        .unwrap_or_else(|| panic!("a WARN line names the cause: {text}"));
    assert!(
        line.contains("reading the haex-vault 2.x file failed"),
        "{line}"
    );
    assert!(!text.contains(PASSWORD), "the passphrase is never logged");
}
