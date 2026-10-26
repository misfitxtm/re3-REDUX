//! `Ped.cpp`: stepping and diving out of the way of a car or a threat.
use super::callbacks;
use super::ctx::Px;
use crate::control::wanted::crime;
use super::data::*;
use super::types::{ped_type, stat_flag};
use crate::anim::{assoc_group, flags as aflags, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::EntityRef;
use crate::general::{limit_radian_angle, radian_angle_between_points};
use crate::world::World;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

impl World {
    /// `CPed::SetEvasiveStep`.
    pub fn set_evasive_step(&mut self, px: &mut Px, r: EntityRef, reason: EntityRef, mut anim_type: u8) {
        let pd = self.ped(r);
        if pd.ped_state == state::STEP_AWAY || !pd.is_ped_in_control() || ((pd.is_player() || !pd.flags.responds_to_threats) && anim_type == 0) {
            return;
        }
        let (reason_pos, pos) = (self.epos(reason), self.epos(r));
        let mut angle_to_face = limit_radian_angle(radian_angle_between_points(reason_pos.x, reason_pos.y, pos.x, pos.y));
        let cur = limit_radian_angle(pd.rotation_cur);
        self.ped_mut(r).rotation_cur = cur;
        let mut needed_turn = (angle_to_face - cur).abs();
        if needed_turn > PI {
            needed_turn = TAU - needed_turn;
        }
        let mut veh_pressed_horn = false;
        if self.try_veh(reason).is_some_and(|v| v.is_car() && v.car_horn_timer != 0) {
            veh_pressed_horn = true;
            if !self.ped(r).is_player() {
                anim_type = 1;
            }
        }
        let rc_bandit = self.ent(reason).base.model_index == self.ents.indices.rc_bandit();
        if !(needed_turn <= 90.0_f32.to_radians() || rc_bandit || veh_pressed_horn || anim_type != 0) {
            return;
        }
        self.set_look_flag_entity(r, reason, true);
        let coin = px.rng.number() & 1 != 0;
        let step_anim = if coin && !rc_bandit && anim_type == 0 {
            anim::STD_HAILTAXI
        } else {
            let ms = self.ent(reason).phys.as_ref().map_or(glam::Vec3::ZERO, |p| p.move_speed);
            let danger_direction = radian_angle_between_points(ms.x, ms.y, 0.0, 0.0);
            // turn our back to the reason
            angle_to_face += PI;
            if angle_to_face > PI {
                angle_to_face -= TAU;
            }
            // and don't run the way the car is going: jump 90 degrees to the side
            let danger_zone = limit_radian_angle(angle_to_face - danger_direction);
            angle_to_face = if danger_zone > 0.0 { danger_direction - FRAC_PI_2 } else { danger_direction + FRAC_PI_2 };
            match anim_type {
                0 | 1 => anim::STD_EVADE_STEP,
                2 => anim::STD_HANDSCOWER,
                _ => anim::STD_NUM,
            }
        };
        if self.ped_assoc(r, step_anim).is_some() {
            return;
        }
        let Some(i) = self.ped_blend_anim(r, assoc_group::STD, step_anim, 8.0) else { return };
        if let Some(a) = self.ped_assoc_at(r, i) {
            a.flags &= !aflags::DELETE_FADED_OUT;
            a.set_finish_callback(callbacks::PED_EVADE);
        }
        if anim_type == 0 {
            self.say(px, r, sound::PED_EVADE);
        }
        self.ped_mut(r).rotation_cur = limit_radian_angle(angle_to_face);
        self.clear_aim_flag(r);
        self.set_stored_state(r);
        self.ped_mut(r).ped_state = state::STEP_AWAY;
    }

    /// `CPed::SetEvasiveDive`.
    pub fn set_evasive_dive(&mut self, px: &mut Px, r: EntityRef, reason: EntityRef, only_random_jump: bool) {
        let pd = self.ped(r);
        if !pd.is_ped_in_control() || !pd.flags.responds_to_threats {
            return;
        }
        let player = pd.is_player();
        let mut only_random_jump = only_random_jump;
        let mut hands_up = false;
        if self.try_veh(reason).is_some_and(|v| v.is_car() && v.car_horn_timer != 0) && !player {
            only_random_jump = true;
        }
        let ms = self.ent(reason).phys.as_ref().map_or(glam::Vec3::ZERO, |p| p.move_speed);
        let veh_direction = radian_angle_between_points(ms.x, ms.y, 0.0, 0.0);
        let angle_to_face = if only_random_jump {
            // jump direction picked at random, no hands up and no sound
            limit_radian_angle((px.rng.number() & 1) as f32 * PI + (-0.5 * PI) + veh_direction)
        } else {
            if player {
                let p = self.ped_mut(r).player.get_or_insert_with(Default::default);
                p.evade_amount = 5;
                p.evading_from = Some(reason);
                return;
            }
            let (reason_pos, pos) = (self.epos(reason), self.epos(r));
            let mut angle_to_face = limit_radian_angle(radian_angle_between_points(reason_pos.x, reason_pos.y, pos.x, pos.y));
            let cur = limit_radian_angle(self.ped(r).rotation_cur);
            self.ped_mut(r).rotation_cur = cur;
            // FIX_BUGS: dive away from the car's path, as the step does
            angle_to_face += PI;
            if angle_to_face > PI {
                angle_to_face -= TAU;
            }
            let danger_zone = limit_radian_angle(angle_to_face - veh_direction);
            angle_to_face = if danger_zone > 0.0 { 0.5 * PI + veh_direction } else { veh_direction - 0.5 * PI };
            let mut needed_turn = (angle_to_face - cur).abs();
            if needed_turn > PI {
                needed_turn = TAU - needed_turn;
            }
            if needed_turn <= 0.5 * PI {
                if px.rng.number() & 1 != 0 {
                    hands_up = true;
                }
            } else if px.rng.number() & 7 != 0 {
                return;
            }
            self.say(px, r, sound::PED_EVADE);
            angle_to_face
        };

        let no_dive = self.ents.ped_stats.0[self.ped(r).ped_stats].flags & stat_flag::NO_DIVE != 0;
        if hands_up || (!player && no_dive) {
            self.ped_mut(r).rotation_cur = angle_to_face;
            self.clear_look_flag(px, r);
            self.clear_aim_flag(r);
            self.set_look_flag_entity(r, reason, true);
            if self.ped_assoc(r, anim::STD_HANDSUP).is_some() {
                return;
            }
            if let Some(i) = self.ped_blend_anim(r, assoc_group::STD, anim::STD_HANDSUP, 8.0)
                && let Some(a) = self.ped_assoc_at(r, i)
            {
                a.flags &= !aflags::DELETE_FADED_OUT;
                a.set_finish_callback(callbacks::PED_EVADE);
            }
            self.set_stored_state(r);
            self.ped_mut(r).ped_state = state::STEP_AWAY;
        } else {
            self.ped_mut(r).rotation_cur = angle_to_face;
            self.clear_look_flag(px, r);
            self.clear_aim_flag(r);
            self.set_stored_state(r);
            self.ped_mut(r).ped_state = state::DIVE_AWAY;
            if let Some(i) = self.ped_blend_anim(r, assoc_group::STD, anim::STD_EVADE_DIVE, 8.0)
                && let Some(a) = self.ped_assoc_at(r, i)
            {
                a.set_finish_callback(callbacks::PED_EVADE);
            }
        }

        if self.ped(r).ped_type == ped_type::COP
            && let Some(v) = self.try_veh(reason)
            && v.driver.is_some_and(|d| self.try_ped(d).is_some_and(|p| p.is_player()))
        {
            let pos = self.epos(r);
            self.register_crime_immediately(px, crime::RECKLESS_DRIVING, pos, r);
            self.register_crime_immediately(px, crime::SPEEDING, pos, r);
        }
    }

    /// `CPed::PedEvadeCB`; `anim_id` is `None` when the original is called without an animation.
    pub fn ped_evade_cb(&mut self, px: &mut Px, r: EntityRef, anim_id: Option<i32>) {
        let evading = |w: &World| matches!(w.ped(r).ped_state, state::DIVE_AWAY | state::STEP_AWAY);
        let Some(id) = anim_id else {
            self.clear_look_flag(px, r);
            if evading(self) {
                self.restore_previous_state(px, r);
            }
            return;
        };
        let Some((flags, blend_delta)) = self.ped_clump(r).and_then(|c| c.anim.association(id)).map(|a| (a.flags, a.blend_delta)) else { return };
        if id == anim::STD_EVADE_DIVE as i32 {
            self.ped_mut(r).flags.update_anim_heading = true;
            self.clear_look_flag(px, r);
            if self.ped(r).ped_state == state::DIVE_AWAY {
                let now = self.now();
                let pd = self.ped_mut(r);
                pd.get_up_timer = now.wrapping_add(1);
                pd.ped_state = state::FALL;
            }
            if let Some(a) = self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(id)) {
                a.flags &= !aflags::FADE_OUT_WHEN_DONE;
                a.flags |= aflags::DELETE_FADED_OUT;
            }
        } else if flags & aflags::FADE_OUT_WHEN_DONE != 0 {
            self.clear_look_flag(px, r);
            if evading(self) {
                self.restore_previous_state(px, r);
            }
        } else if self.ped(r).ped_state != state::ARRESTED {
            if let Some(a) = self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(id)) {
                a.flags |= aflags::DELETE_FADED_OUT;
                if blend_delta >= 0.0 {
                    a.blend_delta = -4.0;
                }
            }
            self.clear_look_flag(px, r);
            if evading(self) {
                self.restore_previous_state(px, r);
            }
        }
    }
}
