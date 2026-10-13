use super::*;

#[test]
fn free_names_count_up_before_the_extension() {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(real.join("x (2).txt"), "").unwrap();
    assert_eq!(free_name(&real, "x.txt"), real.join("x (3).txt"));
    assert_eq!(free_name(&real, "Ordner"), real.join("Ordner (2)"));
    assert_eq!(free_name(&real, ".bashrc"), real.join(".bashrc (2)"));
    assert_eq!(free_name(&real, "a.tar.gz"), real.join("a.tar (2).gz"));
}
