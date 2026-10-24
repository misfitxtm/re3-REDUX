//! `CShotInfo`: the area a flamethrower burst covers. No visuals; it sets peds and cars alight.
use super::info::{flag, WeaponInfos};
use crate::{entity::EntityRef, general::Rng, timer::Timer};
use glam::Vec3;

pub const NUM_SHOTS: usize = 100;
const RAND_TABLE: [f32; 20] = {
    let mut t = [0.0; 20];
    let mut v = -0.05_f32;
    let mut i = 0;
    while i < 20 {
        t[i] = v;
        v += 0.005;
        i += 1;
    }
    t
};

#[derive(Clone, Debug)]
pub struct Shot {
    pub weapon: usize,
    pub start_pos: Vec3,
    pub area_affected: Vec3,
    pub radius: f32,
    pub source: Option<EntityRef>,
    pub timeout: f32,
    pub in_use: bool,
}

impl Default for Shot {
    fn default() -> Self {
        Self { weapon: super::info::weapon_type::COLT45, start_pos: Vec3::ZERO, area_affected: Vec3::ZERO, radius: 1.0, source: None, timeout: 0.0, in_use: false }
    }
}

/// The ped and car side of a burst, which needs the world.
pub trait ShotEnv {
    /// Peds near the shooter inside `radius` of `pos` flee and catch fire.
    fn burn_near_peds(&mut self, source: EntityRef, pos: Vec3, radius: f32);
    fn set_cars_on_fire(&mut self, pos: Vec3, radius: f32, source: Option<EntityRef>);
    fn source_valid(&self, source: EntityRef) -> bool;
}

#[derive(Clone, Debug)]
pub struct Shots(pub Vec<Shot>);

impl Default for Shots {
    fn default() -> Self {
        Self(vec![Shot::default(); NUM_SHOTS])
    }
}

impl Shots {
    /// `CShotInfo::AddShot`.
    #[allow(clippy::too_many_arguments)] // mirrors the original's signature
    pub fn add(&mut self, infos: &WeaponInfos, rng: &mut Rng, timer: &Timer, source: Option<EntityRef>, weapon: usize, start: Vec3, end: Vec3) -> bool {
        let w = infos.get(weapon);
        let Some(s) = self.0.iter_mut().find(|s| !s.in_use) else { return false };
        s.in_use = true;
        s.weapon = weapon;
        s.start_pos = start;
        s.area_affected = end - start;
        s.radius = w.radius;
        let pick = |rng: &mut Rng| RAND_TABLE[rng.number() as usize % RAND_TABLE.len()];
        if w.spread != 0.0 {
            s.area_affected.x += pick(rng) * w.spread;
            s.area_affected.y += pick(rng) * w.spread;
            s.area_affected.z += pick(rng);
        }
        s.area_affected = s.area_affected.normalize_or_zero();
        if w.is_flag_set(flag::RAND_SPEED) {
            s.area_affected *= pick(rng) + w.speed;
        } else {
            s.area_affected *= w.speed;
        }
        s.source = source;
        s.timeout = timer.time_in_ms as f32 + w.lifespan;
        true
    }

    /// `CShotInfo::Update`.
    pub fn update(&mut self, infos: &WeaponInfos, timer: &Timer, env: &mut dyn ShotEnv) {
        for (slot, s) in self.0.iter_mut().enumerate() {
            if s.source.is_some_and(|e| !env.source_valid(e)) {
                s.source = None;
            }
            if !s.in_use {
                continue;
            }
            let w = infos.get(s.weapon);
            if timer.time_in_ms as f32 > s.timeout {
                s.in_use = false;
            }
            if w.is_flag_set(flag::SLOWS_DOWN) {
                s.area_affected *= 0.96_f64.powf(timer.time_step as f64) as f32;
            }
            if w.is_flag_set(flag::EXPANDS) {
                s.radius += 0.075 * timer.time_step;
            }
            s.start_pos += timer.time_step * s.area_affected;
            if let Some(src) = s.source {
                env.burn_near_peds(src, s.start_pos, s.radius.max(1.0));
            }
            if (timer.frame_counter as usize + slot) & 3 == 0 {
                env.set_cars_on_fire(s.start_pos, 4.0, s.source);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Env {
        cars: u32,
        peds: u32,
    }
    impl ShotEnv for Env {
        fn burn_near_peds(&mut self, _: EntityRef, _: Vec3, _: f32) {
            self.peds += 1;
        }
        fn set_cars_on_fire(&mut self, _: Vec3, _: f32, _: Option<EntityRef>) {
            self.cars += 1;
        }
        fn source_valid(&self, _: EntityRef) -> bool {
            true
        }
    }

    #[test]
    fn burst_moves_slows_and_expires() {
        let mut infos = WeaponInfos::default();
        let w = &mut infos.0[super::super::info::weapon_type::FLAMETHROWER];
        (w.speed, w.lifespan, w.radius) = (0.5, 100.0, 1.0);
        let (mut shots, mut rng, mut timer, mut env) = (Shots::default(), Rng::default(), Timer::default(), Env::default());
        let src = Some(EntityRef { pool: crate::entity::PoolKind::Ped, handle: 1 });
        assert!(shots.add(&infos, &mut rng, &timer, src, super::super::info::weapon_type::FLAMETHROWER, Vec3::ZERO, Vec3::X * 5.0));
        let speed0 = shots.0[0].area_affected.length();
        shots.update(&infos, &timer, &mut env);
        assert!(shots.0[0].area_affected.length() < speed0);
        assert!(shots.0[0].start_pos.x > 0.0 && shots.0[0].radius > 1.0);
        assert_eq!((env.peds, env.cars), (1, 1));
        timer.time_in_ms = 200;
        shots.update(&infos, &timer, &mut env);
        assert!(!shots.0[0].in_use);
    }
}
