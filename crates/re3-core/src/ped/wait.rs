//! `Ped.cpp`: the wait states a wandering ped drops into (lights, crossing, doubling back, hands up...).
use super::callbacks;
use super::ctx::Px;
use super::data::*;
use crate::anim::{assoc_group, flags as aflags, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::EntityRef;
use crate::general::limit_radian_angle;
use crate::world::World;
use std::f32::consts::PI;

impl World {
    /// `CPed::RestoreHeadingRate`.
    pub fn restore_heading_rate(&mut self, r: EntityRef) {
        let rate = self.ents.ped_stats.0[self.ped(r).ped_stats].heading_change_rate;
        self.ped_mut(r).heading_rate = rate;
    }

    /// `CPed::FinishedWaitCB`.
    pub fn finished_wait_cb(&mut self, px: &mut Px, r: EntityRef) {
        self.ped_mut(r).wait_timer = 0;
        self.restore_heading_rate(r);
        self.wait(px, r);
    }

    /// Blends `anim` in and returns its index in the clump.
    fn wait_anim(&mut self, r: EntityRef, anim_id: u32, delta: f32) -> Option<usize> {
        self.ped_blend_anim(r, assoc_group::STD, anim_id, delta)
    }

    /// `CPed::SetWaitState`; `time` is the optional duration in ms.
    pub fn set_wait_state(&mut self, px: &mut Px, r: EntityRef, new_state: u8, time: Option<i32>) {
        if !self.ped(r).is_ped_in_control() {
            return;
        }
        if new_state != self.ped(r).wait_state {
            self.finished_wait_cb(px, r);
        }
        let now = self.now();
        let at = |ms: i32| now.wrapping_add(ms as u32);
        // (delta, callback kind, extra flags) of the animation each state plays
        match new_state {
            wait_state::TRAFFIC_LIGHTS => {
                let pd = self.ped_mut(r);
                pd.wait_timer = at(500);
                pd.set_move_state(move_state::STILL);
            }
            wait_state::CROSS_ROAD => {
                self.ped_mut(r).wait_timer = at(1000);
                self.wait_anim(r, anim::STD_IDLE_HBHB, 4.0);
            }
            wait_state::CROSS_ROAD_LOOK => {
                self.wait_anim(r, anim::STD_ROADCROSS, 8.0);
                self.ped_mut(r).wait_timer = at(time.unwrap_or_else(|| px.rng.range_i32(2000, 5000)));
            }
            wait_state::LOOK_PED | wait_state::LOOK_SHOP | wait_state::LOOK_ACCIDENT | wait_state::FACEOFF_GANG => {}
            wait_state::DOUBLEBACK => {
                let pd = self.ped_mut(r);
                pd.heading_rate = 0.0;
                pd.wait_timer = at(3500);
                let i = self.wait_anim(r, anim::STD_IDLE_HBHB, 4.0);
                self.finish_callback_at(r, i, callbacks::RESTORE_HEADING_RATE);
            }
            wait_state::HITWALL => {
                let pd = self.ped_mut(r);
                pd.heading_rate = 2.0;
                pd.wait_timer = at(5000);
                if let Some(a) = self.wait_anim(r, anim::STD_HIT_WALL, 16.0).and_then(|i| self.ped_assoc_at(r, i)) {
                    a.flags |= aflags::DELETE_FADED_OUT | aflags::FADE_OUT_WHEN_DONE;
                    a.set_delete_callback(callbacks::FINISHED_WAIT);
                }
                self.give_up_on_car(px, r);
            }
            wait_state::TURN180 => {
                let pd = self.ped_mut(r);
                pd.heading_rate = 0.0;
                pd.wait_timer = at(5000);
                if let Some(a) = self.wait_anim(r, anim::STD_TURN180, 4.0).and_then(|i| self.ped_assoc_at(r, i)) {
                    a.set_finish_callback(callbacks::FINISHED_WAIT);
                    a.set_delete_callback(callbacks::RESTORE_HEADING_RATE);
                }
            }
            wait_state::SURPRISE => {
                let pd = self.ped_mut(r);
                pd.heading_rate = 0.0;
                pd.wait_timer = at(2000);
                let i = self.wait_anim(r, anim::STD_HIT_WALL, 4.0);
                self.finish_callback_at(r, i, callbacks::FINISHED_WAIT);
            }
            wait_state::STUCK => {
                self.ped_mut(r).set_move_state(move_state::STILL);
                self.set_move_anim(px, r);
                let pd = self.ped_mut(r);
                pd.heading_rate = 0.0;
                pd.wait_timer = at(5000);
                let i = self.wait_anim(r, anim::STD_IDLE_TIRED, 4.0);
                self.finish_callback_at(r, i, callbacks::RESTORE_HEADING_RATE);
                self.give_up_on_car(px, r);
            }
            wait_state::LOOK_ABOUT => {
                self.ped_mut(r).set_move_state(move_state::STILL);
                self.set_move_anim(px, r);
                let pd = self.ped_mut(r);
                pd.heading_rate = 0.0;
                pd.wait_timer = at(5000);
                let i = self.wait_anim(r, anim::STD_IDLE_HBHB, 4.0);
                self.finish_callback_at(r, i, callbacks::RESTORE_HEADING_RATE);
            }
            wait_state::PLAYANIM_COWER | wait_state::PLAYANIM_HANDSUP | wait_state::PLAYANIM_HANDSCOWER => {
                let wait_anim = if new_state == wait_state::PLAYANIM_HANDSUP { anim::STD_HANDSUP } else { anim::STD_HANDSCOWER };
                let pd = self.ped_mut(r);
                pd.heading_rate = 0.0;
                pd.wait_timer = at(time.unwrap_or(3000));
                if let Some(a) = self.wait_anim(r, wait_anim, 4.0).and_then(|i| self.ped_assoc_at(r, i)) {
                    a.set_delete_callback(callbacks::FINISHED_WAIT);
                }
            }
            wait_state::PLAYANIM_DUCK | wait_state::PLAYANIM_TAXI | wait_state::PLAYANIM_CHAT => {
                let wait_anim = match new_state {
                    wait_state::PLAYANIM_DUCK => anim::STD_DUCK_DOWN,
                    wait_state::PLAYANIM_TAXI => anim::STD_HAILTAXI,
                    _ => anim::STD_CHAT,
                };
                self.ped_mut(r).wait_timer = at(time.unwrap_or(3000));
                if let Some(a) = self.wait_anim(r, wait_anim, 4.0).and_then(|i| self.ped_assoc_at(r, i)) {
                    a.flags &= !aflags::FADE_OUT_WHEN_DONE;
                    a.flags |= aflags::DELETE_FADED_OUT;
                    a.set_delete_callback(callbacks::FINISHED_WAIT);
                }
            }
            wait_state::FINISH_FLEE => {
                self.ped_mut(r).set_move_state(move_state::STILL);
                self.set_move_anim(px, r);
                let pd = self.ped_mut(r);
                pd.heading_rate = 0.0;
                pd.wait_timer = at(2500);
                let i = self.wait_anim(r, anim::STD_IDLE_TIRED, 4.0);
                self.finish_callback_at(r, i, callbacks::RESTORE_HEADING_RATE);
            }
            _ => {
                self.ped_mut(r).wait_state = wait_state::FALSE;
                self.restore_heading_rate(r);
                return;
            }
        }
        self.ped_mut(r).wait_state = new_state;
    }

    fn finish_callback_at(&mut self, r: EntityRef, index: Option<usize>, token: u64) {
        if let Some(a) = index.and_then(|i| self.ped_assoc_at(r, i)) {
            a.set_finish_callback(token);
        }
    }

    /// A random ped that walked into a wall or got stuck on the way to a car stops trying to get in.
    fn give_up_on_car(&mut self, px: &mut Px, r: EntityRef) {
        let pd = self.ped(r);
        if pd.objective == objective::ENTER_CAR_AS_PASSENGER && pd.char_created_by == RANDOM_CHAR && pd.ped_state == state::SEEK_CAR {
            self.clear_objective(px, r);
            self.restore_previous_state(px, r);
            let now = self.now();
            self.ped_mut(r).car_jack_timer = now.wrapping_add(30000);
        }
    }

    /// `CPed::Wait`.
    pub fn wait(&mut self, px: &mut Px, r: EntityRef) {
        if self.ped(r).dying_or_dead() {
            self.ped_mut(r).wait_state = wait_state::FALSE;
            self.restore_heading_rate(r);
            return;
        }
        let now = self.now();
        let timer = self.ped(r).wait_timer;
        let fade_out = |w: &mut World, anim_id: u32| {
            if let Some(a) = w.ped_assoc_mut(r, anim_id) {
                a.blend_delta = -8.0;
                a.flags |= aflags::DELETE_FADED_OUT;
            }
        };
        match self.ped(r).wait_state {
            wait_state::TRAFFIC_LIGHTS => {
                if now > timer && px.hooks.lights_for_peds_walk() {
                    let pd = self.ped_mut(r);
                    pd.wait_state = wait_state::FALSE;
                    pd.set_move_state(move_state::WALK);
                }
            }
            wait_state::CROSS_ROAD => {
                if now > timer {
                    if px.rng.number() & 1 != 0 || timer == 0 {
                        self.ped_mut(r).wait_state = wait_state::FALSE;
                    } else {
                        self.set_wait_state(px, r, wait_state::CROSS_ROAD_LOOK, None);
                    }
                    fade_out(self, anim::STD_IDLE_HBHB);
                }
            }
            wait_state::CROSS_ROAD_LOOK => {
                if now > timer {
                    self.ped_mut(r).wait_state = wait_state::FALSE;
                    fade_out(self, anim::STD_ROADCROSS);
                }
            }
            wait_state::DOUBLEBACK => {
                if now <= timer {
                    let left = timer - now;
                    if left < 2500 && left > 2000 {
                        self.ped_mut(r).wait_timer -= 500;
                        self.wait_anim(r, anim::STD_XPRESS_SCRATCH, 4.0);
                    }
                } else {
                    let pd = self.ped_mut(r);
                    pd.wait_state = wait_state::FALSE;
                    pd.set_move_state(move_state::WALK);
                }
            }
            wait_state::HITWALL => {
                let pd = self.ped_mut(r);
                if now <= timer {
                    if pd.colliding_thing_timer > now {
                        pd.colliding_thing_timer = now.wrapping_add(2500);
                    }
                } else {
                    pd.wait_state = wait_state::FALSE;
                }
            }
            wait_state::TURN180 => {
                if now > timer {
                    let pd = self.ped_mut(r);
                    pd.wait_state = wait_state::FALSE;
                    pd.set_move_state(move_state::WALK);
                    pd.rotation_cur += PI;
                    if pd.ped_state == state::INVESTIGATE {
                        self.clear_investigate_event(px, r);
                    }
                }
                let pd = self.ped_mut(r);
                if pd.colliding_thing_timer > now {
                    pd.colliding_thing_timer = now.wrapping_add(2500);
                }
            }
            wait_state::SURPRISE => {
                if now > timer {
                    if self.ped_assoc(r, anim::STD_HIT_WALL).is_some() {
                        let i = self.wait_anim(r, anim::STD_XPRESS_SCRATCH, 4.0);
                        self.finish_callback_at(r, i, callbacks::FINISHED_WAIT);
                        self.ped_mut(r).wait_timer = now.wrapping_add(5000);
                    } else {
                        self.ped_mut(r).wait_state = wait_state::FALSE;
                    }
                }
            }
            wait_state::STUCK => self.wait_stuck(px, r),
            wait_state::LOOK_ABOUT => {
                if now > timer {
                    self.ped_mut(r).wait_state = wait_state::FALSE;
                    fade_out(self, anim::STD_IDLE_HBHB);
                }
            }
            s @ (wait_state::PLAYANIM_HANDSUP | wait_state::PLAYANIM_HANDSCOWER) => {
                let must_have = if s == wait_state::PLAYANIM_HANDSUP { anim::STD_HANDSUP } else { anim::STD_HANDSCOWER };
                self.wait_hands(px, r, must_have);
            }
            s @ (wait_state::PLAYANIM_COWER | wait_state::PLAYANIM_DUCK | wait_state::PLAYANIM_TAXI | wait_state::PLAYANIM_CHAT) => {
                let must_have = match s {
                    wait_state::PLAYANIM_COWER => anim::STD_HANDSCOWER,
                    wait_state::PLAYANIM_DUCK => anim::STD_DUCK_DOWN,
                    wait_state::PLAYANIM_TAXI => anim::STD_HAILTAXI,
                    _ => anim::STD_CHAT,
                };
                if now > timer {
                    if let Some(a) = self.ped_assoc_mut(r, must_have) {
                        a.blend_delta = -4.0;
                        a.flags |= aflags::DELETE_FADED_OUT;
                    }
                    self.ped_mut(r).wait_state = wait_state::FALSE;
                }
            }
            wait_state::FINISH_FLEE => {
                if self.ped_assoc(r, anim::STD_IDLE_TIRED).is_some() {
                    if now > timer {
                        if let Some(a) = self.ped_assoc_mut(r, anim::STD_IDLE_TIRED) {
                            a.flags |= aflags::DELETE_FADED_OUT;
                        }
                        let group = self.ped(r).anim_group as usize;
                        self.ped_blend_anim(r, group, anim::STD_IDLE, 4.0);
                        self.ped_mut(r).wait_state = wait_state::FALSE;
                        self.set_wait_state(px, r, wait_state::CROSS_ROAD_LOOK, Some(2000));
                    }
                } else {
                    self.ped_mut(r).wait_state = wait_state::FALSE;
                }
            }
            _ => {}
        }
        if self.ped(r).wait_state == wait_state::FALSE {
            self.restore_heading_rate(r);
        }
    }

    /// `WAITSTATE_STUCK` of `CPed::Wait`.
    fn wait_stuck(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        if now <= self.ped(r).wait_timer {
            return;
        }
        let found = [anim::STD_IDLE_TIRED, anim::STD_TURN180, anim::STD_XPRESS_SCRATCH, anim::STD_ROADCROSS].into_iter().find_map(|id| self.ped_assoc(r, id).map(|a| (a.anim_id, a.is_partial())));
        if let Some((id, partial)) = found {
            if partial {
                if let Some(a) = self.ped_assoc_mut(r, id as u32) {
                    a.blend_delta = -8.0;
                    a.flags |= aflags::DELETE_FADED_OUT;
                }
            } else {
                if let Some(a) = self.ped_assoc_mut(r, id as u32) {
                    a.flags |= aflags::DELETE_FADED_OUT;
                }
                let group = self.ped(r).anim_group as usize;
                self.ped_blend_anim(r, group, anim::STD_IDLE, 4.0);
            }
            if id == anim::STD_TURN180 as i32 {
                let pd = self.ped_mut(r);
                pd.rotation_cur = limit_radian_angle(PI + pd.rotation_cur);
                pd.wait_state = wait_state::FALSE;
                pd.set_move_state(move_state::WALK);
                pd.stored_move_state = move_state::NONE as i32;
                pd.panic_counter = 0;
                return;
            }
        }
        let to_play = match px.rng.number() & 3 {
            0 => anim::STD_ROADCROSS,
            1 => anim::STD_IDLE_TIRED,
            2 => anim::STD_XPRESS_SCRATCH,
            _ => anim::STD_TURN180,
        };
        let i = self.wait_anim(r, to_play, 4.0);
        if to_play == anim::STD_TURN180 {
            self.finish_callback_at(r, i, callbacks::FINISHED_WAIT);
        }
        let wait = px.rng.range_i32(1500, 5000);
        self.ped_mut(r).wait_timer = now.wrapping_add(wait as u32);
    }

    /// `WAITSTATE_PLAYANIM_HANDSUP` and `_HANDSCOWER` of `CPed::Wait`: hold the pose, then react to whoever scared the ped.
    fn wait_hands(&mut self, px: &mut Px, r: EntityRef, must_have: u32) {
        let now = self.now();
        let has_anim = self.ped_assoc(r, must_have).is_some();
        let pd = self.ped(r);
        let target = pd.look_target.filter(|&t| self.is_ped(t));
        let held_at_gunpoint = target.is_none_or(|t| self.ped(t).point_gun_at.is_some());
        if held_at_gunpoint && !matches!(pd.ped_state, state::FLEE_ENTITY | state::ATTACK) && now <= pd.wait_timer && has_anim {
            self.turn_body(r);
            return;
        }
        let pd = self.ped_mut(r);
        pd.wait_state = wait_state::FALSE;
        pd.wait_timer = 0;
        if let Some(target) = target
            && !matches!(self.ped(r).ped_state, state::FLEE_ENTITY | state::ATTACK)
        {
            let fear = i32::from(self.ents.ped_stats.0[self.ped(r).ped_stats].fear);
            let their_temper = i32::from(self.ents.ped_stats.0[self.ped(target).ped_stats].temper);
            let pd = self.ped(r);
            if fear <= 100 - their_temper {
                if pd.weapons[pd.current_weapon as usize].is_type_melee() {
                    self.set_objective_entity(px, r, objective::FLEE_CHAR_ON_FOOT_TILL_SAFE, target);
                    let pd = self.ped_mut(r);
                    if matches!(pd.ped_state, state::FLEE_ENTITY | state::FLEE_POS) {
                        pd.flags.use_ped_node_seek = true;
                        pd.next_path_node = None;
                    }
                    if pd.move_state != move_state::RUN {
                        pd.set_move_state(move_state::WALK);
                    }
                    if pd.ped_type != super::types::ped_type::COP {
                        self.process_objective(px, r);
                        self.ped_mut(r).set_move_state(move_state::WALK);
                    }
                } else {
                    self.set_objective_entity(px, r, objective::KILL_CHAR_ON_FOOT, target);
                    self.ped_mut(r).set_objective_timer(20000, now);
                }
            } else {
                self.set_objective_entity(px, r, objective::FLEE_CHAR_ON_FOOT_TILL_SAFE, target);
                let pd = self.ped_mut(r);
                if matches!(pd.ped_state, state::FLEE_ENTITY | state::FLEE_POS) {
                    pd.flags.use_ped_node_seek = true;
                    pd.next_path_node = None;
                }
                pd.set_move_state(move_state::RUN);
                self.say(px, r, sound::PED_FLEE_RUN);
            }
        }
        if let Some(a) = self.ped_assoc_mut(r, must_have) {
            a.blend_delta = -4.0;
            a.flags |= aflags::DELETE_FADED_OUT;
        }
    }
}
