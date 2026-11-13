//! Spec 065 T120: the view a screenshot showed, how positions are converted and when one is refused.

use super::*;

fn layout(scroll_y: f64, width: f64) -> Layout {
    Layout {
        scroll_x: 0.0,
        scroll_y,
        width,
        height: 800.0,
        zoom: 1.0,
    }
}

#[test]
fn a_wide_view_is_scaled_down_to_1280_and_a_narrow_one_is_not_scaled() {
    let wide = View::of("doc", layout(0.0, 1920.0));
    assert!((wide.scale() - 1280.0 / 1920.0).abs() < 1e-9);
    assert_eq!(wide.image(), (1280, 533));
    let narrow = View::of("doc", layout(0.0, 800.0));
    assert_eq!(narrow.scale(), 1.0);
    assert_eq!(narrow.image(), (800, 800));
}

#[test]
fn a_position_in_the_image_is_a_point_of_the_view() {
    let view = View::of("doc", layout(0.0, 2560.0));
    // Scale 0.5: image pixel (100, 50) is the CSS pixel (200, 100).
    assert_eq!(view.css(100.0, 50.0), (200.0, 100.0));
    let plain = View::of("doc", layout(0.0, 1000.0));
    assert_eq!(plain.css(100.0, 50.0), (100.0, 50.0));
}

#[test]
fn no_view_is_refused_and_so_is_a_changed_one() {
    let shown = View::of("doc-1", layout(0.0, 1280.0));
    assert_eq!(check(None, &shown), Err(ViewError::NoView));
    assert_eq!(check(Some(&shown), &shown.clone()), Ok(()));
    // Scrolled, another document, resized, zoomed.
    assert_eq!(
        check(Some(&shown), &View::of("doc-1", layout(300.0, 1280.0))),
        Err(ViewError::Changed)
    );
    assert_eq!(
        check(Some(&shown), &View::of("doc-2", layout(0.0, 1280.0))),
        Err(ViewError::Changed)
    );
    assert_eq!(
        check(Some(&shown), &View::of("doc-1", layout(0.0, 1000.0))),
        Err(ViewError::Changed)
    );
    let mut zoomed = layout(0.0, 1280.0);
    zoomed.zoom = 1.25;
    assert_eq!(
        check(Some(&shown), &View::of("doc-1", zoomed)),
        Err(ViewError::Changed)
    );
}

#[test]
fn a_scroll_of_a_fraction_of_a_pixel_is_no_change() {
    let shown = View::of("doc", layout(100.0, 1280.0));
    assert_eq!(
        check(Some(&shown), &View::of("doc", layout(100.3, 1280.0))),
        Ok(())
    );
}

#[test]
fn the_view_is_told_with_its_image_size_and_scale() {
    let told = View::of("doc", layout(120.0, 1920.0)).told();
    assert_eq!(told["width"], 1280);
    assert_eq!(told["height"], 533);
    assert_eq!(told["scale"], 0.667);
    assert_eq!(told["scroll"]["y"], 120);
}
