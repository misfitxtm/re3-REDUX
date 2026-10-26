//! `Ped.cpp`: chatting with another ped and using a phone box.
use super::ctx::Px;
use super::data::*;
use crate::anim::{assoc_group, flags as aflags, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::EntityRef;
use crate::general::{limit_angle, radian_angle_between_points};
use crate::world::World;
use glam::Vec2;

impl World {
    /// `CPed::SetChat`.
    pub fn set_chat(&mut self, r: EntityRef, chat_with: EntityRef, time: u32) {
        if self.ped(r).ped_state != state::CHAT {
            self.set_stored_state(r);
        }
        let pd = self.ped_mut(r);
        pd.ped_state = state::CHAT;
        pd.set_move_state(move_state::STILL);
        pd.look_timer = 0;
        self.set_look_flag_entity(r, chat_with, true);
        let now = self.now();
        let pd = self.ped_mut(r);
        pd.chat_timer = now.wrapping_add(time);
        pd.look_timer = now.wrapping_add(3000);
    }

    /// `CPed::Chat`.
    pub fn chat(&mut self, px: &mut Px, r: EntityRef) {
        if self.ped(r).flags.is_looking && self.turn_body(r) {
            self.clear_look_flag(px, r);
        }
        let Some(partner) = self.ped(r).look_target.filter(|&t| self.is_ped(t)) else {
            self.clear_chat(px, r);
            return;
        };
        if self.ped(partner).ped_state != state::CHAT {
            self.clear_chat(px, r);
            let p = self.ped(partner);
            if let Some(victim) = p.ped_in_objective
                && matches!(p.objective, objective::KILL_CHAR_ON_FOOT | objective::FLEE_CHAR_ON_FOOT_TILL_SAFE)
            {
                self.react_to_attack(px, r, victim);
            }
            return;
        }
        let idle_playing = |w: &World| w.ped_first_assoc(r, aflags::IDLE).is_some();
        if self.ped(r).flags.is_talking {
            if px.rng.number() < 512 {
                if let Some(a) = self.ped_assoc_mut(r, anim::STD_CHAT) {
                    a.blend_delta = -4.0;
                    a.flags |= aflags::DELETE_FADED_OUT;
                }
                self.ped_mut(r).flags.is_talking = false;
            } else {
                self.say(px, r, sound::PED_CHAT);
            }
        } else {
            if px.rng.number() < 20 && !idle_playing(self) {
                self.ped_blend_anim(r, assoc_group::STD, anim::STD_XPRESS_SCRATCH, 4.0);
            }
            if !self.ped(r).flags.is_talking && !idle_playing(self) {
                let i = self.ped_blend_anim(r, assoc_group::STD, anim::STD_CHAT, 4.0);
                let chat_time = px.rng.range_f32(0.0, 3.0);
                if let Some(a) = i.and_then(|i| self.ped_assoc_at(r, i)) {
                    a.set_current_time(chat_time);
                }
                self.ped_mut(r).flags.is_talking = true;
                self.say(px, r, sound::PED_CHAT);
            }
        }
        let (timer, now) = (self.ped(r).chat_timer, self.now());
        if timer != 0 && now > timer {
            self.clear_chat(px, r);
            self.ped_mut(r).chat_timer = now.wrapping_add(30000);
        }
    }

    /// `CPed::ClearChat`.
    pub fn clear_chat(&mut self, px: &mut Px, r: EntityRef) {
        if let Some(a) = self.ped_assoc_mut(r, anim::STD_CHAT) {
            a.blend_delta = -8.0;
            a.flags |= aflags::DELETE_FADED_OUT;
        }
        self.ped_mut(r).flags.is_talking = false;
        self.clear_look_flag(px, r);
        self.restore_previous_state(px, r);
    }

    /// `CPed::FacePhone` (the build without PEDS_REPORT_CRIMES_ON_PHONE).
    pub fn face_phone(&mut self, px: &mut Px, r: EntityRef) -> bool {
        let pos = self.epos(r);
        let phone = px.hooks.phone_position(self.ped(r).phone_id);
        let current_rot = self.ped(r).rotation_cur.to_degrees();
        let mut phone_dir = radian_angle_between_points(phone.x, phone.y, pos.x, pos.y);
        self.set_look_flag_direction(r, phone_dir, false);
        phone_dir = limit_angle(phone_dir);
        self.ped_mut(r).moved = Vec2::ZERO;
        if current_rot - 180.0 > phone_dir {
            phone_dir += 2.0 * 180.0;
        } else if 180.0 + current_rot < phone_dir {
            phone_dir -= 2.0 * 180.0;
        }
        let needed_turn = current_rot - phone_dir;
        if needed_turn.abs() <= 0.75 {
            self.set_idle(px, r);
            self.clear_look_flag(px, r);
            let now = self.now();
            self.ped_mut(r).phone_talk_timer = now.wrapping_add(10000);
            true
        } else {
            self.ped_mut(r).rotation_cur = (current_rot - needed_turn * 0.2).to_radians();
            false
        }
    }

    /// `CPed::MakePhonecall`: true once the call is over.
    pub fn make_phonecall(&mut self, px: &mut Px, r: EntityRef) -> bool {
        if self.now() <= self.ped(r).phone_talk_timer {
            return false;
        }
        self.set_idle(px, r);
        let id = self.ped(r).phone_id;
        px.hooks.free_phone(id);
        self.ped_mut(r).phone_id = -1;
        true
    }
}
