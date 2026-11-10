//! Needs a camera: run with `cargo test link_camera::capture -- --ignored`.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use super::*;

#[test]
#[ignore = "needs a camera"]
fn a_real_camera_keeps_delivering_new_pictures() {
    let device = find_camera().expect("a camera");
    let mut source = start(&device).expect("the camera starts");
    let begun = Instant::now();
    let mut pictures = HashSet::new();
    let mut count = 0;

    while begun.elapsed() < Duration::from_secs(3) {
        let jpeg = source.next_frame().expect("a picture");
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8], "a JPEG starts with its marker");
        pictures.insert(jpeg);
        count += 1;
    }

    // About 30 pictures a second, and a live camera never gives the same bytes twice.
    assert!(count >= 45, "only {count} pictures in 3 s");
    assert!(
        pictures.len() * 10 >= count * 9,
        "{} distinct of {count}",
        pictures.len()
    );
}

#[test]
fn a_wait_that_ran_out_is_no_error_of_the_camera() {
    assert!(is_timeout(&io::Error::new(
        io::ErrorKind::TimedOut,
        "VIDIOC_DQBUF"
    )));
    assert!(!is_timeout(&io::Error::from_raw_os_error(19)));
}
