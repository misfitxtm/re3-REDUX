//! `CWeapon::Fire` for a ped: the shotgun, projectiles, flames, sniper and first-person shots, and the glue that runs them.
use super::ctx::{cam_mode, Px, ShotEffect};
use super::data::*;
use super::fight::local_direction;
use super::pending::Hit;
use super::types::ped_type;
use crate::audio::sound;
use crate::collision::ColPoint;
use crate::entity::{EntityRef, EntityType, Kind};
use crate::general::{radian_angle_between_points, Rng};
use crate::math::{cross, heading, normalise};
use crate::weapons::explosion::ExplosionType;
use crate::weapons::info::weapon_type as wt;
use crate::weapons::projectile::{CollResponse, ProjectileEnv, ProjectileModel, Projectiles, Shooter as ProjectileShooter, Spawn};
use crate::weapons::weapon::{FireAction, FireEnv, Shooter, WeaponState};
use crate::world::{Kinds, LineOpts, World};
use glam::{Vec2, Vec3};

fn ahead_of(angle: f32) -> Vec2 {
    Vec2::new(-angle.sin(), angle.cos())
}

/// Projectiles are made, moved and blown up in the world.
pub struct WorldProjectiles<'a> {
    pub world: &'a mut World,
    pub rng: &'a mut Rng,
    pub camera: Vec3,
    /// Rocket smoke the renderer should show.
    pub smoke: Vec<Vec3>,
}

impl ProjectileEnv for WorldProjectiles<'_> {
    fn spawn(&mut self, s: Spawn) -> Option<EntityRef> {
        let indices = &self.world.ents.indices;
        let model = match s.model {
            ProjectileModel::Missile => indices.missile(),
            ProjectileModel::Molotov => indices.molotov(),
            ProjectileModel::Grenade => indices.grenade(),
        };
        let r = self.world.new_object(model, s.matrix, self.rng)?;
        let e = self.world.get_mut(r)?;
        let p = e.phys.as_mut()?;
        p.move_speed = s.velocity;
        p.flags.affected_by_gravity = s.gravity;
        p.elasticity = s.elasticity;
        // shortcut: the grenade's special collision response (`s.response`) is not modelled by the collision code yet
        let _: CollResponse = s.response;
        let _ = self.world.add(r);
        Some(r)
    }

    fn remove(&mut self, e: EntityRef) {
        self.world.remove(e);
        self.world.delete(e);
    }

    fn pos(&self, e: EntityRef) -> Vec3 {
        self.world.epos(e)
    }

    fn has_collided(&self, e: EntityRef) -> bool {
        self.world.get(e).is_some_and(|e| e.base.flags.has_collided)
    }

    fn line_clear(&mut self, from: Vec3, to: Vec3, ignore: EntityRef) -> bool {
        self.world.ents.ignore_entity = Some(ignore);
        let kinds = Kinds { buildings: true, vehicles: true, peds: true, objects: true, dummies: false };
        let clear = self.world.is_line_of_sight_clear(from, to, kinds, LineOpts::default());
        self.world.ents.ignore_entity = None;
        clear
    }

    fn rocket_hits_aircraft(&mut self, pos: Vec3) -> bool {
        self.world.test_heli_rocket_collision(pos, self.rng) || self.world.test_plane_rocket_collision(pos)
    }

    fn add_explosion(&mut self, creator: Option<EntityRef>, ty: ExplosionType, pos: Vec3) {
        self.world.add_explosion(self.rng, self.camera, None, creator, ty, pos, 0);
    }

    fn entity_pos(&self, e: EntityRef) -> Option<Vec3> {
        self.world.get(e).map(|e| e.base.position())
    }

    fn smoke(&mut self, pos: Vec3) {
        self.smoke.push(pos);
    }

    fn source_valid(&self, e: EntityRef) -> bool {
        if self.world.is_ped(e) { self.world.ped_pointer_valid(e) } else { self.world.get(e).is_some() }
    }
}

/// `CWeapon::Fire` runs against this: it carries out each kind of shot for the ped.
struct PedFire<'a, 'b> {
    world: &'a mut World,
    px: &'a mut Px<'b>,
    shooter: EntityRef,
    weapon: usize,
    melee_contact: bool,
}

impl FireEnv for PedFire<'_, '_> {
    fn fire(&mut self, action: FireAction, source: Vec3) -> bool {
        let (w, px, s, ty) = (&mut *self.world, &mut *self.px, self.shooter, self.weapon);
        match action {
            FireAction::Shotgun => w.fire_shotgun(px, s, ty, source),
            FireAction::InstantHit => w.fire_instant_hit(px, s, ty, source),
            FireAction::Sniper => w.fire_sniper(px, s, ty),
            FireAction::M16FirstPerson => w.fire_m16_first_person(px, s, ty),
            FireAction::Projectile(power) => w.fire_projectile(px, s, ty, source, power),
            FireAction::AreaEffect => w.fire_area_effect(px, s, ty, source),
            FireAction::Detonator => {
                w.use_detonator(s);
                true
            }
        }
    }

    fn fire_melee(&mut self, source: Vec3) {
        self.melee_contact |= self.world.fire_melee_shot(self.px, self.shooter, self.weapon, source);
    }

    fn shot_fired(&mut self, flamethrower_start: bool) {
        let s = self.shooter;
        if flamethrower_start {
            self.px.hooks.play_one_shot(s, sound::WEAPON_FLAMETHROWER_FIRE, 0.0);
        } else {
            self.world.ped_mut(s).flags.is_shooting = true;
            self.px.hooks.play_one_shot(s, sound::WEAPON_SHOT_FIRED, 0.0);
        }
    }

    fn grenade_thrown(&mut self) {
        self.px.hooks.shot_effect(ShotEffect::GrenadeThrown);
    }

    fn round_fired_by_player(&mut self) {
        self.px.hooks.shot_effect(ShotEffect::RoundFiredByPlayer);
    }
}

impl World {
    /// `CWeapon::Fire` for the ped's current weapon; true if it fired.
    pub fn fire_weapon(&mut self, px: &mut Px, r: EntityRef, fire_pos: Vec3) -> bool {
        let slot = self.ped(r).current_weapon as usize;
        let mut weapon = self.ped(r).weapons[slot].clone();
        let infos = self.ents.weapon_infos.clone();
        let timer = self.ents.timer.clone();
        let cam = px.hooks.camera();
        let is_player = self.ents.player_ped == Some(r);
        let pd = self.ped(r);
        let e = self.ent(r);
        let shooter = Shooter {
            is_ped: true,
            is_player,
            seek_target_dist: pd.seek_target.map(|t| (e.base.position() - self.epos(t)).length()),
            attack_button_counter: pd.player.as_ref().map_or(0.0, |p| p.attack_button_counter),
            cam_m16_first_person: cam.player_weapon_mode == cam_mode::M16_1STPERSON,
            cam_heli_cannon_first_person: cam.heli_cannon_first_person,
            fast_reload: self.ents.player_info.fast_reload,
            matrix: e.base.matrix,
            forward: e.base.matrix.forward,
        };
        let ty = weapon.ty;
        let mut env = PedFire { world: self, px, shooter: r, weapon: ty, melee_contact: false };
        let fired = weapon.fire(&infos, &timer, &shooter, Some(fire_pos), &mut env);
        if env.melee_contact {
            weapon.state = WeaponState::MeleeMadeContact;
        }
        if let Some(pd) = self.try_ped_mut_pub(r) {
            pd.weapons[slot] = weapon;
        }
        fired
    }

    pub(super) fn try_ped_mut_pub(&mut self, r: EntityRef) -> Option<&mut PedData> {
        match &mut self.get_mut(r)?.kind {
            Kind::Ped(p) => Some(p),
            _ => None,
        }
    }

    /// `CPed::IsPedDoingDriveByShooting`: the player with an Uzi, looking out of the side window.
    /// `looking_aside` is the camera's `LookingLeft || LookingRight`.
    pub fn is_ped_doing_drive_by_shooting(&self, r: EntityRef, looking_aside: bool) -> bool {
        self.ents.player_ped == Some(r) && self.ped(r).current_weapon as usize == wt::UZI && looking_aside
    }

    /// Makes a projectile for `source` (`CProjectileInfo::AddProjectile`).
    pub fn add_projectile(&mut self, px: &mut Px, source: EntityRef, weapon: usize, pos: Vec3, speed: f32) -> bool {
        let cam = px.hooks.camera();
        let e = self.ent(source);
        let shooter = ProjectileShooter {
            is_player: self.ents.player_ped == Some(source),
            camera_front_up: (cam.front, cam.up),
            seek_target_z: self.try_ped(source).and_then(|p| p.seek_target).map(|t| self.epos(t).z),
            forward: e.base.matrix.forward,
            matrix: e.base.matrix,
        };
        let camera = px.hooks.camera_position();
        let timer = self.ents.timer.clone();
        let mut projectiles = std::mem::take(&mut self.ents.projectiles);
        let ok = projectiles.add(&mut WorldProjectiles { world: self, rng: px.rng, camera, smoke: vec![] }, &timer, Some(source), &shooter, weapon, pos, speed);
        self.ents.projectiles = projectiles;
        ok
    }

    /// `CProjectileInfo::RemoveNotAdd`: the blast of a projectile that never got out.
    pub fn projectile_blows_in_place(&mut self, px: &mut Px, source: EntityRef, weapon: usize, pos: Vec3) {
        let camera = px.hooks.camera_position();
        Projectiles::explode(&mut WorldProjectiles { world: self, rng: px.rng, camera, smoke: vec![] }, Some(source), weapon, pos);
    }

    /// `CWeapon::FireProjectile`: a rocket, grenade or molotov leaves the shooter.
    pub fn fire_projectile(&mut self, px: &mut Px, shooter: EntityRef, weapon: usize, mut fire_source: Vec3, power: f32) -> bool {
        let cam = px.hooks.camera();
        let (fwd, spos) = (self.ent(shooter).base.matrix.forward, self.epos(shooter));
        let (source, target);
        if weapon == wt::ROCKETLAUNCHER {
            source = fire_source;
            if self.ped(shooter).is_player() {
                if !cam.weapon_first_person {
                    return false;
                }
                fire_source += cam.front;
            } else {
                fire_source += fwd;
            }
            target = fire_source;
        } else {
            let dot = (fire_source - spos).dot(fwd);
            if dot < 0.3 {
                fire_source += (0.3 - dot) * fwd;
            }
            let mut t = fire_source;
            if t.z - spos.z > 0.0 {
                t += 0.6 * fwd;
            }
            target = t;
            let rel = fire_source - spos;
            source = fire_source - rel.dot(fwd) * fwd;
        }
        let kinds = Kinds { buildings: true, vehicles: true, objects: true, ..Kinds::NONE };
        if self.is_line_of_sight_clear(source, target, kinds, LineOpts::default()) {
            self.add_projectile(px, shooter, weapon, fire_source, power);
        } else if weapon != wt::GRENADE {
            self.projectile_blows_in_place(px, shooter, weapon, fire_source);
        } else {
            let mut at = spos - fwd;
            at.z -= 0.4;
            if self.test_sphere_against_world(at, 0.5, None, Kinds { peds: true, ..Kinds::NONE }, false).is_none() {
                self.add_projectile(px, shooter, weapon, at, 0.0);
            } else {
                self.projectile_blows_in_place(px, shooter, weapon, fire_source);
            }
        }
        true
    }

    /// `CWeapon::FireAreaEffect`: a flamethrower burst.
    pub fn fire_area_effect(&mut self, px: &mut Px, shooter: EntityRef, weapon: usize, source: Vec3) -> bool {
        let range = self.ents.weapon_infos.get(weapon).range;
        let (dir, target);
        if self.ents.player_ped == Some(shooter) && px.hooks.camera().using_3rd_person_mouse_cam {
            let (src, trgt) = px.hooks.cam_target_vector(range, source);
            dir = (trgt - src) * (1.0 / range);
            target = trgt;
        } else {
            let a = heading(self.ent(shooter).base.matrix.forward);
            dir = Vec3::new(-a.sin() * 0.5, a.cos() * 0.5, 0.0);
            target = source + dir;
        }
        let infos = self.ents.weapon_infos.clone();
        let timer = self.ents.timer.clone();
        self.ents.shots.add(&infos, px.rng, &timer, Some(shooter), weapon, source, target);
        px.hooks.shot_effect(ShotEffect::Flames { pos: source, dir });
        true
    }

    /// `CWeapon::FireSniper`: a slow bullet from the camera.
    pub fn fire_sniper(&mut self, px: &mut Px, shooter: EntityRef, weapon: usize) -> bool {
        let cam = px.hooks.camera();
        if !cam.weapon_first_person {
            return false;
        }
        let mut dir = cam.front;
        if dir.dot(Vec3::new(0.0, -0.9894, 0.145)) > 0.997 {
            px.hooks.shot_effect(ShotEffect::SniperMoon);
        }
        dir = normalise(dir) * 16.0;
        let infos = self.ents.weapon_infos.clone();
        let timer = self.ents.timer.clone();
        self.ents.bullets.add(&infos, &timer, Some(shooter), weapon, cam.source, dir);
        if self.ents.player_ped == Some(shooter) {
            px.hooks.shot_effect(ShotEffect::InstantHitFiredByPlayer);
            let pos = self.epos(shooter);
            px.hooks.shot_effect(ShotEffect::PadShake { duration: 240, frequency: 128, pos });
            px.hooks.shot_effect(ShotEffect::CamShake(0.2));
        }
        true
    }

    /// `CWeapon::FireM16_1stPerson`: a hitscan shot from the camera.
    pub fn fire_m16_first_person(&mut self, px: &mut Px, shooter: EntityRef, weapon: usize) -> bool {
        let cam = px.hooks.camera();
        if !(cam.weapon_first_person || cam.heli_cannon_first_person) {
            return false;
        }
        let range = self.ents.weapon_infos.get(weapon).range;
        let (source, target) = (cam.source, cam.front * range + cam.source);
        // shortcut: car tyres (`bIncludeCarTyres`) are not part of the line test yet
        self.ents.ignore_entity = Some(shooter);
        let hit = self.shot_line(source, target, true);
        self.ents.ignore_entity = None;
        let front = Vec2::new(cam.front.x, cam.front.y).normalize_or_zero();
        self.do_bullet_impact(px, shooter, weapon, hit.map(|h| h.entity), source, target, hit.map(|h| h.point).unwrap_or_default(), front);
        if let Some(p) = self.test_heli_bullet_collision(source, target, 4, px.rng) {
            px.hooks.shot_effect(ShotEffect::Sparks { pos: p, dir: Vec3::ZERO, count: 16 });
        }
        if self.ents.player_ped == Some(shooter) {
            px.hooks.shot_effect(ShotEffect::InstantHitFiredByPlayer);
            let pos = self.epos(shooter);
            px.hooks.shot_effect(ShotEffect::PadShake { duration: 240, frequency: 128, pos });
            let scale = match weapon {
                wt::M16 => 0.0003,
                wt::HELICANNON => 0.0001,
                _ => 0.0,
            };
            if scale != 0.0 {
                let beta = ((px.rng.number() & 127) as f32 - 64.0) * scale;
                let alpha = ((px.rng.number() & 127) as f32 - 64.0) * scale;
                px.hooks.shot_effect(ShotEffect::CameraKick { beta, alpha });
            }
        }
        true
    }

    /// `CWeapon::FireShotgun`: five pellets in a fan.
    pub fn fire_shotgun(&mut self, px: &mut Px, shooter: EntityRef, weapon: usize, source: Vec3) -> bool {
        let info = self.ents.weapon_infos.get(weapon).clone();
        let forward = self.ent(shooter).base.matrix.forward;
        let rot_offset = ahead_of(heading(forward));
        px.hooks.shot_effect(ShotEffect::MuzzleFlash { weapon, pos: source, ahead: rot_offset, shooter_velocity: None });
        for speed in [0.10, 0.15, 0.20, 0.25] {
            px.hooks.shot_effect(ShotEffect::GunSmoke { weapon, pos: source, ahead: rot_offset, speed });
        }
        px.hooks.register_event_with_culprit(event_type::GUNSHOT, shooter, shooter, 1000);
        let threat = self.ped(shooter).point_gun_at;
        let shooter_angle = match threat {
            Some(t) => {
                let tp = self.epos(t);
                radian_angle_between_points(tp.x, tp.y, source.x, source.y).to_degrees()
            }
            None => heading(forward).to_degrees(),
        };
        let is_player = self.ents.player_ped == Some(shooter);
        let mouse_cam = is_player && px.hooks.camera().using_3rd_person_mouse_cam;
        for i in 0..5 {
            let shoot_rot = ahead_of((7.5 * i as f32 + shooter_angle - 15.0).to_radians());
            let (hit, target, origin);
            if mouse_cam {
                let (src, trgt) = px.hooks.cam_target_vector(1.0, source);
                let cam = px.hooks.camera();
                let left = cross(cam.front, cam.up);
                let f = (i as f32 - 2.0) * (7.5f32.to_radians() / 2.0);
                let t = (f * left + trgt - src) * info.range + src;
                origin = src;
                target = t;
                hit = self.shot_line(src, t, false);
            } else {
                let mut t = source + Vec3::new(shoot_rot.x * info.range, shoot_rot.y * info.range, 0.0);
                match threat {
                    None => self.do_doom_aiming(shooter, source, &mut t),
                    Some(th) => {
                        let tp = self.epos(th);
                        let dist = (tp - source).truncate().length();
                        t.z += info.range / dist * (tp.z - t.z);
                    }
                }
                origin = source;
                target = t;
                hit = self.shot_line(source, t, false);
            }
            match hit {
                Some(h) => self.shotgun_pellet_hits(px, shooter, weapon, h.entity, h.point, source),
                None => {
                    let to = source + (target - source) * (info.range.min(30.0) / info.range);
                    px.hooks.shot_effect(ShotEffect::Trace { from: source, to });
                    let _ = origin;
                }
            }
        }
        if is_player {
            let pos = self.epos(shooter);
            px.hooks.shot_effect(ShotEffect::PadShake { duration: 240, frequency: 128, pos });
        }
        true
    }

    /// One pellet of `FireShotgun` hitting `v`.
    fn shotgun_pellet_hits(&mut self, px: &mut Px, shooter: EntityRef, weapon: usize, v: EntityRef, point: ColPoint, source: Vec3) {
        let damage = self.ents.weapon_infos.get(weapon).damage as f32;
        let ty = self.ent(v).base.ty;
        px.hooks.shot_effect(ShotEffect::GlassHit { victim: v, pos: point.point });
        px.hooks.shot_effect(ShotEffect::Trace { from: source, to: point.point });
        match ty {
            EntityType::Ped => {
                let pd = self.ped(v);
                if !pd.on_ground() && v != shooter && self.does_los_bullet_hit_ped(point.point.z) != 0 {
                    let victim_pos = self.epos(v);
                    let mut offset = Vec2::new(source.x - victim_pos.x, source.y - victim_pos.y);
                    let dir = local_direction(self.ped(v).rotation_cur, offset);
                    if let Some(p) = self.ents.player_ped {
                        // the original makes the victim react to the player, whoever fired
                        self.react_to_attack(px, v, p);
                    }
                    offset = offset.normalize_or_zero();
                    let now = self.now();
                    let cant_standup = self.ped(v).get_up_timer <= now.wrapping_sub(3000);
                    let standing = self.ped(v).flags.is_standing;
                    let force = if standing && cant_standup {
                        self.ped_mut(v).flags.is_standing = false;
                        Vec3::new(offset.x * -6.0, offset.y * -6.0, 5.0)
                    } else {
                        Vec3::new(offset.x * -2.0, offset.y * -2.0, 0.0)
                    };
                    self.get_mut(v).and_then(|e| e.phys.as_mut()).expect("physical").apply_move_force(force);
                    if cant_standup {
                        self.set_fall(px, v, 1500, crate::anim::tables_gta3::anim::STD_HIGHIMPACT_FRONT + dir as u32, false);
                    }
                    self.inflict_damage(px, v, Hit { by: Some(shooter), weapon, damage, piece: point.piece_b, direction: dir as u8 });
                    let event = if self.ped(v).ped_type == ped_type::COP { event_type::SHOOT_COP } else { event_type::SHOOT_PED };
                    px.hooks.register_event_with_culprit(event, v, shooter, 10000);
                    if px.hooks.nasty_game() && px.hooks.is_on_screen(v) {
                        let count = if weapon == wt::SHOTGUN { 32 } else { 8 };
                        let blood_dir = Vec3::new((point.point.x - victim_pos.x) * 0.01, (point.point.y - victim_pos.y) * 0.01, 0.01);
                        px.hooks.shot_effect(ShotEffect::BloodSmall { pos: point.point, dir: blood_dir, count });
                    }
                }
            }
            EntityType::Vehicle => {
                if let Some(mut e) = self.ents.pools.take(v) {
                    self.inflict_vehicle_damage(px.rng, &mut e, Some(shooter), weapon as i8, damage);
                    self.ents.pools.restore(v, e);
                }
                px.hooks.shot_effect(ShotEffect::Sparks { pos: point.point, dir: point.normal * 0.05, count: 16 });
                px.hooks.shot_effect(ShotEffect::BulletHitSmoke { pos: point.point });
            }
            EntityType::Building | EntityType::Object => {
                px.hooks.shot_effect(ShotEffect::Sparks { pos: point.point, dir: point.normal * 0.05, count: 16 });
                let mut smoke = point.point;
                smoke.x += px.rng.range_f32(-0.2, 0.2);
                smoke.y += px.rng.range_f32(-0.2, 0.2);
                smoke.z += px.rng.range_f32(-0.2, 0.2);
                px.hooks.shot_effect(ShotEffect::BulletHitSmoke { pos: smoke });
                if ty == EntityType::Object {
                    self.shot_pushes_object(v, point.normal, 5.0);
                }
            }
            _ => {}
        }
        self.bullet_hit_sound(px, v, ty, point.point);
    }
}
