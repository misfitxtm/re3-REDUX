//! `PlayerPed.cpp`, second part: what the pad does to the player each frame.
use super::ctx::{cam_mode, PadState, Px};
use super::data::*;
use crate::anim::{assoc_group, tables_gta3::anim};
use crate::entity::{status, EntityRef};
use crate::general::{limit_angle, limit_radian_angle, radian_angle_between_points};
use crate::math::atan_of_xy;
use crate::vehicle::base::{temp_action, MISSION_VEHICLE};
use crate::weapons::info::{flag as weapon_flag, weapon_type as wt};
use crate::world::{Kinds, World};
use glam::{Vec2, Vec3};
use std::f32::consts::PI;

/// `PAD_MOVE_TO_GAME_WORLD_MOVE`.
const PAD_MOVE_TO_GAME_WORLD_MOVE: f32 = 60.0;
/// `CHECK_NEARBY_THINGS_MAX_DIST`.
const CHECK_NEARBY_THINGS_MAX_DIST: f32 = 15.0;
/// `ASSOC_BLOCK`.
const ASSOC_BLOCK: u32 = 0x40;

impl World {
    fn current_weapon_ty(&self, r: EntityRef) -> usize {
        let pd = self.ped(r);
        pd.weapons[pd.current_weapon as usize].ty
    }

    fn weapon_is_heavy(&self, r: EntityRef) -> bool {
        self.ents.weapon_infos.get(self.current_weapon_ty(r)).is_flag_set(weapon_flag::HEAVY)
    }

    /// `m_nEvadeAmount != 0 && m_pEvadingFrom ? SetEvasiveDive : SetJump`.
    fn dive_or_jump(&mut self, px: &mut Px, r: EntityRef) {
        let p = self.player_data(r);
        match p.evading_from.filter(|_| p.evade_amount != 0) {
            Some(from) => {
                self.set_evasive_dive(px, r, from, true);
                let p = self.player_data_mut(r);
                p.evade_amount = 0;
                p.evading_from = None;
            }
            None => self.set_jump(r),
        }
    }

    /// `CPlayerPed::PlayerControlSniper`.
    pub fn player_control_sniper(&mut self, px: &mut Px, r: EntityRef, pad: &PadState) {
        self.sniper_or_m16_common(px, r, pad);
        if pad.weapon_just_down {
            let fire_pos = self.ent(r).base.matrix.transform_point(Vec3::new(0.0, 0.0, 0.6));
            self.fire_weapon(px, r, fire_pos);
        }
        self.update_current_weapon(r);
    }

    /// `CPlayerPed::PlayerControlM16`.
    pub fn player_control_m16(&mut self, px: &mut Px, r: EntityRef, pad: &PadState) {
        self.sniper_or_m16_common(px, r, pad);
        if pad.weapon != 0 {
            let fire_pos = self.ent(r).base.matrix.transform_point(Vec3::new(0.0, 0.0, 0.6));
            self.fire_weapon(px, r, fire_pos);
        }
        self.update_current_weapon(r);
    }

    fn sniper_or_m16_common(&mut self, px: &mut Px, r: EntityRef, pad: &PadState) {
        self.process_weapon_switch(px, r, pad);
        let stamina = self.player_data(r).current_stamina;
        px.hooks.set_player_exhaustion((1.0 - (stamina + 150.0) / 300.0) * 0.9 + 0.1);
        if !pad.target {
            self.restore_previous_state(px, r);
            px.hooks.clear_player_weapon_mode();
        }
    }

    /// `CPlayerPed::PlayerControlFighter`.
    pub fn player_control_fighter(&mut self, px: &mut Px, r: EntityRef, pad: &PadState) {
        let (lr, ud) = (pad.walk_left_right, pad.walk_up_down);
        let pad_move = Vec2::new(lr, ud).length();
        if pad_move > 0.0 {
            let orientation = px.hooks.camera().orientation;
            let pd = self.ped_mut(r);
            pd.rotation_dest = radian_angle_between_points(0.0, 0.0, -lr, ud) - orientation;
            pd.take_a_step_after_attack = pad_move > 2.0 * PAD_MOVE_TO_GAME_WORLD_MOVE;
            if pad.sprint && pad_move > PAD_MOVE_TO_GAME_WORLD_MOVE {
                pd.flags.is_attacking = false;
            }
        }
        if !self.weapon_is_heavy(r) && pad.jump_just_down {
            self.dive_or_jump(px, r);
        }
    }

    /// The jump boost a player who flew into something gets, shared by both walking modes.
    fn nudge_stuck_jump(&mut self, r: EntityRef) {
        if self.ped(r).ped_state != state::JUMP {
            return;
        }
        if self.ped(r).flags.is_in_the_air {
            let step_secs = self.ents.timer.time_step_in_seconds();
            let (uses_collision, e) = (self.ent(r).base.flags.uses_collision, self.ent(r));
            let p = e.phys.as_ref().expect("physical");
            let pd = self.ped(r);
            if uses_collision && !pd.flags.hit_steep_slope && (!pd.flags.hit_something_last_frame || p.damage_normal.z > 0.6) && p.distance_travelled < step_secs && p.move_speed.length_squared() < 0.01 {
                let (sin, cos) = pd.rotation_cur.sin_cos();
                self.phys_mut(r).apply_move_force(Vec3::new(-sin * 3.0, 3.0 * cos, 0.05));
            }
        } else if self.ped(r).flags.is_landing {
            self.player_data_mut(r).move_speed = 0.0;
        }
    }

    /// The sprint, move animation and jump tail shared by both walking modes.
    fn walking_tail(&mut self, px: &mut Px, r: EntityRef, pad: &PadState) {
        if !self.weapon_is_heavy(r) && pad.sprint {
            self.ped_mut(r).move_state = move_state::SPRINT;
        }
        if self.ped(r).ped_state != state::FIGHT {
            self.set_real_move_anim(px, r);
        }
        if !self.ped(r).flags.is_in_the_air && !self.weapon_is_heavy(r) && pad.jump_just_down && self.ped(r).ped_state != state::JUMP {
            self.clear_attack(px, r);
            self.clear_weapon_target(px, r);
            self.dive_or_jump(px, r);
        }
    }

    /// `CPlayerPed::PlayerControl1stPersonRunAround`.
    pub fn player_control_1st_person_run_around(&mut self, px: &mut Px, r: EntityRef, pad: &PadState) {
        let step = self.time_step();
        let pad_move = Vec2::new(pad.walk_left_right, pad.walk_up_down).length() / PAD_MOVE_TO_GAME_WORLD_MOVE;
        if pad_move > 0.0 {
            let orientation = px.hooks.camera().orientation;
            self.ped_mut(r).rotation_dest = limit_radian_angle(orientation);
            let p = self.player_data_mut(r);
            p.move_speed = pad_move.min(0.07 * step + p.move_speed);
        } else {
            self.player_data_mut(r).move_speed = 0.0;
        }
        self.nudge_stuck_jump(r);
        self.walking_tail(px, r, pad);
    }

    /// `CPlayerPed::PlayerControlZelda`.
    pub fn player_control_zelda(&mut self, px: &mut Px, r: EntityRef, pad: &PadState) {
        let cam = px.hooks.camera();
        let step = self.time_step();
        let mouse = cam.free_cam && cam.using_3rd_person_mouse_cam;
        let smooth_spray = self.do_weapon_smooth_spray(r, mouse);
        let (lr, ud) = (pad.walk_left_right, pad.walk_up_down);
        let (mut pad_move, mut spray_without_move) = (0.0, false);
        if smooth_spray && ud > 0.0 {
            spray_without_move = true;
        } else {
            pad_move = Vec2::new(lr, ud).length() / PAD_MOVE_TO_GAME_WORLD_MOVE;
        }
        if mouse && smooth_spray {
            pad_move = 0.0;
            spray_without_move = false;
        }
        if pad_move > 0.0 || spray_without_move {
            let needed_turn = limit_radian_angle(radian_angle_between_points(0.0, 0.0, -lr, ud) - cam.orientation);
            if smooth_spray {
                let ty = self.current_weapon_ty(r);
                let divisor = if matches!(ty, wt::FLAMETHROWER | wt::COLT45 | wt::UZI) { 80.0 } else { 128.0 };
                let pd = self.ped_mut(r);
                pd.rotation_dest = pd.rotation_cur - lr / 128.0 * (PI / divisor) * step;
            } else {
                self.ped_mut(r).rotation_dest = needed_turn;
            }
            let max_acc = 0.07 * step;
            let p = self.player_data_mut(r);
            p.move_speed = pad_move.min(p.move_speed + max_acc);
        } else {
            self.player_data_mut(r).move_speed = 0.0;
        }
        self.nudge_stuck_jump(r);
        self.walking_tail(px, r, pad);
    }

    /// `CPlayerPed::KeepAreaAroundPlayerClear`: peds and cars near the player get out of the way.
    pub fn keep_area_around_player_clear(&mut self, px: &mut Px, r: EntityRef) {
        self.build_ped_lists(r);
        let pd = self.ped(r);
        let near: Vec<EntityRef> = pd.near_peds.iter().take(pd.num_near_peds as usize).flatten().copied().collect();
        for n in near {
            let Some(d) = self.try_ped(n) else { continue };
            if d.char_created_by != RANDOM_CHAR || d.dying_or_dead() {
                continue;
            }
            if px.hooks.is_on_screen(n) {
                if d.objective == objective::NONE {
                    self.set_find_path_and_flee(px, n, r, 5000, true);
                } else {
                    if d.entering_car() {
                        self.quit_entering_car(px, n);
                    }
                    self.clear_objective(px, n);
                }
            } else {
                self.flag_to_destroy_when_next_processed(n);
            }
        }
        let player_pos = self.player_vehicle().map_or_else(|| self.epos(r), |v| self.epos(v));
        let now = self.now();
        let cars = self.find_objects_in_range(self.epos(r), CHECK_NEARBY_THINGS_MAX_DIST, true, 6, Kinds { vehicles: true, ..Kinds::NONE });
        for v in cars {
            let st = self.ent(v).base.status;
            let Some(vd) = self.try_veh(v) else { continue };
            if vd.created_by == MISSION_VEHICLE || st == status::PLAYER || st == status::PLAYER_DISABLED {
                continue;
            }
            let (vpos, forward) = (self.epos(v), self.ent(v).base.matrix.forward);
            let (action, wait) = if (vpos - player_pos).length_squared() > 25.0 {
                (temp_action::WAIT, 5000)
            } else if (player_pos - vpos).truncate().dot(forward.truncate()) > 0.0 {
                (temp_action::REVERSE, 2000)
            } else {
                (temp_action::GOFORWARD, 2000)
            };
            let ap = &mut self.try_veh_mut(v).expect("vehicle").auto_pilot;
            ap.temp_action = action;
            ap.time_temp_action = now + wait;
            px.hooks.possibly_remove_vehicle(v);
        }
    }

    /// `CPlayerPed::EvaluateNeighbouringTarget`.
    fn evaluate_neighbouring_target(&self, r: EntityRef, candidate: EntityRef, best: &mut Option<(EntityRef, f32)>, dist_limit: f32, angle_offset: f32, look_to_left: bool) {
        let dist = self.epos(candidate) - self.epos(r);
        if dist.truncate().length() > dist_limit || self.does_target_have_to_be_broken(r, self.epos(candidate)) {
            return;
        }
        let angle = limit_angle(atan_of_xy(dist.x, dist.y) - angle_offset);
        let closeness = match (look_to_left, angle > 0.0) {
            (true, true) | (false, false) => -angle.abs(),
            _ => -100000.0,
        };
        if best.is_none_or(|(_, c)| closeness > c) {
            *best = Some((candidate, closeness));
        }
    }

    /// `CPlayerPed::EvaluateTarget`.
    fn evaluate_target(&self, r: EntityRef, candidate: EntityRef, best: &mut Option<(EntityRef, f32)>, dist_limit: f32, angle_offset: f32, priority: bool) {
        let dist = self.epos(candidate) - self.epos(r);
        let flat = dist.truncate().length();
        if flat > dist_limit || self.does_target_have_to_be_broken(r, self.epos(candidate)) {
            return;
        }
        let angle = limit_angle(atan_of_xy(dist.x, dist.y) - angle_offset);
        let mut closeness = -flat - 5.0 * angle.abs();
        if priority {
            closeness += 5.0;
        }
        if best.is_none_or(|(_, c)| closeness > c) {
            *best = Some((candidate, closeness));
        }
    }

    /// The peds and objects the player could lock onto, in the original's order.
    fn lock_on_candidates(&self, r: EntityRef, skip: Option<EntityRef>) -> Vec<EntityRef> {
        let out: Vec<EntityRef> = self
            .ped_refs()
            .into_iter()
            .rev()
            .filter(|&p| p != r && Some(p) != skip && self.try_ped(p).is_some_and(|d| !d.dying_or_dead() && !d.in_vehicle && d.leader != Some(r)))
            .collect();
        out
    }

    /// `CPlayerPed::FindNextWeaponLockOnTarget`.
    pub fn find_next_weapon_lock_on_target(&mut self, px: &mut Px, r: EntityRef, previous: EntityRef, look_to_left: bool) -> bool {
        let range = self.ents.weapon_infos.get(self.current_weapon_ty(r)).range;
        let dist = self.epos(previous) - self.epos(r);
        let reference = atan_of_xy(dist.x, dist.y);
        let mut best = None;
        for ped in self.lock_on_candidates(r, Some(previous)) {
            if self.our_ped_can_see_this_one(r, ped) {
                self.evaluate_neighbouring_target(r, ped, &mut best, range, reference, look_to_left);
            }
        }
        for obj in self.player_data(r).targettable_objects.into_iter().flatten().filter(|&o| self.get(o).is_some()) {
            self.evaluate_neighbouring_target(r, obj, &mut best, range, reference, look_to_left);
        }
        let Some((target, _)) = best else { return false };
        self.set_weapon_lock_on_target(r, Some(target));
        self.set_point_gun_at(px, r, target);
        true
    }

    /// `CPlayerPed::FindWeaponLockOnTarget`.
    pub fn find_weapon_lock_on_target(&mut self, px: &mut Px, r: EntityRef) -> bool {
        let range = self.ents.weapon_infos.get(self.current_weapon_ty(r)).range;
        if let Some(t) = self.ped(r).point_gun_at {
            if (self.epos(t) - self.epos(r)).truncate().length() > range {
                self.set_weapon_lock_on_target(r, None);
                return false;
            }
            return true;
        }
        let fwd = self.ent(r).base.matrix.forward;
        let reference = atan_of_xy(fwd.x, fwd.y);
        let mut best = None;
        for ped in self.lock_on_candidates(r, None) {
            if self.our_ped_can_see_this_one(r, ped) {
                let attacking = self.is_this_ped_attacking_player(r, ped);
                self.evaluate_target(r, ped, &mut best, range, reference, attacking);
            }
        }
        for obj in self.player_data(r).targettable_objects.into_iter().flatten().filter(|&o| self.get(o).is_some()) {
            self.evaluate_target(r, obj, &mut best, range, reference, false);
        }
        let Some((target, _)) = best else { return false };
        self.set_weapon_lock_on_target(r, Some(target));
        self.set_point_gun_at(px, r, target);
        true
    }

    /// `CPlayerPed::ProcessPlayerWeapon`.
    pub fn process_player_weapon(&mut self, px: &mut Px, r: EntityRef, pad: &PadState) {
        let cam = px.hooks.camera();
        let step_clipped = self.ents.timer.time_step_non_clipped;
        let ty = self.current_weapon_ty(r);
        let (fire, flags) = {
            let info = self.ents.weapon_infos.get(ty);
            (info.fire, info.flags)
        };
        let flag = |f: u32| flags & f != 0;
        if self.player_data(r).has_lock_on_target && self.ped(r).point_gun_at.is_none() {
            px.hooks.clear_player_weapon_mode();
            px.hooks.clear_cross_hair();
            self.clear_point_gun_at(px, r);
        }
        if self.ped(r).fire.is_none() && matches!(ty, wt::ROCKETLAUNCHER | wt::SNIPERRIFLE | wt::M16) {
            if pad.target_just_down {
                self.set_stored_state(r);
                self.ped_mut(r).ped_state = state::SNIPER_MODE;
                if cam.free_cam && cam.using_3rd_person_mouse_cam {
                    let rot = limit_radian_angle(-cam.orientation);
                    self.ped_mut(r).rotation_cur = rot;
                    self.get_mut(r).expect("player").base.set_heading(rot);
                }
                px.hooks.set_new_player_weapon_mode(match ty {
                    wt::ROCKETLAUNCHER => cam_mode::ROCKETLAUNCHER,
                    wt::SNIPERRIFLE => cam_mode::SNIPER,
                    _ => cam_mode::M16_1STPERSON,
                });
                self.player_data_mut(r).move_speed = 0.0;
                self.ped_blend_anim(r, assoc_group::STD, anim::STD_IDLE, 1000.0);
            }
            if matches!(ty, wt::ROCKETLAUNCHER | wt::SNIPERRIFLE) || px.hooks.camera().player_weapon_mode == cam_mode::M16_1STPERSON {
                return;
            }
        }
        let not_sprinting = self.ped(r).move_state != move_state::SPRINT;
        let selected_is_current = self.player_data(r).selected_wep_slot == self.ped(r).current_weapon as usize;
        if pad.weapon != 0 && not_sprinting {
            if selected_is_current {
                if let Some(target) = self.ped(r).point_gun_at {
                    if cam.free_cam && fire == crate::weapons::info::WeaponFire::Melee && self.player_data(r).move_speed < 1.0 {
                        self.start_fight_attack(px, r, pad.weapon);
                    } else {
                        self.set_attack(px, r, target);
                    }
                } else if self.ped(r).current_weapon as usize != wt::UNARMED {
                    if self.ped(r).ped_state == state::ATTACK {
                        if pad.weapon_just_down {
                            self.player_data_mut(r).have_target_selected = true;
                        } else if !self.player_data(r).have_target_selected {
                            self.player_data_mut(r).attack_button_counter += step_clipped;
                        }
                    } else {
                        let p = self.player_data_mut(r);
                        p.attack_button_counter = 0.0;
                        p.have_target_selected = false;
                    }
                    self.set_attack(px, r, None);
                } else if pad.weapon_just_down {
                    if self.player_data(r).move_speed < 1.0 {
                        self.start_fight_attack(px, r, pad.weapon);
                    } else {
                        self.set_attack(px, r, None);
                    }
                }
            }
        } else {
            self.ped_mut(r).ik.flags &= !super::ik::ik_flag::LOOKAROUND_HEAD_ONLY;
            if self.ped(r).ped_state == state::ATTACK {
                self.player_data_mut(r).have_target_selected = true;
                self.ped_mut(r).flags.is_attacking = false;
            }
        }
        // shortcut: the free cam's arm aiming (FREE_CAM) is left to the camera port; the classic lock-on is complete
        if pad.target && selected_is_current && not_sprinting {
            if let Some(target) = self.ped(r).point_gun_at {
                let in_car = self.try_ped(target).is_some_and(|d| d.in_vehicle);
                if (!cam.free_cam && cam.use_mouse_3rd_person) || in_car || self.does_target_have_to_be_broken(r, self.epos(target)) {
                    self.clear_weapon_target(px, r);
                    return;
                }
                if pad.shift_target_left_just_down {
                    self.find_next_weapon_lock_on_target(px, r, target, true);
                }
                if pad.shift_target_right_just_down {
                    self.find_next_weapon_lock_on_target(px, r, target, false);
                }
                px.hooks.set_new_player_weapon_mode(cam_mode::SYPHON);
                px.hooks.update_aiming_coors(self.epos(target));
            } else if (cam.free_cam && fire == crate::weapons::info::WeaponFire::Melee || flag(weapon_flag::CANAIM) && !cam.use_mouse_3rd_person) && pad.target_just_down {
                self.find_weapon_lock_on_target(px, r);
            }
        } else if self.ped(r).point_gun_at.is_some() {
            self.clear_weapon_target(px, r);
        }
        if let Some(target) = self.ped(r).point_gun_at {
            let can_point = self.ped(r).flags.can_point_gun_at_target;
            px.hooks.mark_target(self.epos(target), if can_point { 0 } else { 32 });
        }
        let locked = self.ped(r).point_gun_at.is_some();
        self.player_data_mut(r).has_lock_on_target = locked;
    }

    /// `CPlayerPed::ProcessControl`.
    pub fn player_process_control(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let p = self.player_data_mut(r);
        p.evade_amount = p.evade_amount.saturating_sub(1);
        if p.evade_amount == 0 {
            p.evading_from = None;
        }
        let on_boat = self.ped(r).current_phys_surface.is_some_and(|s| self.try_veh(s).is_some_and(|v| v.is_boat()));
        let seed = self.ent(r).base.random_seed as u32;
        if on_boat {
            self.ped_mut(r).flags.trying_to_reach_dry_land = true;
        } else if (self.ents.timer.frame_counter.wrapping_add(seed)) & 0xF == 0 {
            let pos = self.epos(r);
            let near = self.test_sphere_against_world(pos, 7.0, None, Kinds { vehicles: true, ..Kinds::NONE }, false);
            self.ped_mut(r).flags.trying_to_reach_dry_land = near.is_some_and(|v| self.try_veh(v).is_some_and(|d| d.is_boat()));
        }
        self.process_control(px, r);
        if self.ent(r).base.flags.was_postponed {
            return;
        }
        let pad = px.hooks.pad();
        self.update_player_wanted(px);
        let ms = self.ped(r).move_state;
        if ms != move_state::RUN && ms != move_state::SPRINT {
            self.restore_sprint_energy(r, 1.0);
        } else if ms == move_state::RUN {
            self.restore_sprint_energy(r, 0.3);
        }
        match self.ped(r).ped_state {
            state::DEAD => {
                self.clear_weapon_target(px, r);
                return;
            }
            state::DIE => {
                self.clear_weapon_target(px, r);
                if now > self.ped(r).bloody_footprint_count_or_death_time + 4000 {
                    self.set_dead(px, r);
                }
                return;
            }
            state::DRIVING if self.ped(r).objective != objective::LEAVE_CAR => {
                self.player_door_rolling(px, r, &pad);
                return;
            }
            _ => {}
        }
        if self.ped(r).objective == objective::NONE {
            self.ped_mut(r).move_state = move_state::STILL;
        }
        if self.ped(r).flags.is_landing {
            self.running_land(px, r, &pad);
        }
        if pad.weapon_just_down && self.ped(r).ped_state != state::SNIPER_MODE
            && let Some(s) = self.no_zoom_shot_sound(r)
        {
            px.hooks.play_front_end_sound(s);
        }
        let cam = px.hooks.camera();
        match self.ped(r).ped_state {
            state::NONE | state::IDLE | state::FLEE_POS | state::FLEE_ENTITY | state::ATTACK | state::FIGHT | state::AIM_GUN => {
                if self.ped_first_assoc(r, ASSOC_BLOCK).is_none() {
                    if cam.using_3rd_person_mouse_cam && !cam.free_cam {
                        self.player_control_1st_person_run_around(px, r, &pad);
                    } else if self.ped(r).ped_state == state::FIGHT {
                        self.player_control_fighter(px, r, &pad);
                    } else {
                        self.player_control_zelda(px, r, &pad);
                    }
                }
                if self.ped(r).is_ped_in_control() {
                    self.process_player_weapon(px, r, &pad);
                }
            }
            state::SEEK_ENTITY | state::SEEK_POS => {
                if self.ped(r).ped_state == state::SEEK_ENTITY
                    && let Some(t) = self.ped(r).seek_target
                {
                    self.ped_mut(r).seek_pos = self.epos(t);
                }
                self.player_data_mut(r).move_speed = match self.ped(r).move_state {
                    move_state::WALK => 1.0,
                    move_state::RUN => 1.8,
                    move_state::SPRINT => 2.5,
                    _ => 0.0,
                };
                self.set_real_move_anim(px, r);
                if self.seek(px, r) {
                    self.restore_previous_state(px, r);
                    self.ped_mut(r).set_move_state(move_state::STILL);
                }
            }
            state::SNIPER_MODE => {
                if self.current_weapon_ty(r) == wt::M16 {
                    self.player_control_m16(px, r, &pad);
                } else {
                    self.player_control_sniper(px, r, &pad);
                }
            }
            state::SEEK_CAR | state::SEEK_IN_BOAT => {
                let pd = self.ped(r);
                let speed = if pd.flags.veh_enter_door_is_blocked || pd.flags.kinda_stay_in_same_place {
                    0.0
                } else {
                    2.0_f32.min(2.0 * (pd.seek_pos - self.epos(r)).truncate().length())
                };
                self.player_data_mut(r).move_speed = speed;
                if !pad.controls_disabled && (pad.target || pad.direction_just_down) {
                    self.restore_previous_state(px, r);
                    if matches!(self.ped(r).objective, objective::ENTER_CAR_AS_PASSENGER | objective::ENTER_CAR_AS_DRIVER) {
                        self.restore_previous_objective(px, r);
                    }
                }
                if pad.sprint {
                    self.ped_mut(r).move_state = move_state::SPRINT;
                }
                self.set_real_move_anim(px, r);
            }
            state::JUMP => {
                self.player_control_zelda(px, r, &pad);
                if !self.ped(r).flags.is_landing {
                    // added later in the original: nothing below runs while airborne
                    return;
                }
            }
            state::FALL | state::GETUP | state::ENTER_TRAIN | state::EXIT_TRAIN | state::CARJACK | state::DRAG_FROM_CAR | state::ENTER_CAR | state::STEAL_CAR | state::EXIT_CAR => {
                self.clear_weapon_target(px, r);
            }
            state::ARRESTED if self.ped(r).last_ped_state == state::DRAG_FROM_CAR && self.ped(r).vehicle_anim.is_some() => self.being_dragged_from_car(r),
            _ => {}
        }
        if self.ped(r).is_ped_shootable() {
            self.process_weapon_switch(px, r, &pad);
            self.update_current_weapon(r);
        }
        self.process_anim_groups(px, r);
        if cam.looking_behind {
            self.ped_mut(r).look_timer = 0;
            let cam_angle = limit_radian_angle(cam.front_heading);
            let cur = self.ped(r).rotation_cur;
            let between = (cam_angle - cur).abs();
            if self.ped(r).ped_state != state::ATTACK && between > 30.0_f32.to_radians() && between < 330.0_f32.to_radians() {
                let mut face = cam_angle;
                if between > 150.0_f32.to_radians() && between < 210.0_f32.to_radians() {
                    let right = limit_radian_angle(cur - 150.0_f32.to_radians());
                    let left = limit_radian_angle(150.0_f32.to_radians() + cur);
                    let look = self.ped(r).look_direction;
                    face = if look == 999999.0 || (right - look).abs() < (left - look).abs() { right } else { left };
                }
                self.set_look_flag_direction(r, face, true);
                let ms = (self.time_step() * 1000.0 / 50.0) as i32;
                self.set_look_timer(r, ms * 5);
            } else {
                self.clear_look_flag(px, r);
            }
        }
        if self.ped(r).move_state == move_state::SPRINT && self.ped(r).flags.is_looking {
            self.clear_look_flag(px, r);
            self.set_look_timer(r, 250);
        }
        let flat_speed = self.ent(r).phys.as_ref().map_or(0.0, |p| Vec2::new(p.move_speed.x, p.move_speed.y).length());
        let p = self.player_data_mut(r);
        if flat_speed < 0.1 {
            if p.speed_timer != 0 {
                if now > p.speed_timer {
                    p.speed_timer_flag = true;
                }
            } else {
                p.speed_timer = now + 500;
            }
        } else {
            p.speed_timer = 0;
            p.speed_timer_flag = false;
        }
    }

    /// The driver's door swings shut while the player pulls away.
    fn player_door_rolling(&mut self, px: &mut Px, r: EntityRef, pad: &PadState) {
        let Some(veh) = self.ped(r).my_vehicle else { return };
        use crate::vehicle::automobile::{car_door_flag, door_id, node};
        let (is_car, door_status, low) = match (self.car_ref(veh), self.try_veh(veh)) {
            (Some(c), Some(v)) => (true, c.damage.door_status(door_id::FRONT_LEFT), v.flags.low_vehicle),
            _ => (false, 0, false),
        };
        if !is_car || door_status != crate::vehicle::damage::door_status::SWINGING {
            return;
        }
        let flag = car_door_flag(node::DOOR_LF);
        let getting_out = self.try_veh(veh).is_some_and(|v| v.getting_out_flags & flag != 0);
        let mut roll = self.ped_assoc(r, anim::STD_CAR_CLOSE_DOOR_ROLLING_LHS).map(|a| a.current_time);
        let busy = getting_out || roll.is_some() || {
            roll = self.ped_assoc(r, anim::STD_CAR_CLOSE_DOOR_ROLLING_LO_LHS).map(|a| a.current_time);
            roll.is_some()
        };
        if busy {
            if let Some(t) = roll {
                self.process_open_door(veh, door_id::FRONT_LEFT, anim::STD_CAR_CLOSE_DOOR_ROLLING_LHS, t);
            }
        } else if !pad.driving_input {
            self.try_veh_mut(veh).expect("vehicle").getting_out_flags |= flag;
            let id = if low { anim::STD_CAR_CLOSE_DOOR_ROLLING_LO_LHS } else { anim::STD_CAR_CLOSE_DOOR_ROLLING_LHS };
            if let Some(i) = self.ped_add_anim(r, assoc_group::STD, id)
                && let Some(a) = self.ped_assoc_at(r, i)
            {
                a.set_finish_callback(super::callbacks::PED_ANIM_DOOR_CLOSE_ROLLING);
            }
        }
        let _ = px;
    }
}
