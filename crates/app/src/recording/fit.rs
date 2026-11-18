//! How a captured picture is placed on the canvas, whose size is fixed when the recording opens. No side effects.

/// A rectangle on the canvas, in pixels. `right` and `bottom` are exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

/// How a picture of one size goes onto the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Exactly the canvas size: the picture is used as it is.
    Whole,
    /// Larger than the canvas by at most one pixel in each direction, which is what making an odd size even does:
    /// the top-left part of the canvas size is used, without scaling.
    Crop,
    /// Any other size: scaled into this rectangle with its aspect ratio kept, the rest black.
    Scale(Rect),
}

/// Where a picture of `picture` size goes on a canvas of `canvas` size.
pub fn placement(picture: (u32, u32), canvas: (u32, u32)) -> Placement {
    let trims = |picture: u32, canvas: u32| picture == canvas || picture == canvas + 1;
    if picture == canvas {
        Placement::Whole
    } else if trims(picture.0, canvas.0) && trims(picture.1, canvas.1) {
        Placement::Crop
    } else {
        Placement::Scale(fitted(picture, canvas))
    }
}

/// The centered rectangle that results from fitting `source` into `target` while keeping the aspect ratio.
fn fitted(source: (u32, u32), target: (u32, u32)) -> Rect {
    let scale = f64::min(
        target.0 as f64 / source.0.max(1) as f64,
        target.1 as f64 / source.1.max(1) as f64,
    );
    let width = ((source.0 as f64 * scale).round() as u32).clamp(1, target.0.max(1));
    let height = ((source.1 as f64 * scale).round() as u32).clamp(1, target.1.max(1));
    let left = (target.0.saturating_sub(width)) / 2;
    let top = (target.1.saturating_sub(height)) / 2;
    Rect {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scaled_into(picture: (u32, u32), canvas: (u32, u32)) -> (u32, u32, u32, u32) {
        match placement(picture, canvas) {
            Placement::Scale(r) => (r.left, r.top, r.right, r.bottom),
            other => panic!("expected scaling, got {other:?}"),
        }
    }

    #[test]
    fn a_picture_of_the_canvas_size_is_used_whole() {
        assert_eq!(placement((1920, 1080), (1920, 1080)), Placement::Whole);
    }

    #[test]
    fn an_odd_sized_picture_made_even_is_cropped_not_scaled() {
        assert_eq!(placement((1585, 993), (1584, 992)), Placement::Crop);
        assert_eq!(placement((1585, 992), (1584, 992)), Placement::Crop);
        assert_eq!(placement((1584, 993), (1584, 992)), Placement::Crop);
    }

    #[test]
    fn a_picture_smaller_than_the_canvas_or_larger_by_more_than_a_pixel_is_scaled() {
        assert_eq!(scaled_into((1583, 992), (1584, 992)), (0, 0, 1583, 992));
        assert_eq!(scaled_into((1586, 992), (1584, 992)), (0, 0, 1584, 991));
    }

    #[test]
    fn same_aspect_ratio_fills_the_whole_canvas() {
        assert_eq!(scaled_into((1280, 720), (1920, 1080)), (0, 0, 1920, 1080));
        assert_eq!(scaled_into((3840, 2160), (1920, 1080)), (0, 0, 1920, 1080));
    }

    #[test]
    fn wide_pictures_get_margins_above_and_below() {
        assert_eq!(scaled_into((2000, 500), (1000, 1000)), (0, 375, 1000, 625));
    }

    #[test]
    fn tall_pictures_get_margins_left_and_right() {
        assert_eq!(scaled_into((500, 1000), (1000, 1000)), (250, 0, 750, 1000));
    }
}
