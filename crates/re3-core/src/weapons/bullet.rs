//! `CBulletInfo`: fast instant-hit bullets that travel a few metres a frame (sniper, AK, etc).
use super::info::{weapon_type, WeaponInfos};
use crate::{entity::{EntityRef, EntityType}, timer::Timer};
use glam::Vec3;

pub const NUM_BULLETS: usize = 100;
const LIFETIME: u32 = 1000;
const MAP_BORDER: f32 = 1960.0;

#[derive(Clone, Debug)]
pub struct Bullet {
    pub in_use: bool,
    pub weapon: usize,
    pub timer: f32,
    pub source: Option<EntityRef>,
    pub damage: u32,
    pub pos: Vec3,
    pub speed: Vec3,
}

impl Default for Bullet {
    fn default() -> Self {
        Self { in_use: false, weapon: weapon_type::COLT45, timer: 0.0, source: None, damage: 0, pos: Vec3::ZERO, speed: Vec3::ZERO }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BulletHit {
    pub entity: EntityRef,
    pub ty: EntityType,
    pub point: Vec3,
    pub normal: Vec3,
    pub piece: u8,
}

/// What the world did with a hit.
#[derive(Clone, Copy, Debug, Default)]
pub struct Impact {
    /// The bullet ends at the hit point.
    pub stop: bool,
    /// The hit was on a ped's missed body part, so the sniper hit sound is skipped from here on.
    pub quiet: bool,
}

pub trait BulletEnv {
    /// Line of sight with car tyres and dead peds included, ignoring `ignore`.
    fn line_of_sight(&mut self, from: Vec3, to: Vec3, ignore: Option<EntityRef>) -> Option<BulletHit>;
    /// Damage, sparks, blood, glass, sniper sound, explosive things: everything a hit does.
    fn impact(&mut self, bullet: &Bullet, hit: &BulletHit, add_sound: bool) -> Impact;
    fn source_valid(&self, source: EntityRef) -> bool;
}

#[derive(Clone, Debug)]
pub struct Bullets {
    pub list: Vec<Bullet>,
    pub player_sniper_bullet: bool,
    pub sniper_start: Vec3,
    pub sniper_end: Vec3,
}

impl Default for Bullets {
    fn default() -> Self {
        Self { list: vec![Bullet::default(); NUM_BULLETS], player_sniper_bullet: false, sniper_start: Vec3::ZERO, sniper_end: Vec3::ZERO }
    }
}

impl Bullets {
    /// `CBulletInfo::AddBullet`.
    pub fn add(&mut self, infos: &WeaponInfos, timer: &Timer, source: Option<EntityRef>, weapon: usize, pos: Vec3, speed: Vec3) -> bool {
        let Some(b) = self.list.iter_mut().find(|b| !b.in_use) else { return false };
        *b = Bullet { in_use: true, weapon, timer: (timer.time_in_ms + LIFETIME) as f32, source, damage: infos.get(weapon).damage, pos, speed };
        true
    }

    /// `CBulletInfo::Update`.
    pub fn update(&mut self, timer: &Timer, env: &mut dyn BulletEnv) {
        let mut add_sound = true;
        self.player_sniper_bullet = false;
        for i in 0..NUM_BULLETS {
            if self.list[i].source.is_some_and(|s| !env.source_valid(s)) {
                self.list[i].source = None;
            }
            if !self.list[i].in_use {
                continue;
            }
            if timer.time_in_ms as f32 > self.list[i].timer {
                self.list[i].in_use = false;
            }
            let old = self.list[i].pos;
            let mut new = old + self.list[i].speed * timer.time_step * 0.5;
            if let Some(hit) = env.line_of_sight(old, new, self.list[i].source) {
                let r = env.impact(&self.list[i], &hit, add_sound);
                if r.quiet {
                    add_sound = false;
                }
                if r.stop {
                    self.list[i].in_use = false;
                    new = hit.point;
                }
            }
            let b = &mut self.list[i];
            if b.weapon == weapon_type::SNIPERRIFLE {
                self.player_sniper_bullet = true;
                self.sniper_start = b.pos;
                self.sniper_end = new;
            }
            b.pos = new;
            if b.pos.x < -MAP_BORDER || b.pos.x > MAP_BORDER || b.pos.y < -MAP_BORDER || b.pos.y > MAP_BORDER {
                b.in_use = false;
            }
        }
    }

    /// `CBulletInfo::TestForSniperBullet` (the bug-fixed segment-versus-box test).
    pub fn test_for_sniper_bullet(&self, min: Vec3, max: Vec3) -> bool {
        if !self.player_sniper_bullet {
            return false;
        }
        let (a, d) = (self.sniper_start, self.sniper_end - self.sniper_start);
        let (mut t0, mut t1) = (0.0_f32, 1.0_f32);
        for k in 0..3 {
            if d[k] == 0.0 {
                if a[k] < min[k] || a[k] > max[k] {
                    return false;
                }
            } else {
                let (mut u, mut v) = ((min[k] - a[k]) / d[k], (max[k] - a[k]) / d[k]);
                if u > v {
                    std::mem::swap(&mut u, &mut v);
                }
                t0 = t0.max(u);
                t1 = t1.min(v);
                if t0 > t1 {
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::PoolKind;

    struct Env {
        wall_x: f32,
    }
    impl BulletEnv for Env {
        fn line_of_sight(&mut self, from: Vec3, to: Vec3, _: Option<EntityRef>) -> Option<BulletHit> {
            (from.x < self.wall_x && to.x >= self.wall_x).then_some(BulletHit {
                entity: EntityRef { pool: PoolKind::Building, handle: 0 },
                ty: EntityType::Building,
                point: Vec3::new(self.wall_x, from.y, from.z),
                normal: -Vec3::X,
                piece: 0,
            })
        }
        fn impact(&mut self, _: &Bullet, _: &BulletHit, _: bool) -> Impact {
            Impact { stop: true, quiet: false }
        }
        fn source_valid(&self, _: EntityRef) -> bool {
            true
        }
    }

    #[test]
    fn bullet_flies_stops_and_tracks_sniper() {
        let infos = WeaponInfos::default();
        let (mut b, mut timer, mut env) = (Bullets::default(), Timer::default(), Env { wall_x: 10.0 });
        assert!(b.add(&infos, &timer, None, weapon_type::SNIPERRIFLE, Vec3::ZERO, Vec3::X * 8.0));
        b.update(&timer, &mut env);
        assert_eq!(b.list[0].pos.x, 4.0);
        assert!(b.test_for_sniper_bullet(Vec3::new(1.0, -1.0, -1.0), Vec3::new(2.0, 1.0, 1.0)));
        assert!(!b.test_for_sniper_bullet(Vec3::new(5.0, -1.0, -1.0), Vec3::new(6.0, 1.0, 1.0)));
        b.update(&timer, &mut env);
        timer.time_step = 1.0;
        b.update(&timer, &mut env);
        assert!(!b.list[0].in_use);
        assert_eq!(b.list[0].pos.x, 10.0);
    }
}
