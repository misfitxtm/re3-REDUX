//! `CWeapon` shots fired from a car window and the aiming helpers for cars and tanks.
use super::ctx::{Px, ShotEffect};
use super::data::*;
use super::fight::local_direction;
use super::pending::Hit;
use super::types::ped_type;
use crate::anim::tables_gta3::anim;
use crate::anim::assoc_group;
use crate::collision::dist_to_line;
use crate::entity::{status, EntityRef, EntityType};
use crate::weapons::info::weapon_type as wt;
use crate::world::{Kinds, World};
use glam::Vec3;

/// `CAR_POS_FRONTSEAT`.
const CAR_POS_FRONTSEAT: usize = 2;
const DRIVEBYAUTOAIMING_MAXDIST: f32 = 2.5;
const DOOMAUTOAIMING_MAXDIST: f32 = 9000.0;

impl World {
    /// `CWeapon::FireFromCar` with the player's current weapon, as `CAutomobile::ProcessControl` drives it.
    pub fn fire_from_car(&mut self, px: &mut Px, car: EntityRef, left: bool) -> bool {
        let Some(player) = self.ents.player_ped else { return false };
        let slot = self.ped(player).current_weapon as usize;
        let mut weapon = self.ped(player).weapons[slot].clone();
        let (infos, timer) = (self.ents.weapon_infos.clone(), self.ents.timer.clone());
        let mut shot = false;
        let fired = weapon.fire_from_car(&infos, &timer, &mut || {
            shot = self.fire_instant_hit_from_car(px, car, left, slot);
            if shot {
                px.hooks.shot_effect(ShotEffect::Sound { entity: car, sound: crate::audio::sound::WEAPON_SHOT_FIRED });
            }
            shot
        });
        // the round count only goes up when the clip did not just run dry
        if shot && weapon.ammo_in_clip != 0 && self.find_player_vehicle() == Some(car) {
            px.hooks.shot_effect(ShotEffect::RoundFiredByPlayer);
        }
        if let Some(p) = self.try_ped_mut_pub(player) {
            p.weapons[slot] = weapon;
        }
        fired
    }

    /// `CWeapon::FireInstantHitFromCar`: a hitscan shot out of the side window.
    pub fn fire_instant_hit_from_car(&mut self, px: &mut Px, car: EntityRef, left: bool, weapon: usize) -> bool {
        let Some(player) = self.ents.player_ped else { return false };
        let info = self.ents.weapon_infos.get(weapon).clone();
        let model = self.ent(car).base.model_index;
        let seat = match self.ents.models.get(model as i32).map(|m| &m.kind) {
            Some(crate::modelinfo::Kind::Vehicle(v)) => v.positions[CAR_POS_FRONTSEAT],
            _ => Vec3::ZERO,
        };
        let max_x = self.col_model_of(car).map_or(0.0, |c| c.bounding_box.max.x);
        let (matrix, speed) = (self.ent(car).base.matrix, self.ent(car).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed));
        let side = if left { -1.0 } else { 1.0 };
        let y = (px.rng.number() & 255) as f32 * 0.001 + seat.y;
        let mut source = matrix.transform_point(Vec3::new(side * (max_x + 0.2), y, seat.z + 0.5));
        source += self.time_step() * speed;
        let mut target = matrix.transform_point(Vec3::new(side * info.range, seat.y, seat.z + 0.5));
        let lrb = px.hooks.camera().looking_lrb_first_person;
        if lrb && !left {
            source -= 0.3 * matrix.forward;
            target -= 0.3 * matrix.forward;
        }
        let jitter = |px: &mut Px| (px.rng.number() & 255) as f32 * 0.01 - 1.28;
        target += Vec3::new(jitter(px), jitter(px), jitter(px));
        self.do_drive_by_auto_aiming(player, source, &mut target);

        px.hooks.register_event_with_culprit(event_type::GUNSHOT, player, player, 1000);
        if lrb {
            px.hooks.shot_effect(ShotEffect::CamShake(0.01));
        } else {
            let ahead = glam::Vec2::new(matrix.forward.x, matrix.forward.y).normalize_or_zero();
            px.hooks.shot_effect(ShotEffect::MuzzleFlash { weapon, pos: source, ahead, shooter_velocity: None });
        }
        px.hooks.register_event_with_culprit(event_type::GUNSHOT, car, player, 1000);

        let hit = self.shot_line(source, target, false);
        if self.ents.timer.frame_counter & 3 == 0 {
            self.make_peds_jump_at_shot(px, car, source, target);
        }
        if let Some(h) = hit {
            let (v, point) = (h.entity, h.point);
            px.hooks.shot_effect(ShotEffect::Trace { from: source, to: point.point });
            let ty = self.ent(v).base.ty;
            match ty {
                EntityType::Ped => self.car_bullet_hits_ped(px, car, player, weapon, v, source, point.piece_b),
                EntityType::Vehicle => {
                    if let Some(mut e) = self.ents.pools.take(v) {
                        self.inflict_vehicle_damage(px.rng, &mut e, Some(player), wt::UZI_DRIVEBY as i8, info.damage as f32);
                        self.ents.pools.restore(v, e);
                    }
                }
                _ => px.hooks.shot_effect(ShotEffect::GlassHit { victim: v, pos: point.point }),
            }
            self.bullet_hit_sound(px, v, ty, point.point);
        } else {
            let to = (target - source) * (30.0 / info.range) + source;
            px.hooks.shot_effect(ShotEffect::Trace { from: source, to });
        }
        if self.find_player_vehicle() == Some(car) {
            let pos = self.epos(car);
            px.hooks.shot_effect(ShotEffect::PadShake { duration: 240, frequency: 128, pos });
        }
        true
    }

    /// The ped half of the car-window shot.
    #[allow(clippy::too_many_arguments)] // the locals of the original
    fn car_bullet_hits_ped(&mut self, px: &mut Px, car: EntityRef, player: EntityRef, weapon: usize, v: EntityRef, source: Vec3, piece: u8) {
        if self.ped(v).dying_or_dead() || v == car {
            return;
        }
        let damage = self.ents.weapon_infos.get(weapon).damage as f32;
        let mut pos = self.epos(v);
        let dir = local_direction(self.ped(v).rotation_cur, glam::Vec2::new(source.x - pos.x, source.y - pos.y));
        self.react_to_attack(px, v, player);
        self.clear_attack_by_removing_anim(px, v);
        if let Some(i) = self.ped_add_anim(v, assoc_group::STD, anim::STD_HITBYGUN_FRONT + dir as u32)
            && let Some(a) = self.ped_assoc_at(v, i)
        {
            a.blend_amount = 0.0;
            a.blend_delta = 8.0;
        }
        self.inflict_damage(px, v, Hit { by: Some(car), weapon: wt::UZI_DRIVEBY, damage: 3.0 * damage, piece, direction: dir as u8 });
        pos.z += 0.8;
        if px.hooks.is_on_screen(v) && px.hooks.nasty_game() {
            for _ in 0..4 {
                let d = Vec3::new(px.rng.range_f32(-0.1, 0.1), px.rng.range_f32(-0.1, 0.1), px.rng.range_f32(-0.1, 0.1));
                px.hooks.add_blood(pos, d);
            }
        }
        let event = if self.ped(v).ped_type == ped_type::COP { event_type::SHOOT_COP } else { event_type::SHOOT_PED };
        px.hooks.register_event_with_culprit(event, v, player, 10000);
    }

    /// `CWeapon::DoDriveByAutoAiming`: pulls the shot towards the ped closest to its line.
    pub fn do_drive_by_auto_aiming(&mut self, shooter: EntityRef, source: Vec3, target: &mut Vec3) {
        if self.try_ped(shooter).is_some_and(|p| p.flags.crouch_when_shooting) {
            return;
        }
        let found = self.find_objects_in_range(source, (*target - source).length(), true, 15, Kinds { peds: true, ..Kinds::NONE });
        let (mut closest, mut closest_dist) = (None, 10000.0f32);
        let me = self.epos(shooter);
        for &v in &found {
            if v == shooter {
                continue;
            }
            let p = self.epos(v);
            let line_dist = dist_to_line(source, *target, p);
            let ped_dist = 0.15 * (p - me).length() + line_dist;
            if (*target - source).dot(p - source) > 0.0 && ped_dist < closest_dist {
                closest = Some(v);
                closest_dist = ped_dist;
            }
        }
        if let (Some(v), true) = (closest, closest_dist < DRIVEBYAUTOAIMING_MAXDIST) {
            let to_target = (source - *target).length();
            let to_victim = (source - self.epos(v)).length();
            *target = (to_target / to_victim) * (self.epos(v) - source) + source;
        }
    }

    /// `CWeapon::DoTankDoomAiming`: the tank's cannon tilts at the nearest car in front.
    pub fn do_tank_doom_aiming(&mut self, shooter: EntityRef, driver: EntityRef, source: Vec3, target: &mut Vec3) {
        let found = self.find_objects_in_range(source, (*target - source).length(), true, 15, Kinds { vehicles: true, ..Kinds::NONE });
        let norm_z = (target.z - source.z) / (*target - source).length();
        let (mut closest, mut closest_dist) = (None, 10000.0f32);
        let me = self.epos(shooter);
        for &v in &found {
            if v == shooter || v == driver {
                continue;
            }
            let e = self.ent(v);
            if matches!(e.base.status, status::TRAIN_MOVING | status::TRAIN_NOT_MOVING | status::HELI | status::PLANE) || e.base.flags.render_scorched {
                continue;
            }
            let p = e.base.position();
            let dist = (me - p).truncate().length();
            let dist_z = (me.z - (dist * norm_z + p.z)).abs();
            if 3.0 * dist_z < dist && dist_to_line(source, *target, Vec3::new(p.x, p.y, 0.0)) < self.bound_radius_of(v) * 3.0 {
                let d = (dist * dist + dist_z * dist_z).sqrt();
                if d < closest_dist {
                    closest_dist = d;
                    closest = Some(v);
                }
            }
        }
        if let (Some(v), true) = (closest, closest_dist < DOOMAUTOAIMING_MAXDIST) {
            let to_target = (*target - source).truncate().length();
            let to_source = (self.epos(v) - source).truncate().length();
            target.z = (to_target / to_source) * (0.3 + self.epos(v).z - source.z) + source.z;
        }
    }
}
