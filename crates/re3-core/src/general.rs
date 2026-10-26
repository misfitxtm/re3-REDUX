//! The game's random number generator (`CGeneral::GetRandomNumber*`, `myrand`).

/// Which generator the build uses. re3's default PC build uses the PS2 one
/// (`USE_PS2_RAND`, 16-bit results); the original PC executable used the CodeWarrior one (15 bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RandKind {
    Ps2,
    CodeWarrior,
}

#[derive(Debug, Clone)]
pub struct Rng {
    kind: RandKind,
    seed: u64,
}

impl Default for Rng {
    fn default() -> Self {
        Self::new(RandKind::Ps2)
    }
}

impl Rng {
    pub fn new(kind: RandKind) -> Self {
        Self { kind, seed: 1 }
    }

    /// `mysrand`.
    pub fn seed(&mut self, seed: u32) {
        self.seed = seed as u64;
    }

    pub fn max(&self) -> u16 {
        match self.kind {
            RandKind::Ps2 => 65535,
            RandKind::CodeWarrior => 32767,
        }
    }

    /// `myrand()`.
    pub fn myrand(&mut self) -> i32 {
        match self.kind {
            RandKind::Ps2 => {
                self.seed = 0x5851F42D4C957F2D_u64.wrapping_mul(self.seed).wrapping_add(1);
                ((self.seed >> 32) & 0x7FFF_FFFF) as i32
            }
            RandKind::CodeWarrior => {
                // a 32-bit `unsigned long` in the original
                self.seed = (self.seed as u32).wrapping_mul(1103515245).wrapping_add(12345) as u64;
                ((self.seed >> 16) & 0x7FFF) as i32
            }
        }
    }

    /// `GetRandomNumber()`.
    pub fn number(&mut self) -> u16 {
        (self.myrand() & self.max() as i32) as u16
    }

    /// `GetRandomTrueFalse()`.
    pub fn true_false(&mut self) -> bool {
        self.number() < self.max() / 2
    }

    /// `GetRandomNumberInRange(float, float)`.
    pub fn range_f32(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * (self.number() as f32 / (self.max() as f32 + 1.0))
    }

    /// `GetRandomNumberInRange(int32, int32)`.
    pub fn range_i32(&mut self, low: i32, high: i32) -> i32 {
        (low as f32 + (high - low) as f32 * (self.number() as f32 / (self.max() as f32 + 1.0))) as i32
    }
}

/// `CGeneral::GetRadianAngleBetweenPoints`.
pub fn radian_angle_between_points(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let x = x2 - x1;
    let mut y = y2 - y1;
    if y == 0.0 {
        y = 0.0001;
    }
    let a = (x / y).atan2(1.0);
    match (x > 0.0, y > 0.0) {
        (true, true) => std::f32::consts::PI - a,
        (false, true) => -(std::f32::consts::PI + a),
        _ => -a,
    }
}

/// `CGeneral::GetNodeHeadingFromVector`: the 0..7 path direction a vector points in.
pub fn node_heading_from_vector(x: f32, y: f32) -> i32 {
    use std::f32::consts::{PI, TAU};
    let mut angle = radian_angle_between_points(x, y, 0.0, 0.0);
    if angle < 0.0 {
        angle += TAU;
    }
    angle = 22.5 * PI / 180.0 + TAU - angle;
    if angle >= TAU {
        angle -= TAU;
    }
    (angle / (45.0 * PI / 180.0)).floor() as i32
}

/// `CGeneral::LimitAngle`: degrees into -180..180.
pub fn limit_angle(angle: f32) -> f32 {
    let mut r = angle;
    while r >= 180.0 {
        r -= 2.0 * 180.0;
    }
    while r < -180.0 {
        r += 2.0 * 180.0;
    }
    r
}

/// `CGeneral::LimitRadianAngle`: radians into -pi..pi (inputs are clamped to +-25 first).
pub fn limit_radian_angle(angle: f32) -> f32 {
    use std::f32::consts::PI;
    let mut r = angle.clamp(-25.0, 25.0);
    while r >= PI {
        r -= 2.0 * PI;
    }
    while r < -PI {
        r += 2.0 * PI;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps2_generator_is_the_published_lcg() {
        let mut r = Rng::new(RandKind::Ps2);
        // seed 1 -> 0x5851F42D4C957F2D + 1; the result is bits 32..62
        let s1 = 0x5851F42D4C957F2D_u64.wrapping_add(1);
        assert_eq!(r.myrand(), ((s1 >> 32) & 0x7FFF_FFFF) as i32);
        let s2 = 0x5851F42D4C957F2D_u64.wrapping_mul(s1).wrapping_add(1);
        assert_eq!(r.myrand(), ((s2 >> 32) & 0x7FFF_FFFF) as i32);
        assert_eq!(r.max(), 65535);
    }

    #[test]
    fn codewarrior_generator_matches_the_classic_lcg() {
        let mut r = Rng::new(RandKind::CodeWarrior);
        // first outputs of rand() with seed 1 on this LCG: 16838, 5758, 10113
        assert_eq!([r.myrand(), r.myrand(), r.myrand()], [16838, 5758, 10113]);
        assert_eq!(r.max(), 32767);
    }

    #[test]
    fn ranges_stay_inside_their_bounds() {
        let mut r = Rng::default();
        for _ in 0..10_000 {
            let f = r.range_f32(-3.0, 5.0);
            assert!((-3.0..5.0).contains(&f));
            let i = r.range_i32(10, 20);
            assert!((10..20).contains(&i));
        }
        r.seed(7);
        let a = r.number();
        r.seed(7);
        assert_eq!(a, r.number());
    }
}
