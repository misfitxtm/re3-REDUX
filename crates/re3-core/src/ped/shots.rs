//! The shot bodies of `Weapon.cpp` for a ped holding the weapon.
use super::ctx::{Px, ShotEffect};
use super::data::*;
use super::fight::local_direction;
use super::ik::ik_flag;
use super::pending::Hit;
use super::types::ped_type;
use crate::anim::tables_gta3::anim;
use crate::anim::{assoc_group, flags as aflags};
use crate::audio::{script_sound, sound};
use crate::collision::ColPoint;
use crate::entity::{status, EntityRef, EntityType, Kind};
use crate::general::radian_angle_between_points;
use crate::math::heading;
use crate::weapons::info::weapon_type as wt;
use crate::world::{Kinds, LineHit, LineOpts, World};
use glam::{Vec2, Vec3};

/// The direction a heading looks along on the ground.
fn ahead_of(angle: f32) -> Vec2 {
    Vec2::new(-angle.sin(), angle.cos())
}

impl World {
    /// `CWeapon::FireMelee`: true if a punch or swing connected with anyone.
    pub fn fire_melee_shot(&mut self, px: &mut Px, shooter: EntityRef, weapon: usize, source: Vec3) -> bool {
        let info = self.ents.weapon_infos.get(weapon).clone();
        let anim2_playing = self.ped_assoc(shooter, info.anim2).is_some();
        let near: Vec<EntityRef> = {
            let pd = self.ped(shooter);
            pd.near_peds[..pd.num_near_peds as usize].iter().flatten().copied().collect()
        };
        let is_bat = weapon == wt::BASEBALLBAT;
        let mut contact = false;
        for victim in near {
            let Some(vp) = self.try_ped(victim) else { continue };
            let sp = self.ped(shooter);
            let picked = (vp.ped_type != sp.ped_type || Some(victim) == sp.seek_target) && Some(victim) != sp.leader;
            if !(picked || px.rng.number() & 31 == 0) {
                continue;
            }
            let on_ground = self.ped(victim).on_ground() || !self.is_ped_head_above_pos(victim, -0.3);
            let radius = self.bound_radius_of(victim) + info.radius;
            let vpd = self.ped(victim);
            let uses = self.ent(victim).base.flags.uses_collision || vpd.dead() || vpd.driving();
            let victim_pos = self.epos(victim);
            if !uses || radius * radius <= (victim_pos - source).length_squared() {
                continue;
            }
            let col = if on_ground { &self.ents.temp_col.ped_ground_hit } else { &self.ents.temp_col.ped1 };
            let mut collision_dist = Vec3::ZERO;
            let mut collided = false;
            for s in &col.spheres {
                collision_dist = victim_pos + s.center - source;
                if (s.radius + info.radius).powi(2) > collision_dist.length_squared() {
                    collided = true;
                    break;
                }
            }
            if !collided || (self.ped(victim).is_player() && self.ped(victim).ped_state == state::GETUP) {
                continue;
            }
            let victim_health = self.ped(victim).health;
            let blood_pos = source + collision_dist * 0.7;
            let shooter_pos = self.epos(shooter);
            let mut offset = Vec2::new(shooter_pos.x - victim_pos.x, shooter_pos.y - victim_pos.y);
            let dir = local_direction(self.ped(victim).rotation_cur, offset);
            if !self.ped(victim).dying_or_dead() {
                self.react_to_attack(px, victim, shooter);
            }
            let level = if is_bat && self.ped(victim).on_ground() { hit_level::GROUND } else { hit_level::HIGH };
            self.start_fight_defend(px, victim, dir, level, 10);
            if !self.ped(victim).dying_or_dead() {
                let shooter_player = self.ped(shooter).is_player();
                let adrenaline = shooter_player && self.ped(shooter).player.as_ref().is_some_and(|p| p.adrenaline_active);
                let victim_player = self.ped(victim).is_player();
                let damage = if shooter_player && is_bat && anim2_playing {
                    100.0
                } else if adrenaline {
                    3.5 * info.damage as f32
                } else if victim_player && is_bat {
                    // not fair, but that is the original
                    2.0 * info.damage as f32
                } else {
                    info.damage as f32
                };
                self.inflict_damage(px, victim, Hit { by: Some(shooter), weapon, damage, piece: piece::TORSO, direction: dir as u8 });
            }
            if px.hooks.nasty_game() && px.hooks.is_on_screen(victim) {
                let mut d = collision_dist * (1.0 / (1.0 + 10.0 * collision_dist.length_squared()).sqrt());
                for _ in 0..3 {
                    px.hooks.add_blood(blood_pos, d);
                }
                if is_bat {
                    d.x += px.rng.range_f32(-0.05, 0.05);
                    d.y += px.rng.range_f32(-0.05, 0.05);
                    px.hooks.add_blood(blood_pos, d);
                    d.x += px.rng.range_f32(-0.05, 0.05);
                    d.y += px.rng.range_f32(-0.05, 0.05);
                    px.hooks.add_blood(blood_pos, d);
                }
            }
            if !self.ped(victim).on_ground() {
                let health = self.ped(victim).health;
                let victim_player = self.ped(victim).is_player();
                if health > 0.0 && ((health < 20.0 && victim_health > 20.0) || (is_bat && !victim_player)) {
                    offset = offset.normalize_or_zero();
                    self.ped_mut(victim).flags.is_standing = false;
                    self.get_mut(victim).and_then(|e| e.phys.as_mut()).expect("physical").apply_move_force(Vec3::new(offset.x * -5.0, offset.y * -5.0, 3.0));
                    let time = if is_bat && victim_player { 3000 } else { 1500 };
                    self.set_fall(px, victim, time, anim::STD_HIGHIMPACT_FRONT + dir as u32, false);
                    self.ped_mut(shooter).seek_target = Some(victim);
                }
            } else if self.ped(victim).dying() && !anim2_playing {
                offset = offset.normalize_or_zero();
                self.ped_mut(victim).flags.is_standing = false;
                self.get_mut(victim).and_then(|e| e.phys.as_mut()).expect("physical").apply_move_force(Vec3::new(offset.x * -5.0, offset.y * -5.0, 3.0));
            }
            contact = true;
            let event = if self.ped(victim).ped_type == super::types::ped_type::COP { event_type::ASSAULT_POLICE } else { event_type::ASSAULT };
            px.hooks.register_event_with_culprit(event, victim, shooter, 2000);
        }
        contact
    }

    /// `CWeapon::ProcessLineOfSight`: buildings, vehicles, peds, objects and dummies, see-through glass ignored.
    pub(super) fn shot_line(&mut self, from: Vec3, to: Vec3, dead_peds: bool) -> Option<LineHit> {
        self.ents.include_dead_peds = dead_peds;
        let hit = self.process_line_of_sight(from, to, Kinds::ALL, LineOpts { ignore_see_through: true, ..Default::default() });
        self.ents.include_dead_peds = false;
        hit
    }

    /// `CWeapon::FireInstantHit` for a ped: a hitscan shot with a muzzle effect and an impact.
    pub fn fire_instant_hit(&mut self, px: &mut Px, shooter: EntityRef, weapon: usize, fire_source: Vec3) -> bool {
        let info = self.ents.weapon_infos.get(weapon).clone();
        let forward = self.ent(shooter).base.matrix.forward;
        let mut ahead = ahead_of(heading(forward));
        let vel = self.ent(shooter).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
        let moving = vel.x.abs() > 0.0 && vel.y.abs() > 0.0;
        let is_player = self.ents.player_ped == Some(shooter);
        let (hit, target);
        if let Some(threat) = self.ped(shooter).point_gun_at {
            if self.ped(shooter).ik.flags & ik_flag::GUN_POINTED_SUCCESSFULLY != 0 {
                let inaccuracy = 100 - self.ped(shooter).wep_accuracy as i32;
                let mut t = if self.is_ped(threat) {
                    let p = self.ped_node_position(threat, node::MID);
                    self.react_to_point_gun(px, threat, shooter);
                    p
                } else {
                    self.epos(threat)
                };
                t = (t - fire_source) * (info.range / (t - fire_source).length()) + fire_source;
                if inaccuracy != 0 {
                    let k = inaccuracy as f32;
                    t.x += px.rng.range_f32(-0.2, 0.2) * k;
                    t.y += px.rng.range_f32(-0.2, 0.2) * k;
                    t.z += px.rng.range_f32(-0.1, 0.1) * k;
                }
                target = t;
                hit = self.shot_line(fire_source, t, true);
            } else {
                target = self.transform_to_node(shooter, Vec3::new(info.range, 0.0, 0.0), node::HANDR);
                hit = self.shot_line(fire_source, target, false);
            }
            // FIX_BUGS: the muzzle flash follows the shot
            ahead = ahead_of(radian_angle_between_points(fire_source.x, fire_source.y, target.x, target.y));
        } else if is_player && px.hooks.camera().using_3rd_person_mouse_cam {
            let (src, trgt) = px.hooks.cam_target_vector(info.range, fire_source);
            ahead = ahead_of(radian_angle_between_points(src.x, src.y, trgt.x, trgt.y));
            hit = self.shot_line(src, trgt, true);
            if let Some(p) = self.test_heli_bullet_collision(src, trgt, 4, px.rng) {
                px.hooks.shot_effect(ShotEffect::Sparks { pos: p, dir: Vec3::ZERO, count: 16 });
            }
            // shortcut: the original leaves `target` unset here; the trace and the dive test use the aimed end
            target = trgt;
        } else {
            let mut t = fire_source + Vec3::new(ahead.x * info.range, ahead.y * info.range, 0.0);
            self.do_doom_aiming(shooter, fire_source, &mut t);
            hit = self.shot_line(fire_source, t, false);
            if let Some(p) = self.test_heli_bullet_collision(fire_source, t, 4, px.rng) {
                px.hooks.shot_effect(ShotEffect::Sparks { pos: p, dir: Vec3::ZERO, count: 16 });
            }
            target = t;
        }
        let victim = hit.map(|h| h.entity);
        if victim.is_some() && victim == self.ped(shooter).leader {
            return false;
        }
        px.hooks.register_event_with_culprit(event_type::GUNSHOT, shooter, shooter, 1000);
        if is_player {
            px.hooks.shot_effect(ShotEffect::InstantHitFiredByPlayer);
            if self.ents.timer.frame_counter & 3 == 0 {
                self.make_peds_jump_at_shot(px, shooter, fire_source, target);
            }
        }
        self.gun_effects(px, shooter, weapon, fire_source, ahead, moving.then_some(vel));
        self.do_bullet_impact(px, shooter, weapon, victim, fire_source, target, hit.map(|h| h.point).unwrap_or_default(), ahead);
        true
    }

    /// The flash, smoke and shell of one shot; the draws of random numbers stay in the original's order.
    pub(super) fn gun_effects(&mut self, px: &mut Px, shooter: EntityRef, weapon: usize, source: Vec3, ahead: Vec2, vel: Option<Vec3>) {
        let side = Vec2::new(ahead.y, -ahead.x).normalize_or_zero();
        let flash = |px: &mut Px, v: Option<Vec3>| px.hooks.shot_effect(ShotEffect::MuzzleFlash { weapon, pos: source, ahead, shooter_velocity: v });
        match weapon {
            wt::AK47 | wt::M16 => {
                let slot = (weapon == wt::M16) as usize;
                let c = &mut self.ents.gun_flash_counters[slot];
                *c = c.wrapping_add(1);
                if *c & 1 == 0 {
                    flash(px, None);
                    let rnd = px.rng.range_f32(0.05, 0.25);
                    px.hooks.shot_effect(ShotEffect::GunSmoke { weapon, pos: source, ahead, speed: rnd });
                    let (back, size) = if weapon == wt::M16 { (0.65, 0.02) } else { (0.5, 0.018) };
                    self.add_gunshell(px, shooter, source - Vec3::new(back * ahead.x, back * ahead.y, 0.0), side, size);
                }
            }
            wt::UZI => {
                flash(px, vel);
                let rnd = px.rng.range_f32(0.05, 0.25);
                px.hooks.shot_effect(ShotEffect::GunSmoke { weapon, pos: source, ahead, speed: rnd });
                self.add_gunshell(px, shooter, source - Vec3::new(0.2 * ahead.x, 0.2 * ahead.y, 0.0), side, 0.015);
            }
            wt::COLT45 => {
                flash(px, vel);
                for speed in [0.10, 0.15, 0.20] {
                    px.hooks.shot_effect(ShotEffect::GunSmoke { weapon, pos: source, ahead, speed });
                }
                self.add_gunshell(px, shooter, source - Vec3::new(0.2 * ahead.x, 0.2 * ahead.y, 0.0), side, 0.015);
            }
            _ => {}
        }
    }

    /// `CWeapon::AddGunshell`: a shell flies out; the first one after the shooter moved gets a special kind.
    pub fn add_gunshell(&mut self, px: &mut Px, shooter: EntityRef, source: Vec3, direction: Vec2, size: f32) {
        let velocity = Vec3::new(direction.x * 0.05, direction.y * 0.05, px.rng.range_f32(0.02, 0.08));
        let pos = self.epos(shooter);
        let diff = pos - self.ents.gunshell_prev_pos;
        let first = diff.x.abs() + diff.y.abs() + diff.z.abs() > 1.5;
        if first {
            self.ents.gunshell_prev_pos = pos;
        }
        let rotation = px.rng.range_f32(-20.0, 20.0);
        px.hooks.shot_effect(ShotEffect::GunShell { pos: source, velocity, size, rotation, first });
    }

    /// `CWeapon::DoDoomAiming`: tilts the shot up or down at the nearest thing in front of the shooter.
    pub fn do_doom_aiming(&mut self, shooter: EntityRef, source: Vec3, target: &mut Vec3) {
        if self.ped(shooter).flags.crouch_when_shooting {
            return;
        }
        let kinds = Kinds { vehicles: true, peds: true, ..Kinds::NONE };
        let found = self.find_objects_in_range(source, (*target - source).length(), true, 15, kinds);
        self.aim_at_closest(shooter, &found, source, target);
    }

    /// The second half of `DoDoomAiming`: of the entities in range, the nearest one in view sets the shot's height.
    pub(super) fn aim_at_closest(&mut self, shooter: EntityRef, found: &[EntityRef], source: Vec3, target: &mut Vec3) {
        let (mut closest, mut closest_dist) = (None, 10000.0f32);
        let me = self.epos(shooter);
        for &v in found {
            if v == shooter || !self.can_see_entity(shooter, v, 22.5f32.to_radians()) {
                continue;
            }
            let e = self.ent(v);
            if matches!(e.base.status, status::TRAIN_MOVING | status::TRAIN_NOT_MOVING | status::HELI | status::PLANE) {
                continue;
            }
            let p = e.base.position();
            let dist = (me - p).truncate().length();
            let dist_z = (me.z - p.z).abs();
            if 1.5 * dist_z < dist {
                let d = (dist * dist + dist_z * dist_z).sqrt();
                if d < closest_dist {
                    closest_dist = d;
                    closest = Some(v);
                }
            }
        }
        // the original's limit is 9000, which any hit passes
        if let (Some(v), true) = (closest, closest_dist < 9000.0) {
            let dist_to_target = (*target - source).truncate().length();
            let dist_to_source = (self.epos(v) - source).truncate().length();
            let mut z = self.epos(v).z + 0.3;
            if self.try_ped(v).is_some_and(|p| p.flags.is_ducking) {
                z -= 0.8;
            }
            target.z = (dist_to_target / dist_to_source) * (z - source.z) + source.z;
        }
    }

    /// `CWeapon::MakePedsJumpAtShot`: peds near the line of fire dive for cover now and then.
    pub fn make_peds_jump_at_shot(&mut self, px: &mut Px, shooter: EntityRef, source: Vec3, target: Vec3) {
        let (lo, hi) = (source.min(target) - Vec3::splat(2.0), source.max(target) + Vec3::splat(2.0));
        for r in self.ped_refs_descending() {
            let Some(pd) = self.try_ped(r) else { continue };
            let p = self.epos(r);
            let inside = p.x > lo.x && p.x < hi.x && p.y > lo.y && p.y < hi.y && p.z > lo.z && p.z < hi.z;
            let seed = self.ent(r).base.random_seed;
            let _ = pd;
            if inside && self.ents.player_ped != Some(r) && (seed ^ px.rng.number()) as u8 & 31 == 0 {
                self.set_evasive_dive(px, r, shooter, true);
            }
        }
    }

    /// `CWeapon::DoBulletImpact`: damage, effects and sounds where a shot ended.
    #[allow(clippy::too_many_arguments)] // mirrors the original's signature
    pub fn do_bullet_impact(&mut self, px: &mut Px, shooter: EntityRef, weapon: usize, victim: Option<EntityRef>, source: Vec3, target: Vec3, point: ColPoint, ahead: Vec2) {
        let _ = ahead; // FIX_BUGS: the smoke sits at the hit point
        let damage = self.ents.weapon_infos.get(weapon).damage as f32;
        let shooter_is_player = self.ents.player_ped == Some(shooter);
        let Some(v) = victim else {
            px.hooks.shot_effect(ShotEffect::Trace { from: source, to: target });
            self.shot_ends(px, shooter_is_player, None);
            return;
        };
        let (ty, victim_pos) = (self.ent(v).base.ty, self.epos(v));
        px.hooks.shot_effect(ShotEffect::GlassHit { victim: v, pos: point.point });
        px.hooks.shot_effect(ShotEffect::Trace { from: source, to: point.point });
        if shooter_is_player && matches!(ty, EntityType::Ped | EntityType::Vehicle) {
            px.hooks.shot_effect(ShotEffect::InstantHitHitByPlayer);
        }
        let shooter_type = self.ped(shooter).ped_type;
        let blood_dir = Vec3::new((point.point.x - victim_pos.x) * 0.01, (point.point.y - victim_pos.y) * 0.01, 0.01);
        if ty == EntityType::Ped && (shooter_type != self.ped(v).ped_type || shooter_type == ped_type::PLAYER2) {
            if !self.ped(v).dying_or_dead() && v != shooter {
                if self.does_los_bullet_hit_ped(point.point.z) != 0 {
                    self.bullet_hits_ped(px, shooter, weapon, v, source, point, blood_dir);
                }
            } else if px.hooks.nasty_game() {
                if px.hooks.is_on_screen(v) {
                    px.hooks.shot_effect(ShotEffect::BloodSmall { pos: point.point + Vec3::new(0.0, 0.0, 0.15), dir: blood_dir, count: 8 });
                }
                if self.ped(v).dead() {
                    let anim_id = if self.ped_first_assoc(v, aflags::FRONTAL).is_some() { anim::STD_HIT_FLOOR_FRONT } else { anim::STD_HIT_FLOOR };
                    if let Some(i) = self.ped_blend_anim(v, assoc_group::STD, anim_id, 8.0)
                        && let Some(a) = self.ped_assoc_at(v, i)
                    {
                        a.set_current_time(0.0);
                        a.flags |= aflags::RUNNING;
                        a.flags &= !aflags::FADE_OUT_WHEN_DONE;
                    }
                }
            }
        } else {
            match ty {
                EntityType::Building => {
                    px.hooks.shot_effect(ShotEffect::Sparks { pos: point.point, dir: point.normal * 0.05, count: 16 });
                    let mut smoke = point.point;
                    smoke.x += px.rng.range_f32(-0.2, 0.2);
                    smoke.y += px.rng.range_f32(-0.2, 0.2);
                    smoke.z += px.rng.range_f32(-0.2, 0.2);
                    px.hooks.shot_effect(ShotEffect::BulletHitSmoke { pos: smoke });
                }
                EntityType::Vehicle => {
                    if let Some(mut e) = self.ents.pools.take(v) {
                        self.inflict_vehicle_damage(px.rng, &mut e, Some(shooter), weapon as i8, damage);
                        self.ents.pools.restore(v, e);
                    }
                    px.hooks.shot_effect(ShotEffect::Sparks { pos: point.point, dir: point.normal * 0.05, count: 16 });
                    px.hooks.shot_effect(ShotEffect::BulletHitSmoke { pos: point.point });
                    let pd = self.ped(shooter);
                    if pd.flags.not_allowed_to_duck && pd.flags.kinda_stay_in_same_place && Some(v) != pd.point_gun_at {
                        let now = self.now();
                        let pd = self.ped_mut(shooter);
                        pd.flags.kinda_stay_in_same_place = false;
                        pd.duck_and_cover_timer = now + 15000;
                    }
                }
                EntityType::Object => {
                    px.hooks.shot_effect(ShotEffect::Sparks { pos: point.point, dir: point.normal * 0.05, count: 8 });
                    self.shot_pushes_object(v, point.normal, 4.0);
                }
                _ => {}
            }
        }
        self.bullet_hit_sound(px, v, ty, point.point);
        self.shot_ends(px, shooter_is_player, Some(v));
    }

    /// The sound a bullet makes where it ends, by what it hit.
    pub(super) fn bullet_hit_sound(&mut self, px: &mut Px, v: EntityRef, ty: EntityType, at: Vec3) {
        match ty {
            EntityType::Building => px.hooks.shot_effect(ShotEffect::ScriptSound { sound: script_sound::BULLET_HIT_GROUND_1, pos: at }),
            EntityType::Vehicle => px.hooks.shot_effect(ShotEffect::Sound { entity: v, sound: sound::WEAPON_HIT_VEHICLE }),
            EntityType::Ped => {
                px.hooks.shot_effect(ShotEffect::Sound { entity: v, sound: sound::WEAPON_HIT_PED });
                self.say(px, v, sound::PED_BULLET_HIT);
            }
            EntityType::Object => px.hooks.shot_effect(ShotEffect::ScriptSound { sound: script_sound::BULLET_HIT_GROUND_2, pos: at }),
            EntityType::Dummy => px.hooks.shot_effect(ShotEffect::ScriptSound { sound: script_sound::BULLET_HIT_GROUND_3, pos: at }),
            EntityType::Nothing => {}
        }
    }

    /// The end of `DoBulletImpact`: the player's pad shakes and a hit barrel blows up.
    pub(super) fn shot_ends(&mut self, px: &mut Px, shooter_is_player: bool, victim: Option<EntityRef>) {
        if shooter_is_player {
            let pos = self.ents.player_ped.map_or(Vec3::ZERO, |p| self.epos(p));
            px.hooks.shot_effect(ShotEffect::PadShake { duration: 240, frequency: 128, pos });
        }
        if let Some(v) = victim.filter(|&v| matches!(self.get(v).map(|e| &e.kind), Some(Kind::Object(_)))) {
            self.blow_up_explosive_thing(px.rng, v);
        }
    }

    /// A shot pushes a loose object (or tears a static one off its base).
    pub(super) fn shot_pushes_object(&mut self, v: EntityRef, normal: Vec3, strength: f32) {
        let Some(e) = self.get(v) else { return };
        let Kind::Object(o) = &e.kind else { return };
        let Some(p) = e.phys.as_ref() else { return };
        if p.flags.infinite_mass {
            return;
        }
        if e.base.flags.is_static && o.uproot_limit <= 0.0 {
            self.make_moving(v);
        }
        if !self.ent(v).base.flags.is_static {
            let force = normal * -strength;
            self.get_mut(v).and_then(|e| e.phys.as_mut()).expect("physical").apply_move_force(force);
        }
    }

    /// The ped half of `DoBulletImpact`: reaction, damage and blood.
    #[allow(clippy::too_many_arguments)] // the pieces the original keeps in locals
    fn bullet_hits_ped(&mut self, px: &mut Px, shooter: EntityRef, weapon: usize, victim: EntityRef, source: Vec3, point: ColPoint, blood_dir: Vec3) {
        let damage = self.ents.weapon_infos.get(weapon).damage as f32;
        let victim_pos = self.epos(victim);
        let mut offset = Vec2::new(source.x - victim_pos.x, source.y - victim_pos.y);
        let dir = local_direction(self.ped(victim).rotation_cur, offset);
        self.react_to_attack(px, victim, shooter);
        let hit = Hit { by: Some(shooter), weapon, damage, piece: point.piece_b, direction: dir as u8 };
        let pd = self.ped(victim);
        if !pd.is_ped_in_control() || pd.flags.is_ducking {
            self.inflict_damage(px, victim, hit);
        } else if weapon == wt::SHOTGUN || weapon == wt::HELICANNON {
            offset = offset.normalize_or_zero();
            self.ped_mut(victim).flags.is_standing = false;
            self.get_mut(victim).and_then(|e| e.phys.as_mut()).expect("physical").apply_move_force(Vec3::new(offset.x * -5.0, offset.y * -5.0, 5.0));
            self.set_fall(px, victim, 1500, anim::STD_HIGHIMPACT_FRONT + dir as u32, false);
            self.inflict_damage(px, victim, hit);
        } else {
            let now = self.now();
            let gets_anim = if self.ped(victim).is_player() { self.ped(victim).player.as_ref().is_some_and(|p| p.hit_anim_delay_timer < now) } else { true };
            if gets_anim {
                self.clear_attack_by_removing_anim(px, victim);
                if let Some(i) = self.ped_add_anim(victim, assoc_group::STD, anim::STD_HITBYGUN_FRONT + dir as u32)
                    && let Some(a) = self.ped_assoc_at(victim, i)
                {
                    a.blend_amount = 0.0;
                    a.blend_delta = 8.0;
                }
                if self.ped(victim).is_player() {
                    let delay = if matches!(weapon, wt::AK47 | wt::M16) { 2500 } else { 1000 };
                    if let Some(p) = self.ped_mut(victim).player.as_deref_mut() {
                        p.hit_anim_delay_timer = now + delay;
                    }
                }
            }
            self.inflict_damage(px, victim, hit);
        }
        let event = if self.ped(victim).ped_type == ped_type::COP { event_type::SHOOT_COP } else { event_type::SHOOT_PED };
        px.hooks.register_event_with_culprit(event, victim, shooter, 10000);
        if px.hooks.nasty_game() && px.hooks.is_on_screen(victim) {
            let count = if matches!(weapon, wt::SHOTGUN | wt::HELICANNON) { 32 } else { 8 };
            px.hooks.shot_effect(ShotEffect::BloodSmall { pos: point.point, dir: blood_dir, count });
        }
    }
}
