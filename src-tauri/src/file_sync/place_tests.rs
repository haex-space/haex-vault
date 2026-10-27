//! Path helpers of a place.

use super::*;

#[test]
fn split_names_the_folder_and_the_name() {
    assert_eq!(split("a/b/c.txt"), ("a/b", "c.txt"));
    assert_eq!(split("c.txt"), ("", "c.txt"));
}

#[test]
fn trash_keeps_the_path_below_the_run() {
    assert_eq!(trash_path("run1", "a/b.txt"), ".holzi-trash/run1/a/b.txt");
}
