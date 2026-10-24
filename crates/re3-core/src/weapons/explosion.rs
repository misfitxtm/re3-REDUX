//! `CExplosion`. Particles, lights and sounds come out as [`ExplosionFx`]; world effects go through [`ExplosionEnv`].
use crate::{entity::EntityRef, general::Rng, timer::Timer};
use glam::Vec3;

pub const NUM_EXPLOSIONS: usize = 48;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExplosionType {
    Grenade,
    Molotov,
    Rocket,
    Car,
    CarQuick,
    Heli,
    Mine,
    Barrel,
    TankGrenade,
    HeliBomb,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Particle {
    ExplosionLFast,
    ExplosionMFast,
    ExplosionMedium,
    ExplosionLarge,
    EngineSmoke2,
}

/// What the renderer, audio and camera should do.
#[derive(Clone, Debug)]
pub enum ExplosionFx {
    Particle { kind: Particle, pos: Vec3, vel: Vec3, size: f32, rotation_speed: f32, rotation: f32 },
    JetExplosion { pos: Vec3, size: f32, z_speed: f32 },
    Light { pos: Vec3, radius: f32, rgb: [f32; 3] },
    /// `id` stands in for the explosion address plus 0 or 1.
    Corona { id: usize, rgb: [u8; 3], pos: Vec3, size: f32, range: f32, star: bool },
    /// `CEventList::RegisterEvent(EVENT_EXPLOSION, ...)`.
    Event { pos: Vec3, victim: Option<EntityRef>, duration: u32 },
    CamShake { pos: Vec3 },
}

/// What the explosion needs to know about its exploding vehicle.
#[derive(Clone, Copy, Debug)]
pub struct Victim {
    pub pos: Vec3,
    pub is_boat: bool,
    /// World position of the rear wing (bike: rear forks), if the vehicle has it.
    pub wing_pos: Option<Vec3>,
}

pub trait ExplosionEnv {
    /// `FindGroundZFor3DCoord`.
    fn ground_z(&mut self, p: Vec3) -> (f32, bool);
    /// `CWaterLevel::GetWaterLevelNoWaves`.
    fn water_level_no_waves(&mut self, p: Vec3) -> Option<f32>;
    fn start_fire(&mut self, pos: Vec3, size: f32);
    fn trigger_explosion(&mut self, rng: &mut Rng, pos: Vec3, radius: f32, power: f32, culprit: Option<EntityRef>, fiery: bool);
    fn set_peds_on_fire(&mut self, pos: Vec3, radius: f32, culprit: Option<EntityRef>);
    fn set_cars_on_fire(&mut self, pos: Vec3, radius: f32, culprit: Option<EntityRef>);
    /// Z of the first building hit by a line down from `from`.
    fn vertical_line_z(&mut self, from: Vec3) -> f32;
    fn victim(&self, e: EntityRef) -> Option<Victim>;
    fn camera_pos(&self) -> Vec3;
}

#[derive(Clone, Debug)]
pub struct Explosion {
    pub ty: ExplosionType,
    pub pos: Vec3,
    pub radius: f32,
    pub propagation_rate: f32,
    pub creator: Option<EntityRef>,
    pub victim: Option<EntityRef>,
    pub stop_time: f32,
    pub iteration: u8,
    pub active_counter: u8,
    pub start_time: f32,
    pub particles_expire_time: u32,
    pub power: f32,
    pub is_boat: bool,
    pub z_shift: f32,
}

impl Default for Explosion {
    fn default() -> Self {
        Self {
            ty: ExplosionType::Grenade,
            pos: Vec3::ZERO,
            radius: 1.0,
            propagation_rate: 0.0,
            creator: None,
            victim: None,
            stop_time: 0.0,
            iteration: 0,
            active_counter: 0,
            start_time: 0.0,
            particles_expire_time: 0,
            power: 0.0,
            is_boat: false,
            z_shift: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Explosions {
    pub list: Vec<Explosion>,
    pub fx: Vec<ExplosionFx>,
}

impl Default for Explosions {
    fn default() -> Self {
        Self { list: vec![Explosion::default(); NUM_EXPLOSIONS], fx: vec![] }
    }
}

fn is_fiery(t: ExplosionType) -> bool {
    use ExplosionType::*;
    matches!(t, Rocket | CarQuick | Mine | Barrel | TankGrenade | HeliBomb)
}

fn puff(kind: Particle, pos: Vec3, size: f32) -> ExplosionFx {
    ExplosionFx::Particle { kind, pos, vel: Vec3::ZERO, size, rotation_speed: 0.0, rotation: 0.0 }
}

/// `(GetRandomNumber() as uint8) - 128` per axis.
fn rand_offset(rng: &mut Rng) -> Vec3 {
    let x = rng.number() as u8;
    let y = rng.number() as u8;
    let z = rng.number() as u8;
    Vec3::new(x as f32 - 128.0, y as f32 - 128.0, z as f32 - 128.0)
}

impl Explosions {
    pub fn active_counter(&self, id: usize) -> u8 {
        self.list[id].active_counter
    }

    pub fn reset_active_counter(&mut self, id: usize) {
        self.list[id].active_counter = 0;
    }

    /// `CExplosion::AddExplosion`.
    #[allow(clippy::too_many_arguments)] // mirrors the original's signature
    pub fn add(
        &mut self,
        env: &mut dyn ExplosionEnv,
        rng: &mut Rng,
        timer: &Timer,
        exploding: Option<EntityRef>,
        culprit: Option<EntityRef>,
        ty: ExplosionType,
        pos: Vec3,
        lifetime: u32,
    ) -> bool {
        use ExplosionType::*;
        let Some(n) = self.list.iter().position(|e| e.iteration == 0) else { return false };
        let now = timer.time_in_ms;
        let cam = env.camera_pos();
        let mut dont_explode = false;
        let mut fx = vec![];

        let e = &mut self.list[n];
        *e = Explosion { ty, pos, creator: culprit, victim: exploding, iteration: 1, active_counter: 1, ..Explosion::default() };
        e.particles_expire_time = if lifetime != 0 { now + lifetime } else { 0 };
        let stop = |extra: u32| (lifetime + now + extra) as f32;
        let victim = exploding.and_then(|v| env.victim(v));
        match ty {
            Grenade => {
                (e.radius, e.power, e.stop_time, e.propagation_rate) = (9.0, 300.0, stop(750), 0.5);
                let mut ground = pos;
                ground.z = env.ground_z(pos + Vec3::Z * 3.0).0;
                fx.push(ExplosionFx::Event { pos: ground, victim: None, duration: 250 });
                if pos.distance(cam) < 40.0 {
                    fx.push(puff(Particle::ExplosionLFast, pos, 5.5));
                }
            }
            Molotov => {
                (e.radius, e.power, e.stop_time, e.propagation_rate) = (6.0, 0.0, stop(3000), 0.5);
                let (z, found) = env.ground_z(pos + Vec3::Z * 3.0);
                if found {
                    let ground = Vec3::new(pos.x, pos.y, z);
                    match env.water_level_no_waves(ground) {
                        Some(w) if z < w && w - 6.0 < z => dont_explode = true,
                        _ => env.start_fire(ground, 1.8),
                    }
                } else {
                    dont_explode = true;
                }
            }
            Rocket => {
                (e.radius, e.power, e.stop_time, e.propagation_rate) = (10.0, 300.0, stop(750), 0.5);
                fx.push(ExplosionFx::Event { pos, victim: None, duration: 250 });
                if pos.distance(cam) < 40.0 {
                    fx.push(puff(Particle::ExplosionLFast, pos, 5.5));
                }
            }
            Car | CarQuick => {
                (e.radius, e.power, e.stop_time, e.propagation_rate) = (9.0, 300.0, stop(4250), 0.5);
                e.start_time = now as f32;
                match victim {
                    Some(v) => {
                        e.is_boat = v.is_boat;
                        fx.push(ExplosionFx::Event { pos, victim: exploding, duration: 1000 });
                    }
                    None => fx.push(ExplosionFx::Event { pos, victim: None, duration: 1000 }),
                }
                if let Some(v) = victim.filter(|v| !v.is_boat) {
                    let rn = (rng.number() & 1) + 2;
                    for _ in 0..rn {
                        fx.push(puff(Particle::ExplosionMedium, v.pos, 3.5));
                        fx.push(puff(Particle::ExplosionLFast, v.pos, 5.5));
                    }
                    if let Some(w) = v.wing_pos {
                        let rn = (rng.number() & 1) + 1;
                        for _ in 0..rn {
                            fx.push(ExplosionFx::JetExplosion { pos: w, size: 1.4, z_speed: 0.0 });
                        }
                    }
                }
            }
            Heli => {
                (e.radius, e.power, e.stop_time, e.propagation_rate) = (6.0, 300.0, stop(750), 0.5);
                e.start_time = now as f32;
                for _ in 0..10 {
                    fx.push(puff(Particle::ExplosionMFast, pos + rand_offset(rng) / 20.0, 2.5));
                    fx.push(puff(Particle::ExplosionLFast, pos + rand_offset(rng) / 20.0, 5.0));
                    fx.push(ExplosionFx::JetExplosion { pos: pos + rand_offset(rng) / 20.0, size: 1.4, z_speed: 3.0 });
                }
                fx.push(ExplosionFx::Event { pos, victim: None, duration: 1000 });
            }
            Mine => {
                (e.radius, e.power, e.stop_time, e.propagation_rate) = (10.0, 150.0, stop(750), 0.5);
                let mut ground = pos;
                // the original discards this result and keeps the explosion's own z
                env.ground_z(pos + Vec3::Z * 4.0);
                ground.z = pos.z;
                fx.push(ExplosionFx::Event { pos: ground, victim: None, duration: 250 });
            }
            Barrel => {
                (e.radius, e.power, e.stop_time, e.propagation_rate) = (7.0, 150.0, stop(750), 0.5);
                for _ in 0..6 {
                    let o = rand_offset(rng);
                    let p = Vec3::new(o.x / 50.0, o.y / 50.0, o.z / 25.0) + pos;
                    fx.push(puff(Particle::ExplosionMedium, p, 0.0));
                }
                env.ground_z(pos + Vec3::Z * 4.0);
                fx.push(ExplosionFx::Event { pos, victim: None, duration: 250 });
            }
            TankGrenade => {
                (e.radius, e.power, e.stop_time, e.propagation_rate) = (10.0, 150.0, stop(750), 0.5);
                env.ground_z(pos + Vec3::Z * 4.0);
                fx.push(ExplosionFx::Event { pos, victim: None, duration: 250 });
            }
            HeliBomb => {
                (e.radius, e.power, e.stop_time, e.propagation_rate) = (8.0, 50.0, stop(750), 0.5);
                env.ground_z(pos + Vec3::Z * 4.0);
                fx.push(ExplosionFx::Event { pos, victim: None, duration: 250 });
            }
        }
        if dont_explode {
            e.iteration = 0;
            self.fx.extend(fx);
            return false;
        }
        let (radius, power, expire) = (e.radius, e.power, e.particles_expire_time);
        if power != 0.0 && expire == 0 {
            env.trigger_explosion(rng, pos, radius, power, culprit, is_fiery(ty));
        }
        fx.push(ExplosionFx::CamShake { pos });
        self.fx.extend(fx);
        true
    }

    /// `CExplosion::Update`.
    pub fn update(&mut self, env: &mut dyn ExplosionEnv, rng: &mut Rng, timer: &Timer) {
        use ExplosionType::*;
        let now = timer.time_in_ms;
        for i in 0..self.list.len() {
            let e = &mut self.list[i];
            if e.iteration == 0 {
                continue;
            }
            if e.particles_expire_time != 0 {
                if now > e.particles_expire_time {
                    e.particles_expire_time = 0;
                    if e.power != 0.0 {
                        env.trigger_explosion(rng, e.pos, e.radius, e.power, e.creator, is_fiery(e.ty));
                    }
                }
                continue;
            }
            e.radius += e.propagation_rate * timer.time_step;
            let some_time = (e.stop_time - now as f32) as i32;
            let odd_frame = timer.frame_counter & 1 != 0;
            let mut fx = vec![];
            match e.ty {
                Grenade | Rocket | Heli | Mine | Barrel => {
                    if odd_frame {
                        fx.push(ExplosionFx::Light { pos: e.pos, radius: 20.0, rgb: [1.0, 1.0, 0.5] });
                        fx.push(ExplosionFx::Corona { id: i * 2, rgb: [255, 255, 200], pos: e.pos, size: 8.0, range: 120.0, star: false });
                    } else {
                        fx.push(ExplosionFx::Corona { id: i * 2, rgb: [128, 128, 100], pos: e.pos, size: 8.0, range: 120.0, star: false });
                    }
                    fx.push(ExplosionFx::Corona { id: i * 2 + 1, rgb: [30, 30, 25], pos: e.pos, size: e.radius, range: 120.0, star: true });
                }
                Molotov => {
                    env.set_peds_on_fire(e.pos, 6.0, e.creator);
                    env.set_cars_on_fire(e.pos, 6.0, e.creator);
                    if e.iteration < 10 {
                        if e.iteration == 1 {
                            e.z_shift = env.vertical_line_z(e.pos + Vec3::Z * 5.0);
                        }
                        let ff = e.iteration as f32 * 0.55;
                        let mut k = 0;
                        while (k as f32) < 5.0 * ff {
                            #[allow(clippy::approx_constant)] // the original's literal
                            let angle = rng.number() as f32 / 256.0 * 6.28;
                            let p = Vec3::new(e.pos.x + ff * angle.sin(), e.pos.y + ff * angle.cos(), e.z_shift + 0.5);
                            let rotation_speed = rng.range_f32(-3.0, 3.0);
                            let rotation = rng.range_f32(-180.0, 180.0);
                            fx.push(ExplosionFx::Particle { kind: Particle::ExplosionMedium, pos: p, vel: Vec3::ZERO, size: 0.0, rotation_speed, rotation });
                            k += 1;
                        }
                    }
                }
                Car | CarQuick => {
                    if some_time >= 3500 {
                        let victim = e.victim.and_then(|v| env.victim(v));
                        if let Some(v) = victim.filter(|_| !e.is_boat) {
                            if rng.number() & 0xF == 0
                                && let Some(w) = v.wing_pos {
                                    fx.push(ExplosionFx::JetExplosion { pos: w, size: 1.5, z_speed: 0.0 });
                                }
                            if now as f32 > e.start_time {
                                e.start_time = (now + 125 + (rng.number() & 0x7F) as u32) as f32;
                                let mut k = 0;
                                while k < (rng.number() & 1) + 1 {
                                    fx.push(puff(Particle::ExplosionMedium, v.pos, 3.5));
                                    fx.push(puff(Particle::ExplosionLarge, v.pos, 5.5));
                                    k += 1;
                                }
                            }
                        }
                        if odd_frame {
                            fx.push(ExplosionFx::Light { pos: e.pos, radius: 15.0, rgb: [1.0, 0.0, 0.0] });
                            fx.push(ExplosionFx::Corona { id: i * 2, rgb: [200, 100, 0], pos: e.pos, size: 6.0, range: 80.0, star: false });
                        } else {
                            fx.push(ExplosionFx::Corona { id: i * 2, rgb: [128, 0, 0], pos: e.pos, size: 8.0, range: 80.0, star: false });
                        }
                        fx.push(ExplosionFx::Corona { id: i * 2 + 1, rgb: [30, 15, 0], pos: e.pos, size: e.radius, range: 80.0, star: true });
                    } else if e.iteration & 1 != 0 {
                        if let Some(v) = e.victim.and_then(|v| env.victim(v)) {
                            fx.push(puff(Particle::EngineSmoke2, v.pos, rng.range_f32(0.5, 0.8)));
                        }
                        let p = e.pos + Vec3::Z;
                        let size = rng.range_f32(0.5, 2.0);
                        fx.push(ExplosionFx::Particle { kind: Particle::EngineSmoke2, pos: p, vel: Vec3::new(0.0, 0.0, 0.11), size, rotation_speed: 0.0, rotation: 0.0 });
                    }
                }
                TankGrenade | HeliBomb => {
                    if e.iteration < 5 {
                        let ff = e.iteration as f32 * 0.65;
                        let mut k = 0;
                        while (k as f32) < 10.0 * ff {
                            let x = rng.number() as u8;
                            let y = rng.number() as u8;
                            let z = rng.number() as u8;
                            let d = Vec3::new(x as f32 - 128.0, y as f32 - 128.0, (z % 128) as f32 + 1.0);
                            let mut p = d.normalize() * (ff / 5.0) + e.pos;
                            p.z += 0.5;
                            let rotation_speed = rng.range_f32(-3.0, 3.0);
                            let rotation = rng.range_f32(-180.0, 180.0);
                            fx.push(ExplosionFx::Particle { kind: Particle::ExplosionLarge, pos: p, vel: Vec3::ZERO, size: 0.0, rotation_speed, rotation });
                            k += 1;
                        }
                    }
                }
            }
            if some_time > 0 {
                e.iteration = e.iteration.wrapping_add(1);
            } else {
                e.iteration = 0;
            }
            self.fx.extend(fx);
        }
    }

    /// `CExplosion::TestForExplosionInArea`.
    pub fn test_in_area(&self, ty: ExplosionType, min: Vec3, max: Vec3) -> bool {
        self.list.iter().any(|e| e.iteration != 0 && e.ty == ty && e.pos.cmpge(min).all() && e.pos.cmple(max).all())
    }

    /// `CExplosion::RemoveAllExplosionsInArea`.
    pub fn remove_in_area(&mut self, pos: Vec3, radius: f32) {
        for e in self.list.iter_mut().filter(|e| e.iteration != 0) {
            if (pos - e.pos).length_squared() < radius * radius {
                e.iteration = 0;
            }
        }
    }

    /// Forgets a deleted entity (the original's registered references).
    pub fn remove_references(&mut self, r: EntityRef) {
        for e in &mut self.list {
            if e.creator == Some(r) {
                e.creator = None;
            }
            if e.victim == Some(r) {
                e.victim = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Env {
        triggered: Vec<(f32, f32, bool)>,
        fires: u32,
    }
    impl ExplosionEnv for Env {
        fn ground_z(&mut self, _: Vec3) -> (f32, bool) {
            (0.0, true)
        }
        fn water_level_no_waves(&mut self, _: Vec3) -> Option<f32> {
            None
        }
        fn start_fire(&mut self, _: Vec3, _: f32) {
            self.fires += 1;
        }
        fn trigger_explosion(&mut self, _: &mut Rng, _: Vec3, radius: f32, power: f32, _: Option<EntityRef>, fiery: bool) {
            self.triggered.push((radius, power, fiery));
        }
        fn set_peds_on_fire(&mut self, _: Vec3, _: f32, _: Option<EntityRef>) {}
        fn set_cars_on_fire(&mut self, _: Vec3, _: f32, _: Option<EntityRef>) {}
        fn vertical_line_z(&mut self, _: Vec3) -> f32 {
            0.0
        }
        fn victim(&self, _: EntityRef) -> Option<Victim> {
            None
        }
        fn camera_pos(&self) -> Vec3 {
            Vec3::ZERO
        }
    }

    #[test]
    fn rocket_triggers_then_expires() {
        let (mut ex, mut env, mut rng, mut timer) = (Explosions::default(), Env::default(), Rng::default(), Timer::default());
        assert!(ex.add(&mut env, &mut rng, &timer, None, None, ExplosionType::Rocket, Vec3::new(1.0, 2.0, 3.0), 0));
        assert_eq!(env.triggered, vec![(10.0, 300.0, true)]);
        assert!(ex.test_in_area(ExplosionType::Rocket, Vec3::ZERO, Vec3::splat(10.0)));
        timer.time_in_ms = 1000;
        ex.update(&mut env, &mut rng, &timer);
        assert_eq!(ex.list[0].iteration, 0);
        assert!(!ex.test_in_area(ExplosionType::Rocket, Vec3::ZERO, Vec3::splat(10.0)));
    }

    #[test]
    fn molotov_starts_fire_and_slots_run_out() {
        let (mut ex, mut env, mut rng, timer) = (Explosions::default(), Env::default(), Rng::default(), Timer::default());
        assert!(ex.add(&mut env, &mut rng, &timer, None, None, ExplosionType::Molotov, Vec3::ZERO, 0));
        assert_eq!(env.fires, 1);
        for _ in 1..NUM_EXPLOSIONS {
            assert!(ex.add(&mut env, &mut rng, &timer, None, None, ExplosionType::Mine, Vec3::ZERO, 0));
        }
        assert!(!ex.add(&mut env, &mut rng, &timer, None, None, ExplosionType::Mine, Vec3::ZERO, 0));
        ex.remove_in_area(Vec3::ZERO, 1.0);
        assert!(ex.add(&mut env, &mut rng, &timer, None, None, ExplosionType::Mine, Vec3::ZERO, 0));
    }
}
