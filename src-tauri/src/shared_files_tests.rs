use super::*;

struct Setup {
    _root: tempfile::TempDir,
    dir: PathBuf,
    store: SharedFiles,
}

fn setup() -> Setup {
    let root = tempfile::tempdir().expect("tempdir");
    let dir = root.path().join(DIRECTORY);
    fs::create_dir_all(&dir).expect("make the directory");
    Setup {
        store: SharedFiles::new(dir.clone()),
        dir,
        _root: root,
    }
}

impl Setup {
    fn copy(&self, name: &str, bytes: &[u8]) -> String {
        let path = self.dir.join(name);
        fs::write(&path, bytes).expect("write the copy");
        path.to_string_lossy().into_owned()
    }
}

fn incoming(path: &str, name: &str, mime: &str) -> Incoming {
    Incoming {
        path: path.to_owned(),
        name: name.to_owned(),
        mime: mime.to_owned(),
    }
}

#[test]
fn a_copy_waits_until_the_window_takes_it_and_then_no_longer() {
    let s = setup();
    let path = s.copy("a.pkpass", b"zip");
    let view = s
        .store
        .park(incoming(
            &path,
            "Boarding.pkpass",
            "application/vnd.apple.pkpass",
        ))
        .expect("parked");
    assert_eq!(view.kind, SharedKind::Pkpass);
    assert_eq!(view.name, "Boarding.pkpass");

    assert_eq!(s.store.take_all(), vec![view]);
    assert!(s.store.take_all().is_empty(), "nothing waits afterwards");
    assert!(Path::new(&path).exists(), "taking does not remove the copy");
}

#[test]
fn a_discarded_copy_is_gone_whether_it_waits_or_was_taken() {
    let s = setup();
    let waiting = s.copy("w.pdf", b"%PDF-");
    let taken = s.copy("t.pdf", b"%PDF-");
    s.store
        .park(incoming(&waiting, "w.pdf", "application/pdf"))
        .expect("parked");
    s.store
        .park(incoming(&taken, "t.pdf", "application/pdf"))
        .expect("parked");
    // Both are taken by the window; `waiting` is reported again and waits, `taken` stays with it.
    assert_eq!(s.store.take_all().len(), 2);
    s.store
        .park(incoming(&waiting, "w.pdf", "application/pdf"))
        .expect("parked again");

    s.store.discard(&waiting).expect("discard a waiting copy");
    s.store.discard(&taken).expect("discard a taken copy");
    assert!(!Path::new(&waiting).exists());
    assert!(!Path::new(&taken).exists());
    assert!(s.store.take_all().is_empty());
    s.store.discard(&taken).expect("a second discard is fine");
}

#[test]
fn the_kind_comes_from_the_type_the_sender_named_else_from_the_ending() {
    use SharedKind::*;
    for (mime, name, kind) in [
        ("application/vnd.apple.pkpass", "x", Some(Pkpass)),
        ("", "Ticket.PKPASS", Some(Pkpass)),
        ("application/pdf", "x", Some(Pdf)),
        ("application/octet-stream", "ticket.pdf", Some(Pdf)),
        ("image/png", "x", Some(Image)),
        ("image/heic", "photo", Some(Image)),
        ("", "scan.JPG", Some(Image)),
        ("text/plain", "notes.txt", None),
        ("", "", None),
    ] {
        assert_eq!(SharedKind::of(mime, name), kind, "{mime} {name}");
    }
}

#[test]
fn a_kind_the_share_target_does_not_take_is_refused_and_removed() {
    let s = setup();
    let path = s.copy("n.txt", b"hello");
    assert!(s
        .store
        .park(incoming(&path, "n.txt", "text/plain"))
        .is_none());
    assert!(!Path::new(&path).exists());
}

#[test]
fn a_file_over_the_limit_is_refused_and_removed() {
    let s = setup();
    let path = s.dir.join("big.pdf");
    let file = fs::File::create(&path).expect("create");
    file.set_len(MAX_BYTES + 1).expect("make it long");
    let path = path.to_string_lossy().into_owned();
    assert!(s
        .store
        .park(incoming(&path, "big.pdf", "application/pdf"))
        .is_none());
    assert!(!Path::new(&path).exists());
}

#[test]
fn a_path_outside_the_directory_is_neither_taken_nor_removed() {
    let s = setup();
    let outside = s._root.path().join("outside.pdf");
    fs::write(&outside, b"%PDF-").expect("write");
    let outside = outside.to_string_lossy().into_owned();

    assert!(s
        .store
        .park(incoming(&outside, "outside.pdf", "application/pdf"))
        .is_none());
    assert!(Path::new(&outside).exists(), "a file that is no copy stays");
    assert!(s.store.discard(&outside).is_err());
    assert!(Path::new(&outside).exists());

    let dotted = format!("{}/../outside.pdf", s.dir.to_string_lossy());
    assert!(s.store.discard(&dotted).is_err());
    assert!(Path::new(&outside).exists());
    assert!(s.store.discard("relative.pdf").is_err());
}

#[cfg(unix)]
#[test]
fn a_link_in_the_directory_is_not_taken() {
    let s = setup();
    let target = s._root.path().join("secret.pdf");
    fs::write(&target, b"%PDF-").expect("write");
    let link = s.dir.join("link.pdf");
    std::os::unix::fs::symlink(&target, &link).expect("link");
    let link = link.to_string_lossy().into_owned();

    assert!(s
        .store
        .park(incoming(&link, "link.pdf", "application/pdf"))
        .is_none());
    assert!(target.exists(), "what the link points to is not touched");
}

#[test]
fn only_so_many_files_wait() {
    let s = setup();
    for index in 0..MAX_WAITING {
        let path = s.copy(&format!("{index}.pdf"), b"%PDF-");
        assert!(s
            .store
            .park(incoming(&path, "x.pdf", "application/pdf"))
            .is_some());
    }
    let extra = s.copy("extra.pdf", b"%PDF-");
    assert!(s
        .store
        .park(incoming(&extra, "extra.pdf", "application/pdf"))
        .is_none());
    assert!(!Path::new(&extra).exists());
}

#[test]
fn a_name_is_shown_without_a_path_or_control_characters() {
    assert_eq!(display_name("../../etc/passwd"), "passwd");
    assert_eq!(display_name("a\\b\\ticket.pdf"), "ticket.pdf");
    assert_eq!(display_name("tick\u{0}et\n.pdf"), "ticket.pdf");
    assert_eq!(display_name("   "), "File");
    assert_eq!(
        display_name(&"x".repeat(500)).chars().count(),
        MAX_NAME_CHARS
    );
}
