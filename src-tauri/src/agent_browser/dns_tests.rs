use serde_json::json;

use super::dns::{apply, check_template, preference, DnsMode, DnsProvider, DnsSettings, PRESETS};

fn custom(url: &str) -> DnsSettings {
    DnsSettings {
        mode: DnsMode::Secure,
        provider: DnsProvider::Custom,
        custom: url.to_owned(),
    }
}

#[test]
fn the_default_is_automatic_with_quad9() {
    let settings = DnsSettings::default();
    assert_eq!(settings.mode, DnsMode::Automatic);
    assert_eq!(settings.provider, DnsProvider::Quad9);
    assert_eq!(
        settings.template().as_deref(),
        Some("https://dns.quad9.net/dns-query")
    );
}

#[test]
fn every_preset_is_an_https_address_and_off_has_none() {
    for preset in &PRESETS {
        assert!(check_template(preset.url).is_ok(), "{}", preset.name);
        let settings = DnsSettings {
            mode: DnsMode::Secure,
            provider: preset.id,
            custom: String::new(),
        };
        assert_eq!(settings.template().as_deref(), Some(preset.url));
    }
    assert_eq!(DnsSettings::off().template(), None);
}

#[test]
fn an_own_address_is_https_with_a_host_and_nothing_else() {
    for good in [
        "https://dns.example/dns-query",
        "https://dns.example:8443/dns-query{?dns}",
        "https://203.0.113.7/dns-query",
    ] {
        assert!(check_template(good).is_ok(), "{good}");
    }
    for bad in [
        "http://dns.example/dns-query",
        "file:///etc/passwd",
        "https://user:pw@dns.example/dns-query",
        "https://",
        "not an address",
        "",
        "https://dns.example/{x}/dns-query",
        "https://dns.example/dns-query{?dns,x}",
    ] {
        assert!(check_template(bad).is_err(), "{bad}");
    }
    let long = format!("https://dns.example/{}", "a".repeat(2100));
    assert!(check_template(&long).is_err());
}

#[test]
fn an_own_address_is_kept_trimmed_and_dropped_when_another_provider_is_chosen() {
    let kept = custom("  https://dns.example/dns-query  ")
        .validated()
        .unwrap();
    assert_eq!(kept.custom, "https://dns.example/dns-query");
    assert!(custom("http://dns.example/").validated().is_err());
    let preset = DnsSettings {
        mode: DnsMode::Automatic,
        provider: DnsProvider::Google,
        custom: "http://not-checked.example".to_owned(),
    }
    .validated()
    .unwrap();
    assert!(
        preset.custom.is_empty(),
        "an address that does not count is not kept"
    );
}

#[test]
fn the_modes_are_written_as_the_browser_names_them() {
    let secure = preference(&custom("https://dns.example/dns-query"));
    assert_eq!(secure["mode"], "secure");
    assert_eq!(secure["templates"], "https://dns.example/dns-query");
    let automatic = preference(&DnsSettings::default());
    assert_eq!(automatic["mode"], "automatic");
    assert_eq!(automatic["templates"], "https://dns.quad9.net/dns-query");
    let off = preference(&DnsSettings::off());
    assert_eq!(off["mode"], "off", "off is written, not left out");
    assert_eq!(off["templates"], "");
}

#[test]
fn the_choice_replaces_the_one_of_a_saved_profile_and_keeps_every_other_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Local State");
    // No file yet: it is made.
    apply(dir.path(), &DnsSettings::default()).unwrap();
    let made: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(made["dns_over_https"]["mode"], "automatic");

    // A saved profile's file: its own keys stay, its own secure DNS goes.
    std::fs::write(
        &path,
        json!({
            "os_crypt": { "encrypted_key": "k" },
            "dns_over_https": { "mode": "secure", "templates": "https://old.example/dns-query" },
        })
        .to_string(),
    )
    .unwrap();
    apply(dir.path(), &DnsSettings::off()).unwrap();
    let state: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(state["os_crypt"]["encrypted_key"], "k");
    assert_eq!(state["dns_over_https"]["mode"], "off");
    assert_eq!(state["dns_over_https"]["templates"], "");

    // A file that cannot be read is replaced as a whole.
    std::fs::write(&path, "not json at all").unwrap();
    apply(dir.path(), &custom("https://dns.example/dns-query")).unwrap();
    let state: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(state["dns_over_https"]["mode"], "secure");
}
