//! Where the drop-down panel goes and how it gets there: the screen the pointer is on, the panel's place on it,
//! and the curve of its slide. Plain numbers in physical pixels, so that all of it is tested without a window
//! (quick.rs does what the numbers say).

use serde::Serialize;

/// How tall the panel is, as a share of the screen's work area: `visibleFrame.height * 0.42`.
const HEIGHT_SHARE: f64 = 0.42;

/// A rectangle of the desktop, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= f64::from(self.x)
            && x < f64::from(self.x) + f64::from(self.width)
            && y >= f64::from(self.y)
            && y < f64::from(self.y) + f64::from(self.height)
    }

    /// The square of how far a point is from the rectangle: nothing inside it.
    fn distance2(&self, x: f64, y: f64) -> f64 {
        let gap = |low: f64, size: f64, at: f64| (low - at).max(at - (low + size)).max(0.0);
        let dx = gap(f64::from(self.x), f64::from(self.width), x);
        let dy = gap(f64::from(self.y), f64::from(self.height), y);
        dx * dx + dy * dy
    }
}

/// A monitor: all of it, and the part of it the taskbar and the like leave (the Mac's `frame` and `visibleFrame`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Screen {
    pub frame: Rect,
    pub work: Rect,
}

/// The screen the pointer is on: the panel is summoned by a keystroke, and the keystroke's context is wherever the
/// person is working. A pointer on none (the gap a staggered layout leaves) goes to the nearest.
pub fn screen_at(screens: &[Screen], x: f64, y: f64) -> Option<Screen> {
    screens.iter().copied().find(|s| s.frame.contains(x, y)).or_else(|| {
        screens
            .iter()
            .copied()
            .min_by(|a, b| a.frame.distance2(x, y).total_cmp(&b.frame.distance2(x, y)))
    })
}

/// Where the panel rests on a screen: the screen's whole width, `HEIGHT_SHARE` of the work area's height (rounded,
/// as the Mac's), hung from the top of the work area.
pub fn resting(screen: &Screen) -> Rect {
    Rect {
        x: screen.frame.x,
        y: screen.work.y,
        width: screen.frame.width,
        height: (f64::from(screen.work.height) * HEIGHT_SHARE).round() as u32,
    }
}

/// The top edge of the panel when it stands one height above its resting place: where it slides in from.
pub fn above(rest: Rect) -> i32 {
    rest.y - i32::try_from(rest.height).unwrap_or(i32::MAX)
}

/// A cubic Bézier timing curve, as Core Animation's named ones are.
#[derive(Clone, Copy, Debug)]
pub struct Curve {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

impl Curve {
    /// `CAMediaTimingFunction.easeOut`: the slide in.
    pub const EASE_OUT: Curve = Curve {
        x1: 0.0,
        y1: 0.0,
        x2: 0.58,
        y2: 1.0,
    };
    /// `CAMediaTimingFunction.easeIn`: the slide out.
    pub const EASE_IN: Curve = Curve {
        x1: 0.42,
        y1: 0.0,
        x2: 1.0,
        y2: 1.0,
    };

    fn bezier(a: f64, b: f64, s: f64) -> f64 {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    }

    /// How far along the motion is, from 0 to 1, `t` of the way through the time.
    pub fn at(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        // x grows with the parameter, so the parameter for a given x is found by halving.
        let (mut low, mut high) = (0.0, 1.0);
        for _ in 0..40 {
            let mid = (low + high) / 2.0;
            if Self::bezier(self.x1, self.x2, mid) < t {
                low = mid;
            } else {
                high = mid;
            }
        }
        Self::bezier(self.y1, self.y2, (low + high) / 2.0)
    }
}

/// `progress` of the way from `from` to `to`.
pub fn lerp(from: i32, to: i32, progress: f64) -> i32 {
    from + (f64::from(to - from) * progress).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect { x, y, width, height }
    }

    /// A 1920x1080 monitor at the origin whose taskbar, at the bottom, takes 48 pixels.
    const MAIN: Screen = Screen {
        frame: rect(0, 0, 1920, 1080),
        work: rect(0, 0, 1920, 1032),
    };
    /// A second monitor to the left, with its taskbar on top.
    const LEFT: Screen = Screen {
        frame: rect(-1280, -100, 1280, 1024),
        work: rect(-1280, -52, 1280, 976),
    };

    #[test]
    fn the_panel_is_the_screens_width_and_42_percent_of_the_work_areas_height() {
        // 1032 * 0.42 = 433.44, rounded as the Mac's `.rounded()`.
        assert_eq!(resting(&MAIN), rect(0, 0, 1920, 433));
        // Hung from the work area's top (under a taskbar on top), at the monitor's own left edge.
        // 976 * 0.42 = 409.92.
        assert_eq!(resting(&LEFT), rect(-1280, -52, 1280, 410));
    }

    #[test]
    fn the_height_rounds_half_up() {
        let screen = Screen {
            frame: rect(0, 0, 100, 100),
            work: rect(0, 0, 100, 50),
        };
        // 50 * 0.42 = 21 exactly, and 25 * 0.42 = 10.5 goes up.
        assert_eq!(resting(&screen).height, 21);
        let screen = Screen {
            frame: rect(0, 0, 100, 100),
            work: rect(0, 0, 100, 25),
        };
        assert_eq!(resting(&screen).height, 11);
    }

    #[test]
    fn it_slides_in_from_one_height_above() {
        let rest = resting(&MAIN);
        assert_eq!(above(rest), -433);
        assert_eq!(above(resting(&LEFT)), -52 - 410);
    }

    #[test]
    fn the_screen_is_the_one_the_pointer_is_on() {
        let both = [MAIN, LEFT];
        assert_eq!(screen_at(&both, 10.0, 10.0), Some(MAIN));
        assert_eq!(screen_at(&both, -5.0, 300.0), Some(LEFT));
        assert_eq!(screen_at(&both, 1919.0, 1079.0), Some(MAIN));
        // The edge between two screens belongs to the one that starts there.
        assert_eq!(screen_at(&both, -1.0, 10.0), Some(LEFT));
        assert_eq!(screen_at(&both, 0.0, 10.0), Some(MAIN));
    }

    #[test]
    fn a_pointer_on_no_screen_goes_to_the_nearest() {
        let both = [MAIN, LEFT];
        // Below the left screen (it ends at y = 924), nearer to it than to the main one.
        assert_eq!(screen_at(&both, -600.0, 1100.0), Some(LEFT));
        // Far to the right of everything.
        assert_eq!(screen_at(&both, 5000.0, 500.0), Some(MAIN));
        assert_eq!(screen_at(&[], 0.0, 0.0), None);
    }

    #[test]
    fn a_slide_is_a_curve_from_one_end_to_the_other() {
        for curve in [Curve::EASE_OUT, Curve::EASE_IN] {
            assert!(curve.at(0.0).abs() < 1e-9);
            assert!((curve.at(1.0) - 1.0).abs() < 1e-9);
            let mut last = 0.0;
            for step in 1..=100 {
                let at = curve.at(f64::from(step) / 100.0);
                assert!(at >= last - 1e-12, "{curve:?} goes back at {step}");
                last = at;
            }
        }
        // Out starts fast and settles; in starts slow and ends fast.
        assert!(Curve::EASE_OUT.at(0.5) > 0.5);
        assert!(Curve::EASE_IN.at(0.5) < 0.5);
        // Outside the time it is held at the ends.
        assert!(Curve::EASE_OUT.at(-1.0).abs() < 1e-9);
        assert!((Curve::EASE_OUT.at(2.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn lerp_goes_from_one_edge_to_the_other() {
        assert_eq!(lerp(-433, 0, 0.0), -433);
        assert_eq!(lerp(-433, 0, 1.0), 0);
        assert_eq!(lerp(-433, 0, 0.5), -216);
        assert_eq!(lerp(0, -433, 1.0), -433);
    }
}
