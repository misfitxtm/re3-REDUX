//! `CCarGenerator`/`CTheCarGenerators`: parked car spawn points.
use glam::{Vec2, Vec3};

pub const NUM_CARGENS: usize = 160;
pub const CARGEN_MAXACTUALLIMIT: usize = 100;

#[derive(Debug, Clone, Default)]
pub struct CarGenerator {
    pub model_index: i32,
    pub pos: Vec3,
    pub angle: f32,
    pub colour1: i16,
    pub colour2: i16,
    pub force_spawn: u8,
    pub alarm: u8,
    pub doorlock: u8,
    pub min_delay: i16,
    pub max_delay: i16,
    pub timer: u32,
    /// Handle of the vehicle this generator made, or -1.
    pub vehicle_handle: i32,
    pub uses_remaining: u16,
    pub is_blocking: bool,
    pub inf: Vec3,
    pub sup: Vec3,
    pub size: f32,
}

impl CarGenerator {
    /// `Setup`; `inf`/`sup` are the model's bounding box corners.
    #[allow(clippy::too_many_arguments)] // the script command's arguments
    pub fn setup(pos: Vec3, angle: f32, model: i32, colour1: i16, colour2: i16, force: u8, alarm: u8, lock: u8, min_delay: u16, max_delay: u16, now_ms: u32, inf: Vec3, sup: Vec3) -> Self {
        Self {
            model_index: model,
            pos,
            angle,
            colour1,
            colour2,
            force_spawn: force,
            alarm,
            doorlock: lock,
            min_delay: min_delay as i16,
            max_delay: max_delay as i16,
            timer: now_ms + 1,
            vehicle_handle: -1,
            uses_remaining: 0,
            is_blocking: false,
            inf,
            sup,
            size: crate::math::magnitude(inf).max(crate::math::magnitude(sup)),
        }
    }

    /// `CalcNextGen`.
    pub fn calc_next_gen(now_ms: u32) -> u32 {
        now_ms + 4
    }

    /// Whether the player is in the range where this generator spawns (`CheckIfWithinRangeOfAnyPlayer`).
    pub fn within_range(&mut self, centre: Vec2, speed: Vec2, generation_dist_multiplier: f32, even_if_close: bool) -> bool {
        let direction = centre - Vec2::new(self.pos.x, self.pos.y);
        let distance = direction.length();
        let farclip = 120.0 * generation_dist_multiplier;
        let nearclip = farclip - 20.0;
        if distance >= farclip {
            if self.is_blocking {
                self.is_blocking = false;
            }
            return false;
        }
        if even_if_close {
            return true;
        }
        if self.is_blocking || distance < nearclip {
            return false;
        }
        direction.dot(speed) <= 0.0
    }
}

#[derive(Debug, Clone)]
#[derive(Default)]
pub struct CarGenManager {
    pub process_counter: u8,
    pub generators: Vec<CarGenerator>,
    pub generate_even_if_player_is_close_counter: u8,
    pub current_active_count: u32,
}


impl CarGenManager {
    /// `CreateCarGenerator`: returns the new generator's index.
    pub fn create(&mut self, g: CarGenerator) -> Option<usize> {
        if self.generators.len() >= NUM_CARGENS {
            return None;
        }
        self.generators.push(g);
        Some(self.generators.len() - 1)
    }

    /// `SwitchOn`.
    pub fn switch_on(&mut self, i: usize, now_ms: u32) {
        let g = &mut self.generators[i];
        g.uses_remaining = u16::MAX;
        g.timer = CarGenerator::calc_next_gen(now_ms);
        self.current_active_count += 1;
    }

    /// `SwitchOff` (`FIX_BUGS`: only counts a generator that was on).
    pub fn switch_off(&mut self, i: usize) {
        let g = &mut self.generators[i];
        if g.uses_remaining != 0 {
            g.uses_remaining = 0;
            self.current_active_count -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gen_at(x: f32) -> CarGenerator {
        CarGenerator::setup(Vec3::new(x, 0.0, 0.0), 90.0, 10, -1, 0, 0, 10, 20, 0, 0, 1000, Vec3::new(-1.0, -2.0, -0.5), Vec3::new(1.0, 2.5, 1.0))
    }

    #[test]
    fn setup_measures_the_model_and_starts_idle() {
        let g = gen_at(0.0);
        assert_eq!((g.vehicle_handle, g.uses_remaining, g.timer, g.is_blocking), (-1, 0, 1001, false));
        assert!((g.size - (1.0f32 + 6.25 + 1.0).sqrt()).abs() < 1e-5);
    }

    #[test]
    fn switching_counts_active_generators() {
        let mut m = CarGenManager::default();
        let i = m.create(gen_at(0.0)).unwrap();
        m.switch_on(i, 500);
        assert_eq!((m.current_active_count, m.generators[i].uses_remaining, m.generators[i].timer), (1, u16::MAX, 504));
        m.switch_off(i);
        m.switch_off(i);
        assert_eq!(m.current_active_count, 0);
    }

    #[test]
    fn generators_spawn_only_ahead_of_a_moving_player_in_the_ring() {
        let mut g = gen_at(100.0);
        let speed = Vec2::new(1.0, 0.0);
        // player at x = 0 heading for it: it is 100 away, in the ring of 100..120, so cars appear ahead
        assert!(g.within_range(Vec2::ZERO, speed, 1.0, false), "ahead of the player is allowed");
        // driving away from it: not allowed
        assert!(!g.within_range(Vec2::ZERO, -speed, 1.0, false));
        // inside the near radius: no
        assert!(!g.within_range(Vec2::new(50.0, 0.0), speed, 1.0, false));
        // too far: no, and a blocking flag clears
        g.is_blocking = true;
        assert!(!g.within_range(Vec2::new(-100.0, 0.0), speed, 1.0, false), "more than 120 away");
        assert!(!g.is_blocking);
        // forced generation ignores the ring
        assert!(g.within_range(Vec2::new(90.0, 0.0), speed, 1.0, true));
    }
}
