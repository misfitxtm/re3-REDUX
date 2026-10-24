//! `CWeapon`: ammo, clip and firing state. The shots themselves (instant hit, shotgun, sniper,
//! driveby aiming, melee) read peds, the camera and the pad, so the `FireEnv` the ped code
//! provides carries them out.
use super::info::{weapon_type as wt, WeaponFire, WeaponInfos};
use crate::{math::Matrix, timer::Timer};
use glam::Vec3;
use std::f32::consts::PI;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WeaponState {
    Ready,
    Firing,
    Reloading,
    OutOfAmmo,
    MeleeMadeContact,
}

const RELOAD_SAMPLE_TIME: [u32; wt::LAST] = [0, 0, 250, 400, 650, 300, 300, 423, 400, 0, 0, 0, 0, 0];

#[derive(Clone, Debug)]
pub struct Weapon {
    pub ty: usize,
    pub state: WeaponState,
    pub ammo_in_clip: i32,
    pub ammo_total: i32,
    pub timer: u32,
    pub add_rot_offset: bool,
}

impl Default for Weapon {
    fn default() -> Self {
        Self { ty: wt::UNARMED, state: WeaponState::Ready, ammo_in_clip: 0, ammo_total: 0, timer: 0, add_rot_offset: false }
    }
}

/// What `Fire` needs to know about whoever holds the weapon.
#[derive(Clone, Copy, Debug)]
pub struct Shooter {
    pub is_ped: bool,
    pub is_player: bool,
    /// Distance to the ped's seek target, if it has one.
    pub seek_target_dist: Option<f32>,
    /// `m_fAttackButtonCounter` of the player ped.
    pub attack_button_counter: f32,
    /// Camera is in the first-person M16 mode.
    pub cam_m16_first_person: bool,
    pub cam_heli_cannon_first_person: bool,
    /// The player has the fast-reload cheat/perk.
    pub fast_reload: bool,
    pub matrix: Matrix,
    pub forward: Vec3,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FireAction {
    Shotgun,
    InstantHit,
    Sniper,
    M16FirstPerson,
    /// Throw or launch with this power.
    Projectile(f32),
    AreaEffect,
    Detonator,
}

pub trait FireEnv {
    /// Carries out the shot. Returns whether it fired.
    fn fire(&mut self, action: FireAction, source: Vec3) -> bool;
    /// `FireMelee`.
    fn fire_melee(&mut self, source: Vec3);
    /// The ped is marked as shooting and the shot sound plays.
    fn shot_fired(&mut self, flamethrower_start: bool);
    /// Grenades thrown by the player.
    fn grenade_thrown(&mut self);
    fn round_fired_by_player(&mut self);
}

impl Weapon {
    pub fn new(infos: &WeaponInfos, ty: usize, ammo: i32) -> Self {
        let mut w = Self::default();
        w.initialise(infos, ty, ammo);
        w
    }

    /// `CWeapon::Initialise`.
    pub fn initialise(&mut self, infos: &WeaponInfos, ty: usize, ammo: i32) {
        self.ty = ty;
        self.state = WeaponState::Ready;
        self.ammo_total = ammo.min(99999);
        self.ammo_in_clip = 0;
        self.reload(infos);
        self.timer = 0;
    }

    /// `CWeapon::Reload`.
    pub fn reload(&mut self, infos: &WeaponInfos) {
        if self.ammo_total == 0 {
            return;
        }
        let clip = infos.get(self.ty).ammo;
        self.ammo_in_clip = if self.ammo_total >= clip { clip } else { self.ammo_total };
    }

    pub fn is_type_melee(&self) -> bool {
        self.ty == wt::UNARMED || self.ty == wt::BASEBALLBAT
    }

    pub fn is_type_2_handed(&self) -> bool {
        (wt::SHOTGUN..=wt::FLAMETHROWER).contains(&self.ty) && self.ty != wt::ROCKETLAUNCHER
    }

    pub fn has_ammo_to_be_used(&self) -> bool {
        self.is_type_melee() || self.ammo_total != 0
    }

    /// `CWeapon::Fire`. `fire_source` is the muzzle if the caller knows it.
    pub fn fire(&mut self, infos: &WeaponInfos, timer: &Timer, shooter: &Shooter, fire_source: Option<Vec3>, env: &mut dyn FireEnv) -> bool {
        let mut source = fire_source.unwrap_or_else(|| shooter.matrix.transform_point(Vec3::new(0.0, 0.0, 0.6)));
        if self.add_rot_offset {
            let heading = (-shooter.forward.x).atan2(shooter.forward.y) * (180.0 / PI);
            let angle = heading * (PI / 180.0);
            source.x += -angle.sin() * 0.15;
            source.y += angle.cos() * 0.15;
        }
        if self.state != WeaponState::Ready && self.state != WeaponState::Firing {
            return false;
        }
        let info = infos.get(self.ty);
        let mut fired = false;
        if info.fire != WeaponFire::Melee {
            if self.ammo_in_clip <= 0 {
                return false;
            }
            let action = match self.ty {
                wt::SHOTGUN => Some(FireAction::Shotgun),
                wt::COLT45 | wt::UZI | wt::AK47 => Some(FireAction::InstantHit),
                wt::SNIPERRIFLE => Some(FireAction::Sniper),
                wt::M16 => Some(if shooter.cam_m16_first_person && shooter.is_player { FireAction::M16FirstPerson } else { FireAction::InstantHit }),
                wt::HELICANNON => {
                    Some(if (shooter.cam_heli_cannon_first_person || shooter.cam_m16_first_person) && shooter.is_player { FireAction::M16FirstPerson } else { FireAction::InstantHit })
                }
                wt::FLAMETHROWER => Some(FireAction::AreaEffect),
                wt::DETONATOR => Some(FireAction::Detonator),
                wt::ROCKETLAUNCHER | wt::MOLOTOV | wt::GRENADE => None,
                _ => {
                    fired = false;
                    None
                }
            };
            match (self.ty, action) {
                (wt::ROCKETLAUNCHER, _) => {
                    fired = match shooter.seek_target_dist {
                        Some(d) if shooter.is_ped => {
                            (d > 8.0 || shooter.is_player) && env.fire(FireAction::Projectile(0.0), source)
                        }
                        _ => env.fire(FireAction::Projectile(0.0), source),
                    };
                }
                (wt::MOLOTOV | wt::GRENADE, _) => {
                    if shooter.is_player {
                        fired = env.fire(FireAction::Projectile(shooter.attack_button_counter * 0.0375), source);
                        if self.ty == wt::GRENADE {
                            env.grenade_thrown();
                        }
                    } else if let Some(d) = shooter.seek_target_dist.filter(|_| shooter.is_ped) {
                        let power = ((d - 10.0) * 0.02).clamp(0.2, 1.0);
                        fired = env.fire(FireAction::Projectile(power), source);
                    } else {
                        fired = env.fire(FireAction::Projectile(0.3), source);
                    }
                }
                (wt::DETONATOR, Some(a)) => {
                    env.fire(a, source);
                    self.ammo_total = 1;
                    self.ammo_in_clip = 1;
                    fired = true;
                }
                (_, Some(a)) => fired = env.fire(a, source),
                _ => {}
            }
            if fired {
                let is_player = shooter.is_ped && shooter.is_player;
                if shooter.is_ped {
                    env.shot_fired(false);
                }
                if self.ammo_in_clip > 0 {
                    self.ammo_in_clip -= 1;
                }
                if self.ammo_total > 0 && (self.ammo_total < 25000 || is_player) {
                    self.ammo_total -= 1;
                }
                if self.state == WeaponState::Ready && self.ty == wt::FLAMETHROWER {
                    env.shot_fired(true);
                }
                self.state = WeaponState::Firing;
                if self.ammo_in_clip == 0 {
                    if self.ammo_total == 0 {
                        return true;
                    }
                    self.state = WeaponState::Reloading;
                    self.timer = timer.time_in_ms + info.reload;
                    if shooter.is_player && shooter.fast_reload {
                        self.timer = timer.time_in_ms + info.reload / 4;
                    }
                    return true;
                }
                self.timer = timer.time_in_ms + 1000;
                if shooter.is_player {
                    env.round_fired_by_player();
                }
            }
        } else {
            if self.state != WeaponState::Firing {
                self.timer = timer.time_in_ms + info.reload;
                self.state = WeaponState::Firing;
            }
            env.fire_melee(source);
        }
        self.is_type_melee() || fired
    }

    /// `CWeapon::FireFromCar`, with `fired` the result of the instant-hit shot from the car window.
    pub fn fire_from_car(&mut self, infos: &WeaponInfos, timer: &Timer, env: &mut dyn FnMut() -> bool) -> bool {
        if self.state != WeaponState::Ready && self.state != WeaponState::Firing {
            return false;
        }
        if self.ammo_in_clip <= 0 {
            return false;
        }
        if env() {
            if self.ammo_in_clip > 0 {
                self.ammo_in_clip -= 1;
            }
            if self.ammo_total < 25000 && self.ammo_total > 0 {
                self.ammo_total -= 1;
            }
            self.state = WeaponState::Firing;
            if self.ammo_in_clip == 0 {
                if self.ammo_total == 0 {
                    return true;
                }
                self.state = WeaponState::Reloading;
                self.timer = timer.time_in_ms + infos.get(self.ty).reload;
                return true;
            }
            self.timer = timer.time_in_ms + 1000;
        }
        true
    }

    /// `CWeapon::Update`. Returns true when the reload sound should play.
    pub fn update(&mut self, infos: &WeaponInfos, timer: &Timer, has_audio: bool) -> bool {
        let mut reload_sound = false;
        let crossed = |at: u32| timer.previous_time_in_ms < at && timer.time_in_ms >= at;
        match self.state {
            WeaponState::MeleeMadeContact => self.state = WeaponState::Ready,
            WeaponState::Firing => {
                if self.ty == wt::SHOTGUN && has_audio {
                    reload_sound = crossed(self.timer.wrapping_sub(RELOAD_SAMPLE_TIME[wt::SHOTGUN]));
                }
                if timer.time_in_ms > self.timer {
                    self.state = if infos.get(self.ty).fire != WeaponFire::Melee && self.ammo_total == 0 { WeaponState::OutOfAmmo } else { WeaponState::Ready };
                }
            }
            WeaponState::Reloading => {
                if has_audio && self.ty < wt::LAST {
                    reload_sound = crossed(self.timer.wrapping_sub(RELOAD_SAMPLE_TIME[self.ty]));
                }
                if timer.time_in_ms > self.timer {
                    self.reload(infos);
                    self.state = WeaponState::Ready;
                }
            }
            _ => {}
        }
        reload_sound
    }

    /// `CWeapon::HitsGround`: a building lies just below the line to the target.
    /// `building_hit_z` is the z of the building the line from `source` to the target hits.
    pub fn hits_ground(source_z: f32, building_hit_z: Option<f32>) -> bool {
        building_hit_z.is_some_and(|z| {
            let diff = z - source_z;
            diff < 0.0 && diff > -3.0
        })
    }

    /// Box a shot passes through, grown by 2 (`MakePedsJumpAtShot`).
    pub fn shot_bounds(source: Vec3, target: Vec3) -> (Vec3, Vec3) {
        (source.min(target) - Vec3::splat(2.0), source.max(target) + Vec3::splat(2.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Env {
        actions: Vec<FireAction>,
        fires: bool,
        sounds: u32,
        melee: u32,
    }
    impl FireEnv for Env {
        fn fire(&mut self, a: FireAction, _: Vec3) -> bool {
            self.actions.push(a);
            self.fires
        }
        fn fire_melee(&mut self, _: Vec3) {
            self.melee += 1;
        }
        fn shot_fired(&mut self, _: bool) {
            self.sounds += 1;
        }
        fn grenade_thrown(&mut self) {}
        fn round_fired_by_player(&mut self) {}
    }

    fn shooter(player: bool) -> Shooter {
        Shooter {
            is_ped: true,
            is_player: player,
            seek_target_dist: None,
            attack_button_counter: 10.0,
            cam_m16_first_person: false,
            cam_heli_cannon_first_person: false,
            fast_reload: false,
            matrix: Matrix::IDENTITY,
            forward: Vec3::Y,
        }
    }

    fn infos() -> WeaponInfos {
        let mut i = WeaponInfos::default();
        i.0[wt::COLT45].fire = WeaponFire::InstantHit;
        i.0[wt::COLT45].ammo = 2;
        i.0[wt::COLT45].reload = 400;
        i.0[wt::UNARMED].fire = WeaponFire::Melee;
        i
    }

    #[test]
    fn clip_empties_then_reloads() {
        let (infos, mut timer) = (infos(), Timer::default());
        let (mut w, mut env) = (Weapon::new(&infos, wt::COLT45, 10), Env { fires: true, ..Env::default() });
        assert_eq!(w.ammo_in_clip, 2);
        assert!(w.fire(&infos, &timer, &shooter(true), None, &mut env));
        assert_eq!((w.state, w.ammo_in_clip, w.ammo_total, w.timer), (WeaponState::Firing, 1, 9, 1000));
        timer.time_in_ms = 1001;
        w.update(&infos, &timer, false);
        assert_eq!(w.state, WeaponState::Ready);
        assert!(w.fire(&infos, &timer, &shooter(true), None, &mut env));
        assert_eq!((w.state, w.timer), (WeaponState::Reloading, 1401));
        timer.time_in_ms = 1402;
        w.update(&infos, &timer, false);
        assert_eq!((w.state, w.ammo_in_clip), (WeaponState::Ready, 2));
        assert_eq!(env.actions, vec![FireAction::InstantHit; 2]);
    }

    #[test]
    fn empty_and_melee() {
        let (infos, timer) = (infos(), Timer::default());
        let (mut w, mut env) = (Weapon::new(&infos, wt::COLT45, 0), Env::default());
        assert!(!w.fire(&infos, &timer, &shooter(false), None, &mut env));
        let mut fist = Weapon::new(&infos, wt::UNARMED, 0);
        assert!(fist.fire(&infos, &timer, &shooter(true), None, &mut env));
        assert_eq!(env.melee, 1);
        assert!(fist.has_ammo_to_be_used() && !w.has_ammo_to_be_used());
        assert!(Weapon::hits_ground(1.0, Some(-1.0)) && !Weapon::hits_ground(1.0, Some(-3.5)));
    }
}
