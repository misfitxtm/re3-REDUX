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
