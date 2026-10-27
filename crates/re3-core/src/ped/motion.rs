//! `Ped.cpp`: collision avoidance, idle, pause, fall, in the air, landing, get up, mug and the small timers.
use super::callbacks;
use super::ctx::Px;
use super::data::*;
use super::pending::Hit;
use crate::anim::{assoc_group, flags as aflags, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::EntityRef;
use crate::weapons::info::weapon_type as wt;
use crate::world::{Kinds, LineOpts, World};
use glam::Vec3;
use std::f32::consts::FRAC_PI_2;

impl World {
    /// `CPed::IsPedHeadAbovePos`.
    pub fn is_ped_head_above_pos(&self, r: EntityRef, z_offset: f32) -> bool {
        z_offset + self.epos(r).z < self.ped_node_position(r, node::HEAD).z
    }

    /// `CPedPlacement::IsPositionClearOfCars`: the first building or vehicle in the way.
    pub fn is_position_clear_of_cars(&mut self, pos: Vec3) -> Option<EntityRef> {
        let kinds = Kinds { buildings: true, vehicles: true, ..Kinds::NONE };
        self.test_sphere_against_world(pos, 0.25, None, kinds, false)
    }

    /// `CPed::CheckAroundForPossibleCollisions`.
    pub fn check_around_for_possible_collisions(&mut self, r: EntityRef) {
        if self.now() <= self.ped(r).ped_state_timer {
            return;
        }
        let our_centre = self.bound_centre(r);
        let kinds = Kinds { vehicles: true, objects: true, ..Kinds::NONE };
        for object in self.find_objects_in_range(our_centre, 10.0, true, 6, kinds) {
            if self.ped(r).running_to_phone && self.ents.phones.phone_at_this_position(self.epos(object)) {
                break;
            }
            let obj_centre = self.bound_centre(object);
            let mut radius = self.bound_radius_of(object);
            if radius > 4.5 || radius < 1.0 {
                radius = 1.0;
            }
            // the z difference is ignored, as in the original
            let d = our_centre - obj_centre;
            let diff = d.x * d.x + d.y * d.y;
            if (radius + 1.0) * (radius + 1.0) > diff {
                self.ped_mut(r).rotation_dest += 22.5_f32.to_radians();
            }
        }
    }

    /// `CPed::Idle`.
    pub fn idle(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        if let Some(veh) = self.ped(r).my_vehicle {
            let door = self.ped(r).veh_door;
            let out = self.try_veh(veh).map_or(0, |v| v.getting_out_flags);
            if out != 0 && door != 0 && out & crate::vehicle::automobile::car_door_flag(door as usize) != 0 && self.ped(r).objective != objective::KILL_CHAR_ON_FOOT {
                let door_pos = self.get_position_to_open_car_door(veh, door as u32);
                if (self.epos(r) - door_pos).length_squared() < 0.5 * 0.5 {
                    self.ped_mut(r).set_move_state(move_state::WALK);
                    return;
                }
            }
        }
        let armed_idle = self.ped_assoc(r, anim::STD_IDLE_BIGGUN).is_some();
        let unarmed_idle = self.ped_assoc(r, anim::STD_IDLE).map(|a| a.blend_amount);
        let pd = self.ped(r);
        if pd.move_state == move_state::STILL {
            let cur = pd.weapons[pd.current_weapon as usize].ty;
            let two_handed = pd.weapons[pd.current_weapon as usize].is_type_2_handed();
            let wait_time = if !armed_idle || now <= pd.wait_timer && cur != wt::UNARMED && cur != wt::MOLOTOV && cur != wt::GRENADE {
                if (!two_handed || cur == wt::SHOTGUN) && cur != wt::BASEBALLBAT
                    || unarmed_idle.is_none_or(|b| b <= 0.95)
                    || pd.wait_state != wait_state::FALSE
                    || now <= pd.wait_timer
                {
                    self.ped_mut(r).moved = glam::Vec2::ZERO;
                    return;
                }
                self.ped_blend_anim(r, assoc_group::STD, anim::STD_IDLE_BIGGUN, 3.0);
                px.rng.range_i32(4000, 7500)
            } else {
                if let Some(a) = self.ped_assoc_mut(r, anim::STD_IDLE_BIGGUN) {
                    a.blend_delta = -2.0;
                    a.flags |= aflags::DELETE_FADED_OUT;
                }
                px.rng.range_i32(3000, 8500)
            };
            self.ped_mut(r).wait_timer = now.wrapping_add(wait_time as u32);
        } else {
            if let Some(a) = self.ped_assoc_mut(r, anim::STD_IDLE_BIGGUN) {
                a.blend_delta = -8.0;
                a.flags |= aflags::DELETE_FADED_OUT;
                self.ped_mut(r).wait_timer = 0;
            }
            if !self.ped(r).is_player() {
                self.ped_mut(r).set_move_state(move_state::STILL);
            }
        }
        self.ped_mut(r).moved = glam::Vec2::ZERO;
    }

    /// `CPed::ClearPause`.
    pub fn clear_pause(&mut self, px: &mut Px, r: EntityRef) {
        self.restore_previous_state(px, r);
    }

    /// `CPed::Pause`.
    pub fn pause(&mut self, px: &mut Px, r: EntityRef) {
        self.ped_mut(r).moved = glam::Vec2::ZERO;
        if self.now() > self.ped(r).leave_car_timer {
            self.clear_pause(px, r);
        }
    }

    /// `CPed::SetFall`.
    pub fn set_fall(&mut self, px: &mut Px, r: EntityRef, extra_time: i32, anim_id: u32, even_if_not_in_control: bool) {
        let pd = self.ped(r);
        if !pd.is_ped_in_control() && (!even_if_not_in_control || pd.dying_or_dead()) {
            return;
        }
        self.clear_look_flag(px, r);
        self.clear_aim_flag(r);
        self.set_stored_state(r);
        self.ped_mut(r).ped_state = state::FALL;
        let length = if let Some(a) = self.ped_assoc_mut(r, anim_id) {
            a.set_current_time(0.0);
            a.blend_amount = 0.0;
            a.blend_delta = 8.0;
            a.flags |= aflags::RUNNING;
            Some(a.hierarchy.total_length)
        } else {
            let i = self.ped_blend_anim(r, assoc_group::STD, anim_id, 8.0);
            i.and_then(|i| self.ped_assoc_at(r, i)).map(|a| a.hierarchy.total_length)
        };
        let now = self.now();
        let jitter = (self.ent(r).base.random_seed as u32).wrapping_add(self.ents.timer.frame_counter) % 1000;
        let player = self.ped(r).is_player();
        self.ped_mut(r).get_up_timer = if extra_time == -1 {
            u32::MAX
        } else if let Some(len) = length {
            if player {
                (1000.0 * len + now as f32 + 500.0) as u32
            } else {
                (1000.0 * len + now as f32 + extra_time as f32 + jitter as f32) as u32
            }
        } else {
            (extra_time as u32).wrapping_add(now).wrapping_add(1000).wrapping_add(jitter)
        };
        self.ped_mut(r).flags.fallen_down = true;
    }

    /// `CPed::ClearFall`.
    pub fn clear_fall(&mut self, px: &mut Px, r: EntityRef) {
        self.set_get_up(px, r);
    }

    /// `CPed::Fall`.
    pub fn fall(&mut self, px: &mut Px, r: EntityRef) {
        let t = self.ped(r).get_up_timer;
        if t != u32::MAX && self.now() > t {
            self.clear_fall(px, r);
        }
    }

    /// `CPed::CheckIfInTheAir`.
    pub fn check_if_in_the_air(&mut self, r: EntityRef) -> bool {
        if self.ped(r).in_vehicle {
            return false;
        }
        let mut pos = self.epos(r);
        let kinds = Kinds { buildings: true, vehicles: true, objects: true, ..Kinds::NONE };
        let mut found_ground = self.process_vertical_line(pos, pos.z - 1.54, kinds, false, None).is_some();
        if !found_ground && self.ped(r).ped_state != state::JUMP {
            pos.z -= FEET_OFFSET;
            found_ground = self.test_sphere_against_world(pos, 0.15, Some(r), Kinds { buildings: true, ..Kinds::NONE }, false).is_some();
        }
        !found_ground
    }

    /// `CPed::SetInTheAir`.
    pub fn set_in_the_air(&mut self, px: &mut Px, r: EntityRef) {
        if self.ped(r).flags.is_in_the_air {
            return;
        }
        self.ped_mut(r).flags.is_in_the_air = true;
        self.ped_blend_anim(r, assoc_group::STD, anim::STD_FALL_GLIDE, 4.0);
        match self.ped(r).ped_state {
            state::ATTACK => {
                self.clear_attack(px, r);
                self.clear_point_gun_at(px, r);
            }
            state::FIGHT => self.end_fight(px, r, ENDFIGHT_FAST),
            _ => {}
        }
    }

    /// `CPed::InTheAir`.
    pub fn in_the_air(&mut self, px: &mut Px, r: EntityRef) {
        let our_pos = self.epos(r);
        let bit_below = Vec3::new(our_pos.x, our_pos.y, our_pos.z - 4.04);
        let move_speed_z = self.ent(r).phys.as_ref().map_or(0.0, |p| p.move_speed.z);
        if move_speed_z < 0.0 && !self.ped(r).flags.is_ped_die_anim_playing && !self.ped(r).dying_or_dead() {
            let kinds = Kinds { buildings: true, vehicles: true, objects: true, ..Kinds::NONE };
            if let Some(hit) = self.process_line_of_sight(our_pos, bit_below, kinds, LineOpts::default()) {
                if our_pos.z - hit.point.point.z < 1.3 {
                    self.set_landing(px, r);
                }
            } else if self.ped_assoc(r, anim::STD_FALL).is_none() && move_speed_z < -0.1 {
                self.ped_blend_anim(r, assoc_group::STD, anim::STD_FALL, 4.0);
            }
        }
    }

    /// `CPed::SetLanding`.
    pub fn set_landing(&mut self, px: &mut Px, r: EntityRef) {
        if self.ped(r).dying_or_dead() {
            return;
        }
        let falling = self.ped_assoc(r, anim::STD_FALL).is_some();
        self.ped_set_blend_deltas(r, aflags::PARTIAL, -1000.0);
        let land = if falling {
            let i = self.ped_add_anim(r, assoc_group::STD, anim::STD_FALL_COLLAPSE);
            px.hooks.play_one_shot(r, sound::FALL_COLLAPSE, 1.0);
            if self.ped(r).is_player() {
                self.say(px, r, sound::PED_LAND);
            }
            i
        } else {
            let i = self.ped_add_anim(r, assoc_group::STD, anim::STD_FALL_LAND);
            px.hooks.play_one_shot(r, sound::FALL_LAND, 1.0);
            i
        };
        if let Some(a) = land.and_then(|i| self.ped_assoc_at(r, i)) {
            a.set_finish_callback(callbacks::PED_LAND);
        }
        let pd = self.ped_mut(r);
        pd.flags.is_in_the_air = false;
        pd.flags.is_landing = true;
    }

    /// `CPed::SetGetUp`.
    pub fn set_get_up(&mut self, px: &mut Px, r: EntityRef) {
        let pd = self.ped(r);
        if pd.ped_state == state::GETUP && pd.flags.get_up_anim_started || !pd.can_set_ped_state() {
            return;
        }
        if !(pd.health >= 1.0 || self.is_ped_head_above_pos(r, -0.3)) {
            self.ped_mut(r).health = 0.0;
            self.set_die(px, r, anim::STD_NUM, 4.0, 0.0);
            return;
        }
        if self.ped(r).flags.update_anim_heading {
            let pd = self.ped_mut(r);
            pd.rotation_cur = crate::general::limit_radian_angle(pd.rotation_cur) - FRAC_PI_2;
            pd.flags.update_anim_heading = false;
        }
        if self.ped(r).ped_state != state::GETUP {
            self.set_stored_state(r);
            self.ped_mut(r).ped_state = state::GETUP;
        }
        let pos = self.epos(r);
        let colliding = self.ped(r).colliding_entity;
        let not_bike = |w: &World, v: EntityRef| w.try_veh(v).is_some_and(|d| !d.is_bike());
        // the original reads the vehicle type of whatever is hit, a building included; here only a car counts
        let clear_veh = self.is_position_clear_of_cars(pos).filter(|&v| not_bike(self, v));
        let seed = self.ent(r).base.random_seed as u32;
        let tick = !(self.ents.timer.frame_counter.wrapping_add(seed).wrapping_add(5) as u8).is_multiple_of(8);
        let run_over = clear_veh.is_some()
            || colliding.is_some_and(|v| not_bike(self, v) && (tick || self.ped_overlaps_vehicle(r, v)));
        if run_over {
            self.ped_mut(r).flags.get_up_anim_started = false;
            if self.ped(r).is_player() {
                let step = self.time_step();
                self.inflict_damage(px, r, Hit { by: None, weapon: wt::RUNOVERBYCAR, damage: step, piece: piece::TORSO, direction: 0 });
            } else {
                if !px.hooks.player_controls_disabled() {
                    return;
                }
                self.inflict_damage(px, r, Hit { by: None, weapon: wt::RUNOVERBYCAR, damage: 1000.0, piece: piece::TORSO, direction: 0 });
            }
            return;
        }
        let pd = self.ped_mut(r);
        pd.flags.get_up_anim_started = true;
        pd.colliding_entity = None;
        pd.flags.knocked_up_into_air = false;
        let group = pd.anim_group as usize;
        if self.ped_assoc(r, anim::STD_RUNFAST).is_some() {
            if self.ped_assoc(r, anim::STD_RUN).is_some() {
                self.ped_blend_anim(r, group, anim::STD_RUN, 8.0);
            } else {
                self.ped_blend_anim(r, group, anim::STD_IDLE, 8.0);
            }
            if let Some(a) = self.ped_assoc_mut(r, anim::STD_RUNFAST) {
                a.flags |= aflags::DELETE_FADED_OUT;
            }
        }
        let up = if self.ped_first_assoc(r, aflags::FRONTAL).is_some() { anim::STD_GET_UP_FRONT } else { anim::STD_GET_UP };
        let i = self.ped_blend_anim(r, assoc_group::STD, up, 1000.0);
        if let Some(a) = i.and_then(|i| self.ped_assoc_at(r, i)) {
            a.set_finish_callback(callbacks::PED_GETUP);
        }
    }

    /// `CCollision::ProcessColModels(ped, vehicle) > 0` with the ped's scratch points.
    fn ped_overlaps_vehicle(&self, ped: EntityRef, veh: EntityRef) -> bool {
        let (Some(a), Some(b)) = (self.col_model_of(ped), self.col_model_of(veh)) else { return false };
        let mut points = [crate::collision::ColPoint::default(); 32];
        crate::collision::process_col_models(&self.ent(ped).base.matrix, &a, &self.ent(veh).base.matrix, &b, &mut points, &mut [], &mut []) > 0
    }

    /// `CPed::Mug`.
    pub fn mug(&mut self, px: &mut Px, r: EntityRef) {
        let Some(target) = self.ped(r).seek_target.filter(|&t| self.is_ped(t)) else {
            self.set_idle(px, r);
            return;
        };
        if self.now() <= self.ped(r).attack_timer.wrapping_sub(2000) {
            if (self.epos(target) - self.epos(r)).length() > 3.0 {
                self.ped_mut(r).wep_skills = 50;
            }
            self.say(px, r, sound::PED_MUGGING);
            self.say(px, target, sound::PED_ROBBED);
        } else {
            let dir = (px.rng.number() & 7) as i8;
            self.set_wander_path(px, r, dir);
            self.set_flee(px, r, target, 20000);
        }
    }

    /// `CPed::SetLookTimer`.
    pub fn set_look_timer(&mut self, r: EntityRef, time: i32) {
        let now = self.now();
        if now > self.ped(r).look_timer {
            self.ped_mut(r).look_timer = now.wrapping_add(time as u32);
        }
    }

    /// `CPed::SetAttackTimer`.
    pub fn set_attack_timer(&mut self, r: EntityRef, time: u32) {
        let now = self.now();
        let pd = self.ped_mut(r);
        if now > pd.attack_timer {
            pd.attack_timer = pd.shoot_timer.max(now).wrapping_add(time);
        }
    }

    /// `CPed::SetShootTimer`.
    pub fn set_shoot_timer(&mut self, r: EntityRef, time: u32) {
        let now = self.now();
        if now > self.ped(r).shoot_timer {
            self.ped_mut(r).shoot_timer = now.wrapping_add(time);
        }
    }

    /// `CPed::Teleport`.
    pub fn teleport(&mut self, r: EntityRef, pos: Vec3) {
        self.remove(r);
        self.get_mut(r).expect("ped").base.set_position(pos);
        let pd = self.ped_mut(r);
        pd.flags.is_standing = false;
        pd.ped_state_timer = 0;
        pd.action_x = 0.0;
        pd.action_y = 0.0;
        self.get_mut(r).expect("ped").phys.as_mut().expect("physical").damage_entity = None;
        // a ped without a collision model stays out of the sectors
        self.add(r).ok();
    }
}
