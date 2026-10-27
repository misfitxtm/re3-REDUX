//! `Curves.cpp`: the smooth path a car follows between two road links.
use glam::{Vec2, Vec3};

/// The original's pi.
#[allow(clippy::approx_constant)]
const NOT_PI: f32 = 3.1415;

/// `CCurves::CalcSpeedScaleFactor`: how long the curve is, relative to the straight distance.
pub fn calc_speed_scale_factor(p1: Vec3, p2: Vec3, dir1: Vec2, dir2: Vec2) -> f32 {
    let distance = Vec2::new(p1.x - p2.x, p1.y - p2.y).length();
    let dp = dir1.dot(dir2);
    if dp > 0.9 {
        distance + ((p1.x * dir1.y - p1.y * dir1.x) - (p2.x * dir1.y - p2.y * dir1.x)).abs()
    } else {
        ((1.0 - dp) * 0.2 + 1.0) * distance
    }
}

/// `CCurves::CalcCurvePoint`: the position and velocity `between` (0..1) along the curve, which takes `time_on_curve` ms.
pub fn calc_curve_point(pos1: Vec3, pos2: Vec3, dir1: Vec3, dir2: Vec3, between: f32, time_on_curve: i32) -> (Vec3, Vec3) {
    let factor = calc_speed_scale_factor(pos1, pos2, Vec2::new(dir1.x, dir1.y), Vec2::new(dir2.x, dir2.y));
    let d1 = Vec2::new(dir1.x, dir1.y) * factor;
    let d2 = Vec2::new(dir2.x, dir2.y) * factor;
    let coef = 0.5 - 0.5 * (NOT_PI * between).cos();
    let pos = Vec3::new(
        (pos1.x + between * d1.x) * (1.0 - coef) + (pos2.x - (1.0 - between) * d2.x) * coef,
        (pos1.y + between * d1.y) * (1.0 - coef) + (pos2.y - (1.0 - between) * d2.y) * coef,
        0.0,
    );
    let t = time_on_curve as f32 * 0.001;
    let dir = Vec3::new((d1.x * (1.0 - coef) + d2.x * coef) / t, (d1.y * (1.0 - coef) + d2.y * coef) / t, 0.0);
    (pos, dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_straight_link_is_as_long_as_the_distance_plus_the_lane_shift() {
        let (d, a, b) = (Vec2::Y, Vec3::ZERO, Vec3::new(2.0, 10.0, 0.0));
        let f = calc_speed_scale_factor(a, b, d, d);
        assert!((f - (Vec2::new(2.0, 10.0).length() + 2.0)).abs() < 1e-5);
    }

    #[test]
    fn a_turn_is_longer_than_the_chord() {
        let f = calc_speed_scale_factor(Vec3::ZERO, Vec3::new(10.0, 10.0, 0.0), Vec2::Y, Vec2::X);
        assert!((f - 1.2 * Vec2::new(10.0, 10.0).length()).abs() < 1e-4);
    }

    #[test]
    fn the_curve_starts_and_ends_on_its_points() {
        let (a, b, d) = (Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 20.0, 0.0), Vec3::Y);
        assert!((calc_curve_point(a, b, d, d, 0.0, 1000).0 - a).length() < 1e-4);
        assert!((calc_curve_point(a, b, d, d, 1.0, 1000).0 - b).length() < 1e-4);
    }
}
