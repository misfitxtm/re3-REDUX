//! `CPed::ProcessControl`: one ped's whole frame.
use super::ctx::Px;
use super::data::*;
use crate::anim::{assoc_group, flags as aflags, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::physical;
use crate::entity::EntityRef;
use crate::general::{limit_radian_angle, radian_angle_between_points};
use crate::math::normalise;
use crate::world::{Kinds, World};
use glam::Vec3;
use std::f32::consts::FRAC_PI_2;

/// `LEVEL_GENERIC`.
const LEVEL_GENERIC: i8 = 0;

impl World {
    /// `CPed::ProcessControl`.
    pub fn process_control(&mut self, px: &mut Px, r: EntityRef) {
        let zone = self.ent(r).phys.as_ref().map_or(0, |p| p.zone_level);
        if zone > LEVEL_GENERIC && zone as i32 != self.ents.collision_in_memory {
            return;
        }
        // shortcut: the clump alpha fade belongs to the renderer, `fade_out` is all the ped keeps
        self.ped_mut(r).flags.is_shooting = false;
        self.build_ped_lists(r);
        self.phys_mut(r).flags.is_in_water = false;
        self.process_buoyancy(px, r);
        if self.ped(r).ped_state == state::ARRESTED {
            return;
        }
        if self.ped(r).ped_state == state::DEAD {
            self.process_dead(px, r);
            return;
        }
        self.process_alive(px, r);
    }

    /// The dead branch of `ProcessControl`: bloody tyres, blood on the neighbours, a pool, a last word.
    fn process_dead(&mut self, px: &mut Px, r: EntityRef) {
        self.dead_ped_makes_tyres_bloody(px, r);
        if px.hooks.nasty_game() {
            let now = self.now();
            let since = now.wrapping_sub(self.ped(r).bloody_footprint_count_or_death_time);
            let reach = if since >= 2000 {
                if since <= 7000 { (since - 2000) as f32 / 5000.0 * 0.75 } else { 0.75 }
            } else {
                0.0
            };
            let pos = self.epos(r);
            for i in 0..self.ped(r).num_near_peds as usize {
                let Some(near) = self.ped(r).near_peds[i] else { continue };
                if self.try_ped(near).is_none_or(|d| d.dying_or_dead()) || (self.epos(near) - pos).length_squared() >= reach * reach {
                    continue;
                }
                let pd = self.ped_mut(near);
                pd.bloody_footprint_count_or_death_time = 200;
                pd.flags.do_bloody_footprints = true;
                if pd.is_player() && !pd.flags.is_looking && pd.ped_state != state::ATTACK && !px.hooks.camera_first_person_weapon() && !px.hooks.weapon_first_person_on() {
                    self.set_look_flag_entity(near, r, true);
                    self.set_look_timer(near, 500);
                }
            }
            if since > 2000 {
                if since - 2000 >= 5000 {
                    if !self.ped(r).dead_bleeding {
                        px.hooks.blood_pool(r, pos, 0.75, true);
                        self.ped_mut(r).dead_bleeding = true;
                    }
                } else {
                    px.hooks.blood_pool(r, pos, (since - 2000) as f32 / 5000.0 * 0.75, false);
                }
            }
        }
        if self.service_talking_when_dead(r) {
            self.service_talking(px, r);
        }
    }

    /// Everything after the dead branch. False if the ped was postponed or removed and the frame ends.
    fn process_alive(&mut self, px: &mut Px, r: EntityRef) -> bool {
        let step = self.time_step();
        self.ped_mut(r).flags.was_standing = false;
        if self.ped(r).flags.is_standing && !self.ents.force_process_control {
            let surface = self.ped(r).current_phys_surface.and_then(|s| self.get(s));
            if surface.is_some_and(|s| s.base.flags.is_in_safe_position) {
                self.get_mut(r).unwrap().base.flags.was_postponed = true;
                return false;
            }
        }
        let travelled = self.ent(r).phys.as_ref().map_or(0.0, |p| p.distance_travelled);
        let pd = self.ped(r);
        let walking = [move_state::WALK, move_state::RUN, move_state::SPRINT].iter().any(|&m| pd.stored_move_state == m as i32);
        if !pd.is_ped_in_control() || pd.wait_state != wait_state::FALSE || 0.01 * step <= travelled || !walking {
            self.ped_mut(r).panic_counter = 0;
        } else if self.ped(r).panic_counter < 50 {
            self.ped_mut(r).panic_counter += 1;
        }
        let pd = self.ped(r);
        if pd.health <= 1.0 && pd.ped_state <= state::STATES_NO_AI && !pd.flags.is_in_the_air && !pd.flags.is_landing {
            self.set_die(px, r, anim::STD_KO_FRONT, 4.0, 0.0);
        }
        self.ped_mut(r).flags.collided_with_my_vehicle = false;
        let (hit, impulse) = self.ent(r).phys.as_ref().map_or((None, 0.0), |p| (p.damage_entity, p.damage_impulse));
        let uses_collision = uses_collision_now(self, r);
        match hit {
            Some(hit) if uses_collision && impulse > 0.0 && self.ped(r).ped_state != state::DIE => self.process_hit_something(px, r, hit),
            _ => {
                let pd = self.ped_mut(r);
                pd.flags.hit_something_last_frame = false;
                if pd.ped_state_timer <= 500 && pd.flags.is_in_the_air {
                    pd.ped_state_timer = pd.ped_state_timer.saturating_sub(1);
                } else if pd.ped_state_timer < 1001 {
                    pd.ped_state_timer = 0;
                }
            }
        }
        if self.ped(r).flags.is_ducking {
            self.duck(r);
        }
        if self.ped(r).flags.start_wander_path_on_foot {
            if self.ped(r).is_ped_in_control() {
                self.clear_all(px, r);
                let dir = self.ped(r).path_dir;
                self.set_wander_path(px, r, dir);
                self.ped_mut(r).flags.start_wander_path_on_foot = false;
            } else if self.ped(r).ped_state == state::DRIVING {
                self.ped_mut(r).flags.wander_path_after_exiting_car = true;
                if let Some(veh) = self.ped(r).my_vehicle {
                    self.set_objective_entity(px, r, objective::LEAVE_CAR, veh);
                }
            }
        }
        if !self.ped(r).flags.is_standing && self.ent(r).phys.as_ref().is_some_and(|p| p.move_speed.z > 0.25) {
            self.phys_mut(r).move_speed *= 0.95_f32.powf(step);
        }
        let e = self.get_mut(r).unwrap();
        physical::process_control(&mut e.base, e.phys.as_mut().expect("ped physical"), step);

        let pd = self.ped(r);
        if pd.ped_state == state::DIE && !pd.flags.is_ped_die_anim_playing {
            let in_water = self.ent(r).phys.as_ref().is_some_and(|p| p.flags.is_in_water);
            if pd.flags.is_standing && (pd.current_phys_surface.is_none() || pd.is_player()) || in_water || !uses_collision_now(self, r) {
                self.set_dead(px, r);
            }
            self.ped_mut(r).current_phys_surface = None;
            return true;
        }
        if self.ped(r).ped_state != state::DEAD {
            self.calculate_new_velocity(px, r);
            self.calculate_new_orientation(r);
        }
        self.update_position(r);
        self.play_foot_steps(px, r);
        let pd = self.ped(r);
        let no_damage_entity = self.ent(r).phys.as_ref().is_none_or(|p| p.damage_entity.is_none());
        if pd.is_ped_in_control() && !pd.flags.is_standing && no_damage_entity && self.check_if_in_the_air(r) {
            self.set_in_the_air(px, r);
        }
        self.process_objective(px, r);
        let pd = self.ped(r);
        if pd.flags.is_aiming_gun {
            self.aim_gun(px, r);
        } else if pd.flags.is_restoring_gun {
            self.restore_gun_position(r);
        }
        let pd = self.ped(r);
        if pd.flags.is_looking {
            self.move_head_to_look(px, r);
        } else if pd.flags.is_restoring_look {
            self.restore_head_position(r);
        }
        if self.ped(r).flags.is_in_the_air {
            self.in_the_air(px, r);
        }
        let pd = self.ped_mut(r);
        if pd.flags.update_anim_heading && pd.ped_state != state::GETUP && pd.ped_state != state::FALL {
            pd.rotation_cur -= FRAC_PI_2;
            pd.rotation_dest = pd.rotation_cur;
            pd.flags.update_anim_heading = false;
        }
        if self.ped(r).wait_state != wait_state::FALSE {
            self.wait(px, r);
        }
        if self.ped(r).ped_state != state::IDLE
            && let Some(a) = self.ped_assoc_mut(r, anim::STD_IDLE_BIGGUN)
        {
            a.blend_delta = -8.0;
            a.flags |= aflags::DELETE_FADED_OUT;
        }
        if self.ped(r).is_player() {
            self.cancel_car_enter(px, r);
        }
        if !self.process_state(px, r) {
            return false;
        }
        self.set_move_anim(px, r);
        if self.ped(r).flags.ped_is_bleeding && px.hooks.nasty_game() && self.ents.timer.frame_counter & 3 == 0 {
            let pos = self.epos(r);
            if (pos - px.hooks.camera_position()).length_squared() < 50.0 * 50.0 {
                let size = (px.rng.number() & 127) as f32 * 0.0015 + 0.15;
                let blood = Vec3::new(((px.rng.number() & 127) as f32 - 64.0) * 0.007, ((px.rng.number() & 127) as f32 - 64.0) * 0.007, 1.0) + pos;
                px.hooks.blood_pool(r, blood, size, true);
            }
        }
        self.service_talking(px, r);
        if self.ped(r).in_vehicle && self.ped(r).my_vehicle.is_none() {
            self.ped_mut(r).in_vehicle = false;
        }
        self.ped_mut(r).current_phys_surface = None;
        true
    }

    /// The `else` branch of the collision test: the ped was pushed by something last frame.
    fn process_hit_something(&mut self, px: &mut Px, r: EntityRef, hit: EntityRef) {
        if self.ped(r).panic_counter == 50 && self.ped(r).is_ped_in_control() {
            self.set_wait_state(px, r, wait_state::STUCK, None);
        } else {
            self.respond_to_collision(px, r, hit);
        }
        let (normal, in_air) = (self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.damage_normal), self.ped(r).flags.is_in_the_air);
        if !in_air && self.ped(r).ped_state != state::JUMP {
            let mut force = Vec3::new(normal.x, normal.y, 0.0);
            force *= if self.ped(r).flags.is_standing { 0.5 } else { 4.0 };
            self.phys_mut(r).apply_move_force(force);
        }
        if self.ped(r).flags.is_in_the_air && !self.ped(r).dying_or_dead() {
            self.ped_in_air_collision(px, r);
        } else if self.ped(r).ped_state_timer < 1001 {
            self.ped_mut(r).ped_state_timer = 0;
        }
    }

    /// A ped that hit something while flying: bounce off a wall, keep steering, or land.
    fn ped_in_air_collision(&mut self, px: &mut Px, r: EntityRef) {
        let pos = self.epos(r);
        let step = self.time_step().max(0.01);
        let timer = self.ped(r).ped_state_timer;
        let mut force_dir = Vec3::ZERO;
        if timer > 0 && timer <= 1000 {
            force_dir = pos - self.ped(r).hit_last_pos;
        } else {
            let pd = self.ped_mut(r);
            pd.ped_state_timer = 0;
            pd.hit_last_pos = pos;
        }
        self.ped_mut(r).ped_state_timer += 1;
        let timer = self.ped(r).ped_state_timer as f32;
        let records = self.ent(r).phys.as_ref().map_or(vec![], |p| p.collision_records.clone());
        let normal = self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.damage_normal);
        let early = timer <= 50.0 / (4.0 * step) || timer * 0.01 <= force_dir.length_squared();
        let few = records.len() <= 1 || timer <= 50.0 / (2.0 * step) || timer / 250.0 <= force_dir.z.abs();
        if early && few {
            let building = records.len() == 1 && self.get(records[0]).is_some_and(|e| e.base.ty == crate::entity::EntityType::Building);
            if building && timer > 50.0 / (2.0 * step) && timer / 250.0 > force_dir.z.abs() {
                self.knock_off_wall(px, r, force_dir);
            } else if normal.z > 0.4 {
                let mut away = normal;
                away.z = 0.0;
                self.phys_mut(r).apply_move_force(2.0 * normalise(away));
            }
        } else if (self.ents.timer.frame_counter.wrapping_add(self.ent(r).base.random_seed as u32 % 256).wrapping_add(3)) & 7 != 0 {
            self.steer_in_air(px, r);
        } else {
            let from = Vec3::new(pos.x, pos.y, pos.z + 0.5);
            let kinds = Kinds { buildings: true, vehicles: true, objects: true, ..Kinds::NONE };
            if let Some(h) = self.process_vertical_line(from, pos.z - FEET_OFFSET, kinds, false, None) {
                self.get_mut(r).unwrap().base.matrix.pos.z = FEET_OFFSET + h.point.point.z;
                self.set_landing(px, r);
                self.ped_mut(r).flags.is_standing = true;
            }
        }
    }

    /// The ped is flying along a wall: push it off sideways and drop it.
    fn knock_off_wall(&mut self, px: &mut Px, r: EntityRef, force_dir: Vec3) {
        let pos = self.epos(r);
        let mut offset = normalise(Vec3::new(-force_dir.y, force_dir.x, 0.0));
        let buildings = Kinds { buildings: true, ..Kinds::NONE };
        let z_forward = self.process_vertical_line(pos + offset, -20.0, buildings, false, None).map_or(500.0, |h| h.point.point.z);
        let z_back = self.process_vertical_line(pos - offset, -20.0, buildings, false, None).map_or(501.0, |h| h.point.point.z);
        if z_forward < z_back {
            offset *= -1.0;
        }
        offset.z = 1.0;
        let mut push = 4.0 * offset;
        push.z = 4.0;
        self.phys_mut(r).apply_move_force(push);
        // FIX_BUGS: shoving the player into the wall kills them
        if !self.ped(r).is_player() {
            self.get_mut(r).unwrap().base.matrix.pos += 0.25 * offset;
        }
        let cur = limit_radian_angle(radian_angle_between_points(offset.x, offset.y, 0.0, 0.0));
        let pd = self.ped_mut(r);
        pd.rotation_cur = cur;
        pd.rotation_dest = cur;
        self.get_mut(r).unwrap().base.set_heading(cur);
        if self.ped(r).ped_state != state::FALL && !self.ped(r).flags.is_ped_die_anim_playing {
            self.set_fall(px, r, 1000, anim::STD_HIGHIMPACT_BACK, true);
        }
        self.ped_mut(r).flags.is_in_the_air = false;
    }

    /// The player steers or jumps while in the air; anyone else just stops gliding.
    fn steer_in_air(&mut self, px: &mut Px, r: EntityRef) {
        let is_player = self.ped(r).is_player();
        let pos = self.epos(r);
        let turn_to_pad = |w: &mut World, px: &mut Px| {
            let (x, y) = px.hooks.ped_walk_axes();
            if x.abs() > 0.0 || y.abs() > 0.0 {
                let dest = limit_radian_angle(radian_angle_between_points(0.0, 0.0, -x, y) - px.hooks.camera_orientation());
                let pd = w.ped_mut(r);
                pd.rotation_dest = dest;
                pd.rotation_cur = dest;
                w.get_mut(r).unwrap().base.set_heading(dest);
            }
        };
        if is_player && self.ped(r).ped_state != state::JUMP && px.hooks.jump_just_down() {
            turn_to_pad(self, px);
            self.set_jump(r);
            let pd = self.ped_mut(r);
            pd.ped_state_timer = 0;
            pd.hit_last_pos = pos;
            return;
        }
        if is_player {
            turn_to_pad(self, px);
        }
        if let Some(a) = self.ped_assoc_mut(r, anim::STD_JUMP_GLIDE) {
            a.blend_delta = -3.0;
            a.flags |= aflags::DELETE_FADED_OUT;
        } else if let Some(a) = self.ped_assoc_mut(r, anim::STD_FALL_GLIDE) {
            a.blend_delta = -3.0;
            a.flags |= aflags::DELETE_FADED_OUT;
        }
        if is_player && self.ped(r).ped_state == state::JUMP {
            self.ped_mut(r).ped_state = state::IDLE;
        }
    }

    /// `CANCELLABLE_CAR_ENTER`: the player walks away from a door or a jack.
    fn cancel_car_enter(&mut self, px: &mut Px, r: EntityRef) {
        let Some(vehicle_anim) = self.ped(r).vehicle_anim.filter(|_| self.ped(r).entering_car()) else {
            self.ents.cancel_jack = false;
            return;
        };
        if px.hooks.player_controls_disabled() {
            return;
        }
        let id = vehicle_anim as u32;
        let time_left = |w: &World| w.ped_assoc(r, id).map_or(0.0, |a| a.time_left());
        let (x, y) = px.hooks.ped_walk_axes();
        let door = [anim::STD_CAR_OPEN_DOOR_LHS, anim::STD_CAR_OPEN_DOOR_RHS, anim::STD_COACH_OPEN_LHS, anim::STD_COACH_OPEN_RHS, anim::STD_VAN_OPEN_DOOR_REAR_LHS, anim::STD_VAN_OPEN_DOOR_REAR_RHS].contains(&id);
        let pull_out = [anim::STD_CAR_PULL_OUT_PED_LHS, anim::STD_CAR_PULL_OUT_PED_LO_LHS, anim::STD_CAR_PULL_OUT_PED_LO_RHS, anim::STD_CAR_PULL_OUT_PED_RHS].contains(&id);
        if x.abs() > 0.0 || y.abs() > 0.0 {
            if door {
                let occupied = self.ped(r).my_vehicle.and_then(|v| self.try_veh(v)).is_some_and(|v| v.driver.is_some());
                self.ents.cancel_jack = occupied;
                if !occupied {
                    self.ped_mut(r).flags.cancel_entering_car = true;
                }
            } else if id == anim::STD_QUICKJACK && time_left(self) > 0.75 {
                self.ents.cancel_jack = true;
            } else if pull_out {
                self.ped_mut(r).flags.cancel_entering_car = true;
                self.ents.cancel_jack = false;
            }
        }
        let left = time_left(self);
        if self.ents.cancel_jack && id == anim::STD_QUICKJACK && left > 0.75 && left < 0.78 {
            self.ents.cancel_jack = false;
            self.quit_entering_car(px, r);
            self.restore_previous_objective(px, r);
        }
        if self.ents.cancel_jack && pull_out {
            self.ents.cancel_jack = false;
            self.ped_mut(r).flags.cancel_entering_car = true;
        }
    }

    /// The `switch (m_nPedState)` of `ProcessControl`. False if the ped was flagged for removal.
    fn process_state(&mut self, px: &mut Px, r: EntityRef) -> bool {
        let now = self.now();
        match self.ped(r).ped_state {
            state::IDLE => self.idle(px, r),
            state::LOOK_ENTITY | state::LOOK_HEADING => {} // `CPed::Look` is empty
            state::WANDER_RANGE => {
                self.wander_range(px, r);
                self.check_around_for_possible_collisions(r);
            }
            state::WANDER_PATH => self.wander_path(px, r),
            state::FLEE_POS => {
                let pd = self.ped(r);
                self.ents.flee_position = glam::Vec2::new(pd.flee_from_pos_x, pd.flee_from_pos_y);
                self.flee(px, r);
            }
            state::FLEE_ENTITY => {
                let Some(from) = self.ped(r).flee_from else {
                    self.set_idle(px, r);
                    return true;
                };
                if now > self.ped(r).ped_state_timer {
                    let p = self.epos(from);
                    self.ents.flee_position = glam::Vec2::new(p.x, p.y);
                    self.flee(px, r);
                }
            }
            state::FOLLOW_PATH => self.follow_path(px, r),
            state::PAUSE => self.pause(px, r),
            state::ATTACK => self.attack(px, r),
            state::FIGHT => self.fight(px, r),
            state::CHAT => self.chat(px, r),
            state::AIM_GUN => {
                let target = self.ped(r).point_gun_at;
                if let Some(t) = target.filter(|&t| self.is_ped(t)) {
                    let pd = self.ped(r);
                    let melee = pd.weapons[pd.current_weapon as usize].is_type_melee();
                    if !melee && self.can_see_entity(t, r, CAN_SEE_ENTITY_ANGLE_THRESHOLD * 2.0) {
                        self.react_to_point_gun(px, t, r);
                    }
                }
                self.point_gun_at(r);
            }
            state::SEEK_CAR => self.seek_car(px, r),
            state::SEEK_IN_BOAT => self.seek_boat_position(px, r),
            state::INVESTIGATE => self.investigate_event(px, r),
            state::ON_FIRE => {
                if self.ped(r).is_player() {
                    return true;
                }
                if now <= self.ped(r).flee_timer {
                    let pd = self.ped(r);
                    let p = match pd.flee_from {
                        Some(f) => {
                            let p = self.epos(f);
                            glam::Vec2::new(p.x, p.y)
                        }
                        None => glam::Vec2::new(pd.flee_from_pos_x, pd.flee_from_pos_y),
                    };
                    self.ents.flee_position = p;
                    self.flee(px, r);
                } else if let Some(fire) = self.ped(r).fire {
                    self.extinguish_fire_index(px, fire);
                }
            }
            state::FALL => self.fall(px, r),
            state::GETUP => self.set_get_up(px, r),
            state::ENTER_TRAIN => self.enter_train(r),
            state::EXIT_TRAIN => self.exit_train(r),
            state::DRIVING => return self.process_driving(px, r),
            state::DIE => {} // `CPed::Die` is empty
            state::HANDS_UP => self.process_hands_up(px, r),
            _ => {}
        }
        true
    }

    fn process_hands_up(&mut self, px: &mut Px, r: EntityRef) {
        let temper = self.ents.ped_stats.0[self.ped(r).ped_stats].temper;
        let (anim_id, said) = if temper <= 50 { (anim::STD_HANDSCOWER, sound::PED_HANDS_COWER) } else { (anim::STD_HANDSUP, sound::PED_HANDS_UP) };
        if self.ped_assoc(r, anim_id).is_none() {
            self.ped_add_anim(r, assoc_group::STD, anim_id);
            self.say(px, r, said);
        }
    }

    /// `case PED_DRIVING`: the driver's hands follow the wheel. False if the car is gone.
    fn process_driving(&mut self, px: &mut Px, r: EntityRef) -> bool {
        let Some(veh) = self.ped(r).my_vehicle else {
            self.ped_mut(r).in_vehicle = false;
            self.flag_to_destroy_when_next_processed(r);
            return false;
        };
        let Some(vd) = self.try_veh(veh) else { return true };
        let (driver, boat, bike, low, steer, gas) = (vd.driver, vd.is_boat(), vd.is_bike(), vd.flags.low_vehicle, vd.steer_angle, vd.gas_pedal);
        if driver != Some(r) || boat {
            self.look_for_sexy_peds(px, r);
            self.look_for_sexy_cars(r);
            return true;
        }
        if bike || !self.ped(r).is_player() {
            return true;
        }
        let (sit, drive_left, drive_right, look_behind) = if low {
            (anim::STD_CAR_SIT_LO, anim::STD_CAR_DRIVE_LEFT_LO, anim::STD_CAR_DRIVE_RIGHT_LO, None)
        } else {
            (anim::STD_CAR_SIT, anim::STD_CAR_DRIVE_LEFT, anim::STD_CAR_DRIVE_RIGHT, Some(anim::STD_CAR_LOOKBEHIND))
        };
        if self.ped_assoc(r, sit).is_none_or(|a| a.blend_amount < 1.0) {
            return true;
        }
        let looking_left = px.hooks.camera_first_person_looking_left();
        if looking_left && let Some(a) = look_behind.and_then(|id| self.ped_assoc_mut(r, id)) {
            a.blend_delta = -1000.0;
        }
        let drive_by = self.ped_assoc(r, anim::STD_CAR_DRIVEBY_LEFT).is_some() || self.ped_assoc(r, anim::STD_CAR_DRIVEBY_RIGHT).is_some();
        if low || gas >= 0.0 || drive_by {
            let set_blend = |w: &mut World, id: u32, amount: f32| match w.ped_assoc_mut(r, id) {
                Some(a) => {
                    a.blend_amount = amount;
                    true
                }
                None => false,
            };
            if steer == 0.0 || drive_by {
                set_blend(self, drive_left, 0.0);
                set_blend(self, drive_right, 0.0);
            } else if steer <= 0.0 {
                set_blend(self, drive_left, 0.0);
                if !set_blend(self, drive_right, (steer * -100.0 / 61.0).clamp(0.0, 1.0)) {
                    self.ped_add_anim(r, assoc_group::STD, drive_right);
                }
            } else {
                set_blend(self, drive_right, 0.0);
                if !set_blend(self, drive_left, (steer * 100.0 / 61.0).clamp(0.0, 1.0)) {
                    self.ped_add_anim(r, assoc_group::STD, drive_left);
                }
            }
            if let Some(a) = look_behind.and_then(|id| self.ped_assoc_mut(r, id)) {
                a.blend_delta = -4.0;
            }
        } else if let Some(id) = look_behind
            && !looking_left
            && self.ped_assoc(r, id).is_none_or(|a| a.blend_amount < 1.0)
        {
            self.ped_blend_anim(r, assoc_group::STD, id, 4.0);
        }
        true
    }
}

fn uses_collision_now(w: &World, r: EntityRef) -> bool {
    w.ent(r).base.flags.uses_collision
}
