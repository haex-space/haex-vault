use super::*;

fn blank(width: u32, height: u32) -> image::RgbImage {
    image::RgbImage::from_pixel(width, height, image::Rgb([10, 20, 30]))
}

#[test]
fn an_image_of_the_expected_size_stays_as_it_is() {
    let image = fit(blank(640, 400), 640, 400);
    assert_eq!(image.dimensions(), (640, 400));
    // A pixel of rounding is not scaled away.
    let image = fit(blank(641, 399), 640, 400);
    assert_eq!(image.dimensions(), (641, 399));
}

#[test]
fn an_image_of_a_hidpi_display_is_scaled_to_the_expected_size() {
    let image = fit(blank(1280, 800), 640, 400);
    assert_eq!(image.dimensions(), (640, 400));
    assert_eq!(*image.get_pixel(10, 10), image::Rgb([10, 20, 30]));
}
