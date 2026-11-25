//! Tests for the tag names an import collects (spec 100, FR-017): a blank name is dropped, the
//! rest is trimmed and an equal name by `fold` is kept once.

use super::push_tag;

#[test]
fn an_import_drops_blank_tag_names_and_keeps_each_name_once() {
    let mut tags = Vec::new();
    for name in [
        "",
        " ",
        "   ",
        "\t",
        "\u{a0}",
        " Work ",
        "work",
        "M\u{fc}ller",
        "Mu\u{308}ller",
    ] {
        push_tag(&mut tags, name);
    }
    assert_eq!(tags, ["Work", "M\u{fc}ller"]);
}
