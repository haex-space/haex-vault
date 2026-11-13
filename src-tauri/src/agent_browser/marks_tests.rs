//! Spec 065 T121: boxes and numbers on a fixed image.

use image::{Rgb, RgbImage};

use super::*;

const GREY: Rgb<u8> = Rgb([128, 128, 128]);
const WHITE: Rgb<u8> = Rgb([255, 255, 255]);

fn grey(width: u32, height: u32) -> RgbImage {
    RgbImage::from_pixel(width, height, GREY)
}

fn mark(n: u32, x: f64, y: f64, w: f64, h: f64) -> Mark {
    Mark { n, x, y, w, h }
}

fn count(image: &RgbImage, colour: Rgb<u8>) -> usize {
    image.pixels().filter(|pixel| **pixel == colour).count()
}

#[test]
fn a_box_has_an_outline_and_leaves_its_inside_alone() {
    let mut image = grey(200, 100);
    draw(&mut image, &[mark(7, 40.0, 40.0, 80.0, 30.0)]);
    let outline = colour(7);
    for (x, y) in [
        (40, 40),
        (119, 40),
        (40, 69),
        (119, 69),
        (80, 40),
        (80, 69),
        (40, 55),
    ] {
        assert_eq!(*image.get_pixel(x, y), outline, "({x}, {y})");
    }
    assert_eq!(*image.get_pixel(80, 55), GREY, "inside");
    assert_eq!(*image.get_pixel(10, 10), GREY, "outside");
}

#[test]
fn the_number_is_drawn_in_white_on_a_tag_above_the_box() {
    let mut image = grey(200, 100);
    draw(&mut image, &[mark(5, 40.0, 40.0, 80.0, 30.0)]);
    // The tag is above the box: tag_size(5) = (14, 18), so rows 22 to 39.
    assert_eq!(*image.get_pixel(41, 30), colour(5), "the tag's background");
    let white: usize = (22..40)
        .map(|y| {
            (40..54)
                .filter(|x| *image.get_pixel(*x, y) == WHITE)
                .count()
        })
        .sum();
    assert!(white > 10, "the digit is white: {white}");
    // Nothing white outside the tag.
    assert_eq!(count(&image, WHITE), white);
}

#[test]
fn every_digit_draws_something_and_no_two_digits_look_alike() {
    let mut seen = Vec::new();
    for digit in 0..10u32 {
        let mut image = grey(30, 30);
        draw(&mut image, &[mark(digit, 5.0, 20.0, 20.0, 8.0)]);
        let white = count(&image, WHITE);
        assert!(white > 5, "digit {digit} has pixels: {white}");
        seen.push(image.into_raw());
    }
    for a in 0..10 {
        for b in (a + 1)..10 {
            assert_ne!(seen[a], seen[b], "digits {a} and {b} differ");
        }
    }
}

#[test]
fn numbers_up_to_four_digits_fit_on_their_tag() {
    for n in [1, 12, 123, 1234] {
        let mut image = grey(200, 100);
        draw(&mut image, &[mark(n, 20.0, 50.0, 100.0, 20.0)]);
        let (tag_w, _) = tag_size(n);
        // The tag's right end is a pixel of tag colour, one past it is not.
        assert_eq!(
            *image.get_pixel(20 + tag_w as u32 - 1, 40),
            colour(n),
            "{n}"
        );
        assert_eq!(*image.get_pixel(20 + tag_w as u32, 40), GREY, "{n}");
    }
}

#[test]
fn a_tag_goes_inside_the_box_when_there_is_no_room_above_and_stays_in_the_image() {
    let mut image = grey(100, 60);
    draw(&mut image, &[mark(3, 0.0, 0.0, 50.0, 20.0)]);
    assert!(count(&image, WHITE) > 5);
    // A box at the right edge: the tag moves left to stay inside.
    let mut image = grey(100, 60);
    draw(&mut image, &[mark(1234, 90.0, 30.0, 10.0, 10.0)]);
    let (tag_w, _) = tag_size(1234);
    assert_eq!(*image.get_pixel(100 - tag_w as u32, 20), colour(1234));
    // A box partly outside the image does not panic.
    let mut image = grey(50, 50);
    draw(
        &mut image,
        &[
            mark(9, 40.0, 40.0, 40.0, 40.0),
            mark(8, -10.0, -10.0, 20.0, 20.0),
        ],
    );
}

#[test]
fn at_most_eighty_marks_are_drawn() {
    let marks: Vec<Mark> = (1..=100u32)
        .map(|n| {
            mark(
                n,
                f64::from((n % 10) * 20),
                f64::from((n / 10) * 12),
                10.0,
                6.0,
            )
        })
        .collect();
    let mut limited = grey(300, 160);
    draw(&mut limited, &marks);
    let mut eighty = grey(300, 160);
    draw(&mut eighty, &marks[..MAX_MARKS]);
    assert_eq!(limited, eighty);
}

#[test]
fn neighbouring_numbers_get_different_colours() {
    assert_ne!(colour(1), colour(2));
    assert_ne!(colour(2), colour(3));
    assert_eq!(colour(1), colour(1 + COLOURS.len() as u32));
}
