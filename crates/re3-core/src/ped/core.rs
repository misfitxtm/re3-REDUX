//! `Ped.cpp`, first part: ped lists, threat scanning, movement animation and the look/aim helpers.
use super::ctx::Px;
use super::data::*;
use super::ik::IkPed;
use crate::anim::{flags as aflags, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::{EntityRef, Kind};
use crate::math::normalise_2d;
use crate::world::{Kinds, LineOpts, World, PEDS};
use glam::{Vec2, Vec3};
use std::f32::consts::{PI, TAU};

/// `nThreatReactionRangeMultiplier` (set by the script).
pub const THREAT_REACTION_RANGE_MULTIPLIER: u16 = 1;

/// `CPed::SortPeds`: orders `list[min..=max]` by distance from `from` (the original's partition scheme).
// NaN must end the scan like the original's `while (middle > d)`
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub(super) fn sort_peds(list: &mut [EntityRef], min: i32, max: i32, dist: &dyn Fn(EntityRef) -> f32) {
    if min >= max {
        return;
    }
    let middle = dist(list[((max + min) / 2) as usize]);
    let mut left = max;
    let mut right = min;
    while right <= left {
        loop {
            let d = dist(list[right as usize]);
            if !(middle > d) {
                break;
            }
            right += 1;
            if right > max {
                break;
            }
        }
        loop {
            let d = dist(list[left as usize]);
            if !(middle < d) {
                break;
            }
            let old = left;
            left -= 1;
            if old == 0 {
                break;
            }
        }
        if right <= left {
            list.swap(right as usize, left as usize);
            right += 1;
            left -= 1;
        }
    }
    sort_peds(list, min, left, dist);
    sort_peds(list, right, max, dist);
}

impl World {
    /// `CPed::SetPedStats`.
    pub fn set_ped_stats(&mut self, r: EntityRef, stat: usize) {
        self.ped_mut(r).ped_stats = stat;
    }

    /// `CPed::BuildPedLists`: the up to ten closest peds, refreshed every 16 frames.
    pub fn build_ped_lists(&mut self, r: EntityRef) {
        let frame = self.ents.timer.frame_counter;
        let seed = self.ent(r).base.random_seed as u32;
        let pos = self.epos(r);
        if (frame + seed).is_multiple_of(16) {
            let g = self.grid;
            let (x0, y0) = (g.index_x(pos.x - 20.0), g.index_y(pos.y - 20.0));
            let (x1, y1) = (g.index_x(pos.x + 20.0), g.index_y(pos.y + 20.0));
            let mut list: Vec<EntityRef> = vec![];
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let Some(sector) = self.sector(x, y) else { continue };
                    let members: Vec<EntityRef> = sector.lists[PEDS].iter().copied().collect();
                    for ped in members {
                        let Some(pd) = self.try_ped(ped) else { continue };
                        if ped != r && !pd.in_vehicle {
                            let d = self.epos(ped) - pos;
                            let dist = (d.x * d.x + d.y * d.y).sqrt();
                            if THREAT_REACTION_RANGE_MULTIPLIER as f32 * 30.0 > dist {
                                // FIX_BUGS: a full list is sorted and cut down before more are added
                                if list.len() == 49 {
                                    let me = pos;
                                    sort_peds(&mut list, 0, 48, &|e| (me - self.epos(e)).length());
                                    list.truncate(10);
                                }
                                list.push(ped);
                            }
                        }
                    }
                }
            }
            let n = list.len() as i32;
            let me = pos;
            sort_peds(&mut list, 0, n - 1, &|e| (me - self.epos(e)).length());
            let pd = self.ped_mut(r);
            pd.num_near_peds = 0;
            for i in 0..10 {
                match list.get(i) {
                    Some(&p) => {
                        pd.near_peds[i] = Some(p);
                        pd.num_near_peds += 1;
                    }
                    None => pd.near_peds[i] = None,
                }
            }
        } else {
            let mut i = 0;
            while i < 10 {
                let remove = match self.ped(r).near_peds[i] {
                    Some(n) => !self.ped_pointer_valid(n) || {
                        let d = pos - self.epos(n);
                        d.x * d.x + d.y * d.y > 900.0
                    },
                    None => false,
                };
                if remove {
                    let pd = self.ped_mut(r);
                    for j in i..9 {
                        pd.near_peds[j] = pd.near_peds[j + 1];
                        pd.near_peds[j + 1] = None;
                    }
                    pd.near_peds[9] = None;
                    pd.num_near_peds = pd.num_near_peds.saturating_sub(1);
                } else {
                    i += 1;
                }
            }
        }
    }

    /// `CPed::OurPedCanSeeThisOne`.
    pub fn our_ped_can_see_this_one(&mut self, r: EntityRef, target: EntityRef) -> bool {
        let (me, them) = (self.epos(r), self.epos(target));
        let dist = Vec2::new(them.x - me.x, them.y - me.y);
        let fwd = self.ent(r).base.matrix.forward;
        if dist.x * fwd.x + dist.y * fwd.y < 0.0 {
            return false;
        }
        if dist.length() >= 40.0 {
            return false;
        }
        let head = Vec3::new(me.x, me.y, me.z + 1.0);
        let kinds = Kinds { buildings: true, ..Kinds::NONE };
        self.process_line_of_sight(head, them, kinds, LineOpts::default()).is_none()
    }

    /// `CPed::SetMoveAnim`.
    pub fn set_move_anim(&mut self, px: &mut Px, r: EntityRef) {
        let pd = self.ped(r);
        if pd.stored_move_state == pd.move_state as i32 || !pd.is_ped_in_control() {
            return;
        }
        if pd.move_state == move_state::NONE {
            self.ped_mut(r).stored_move_state = move_state::NONE as i32;
            return;
        }
        let group = match pd.leader {
            Some(l) if self.try_ped(l).is_some_and(|p| p.is_player()) => crate::anim::assoc_group::PLAYER,
            _ => pd.anim_group as usize,
        };
        let (ped_state, wait_state, move_state_now) = (pd.ped_state, pd.wait_state, pd.move_state);
        let mut have = self.ped_first_assoc(r, aflags::BLOCK).is_some();
        if !have {
            let fight_idle = self.ped_assoc(r, anim::STD_FIGHT_IDLE).is_some();
            have = fight_idle;
            if fight_idle && ped_state == state::FIGHT {
                return;
            }
            if fight_idle {
                let idle_ok = self.ped_assoc(r, anim::STD_IDLE).is_some_and(|a| a.blend_delta > 0.0);
                if !idle_ok {
                    if let Some(a) = self.ped_assoc_mut(r, anim::STD_FIGHT_IDLE) {
                        a.flags |= aflags::DELETE_FADED_OUT;
                    }
                    have = self.ped_blend_anim(r, group, anim::STD_IDLE, 8.0).is_some();
                }
            }
        }
        if !have {
            let tired = self.ped_assoc(r, anim::STD_IDLE_TIRED).is_some();
            have = tired;
            if tired && (wait_state == wait_state::STUCK || wait_state == wait_state::FINISH_FLEE) {
                return;
            }
            if tired {
                let idle_ok = self.ped_assoc(r, anim::STD_IDLE).is_some_and(|a| a.blend_delta > 0.0);
                if !idle_ok {
                    if let Some(a) = self.ped_assoc_mut(r, anim::STD_IDLE_TIRED) {
                        a.flags |= aflags::DELETE_FADED_OUT;
                    }
                    have = self.ped_blend_anim(r, group, anim::STD_IDLE, 4.0).is_some();
                }
            }
        }
        if have {
            return;
        }
        self.ped_mut(r).stored_move_state = move_state_now as i32;
        if matches!(move_state_now, move_state::WALK | move_state::RUN | move_state::SPRINT) {
            if let Some(c) = self.ped_clump_mut(r) {
                for a in c.anim.associations.iter_mut().filter(|a| a.is_partial()) {
                    if a.flags & aflags::FADE_OUT_WHEN_DONE == 0 {
                        a.blend_delta = -2.0;
                        a.flags |= aflags::DELETE_FADED_OUT;
                    }
                }
            }
            self.clear_aim_flag(r);
            self.clear_look_flag(px, r);
        }
        let idx = match move_state_now {
            move_state::STILL => self.ped_blend_anim(r, group, anim::STD_IDLE, 4.0),
            move_state::WALK => self.ped_blend_anim(r, group, anim::STD_WALK, 1.0),
            move_state::RUN => {
                let delta = if ped_state == state::FLEE_ENTITY { 3.0 } else { 1.0 };
                self.ped_blend_anim(r, group, anim::STD_RUN, delta)
            }
            move_state::SPRINT => self.ped_blend_anim(r, group, anim::STD_RUNFAST, 1.0),
            _ => None,
        };
        if let Some(i) = idx {
            let mission = self.ped(r).char_created_by == MISSION_CHAR;
            let seed = self.ent(r).base.random_seed as f32;
            let max = px.rng.max() as f32;
            let leader_speed = self.ped(r).leader.and_then(|l| {
                [anim::STD_WALK, anim::STD_RUN, anim::STD_RUNFAST].into_iter().find_map(|id| self.ped_assoc(l, id)).map(|a| a.speed)
            });
            let speed = match leader_speed {
                Some(s) => s,
                None if mission => 1.0,
                None => 1.2 - seed * 0.4 / max,
            };
            if let Some(a) = self.ped_assoc_at(r, i) {
                a.speed = speed;
            }
        }
    }

    /// `CPed::StopNonPartialAnims`.
    pub fn stop_non_partial_anims(&mut self, r: EntityRef) {
        if let Some(c) = self.ped_clump_mut(r) {
            for a in c.anim.associations.iter_mut().filter(|a| !a.is_partial()) {
                a.flags &= !aflags::RUNNING;
            }
        }
    }

    /// `CPed::RestartNonPartialAnims`.
    pub fn restart_non_partial_anims(&mut self, r: EntityRef) {
        if let Some(c) = self.ped_clump_mut(r) {
            for a in c.anim.associations.iter_mut().filter(|a| !a.is_partial()) {
                a.flags |= aflags::RUNNING;
            }
        }
    }

    /// `CPed::SetStoredState`.
    pub fn set_stored_state(&mut self, r: EntityRef) {
        self.ped_mut(r).set_stored_state();
    }

    /// `CPed::RestorePreviousState`.
    pub fn restore_previous_state(&mut self, px: &mut Px, r: EntityRef) {
        let pd = self.ped(r);
        if !pd.can_set_ped_state() || pd.ped_state == state::FALL {
            return;
        }
        if pd.ped_state == state::GETUP && !pd.flags.get_up_anim_started {
            return;
        }
        if pd.in_vehicle_now() {
            let pd = self.ped_mut(r);
            pd.ped_state = state::DRIVING;
            pd.last_ped_state = state::NONE;
            return;
        }
        if pd.last_ped_state == state::NONE {
            if !pd.is_player() && pd.char_created_by != MISSION_CHAR && pd.objective == objective::NONE {
                let dir = (px.rng.number() & 7) as i8;
                if self.set_wander_path(px, r, dir) {
                    return;
                }
            }
            self.set_idle(px, r);
            return;
        }
        match pd.last_ped_state {
            state::IDLE => self.set_idle(px, r),
            state::WANDER_PATH => {
                let find_new = {
                    let pd = self.ped_mut(r);
                    pd.ped_state = state::WANDER_PATH;
                    pd.flags.is_running = false;
                    pd.flags.find_new_node_after_state_restore
                };
                let mut done = false;
                if find_new
                    && let Some(n) = self.ped(r).next_path_node {
                        let diff = self.ents.paths.nodes[n].pos - self.epos(r);
                        if diff.length_squared() < 7.0 * 7.0 {
                            self.ped_mut(r).set_move_state(move_state::WALK);
                            done = true;
                        }
                    }
                if !done {
                    let dir = (px.rng.number() & 7) as i8;
                    self.set_wander_path(px, r, dir);
                }
            }
            last => {
                let prev = self.ped(r).prev_move_state;
                let pd = self.ped_mut(r);
                pd.ped_state = last;
                pd.set_move_state(prev as u8);
            }
        }
        self.ped_mut(r).last_ped_state = state::NONE;
    }

    /// `CPed::SetLookFlag(direction)`.
    pub fn set_look_flag_direction(&mut self, r: EntityRef, direction: f32, keep_trying: bool) {
        let now = self.now();
        self.ped_mut(r).set_look_flag_direction(direction, keep_trying, now);
    }

    /// `CPed::SetLookFlag(entity)`.
    pub fn set_look_flag_entity(&mut self, r: EntityRef, target: EntityRef, keep_trying: bool) {
        let now = self.now();
        self.ped_mut(r).set_look_flag_entity(target, keep_trying, now);
    }

    /// `CPed::ClearLookFlag`.
    pub fn clear_look_flag(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let pd = self.ped_mut(r);
        if pd.flags.is_looking {
            pd.flags.is_looking = false;
            pd.flags.is_restoring_look = true;
            pd.flags.shake_fist = false;
            pd.ik.flags &= !super::ik::ik_flag::LOOKAROUND_HEAD_ONLY;
            pd.look_timer = now + if pd.is_player() { 2000 } else { 4000 };
            if pd.ped_state == state::LOOK_HEADING || pd.ped_state == state::LOOK_ENTITY {
                self.clear_look(px, r);
            }
        }
    }

    /// `CPed::ClearAimFlag`.
    pub fn clear_aim_flag(&mut self, r: EntityRef) {
        self.ped_mut(r).clear_aim_flag();
        if self.ped(r).is_player()
            && let Some(p) = self.ped_mut(r).player.as_mut()
        {
            p.fps_move_heading = 0.0;
            p.free_aim_active = false;
        }
    }

    /// `CPed::SetAimFlag(angle)`.
    pub fn set_aim_flag_angle(&mut self, r: EntityRef, angle: f32) {
        let w = self.ped(r).current_weapon as usize;
        let ty = self.ped(r).weapons[w].ty;
        let arm = self.ents.weapon_infos.get(ty).is_flag_set(crate::weapons::info::flag::CANAIM_WITHARM);
        self.ped_mut(r).set_aim_flag_angle(angle, arm);
    }

    /// `CPed::SetAimFlag(entity)`.
    pub fn set_aim_flag_entity(&mut self, r: EntityRef, to: EntityRef) {
        self.ped_mut(r).set_aim_flag_entity(to);
    }

    fn with_ik<T>(&mut self, r: EntityRef, f: impl FnOnce(&mut super::ik::PedIk, &mut IkPed) -> T) -> Option<T> {
        let pos = self.epos(r);
        let third = false; // shortcut: the 3rd-person mouse camera is a camera-system query, off by default
        let Kind::Ped(pd) = &mut self.get_mut(r)?.kind else { return None };
        let PedData { ik, clump, rotation_cur, .. } = &mut **pd;
        let clump = clump.as_mut()?;
        let mut ped = IkPed { clump, rotation_cur: *rotation_cur, pos, third_person_mouse_cam: third };
        Some(f(ik, &mut ped))
    }

    /// The position of one of a ped's nodes in world space (`GetNodePosition`).
    pub fn ped_node_position(&self, r: EntityRef, node: usize) -> Vec3 {
        let e = self.ent(r);
        match self.ped_clump(r) {
            Some(c) => e.base.matrix.transform_point(c.skeleton.component_position(c.nodes[node])),
            None => e.base.position(),
        }
    }

    /// `CPedIK::GetComponentPosition` in the ped's own frame.
    pub fn ped_component_position(&self, r: EntityRef, node: usize) -> Vec3 {
        self.ped_clump(r).map_or(Vec3::ZERO, |c| c.skeleton.component_position(c.nodes[node]))
    }

    /// `CPed::MoveHeadToLook`.
    pub fn move_head_to_look(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let pd = self.ped(r);
        if pd.look_timer != 0 && now > pd.look_timer {
            self.clear_look_flag(px, r);
        } else if pd.ped_state == state::DRIVING {
            self.ped_mut(r).ik.flags |= super::ik::ik_flag::LOOKAROUND_HEAD_ONLY;
        }
        let Some(target) = self.ped(r).look_target else {
            let pd = self.ped(r);
            if pd.look_direction == 999999.0 {
                self.clear_look_flag(px, r);
            } else {
                let dir = pd.look_direction;
                let ok = self.with_ik(r, |ik, ped| ik.look_in_direction(ped, dir, 0.0)).unwrap_or(true);
                if !ok && !self.ped(r).flags.keep_trying_to_look {
                    self.clear_look_flag(px, r);
                }
            }
            return;
        };
        let pd = self.ped(r);
        let weapon_type = pd.weapons[pd.current_weapon as usize].ty;
        if !pd.flags.shake_fist && weapon_type == crate::weapons::info::weapon_type::UNARMED
            && let Some(a) = self.ped_assoc(r, anim::STD_PARTIAL_FUCKU) {
                let (time, step) = (a.current_time, a.time_step);
                if time > 4.0 / 30.0 && time - step > 4.0 / 30.0 {
                    let looking_to_cop = self.ent(target).base.model_index == crate::modelinfo::indices_gta3::mi::POLICE || self.try_ped(target).is_some_and(|t| t.ped_type == super::types::ped_type::COP);
                    let temper = self.ents.ped_stats.0[self.ped(r).ped_stats].temper;
                    if self.ped(r).is_player() && (temper >= 52 || looking_to_cop) {
                        self.add_weapon_model(r, crate::modelinfo::indices_gta3::mi::FINGERS as i32);
                        self.annoy_player_ped(r, true);
                    } else if px.rng.number() & 3 == 0 {
                        self.add_weapon_model(r, crate::modelinfo::indices_gta3::mi::FINGERS as i32);
                    }
                }
            }
        let look_pos = if self.is_ped(target) { self.ent(target).base.matrix.transform_point(self.ped_component_position(target, node::MID)) } else { self.epos(target) };
        let ok = self.with_ik(r, |ik, ped| ik.look_at_position(ped, look_pos)).unwrap_or(true);
        if !ok {
            if !self.ped(r).flags.keep_trying_to_look {
                self.clear_look_flag(px, r);
            }
            return;
        }
        let pd = self.ped(r);
        if !pd.flags.shake_fist || pd.flags.is_aiming_gun || pd.flags.is_restoring_gun {
            return;
        }
        if pd.look_timer.wrapping_sub(now) >= 1000 {
            return;
        }
        let ty = pd.weapons[pd.current_weapon as usize].ty;
        let two_handed = pd.weapons[pd.current_weapon as usize].is_type_2_handed();
        let not_rocket = !two_handed && ty != crate::weapons::info::weapon_type::ROCKETLAUNCHER;
        let mut to_play = anim::STD_NUM;
        if self.ped(r).is_player() && not_rocket {
            let temper = self.ents.ped_stats.0[self.ped(r).ped_stats].temper;
            if let Some(t) = self.try_ped(target) {
                if temper >= 49 && t.ped_type != super::types::ped_type::COP {
                    to_play = anim::STD_PARTIAL_PUNCH;
                } else {
                    to_play = anim::STD_PARTIAL_FUCKU;
                }
            } else if temper > 49 || self.ent(target).base.model_index == crate::modelinfo::indices_gta3::mi::POLICE {
                to_play = anim::STD_PARTIAL_FUCKU;
            }
        } else if not_rocket && px.rng.number() & 1 != 0 {
            to_play = anim::STD_PARTIAL_FUCKU;
        }
        if to_play != anim::STD_NUM
            && let Some(i) = self.ped_blend_anim(r, crate::anim::assoc_group::STD, to_play, 4.0)
                && let Some(a) = self.ped_assoc_at(r, i)
            {
                a.flags |= aflags::FADE_OUT_WHEN_DONE | aflags::DELETE_FADED_OUT;
                if a.anim_id == anim::STD_PARTIAL_FUCKU as i32 {
                    a.set_delete_callback(super::callbacks::FINISH_FUCK_U);
                }
            }
        self.ped_mut(r).flags.shake_fist = false;
    }

    /// `CPed::RestoreHeadPosition`.
    pub fn restore_head_position(&mut self, r: EntityRef) {
        if self.with_ik(r, |ik, ped| ik.restore_look_at(ped)).unwrap_or(true) {
            self.ped_mut(r).flags.is_restoring_look = false;
        }
    }

    /// `CPed::AimGun`.
    pub fn aim_gun(&mut self, px: &mut Px, r: EntityRef) {
        if let Some(t) = self.ped(r).seek_target {
            let pos = if self.is_ped(t) { self.ent(t).base.matrix.transform_point(self.ped_component_position(t, node::MID)) } else { self.epos(t) };
            self.say(px, r, sound::PED_ATTACK);
            let ok = self.with_ik(r, |ik, ped| ik.point_gun_at_position(ped, pos)).unwrap_or(true);
            self.ped_mut(r).flags.can_point_gun_at_target = ok;
            if self.ped(r).look_target != Some(t) {
                self.set_look_flag_entity(r, t, true);
            }
        } else {
            let dir = self.ped(r).look_direction;
            let pitch = if self.ped(r).is_player() { self.ped(r).player.as_ref().map_or(0.0, |p| p.fps_move_heading) } else { 0.0 };
            let ok = self.with_ik(r, |ik, ped| ik.point_gun_in_direction(ped, dir, pitch)).unwrap_or(true);
            self.ped_mut(r).flags.can_point_gun_at_target = ok;
        }
    }

    /// `CPed::RestoreGunPosition`.
    pub fn restore_gun_position(&mut self, r: EntityRef) {
        if self.ped(r).flags.is_looking {
            let pd = self.ped_mut(r);
            pd.ik.flags &= !super::ik::ik_flag::LOOKAROUND_HEAD_ONLY;
            pd.flags.is_restoring_gun = false;
        } else if self.with_ik(r, |ik, ped| ik.restore_gun_posn(ped)).unwrap_or(true) {
            self.ped_mut(r).flags.is_restoring_gun = false;
        } else if let Some(p) = self.ped_mut(r).player.as_mut() {
            p.fps_move_heading = 0.0;
        }
    }

    /// `CPed::WillChat`.
    pub fn will_chat(&mut self, r: EntityRef, stranger: EntityRef) -> bool {
        let pd = self.ped(r);
        if let (Some(n), Some(l)) = (pd.next_path_node, pd.last_path_node)
            && n != l
            && self.ents.paths.test_crosses_road(n, l)
        {
            return false;
        }
        if self.ents.pools.get(r).and_then(|_| self.get(r)).is_some_and(|e| e.phys.as_ref().is_some_and(|p| p.surface_touched == crate::surface::TARMAC)) {
            return false;
        }
        if stranger == r {
            return false;
        }
        let (my_type, their_type) = (pd.ped_type, self.ped(stranger).ped_type);
        if my_type == their_type {
            return true;
        }
        if my_type == super::types::ped_type::CRIMINAL {
            return false;
        }
        if (self.ped(r).is_gang_member() || self.ped(stranger).is_gang_member()) && my_type != their_type {
            return false;
        }
        true
    }

    /// `CPed::CanSeeEntity`.
    pub fn can_see_entity(&self, r: EntityRef, target: EntityRef, threshold: f32) -> bool {
        let (t, me) = (self.epos(target), self.epos(r));
        let needed = crate::general::radian_angle_between_points(t.x, t.y, me.x, me.y);
        self.ped(r).can_see_angle(needed, threshold)
    }

    /// `CPed::ClearAll`.
    pub fn clear_all(&mut self, px: &mut Px, r: EntityRef) {
        let pd = self.ped(r);
        if !pd.is_ped_in_control() && pd.ped_state != state::DEAD {
            return;
        }
        {
            let pd = self.ped_mut(r);
            pd.ped_state = state::NONE;
            pd.set_move_state(move_state::NONE);
            pd.seek_target = None;
            pd.seek_pos = Vec3::ZERO;
            pd.flee_from_pos_x = 0.0;
            pd.flee_from_pos_y = 0.0;
            pd.flee_from = None;
            pd.flee_timer = 0;
        }
        self.get_mut(r).unwrap().base.flags.uses_collision = true;
        self.clear_aim_flag(r);
        self.clear_look_flag(px, r);
        let pd = self.ped_mut(r);
        pd.flags.is_pointing_gun_at = false;
        pd.flags.render_ped_in_car = true;
        pd.flags.knocked_up_into_air = false;
        pd.colliding_entity = None;
    }

    /// `CPed::CalculateNewOrientation`.
    pub fn calculate_new_orientation(&mut self, r: EntityRef) {
        if !self.ped(r).is_ped_in_control() {
            return;
        }
        let h = self.ped(r).rotation_cur;
        self.get_mut(r).unwrap().base.set_heading(h);
    }

    /// `CPed::CalculateNewVelocity`.
    pub fn calculate_new_velocity(&mut self, px: &mut Px, r: EntityRef) {
        let step = self.time_step();
        if self.ped(r).is_ped_in_control() {
            let pd = self.ped_mut(r);
            let mut head_amount = pd.heading_rate.to_radians() * step;
            pd.rotation_cur = crate::general::limit_radian_angle(pd.rotation_cur);
            let mut dest = crate::general::limit_radian_angle(pd.rotation_dest);
            if pd.rotation_cur - PI > dest {
                dest += TAU;
            } else if PI + pd.rotation_cur < dest {
                dest -= TAU;
            }
            if pd.is_player() && pd.ped_state == state::ATTACK {
                head_amount /= 4.0;
            }
            let needed = dest - pd.rotation_cur;
            if needed <= head_amount {
                if needed > -head_amount {
                    pd.rotation_cur += needed;
                } else {
                    pd.rotation_cur -= head_amount;
                }
            } else {
                pd.rotation_cur += head_amount;
            }
        }
        let (cur, delta) = (self.ped(r).rotation_cur, self.ped(r).anim_move_delta);
        let forward = Vec2::new(cur.sin(), cur.cos());
        let mut moved = Vec2::new(delta.x * forward.y - delta.y * forward.x, delta.x * forward.x + delta.y * forward.y);
        moved *= if step >= 0.01 { 1.0 / step } else { 1.0 / 100.0 };
        self.ped_mut(r).moved = moved;

        let first_person = px.hooks.weapon_first_person_on() || px.hooks.third_person_mouse_cam();
        if !first_person || self.ents.player_ped != Some(r) || !self.ped(r).can_strafe_or_mouse_control() {
            return;
        }
        let walk_angle = self.work_out_heading_for_moving_first_person(px, r, cur);
        let speed = moved.length();
        let mut local = crate::general::limit_radian_angle(walk_angle - cur);
        if local < -0.5 * PI {
            local += PI;
        } else if local > 0.5 * PI {
            local -= PI;
        }
        if local > -50.0_f32.to_radians() && local < 50.0_f32.to_radians() {
            px.hooks.set_player_velocity(speed);
            self.ped_mut(r).moved = Vec2::new(-walk_angle.sin(), walk_angle.cos()) * speed;
        }
        let idle_blend = self.ped_assoc(r, anim::STD_IDLE).map(|a| a.blend_amount);
        let fight = self.ped_assoc(r, anim::STD_FIGHT_IDLE).is_some();
        if idle_blend.is_none_or(|b| b < 0.5) && !fight {
            let mut yaw = local;
            if yaw < -100.0_f32.to_radians() {
                yaw += PI;
            } else if yaw > 100.0_f32.to_radians() {
                yaw -= PI;
            }
            if yaw > -50.0_f32.to_radians() && yaw < 50.0_f32.to_radians() {
                let limb = super::ik::LimbOrientation { yaw, pitch: 0.0 };
                for n in [node::UPPERLEGL, node::UPPERLEGR] {
                    self.with_ik(r, |ik, ped| {
                        let f = ped.clump.nodes[n];
                        ik.rotate_torso(ped, f, limb, false);
                    });
                }
            }
        }
    }

    /// `CPed::WorkOutHeadingForMovingFirstPerson`.
    pub fn work_out_heading_for_moving_first_person(&mut self, px: &mut Px, r: EntityRef, offset: f32) -> f32 {
        if !self.ped(r).is_player() {
            return 0.0;
        }
        let (left_right, up_down) = px.hooks.ped_walk_axes();
        let walk = self.ped_mut(r).player.get_or_insert_with(Default::default);
        if up_down != 0.0 {
            walk.walk_angle = crate::general::radian_angle_between_points(0.0, 0.0, -left_right, up_down);
        } else if left_right < 0.0 {
            walk.walk_angle = 0.5 * PI;
        } else if left_right > 0.0 {
            walk.walk_angle = -0.5 * PI;
        }
        let a = walk.walk_angle;
        crate::general::limit_radian_angle(offset + a)
    }

    /// `CPed::UpdatePosition`.
    pub fn update_position(&mut self, r: EntityRef) {
        if !self.ped(r).flags.is_standing {
            return;
        }
        let step = self.time_step();
        let cur = self.ped(r).rotation_cur;
        self.get_mut(r).unwrap().base.set_heading(cur);
        let moved = self.ped(r).moved;
        let surface = self.ped(r).current_phys_surface;
        let mut velocity_change;
        let normal_xy = |n: Vec3| Vec2::new(n.x, n.y);
        if let Some(s) = surface.filter(|&s| self.get(s).is_some()) {
            let is_player = self.ped(r).is_player();
            let s_ent = self.ent(s);
            let is_boat = matches!(&s_ent.kind, Kind::Vehicle(v) if v.is_boat());
            let s_phys = s_ent.phys.as_ref().unwrap();
            let velocity_of_surface;
            if !is_player && is_boat {
                let mut offset = self.epos(r) - self.epos(s);
                offset.z -= FEET_OFFSET;
                let surface_move = s_phys.move_speed;
                let surface_turn = crate::math::cross(s_phys.turn_speed, offset);
                let slide = -s_phys.turn_speed.length_squared();
                let v = slide * offset * step + (surface_turn + surface_move);
                velocity_of_surface = Vec2::new(v.x, v.y);
                let z = slide * offset.z * step + (surface_turn.z + surface_move.z);
                self.get_mut(r).unwrap().phys.as_mut().unwrap().move_speed.z = z;
            } else {
                let v = s_phys.speed_at(self.ped(r).offset_from_phys_surface);
                velocity_of_surface = Vec2::new(v.x, v.y);
            }
            let ms = Vec2::new(self.get(r).unwrap().phys.as_ref().unwrap().move_speed.x, self.get(r).unwrap().phys.as_ref().unwrap().move_speed.y);
            velocity_change = moved + velocity_of_surface - ms;
            let turn_z = self.get(s).unwrap().phys.as_ref().unwrap().turn_speed.z;
            let pd = self.ped_mut(r);
            pd.rotation_cur += turn_z * step;
            pd.rotation_dest += turn_z * step;
        } else if self.get(r).unwrap().phys.as_ref().unwrap().surface_touched == crate::surface::STEEP_CLIFF && {
            let n = self.get(r).unwrap().phys.as_ref().unwrap().damage_normal;
            n.x != 0.0 || n.y != 0.0
        } {
            let n = self.get(r).unwrap().phys.as_ref().unwrap().damage_normal;
            self.get_mut(r).unwrap().phys.as_mut().unwrap().move_speed = Vec3::new(0.0, 0.0, -0.001);
            let reaction = normalise_2d(normal_xy(n));
            velocity_change = 0.02 * reaction + moved;
            let dot = reaction.dot(velocity_change);
            if dot < 0.0 {
                velocity_change -= dot * reaction;
            }
        } else {
            let ms = self.get(r).unwrap().phys.as_ref().unwrap().move_speed;
            velocity_change = moved - Vec2::new(ms.x, ms.y);
        }
        if let Some(s) = surface.filter(|&s| self.get(s).is_some()) {
            let speed_change = velocity_change.length();
            let mut change_mult = speed_change;
            let (is_vehicle, is_boat) = match &self.ent(s).kind {
                Kind::Vehicle(v) => (true, v.is_boat()),
                _ => (false, false),
            };
            if self.ped(r).ped_state == state::DIE && is_vehicle {
                change_mult = 0.002 * step;
            } else if !(is_vehicle && is_boat) {
                change_mult = 0.01 * step;
            }
            if speed_change > change_mult {
                velocity_change *= change_mult / speed_change;
            }
        }
        let p = self.get_mut(r).unwrap().phys.as_mut().unwrap();
        p.move_speed.x += velocity_change.x;
        p.move_speed.y += velocity_change.y;
    }
}
