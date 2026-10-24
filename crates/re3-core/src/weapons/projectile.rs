//! `CProjectileInfo`: rockets, molotovs and grenades in flight. The flying object lives in the world.
use super::{explosion::ExplosionType, info::weapon_type};
use crate::{entity::EntityRef, general::radian_angle_between_points, math::Matrix, timer::Timer};
use glam::Vec3;

pub const NUM_PROJECTILES: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjectileModel {
    Missile,
    Molotov,
    Grenade,
}

/// `m_nSpecialCollisionResponseCases`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CollResponse {
    None,
    Unknown5,
}

pub struct Spawn {
    pub model: ProjectileModel,
    pub matrix: Matrix,
    pub velocity: Vec3,
    pub gravity: bool,
    pub elasticity: f32,
    pub response: CollResponse,
}

/// How the shooter aims, which the original reads off the ped.
pub struct Shooter {
    pub is_player: bool,
    /// Active camera front and up, used for the player's rockets.
    pub camera_front_up: (Vec3, Vec3),
    /// Z of the ped's seek target, if it has one.
    pub seek_target_z: Option<f32>,
    pub forward: Vec3,
    pub matrix: Matrix,
}

pub trait ProjectileEnv {
    fn spawn(&mut self, s: Spawn) -> Option<EntityRef>;
    fn remove(&mut self, e: EntityRef);
    fn pos(&self, e: EntityRef) -> Vec3;
    fn has_collided(&self, e: EntityRef) -> bool;
    /// `GetIsLineOfSightClear` over buildings, vehicles, peds and objects, ignoring the projectile.
    fn line_clear(&mut self, from: Vec3, to: Vec3, ignore: EntityRef) -> bool;
    /// `CHeli::TestRocketCollision` or `CPlane::TestRocketCollision`.
    fn rocket_hits_aircraft(&mut self, pos: Vec3) -> bool;
    fn add_explosion(&mut self, creator: Option<EntityRef>, ty: ExplosionType, pos: Vec3);
    fn entity_pos(&self, e: EntityRef) -> Option<Vec3>;
    /// A rocket's smoke trail.
    fn smoke(&mut self, pos: Vec3);
    fn source_valid(&self, e: EntityRef) -> bool;
}

#[derive(Clone, Debug)]
pub struct Projectile {
    pub weapon: usize,
    pub source: Option<EntityRef>,
    pub explosion_time: u32,
    pub in_use: bool,
    pub pos: Vec3,
    pub object: Option<EntityRef>,
}

impl Default for Projectile {
    fn default() -> Self {
        Self { weapon: weapon_type::GRENADE, source: None, explosion_time: 0, in_use: false, pos: Vec3::ZERO, object: None }
    }
}

#[derive(Clone, Debug)]
pub struct Projectiles(pub Vec<Projectile>);

impl Default for Projectiles {
    fn default() -> Self {
        Self(vec![Projectile::default(); NUM_PROJECTILES])
    }
}

fn heading(forward: Vec3) -> f32 {
    (-forward.x).atan2(forward.y)
}

impl Projectiles {
    /// `CProjectileInfo::AddProjectile`.
    #[allow(clippy::too_many_arguments)] // mirrors the original's signature
    pub fn add(&mut self, env: &mut dyn ProjectileEnv, timer: &Timer, source: Option<EntityRef>, shooter: &Shooter, weapon: usize, pos: Vec3, speed: f32) -> bool {
        let now = timer.time_in_ms;
        let mut response = CollResponse::None;
        let mut gravity = true;
        let mut elasticity = 0.75;
        let (time, matrix, velocity, model);
        match weapon {
            weapon_type::ROCKETLAUNCHER => {
                let mut vy = 1.25;
                time = now + 1400;
                let mut m = Matrix::IDENTITY;
                if shooter.is_player {
                    let (front, up) = shooter.camera_front_up;
                    (m.forward, m.up, m.right, m.pos) = (front, up, crate::math::cross(up, front), pos);
                } else if let Some(tz) = shooter.seek_target_z {
                    let ry = radian_angle_between_points(1.0, tz, 1.0, pos.z);
                    let rz = (-shooter.forward.x).atan2(shooter.forward.y);
                    vy = 0.35 * speed + 0.15;
                    m = Matrix::from_translation(Vec3::new(0.0, 1.0, 1.0));
                    m.rotate(0.0, ry, rz);
                    m.pos += pos;
                } else {
                    m = shooter.matrix;
                }
                velocity = m.rotate_vector(Vec3::new(0.0, vy, 0.0));
                matrix = m;
                gravity = false;
                model = ProjectileModel::Missile;
            }
            weapon_type::MOLOTOV => {
                time = now + 2000;
                let scale = (0.22 * speed + 0.15).max(0.2);
                let angle = heading(shooter.forward);
                let mut m = Matrix::IDENTITY;
                m.rotate_z(angle);
                m.pos += pos;
                matrix = m;
                velocity = Vec3::new(-scale * angle.sin(), scale * angle.cos(), (0.2 * speed + 0.4) * scale);
                model = ProjectileModel::Molotov;
            }
            weapon_type::GRENADE => {
                time = now + 2000;
                let scale = if speed != 0.0 { 0.22 * speed + 0.15 } else { 0.0 };
                let angle = heading(shooter.forward);
                let mut m = Matrix::IDENTITY;
                m.rotate_z(angle);
                m.pos += pos;
                matrix = m;
                response = CollResponse::Unknown5;
                velocity = Vec3::new(-scale * angle.sin(), scale * angle.cos(), (0.4 * speed + 0.4) * scale);
                elasticity = 0.5;
                model = ProjectileModel::Grenade;
            }
            _ => return false,
        }
        let Some(i) = self.0.iter().position(|p| !p.in_use) else { return false };
        let Some(obj) = env.spawn(Spawn { model, matrix, velocity, gravity, elasticity, response }) else { return false };
        self.0[i] = Projectile { weapon, source, explosion_time: time, in_use: true, pos: env.pos(obj), object: Some(obj) };
        true
    }

    /// `RemoveNotAdd`: the explosion a projectile leaves.
    pub fn explode(env: &mut dyn ProjectileEnv, source: Option<EntityRef>, weapon: usize, pos: Vec3) {
        let ty = match weapon {
            weapon_type::GRENADE => ExplosionType::Grenade,
            weapon_type::MOLOTOV => ExplosionType::Molotov,
            weapon_type::ROCKETLAUNCHER => ExplosionType::Rocket,
            _ => return,
        };
        env.add_explosion(source, ty, pos);
    }

    fn remove_with_explosion(&mut self, i: usize, env: &mut dyn ProjectileEnv) {
        let p = &mut self.0[i];
        let obj = p.object.take();
        if let Some(o) = obj {
            let pos = env.pos(o);
            Self::explode(env, p.source, p.weapon, pos);
            env.remove(o);
        }
        p.in_use = false;
    }

    /// `CProjectileInfo::Update`.
    pub fn update(&mut self, env: &mut dyn ProjectileEnv, timer: &Timer) {
        for i in 0..self.0.len() {
            if !self.0[i].in_use {
                continue;
            }
            if self.0[i].source.is_some_and(|s| !env.source_valid(s)) {
                self.0[i].source = None;
            }
            let Some(obj) = self.0[i].object else {
                self.0[i].in_use = false;
                continue;
            };
            let weapon = self.0[i].weapon;
            if weapon == weapon_type::ROCKETLAUNCHER {
                let pos = env.pos(obj);
                env.smoke(pos);
            }
            if timer.time_in_ms <= self.0[i].explosion_time {
                let pos = env.pos(obj);
                let last = self.0[i].pos;
                if weapon == weapon_type::ROCKETLAUNCHER {
                    if env.has_collided(obj) || !env.line_clear(last, pos, obj) || env.rocket_hits_aircraft(pos) {
                        self.remove_with_explosion(i, env);
                    }
                } else if weapon == weapon_type::MOLOTOV {
                    // a molotov can't go off in the thrower's hand
                    let away = self.0[i].source.and_then(|s| env.entity_pos(s)).is_none_or(|sp| (last - sp).length_squared() >= 2.0);
                    if away && (env.has_collided(obj) || !env.line_clear(last, pos, obj)) {
                        self.remove_with_explosion(i, env);
                    }
                }
            } else {
                self.remove_with_explosion(i, env);
            }
            if let Some(o) = self.0[i].object {
                self.0[i].pos = env.pos(o);
            }
        }
    }

    /// `IsProjectileInRange`. Removing doesn't explode.
    pub fn in_range(&mut self, env: &mut dyn ProjectileEnv, min: Vec3, max: Vec3, remove: bool) -> bool {
        let mut result = false;
        for p in self.0.iter_mut().filter(|p| p.in_use) {
            let Some(o) = p.object else { continue };
            let pos = env.pos(o);
            if pos.cmpge(min).all() && pos.cmple(max).all() {
                result = true;
                if remove {
                    p.in_use = false;
                    p.object = None;
                    env.remove(o);
                }
            }
        }
        result
    }

    pub fn remove_all(&mut self, env: &mut dyn ProjectileEnv) {
        for p in self.0.iter_mut().filter(|p| p.in_use) {
            p.in_use = false;
            if let Some(o) = p.object.take() {
                env.remove(o);
            }
        }
    }

    /// `RemoveIfThisIsAProjectile` (the original frees the wrong slot after its increment; this frees the right one).
    pub fn remove_if_projectile(&mut self, env: &mut dyn ProjectileEnv, object: EntityRef) -> bool {
        let Some(p) = self.0.iter_mut().find(|p| p.object == Some(object)) else { return false };
        p.in_use = false;
        p.object = None;
        env.remove(object);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::PoolKind;

    #[derive(Default)]
    struct Env {
        pos: Vec3,
        collided: bool,
        explosions: Vec<ExplosionType>,
        removed: u32,
    }
    impl ProjectileEnv for Env {
        fn spawn(&mut self, s: Spawn) -> Option<EntityRef> {
            self.pos = s.matrix.pos;
            Some(EntityRef { pool: PoolKind::Object, handle: 3 })
        }
        fn remove(&mut self, _: EntityRef) {
            self.removed += 1;
        }
        fn pos(&self, _: EntityRef) -> Vec3 {
            self.pos
        }
        fn has_collided(&self, _: EntityRef) -> bool {
            self.collided
        }
        fn line_clear(&mut self, _: Vec3, _: Vec3, _: EntityRef) -> bool {
            true
        }
        fn rocket_hits_aircraft(&mut self, _: Vec3) -> bool {
            false
        }
        fn add_explosion(&mut self, _: Option<EntityRef>, ty: ExplosionType, _: Vec3) {
            self.explosions.push(ty);
        }
        fn entity_pos(&self, _: EntityRef) -> Option<Vec3> {
            None
        }
        fn smoke(&mut self, _: Vec3) {}
        fn source_valid(&self, _: EntityRef) -> bool {
            true
        }
    }

    fn shooter() -> Shooter {
        Shooter { is_player: false, camera_front_up: (Vec3::Y, Vec3::Z), seek_target_z: None, forward: Vec3::Y, matrix: Matrix::IDENTITY }
    }

    #[test]
    fn grenade_explodes_on_timeout_and_rocket_on_hit() {
        let (mut ps, mut env, mut timer) = (Projectiles::default(), Env::default(), Timer::default());
        assert!(ps.add(&mut env, &timer, None, &shooter(), weapon_type::GRENADE, Vec3::ZERO, 1.0));
        assert!(ps.add(&mut env, &timer, None, &shooter(), weapon_type::ROCKETLAUNCHER, Vec3::ZERO, 1.0));
        ps.update(&mut env, &timer);
        assert!(env.explosions.is_empty());
        env.collided = true;
        ps.update(&mut env, &timer);
        assert_eq!(env.explosions, vec![ExplosionType::Rocket]);
        timer.time_in_ms = 2001;
        ps.update(&mut env, &timer);
        assert_eq!(env.explosions, vec![ExplosionType::Rocket, ExplosionType::Grenade]);
        assert!(ps.0.iter().all(|p| !p.in_use));
        assert!(!ps.add(&mut env, &timer, None, &shooter(), weapon_type::FLAMETHROWER, Vec3::ZERO, 1.0));
    }
}
