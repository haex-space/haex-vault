use super::*;

#[test]
fn neither_format_shows_the_path() {
    let path = "/Privat/Unterlagen/Belege/strom.pdf";
    for shown in [
        format!("{}", Redacted(path)),
        format!("{:?}", Redacted(path)),
        format!("{:#?}", Redacted(path)),
        format!("files: no thumbnail for {}", Redacted(path)),
    ] {
        assert!(!shown.contains("Unterlagen"), "{shown}");
        assert!(!shown.contains("strom"), "{shown}");
        assert!(shown.contains("<verschlüsselt>"), "{shown}");
    }
}
