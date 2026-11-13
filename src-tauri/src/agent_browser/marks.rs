//! Marks on a screenshot (spec 065 FR-056, research R21): a box and the element's number on every
//! listed element in view. holzi draws them itself on the captured image, after the capture, so the
//! page neither sees nor changes them, and with a built-in 5×7 font for the digits, so no font file
//! and no new dependency. The numbers are those of `browser.page.read`.

use image::{Rgb, RgbImage};

/// The most marks one image gets.
pub const MAX_MARKS: usize = 80;
/// The size of one pixel of the font, in image pixels.
const SCALE: u32 = 2;
/// The width of the outline of a box.
const OUTLINE: u32 = 2;

/// One element to mark: its number and its rectangle in image pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mark {
    pub n: u32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Strong colours that stay apart from each other, so neighbouring boxes are told apart.
const COLOURS: [[u8; 3]; 6] = [
    [220, 30, 30],
    [20, 90, 220],
    [20, 150, 60],
    [230, 120, 0],
    [150, 40, 190],
    [0, 150, 160],
];

/// The digits 0 to 9, five columns wide and seven rows high, one row per byte (bit 4 is the left).
const DIGITS: [[u8; 7]; 10] = [
    [
        0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
    ],
    [
        0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
    ],
    [
        0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
    ],
    [
        0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
    ],
    [
        0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
    ],
    [
        0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
    ],
    [
        0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
    ],
    [
        0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
    ],
    [
        0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
    ],
    [
        0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
    ],
];

/// The colour of the box of element `n`.
pub fn colour(n: u32) -> Rgb<u8> {
    Rgb(COLOURS[n as usize % COLOURS.len()])
}

fn fill(image: &mut RgbImage, x: i64, y: i64, w: i64, h: i64, colour: Rgb<u8>) {
    let (width, height) = (i64::from(image.width()), i64::from(image.height()));
    for py in y.max(0)..(y + h).min(height) {
        for px in x.max(0)..(x + w).min(width) {
            image.put_pixel(px as u32, py as u32, colour);
        }
    }
}

/// The size of the tag that shows `n`: the digits, a pixel of room between them, and a margin.
fn tag_size(n: u32) -> (i64, i64) {
    let digits = n.to_string().len() as i64;
    let scale = i64::from(SCALE);
    (digits * 6 * scale + scale, 7 * scale + 2 * scale)
}

/// Draws the box and the number of every mark in `marks` (at most [`MAX_MARKS`]) on `image`. The
/// tag sits above the box, or inside its top edge when there is no room above, and never leaves
/// the image.
pub fn draw(image: &mut RgbImage, marks: &[Mark]) {
    let (width, height) = (i64::from(image.width()), i64::from(image.height()));
    for mark in marks.iter().take(MAX_MARKS) {
        let colour = colour(mark.n);
        let (x, y) = (mark.x.round() as i64, mark.y.round() as i64);
        let (w, h) = (
            (mark.w.round() as i64).max(1),
            (mark.h.round() as i64).max(1),
        );
        let line = i64::from(OUTLINE);
        fill(image, x, y, w, line, colour);
        fill(image, x, y + h - line, w, line, colour);
        fill(image, x, y, line, h, colour);
        fill(image, x + w - line, y, line, h, colour);

        let (tag_w, tag_h) = tag_size(mark.n);
        let tag_x = x.clamp(0, (width - tag_w).max(0));
        let tag_y = if y - tag_h >= 0 { y - tag_h } else { y.max(0) };
        let tag_y = tag_y.min((height - tag_h).max(0));
        fill(image, tag_x, tag_y, tag_w, tag_h, colour);
        let scale = i64::from(SCALE);
        for (index, digit) in mark.n.to_string().bytes().enumerate() {
            let glyph = &DIGITS[usize::from(digit - b'0')];
            let left = tag_x + scale + index as i64 * 6 * scale;
            for (row, bits) in glyph.iter().enumerate() {
                for column in 0..5 {
                    if bits >> (4 - column) & 1 == 1 {
                        fill(
                            image,
                            left + column * scale,
                            tag_y + scale + row as i64 * scale,
                            scale,
                            scale,
                            Rgb([255, 255, 255]),
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "marks_tests.rs"]
mod tests;
