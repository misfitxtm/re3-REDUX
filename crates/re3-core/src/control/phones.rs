//! `Phones.cpp`: the phone boxes, the missions' phone calls and civilians reporting crimes.
use crate::anim::tables_gta3::anim;
use crate::anim::{assoc_group, flags as aflags};
use crate::entity::{EntityRef, PoolKind};
use crate::math::atan_of_xy;
use crate::ped::callbacks::{PHONE_PICK_UP_BASE, PHONE_PUT_DOWN};
use crate::ped::ctx::Px;
use crate::ped::data::{move_state, state};
use crate::world::World;
use glam::{Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2, TAU};

pub const NUMPHONES: usize = 50;

/// `PhoneState`.
pub mod phone_state {
    pub const FREE: u8 = 0;
    pub const REPORTING_CRIME: u8 = 1;
    pub const STATE_2: u8 = 2;
    pub const MESSAGE_REMOVED: u8 = 3;
    pub const ONETIME_MESSAGE_SET: u8 = 4;
    pub const REPEATED_MESSAGE_SET: u8 = 5;
    pub const REPEATED_MESSAGE_SHOWN_ONCE: u8 = 6;
    pub const ONETIME_MESSAGE_STARTED: u8 = 7;
    pub const REPEATED_MESSAGE_STARTED: u8 = 8;
    /// Only rings; the script handles the pick-up.
    pub const STATE_9: u8 = 9;
}

/// `CPhone`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Phone {
    pub pos: Vec3,
    pub messages: [Option<String>; 6],
    pub repeated_message_pickup_start: u32,
    /// The phone box building.
    pub entity: Option<EntityRef>,
    pub state: u8,
    pub visible_to_cam: bool,
}

/// `CPhoneInfo`.
#[derive(Debug, Clone, Default)]
pub struct PhoneInfo {
    pub max: usize,
    pub script_phones_max: usize,
    pub phones: Vec<Phone>,
    /// `bDisplayingPhoneMessage`: the phone is picked up and the messages are showing.
    pub displaying_phone_message: bool,
    pub phone_enable_controls_timer: u32,
    pub phone_displaying_messages: Option<usize>,
    pub picking_up_phone: bool,
    pub call_back_ped: Option<EntityRef>,
}

impl PhoneInfo {
    /// `FindNearestFreePhone`: within 60 m.
    pub fn find_nearest_free_phone(&self, pos: Vec3) -> Option<usize> {
        let mut nearest = None;
        let mut dist = 60.0;
        for (i, p) in self.phones.iter().enumerate().take(self.max) {
            if p.state == phone_state::FREE {
                let d = Vec2::new(p.pos.x - pos.x, p.pos.y - pos.y).length();
                if d < dist {
                    dist = d;
                    nearest = Some(i);
                }
            }
        }
        nearest
    }

    /// `PhoneAtThisPosition`.
    pub fn phone_at_this_position(&self, pos: Vec3) -> bool {
        self.phones.iter().take(self.max).any(|p| p.pos.x == pos.x && p.pos.y == pos.y)
    }

    /// `HasMessageBeenDisplayed`.
    pub fn has_message_been_displayed(&self, phone: usize) -> bool {
        !self.displaying_phone_message && matches!(self.phones[phone].state, phone_state::REPEATED_MESSAGE_SHOWN_ONCE | phone_state::ONETIME_MESSAGE_STARTED | phone_state::REPEATED_MESSAGE_STARTED)
    }

    /// `IsMessageBeingDisplayed`.
    pub fn is_message_being_displayed(&self, phone: usize) -> bool {
        self.phone_displaying_messages == Some(phone)
    }

    fn set_message(&mut self, phone: usize, messages: [Option<String>; 6], set_state: u8) {
        let p = &mut self.phones[phone];
        // with at least one message it is the first
        if messages[0].is_some() {
            p.messages = messages;
            p.state = set_state;
        } else {
            p.state = phone_state::MESSAGE_REMOVED;
        }
    }

    /// `SetPhoneMessage_JustOnce`.
    pub fn set_phone_message_just_once(&mut self, phone: usize, messages: [Option<String>; 6]) {
        self.set_message(phone, messages, phone_state::ONETIME_MESSAGE_SET);
    }

    /// `SetPhoneMessage_Repeatedly`.
    pub fn set_phone_message_repeatedly(&mut self, phone: usize, messages: [Option<String>; 6]) {
        self.set_message(phone, messages, phone_state::REPEATED_MESSAGE_SET);
    }

    /// `GrabPhone`: gives the script the phone nearest to the spot (within 100 m); the phone is not available again.
    pub fn grab_phone(&mut self, x: f32, y: f32) -> Option<usize> {
        let mut nearest = None;
        let mut dist = 100.0;
        for i in self.script_phones_max..self.max {
            let d = Vec2::new(self.phones[i].pos.x - x, self.phones[i].pos.y - y).length();
            if d < dist {
                dist = d;
                nearest = Some(i);
            }
        }
        let n = nearest?;
        self.phones[n].state = phone_state::MESSAGE_REMOVED;
        self.phones.swap(self.script_phones_max, n);
        self.script_phones_max += 1;
        Some(self.script_phones_max - 1)
    }

    /// `Shutdown`.
    pub fn shutdown(&mut self) {
        self.max = 0;
        self.script_phones_max = 0;
    }
}

impl World {
    /// `CPhoneInfo::Initialise`: every phone box building becomes a free phone.
    pub fn initialise_phones(&mut self) {
        let model = match &self.ents.indices {
            crate::modelinfo::Indices::Gta3(i) => i.get(crate::modelinfo::indices_gta3::slot::PHONEBOOTH1),
            _ => -1,
        };
        let pool = &self.ents.pools.buildings;
        let mut phones = vec![];
        let all: Vec<_> = pool.iter().collect();
        for (i, e) in all.into_iter().rev() {
            if e.base.model_index == model && phones.len() < NUMPHONES {
                phones.push(Phone { state: phone_state::FREE, pos: e.base.position(), entity: Some(EntityRef { pool: PoolKind::Building, handle: pool.handle_of(i) }), ..Default::default() });
            }
        }
        let p = &mut self.ents.phones;
        *p = PhoneInfo { max: phones.len(), phones, ..Default::default() };
    }

    /// `CPhoneInfo::Update`: the rings, the player walking up to a phone and the messages shown once picked up.
    pub fn update_phones(&mut self, px: &mut Px) {
        let Some(player) = self.ents.player_ped else { return };
        let now = self.now();
        let prev = self.ents.timer.previous_time_in_ms;
        if self.ents.phones.displaying_phone_message && now > self.ents.phones.phone_enable_controls_timer {
            self.make_player_safe(px, false);
            px.hooks.camera_widescreen(false);
            self.ents.phones.phone_displaying_messages = None;
            self.ents.phones.displaying_phone_message = false;
            let talking = self.ped_assoc(player, anim::STD_PHONE_TALK).is_some_and(|a| a.blend_amount > 0.5);
            if talking {
                if let Some(i) = self.ped_blend_anim(player, assoc_group::STD, anim::STD_PHONE_OUT, 8.0)
                    && let Some(a) = self.ped_assoc_at(player, i)
                {
                    a.flags &= !aflags::DELETE_FADED_OUT;
                    a.set_finish_callback(PHONE_PUT_DOWN);
                }
            } else {
                px.hooks.phone_controls(true);
                if self.ped(player).ped_state == state::MAKE_CALL {
                    self.ped_mut(player).ped_state = state::IDLE;
                }
            }
        }
        let (not_in_car, player_pos) = match self.find_player_vehicle() {
            Some(v) => (false, self.epos(v)),
            None => (true, self.epos(player)),
        };
        let cam = px.hooks.camera_position();
        for id in 0..self.ents.phones.script_phones_max {
            let phone = self.ents.phones.phones[id].clone();
            let Some(entity) = phone.entity else { continue };
            if phone.visible_to_cam {
                match phone.state {
                    phone_state::ONETIME_MESSAGE_SET | phone_state::REPEATED_MESSAGE_SET | phone_state::REPEATED_MESSAGE_SHOWN_ONCE => {
                        let (scratch, rings) = if self.ents.phones.picking_up_phone { (false, false) } else { ((now / 1880) % 2 == 1, (prev / 1880) % 2 == 1) };
                        self.shake_phone_box(px, entity, scratch, rings);
                        if not_in_car && !self.ents.phones.picking_up_phone && self.ped(player).is_ped_in_control() {
                            self.try_pick_up_phone(px, player, id, entity, phone.pos, player_pos);
                        }
                    }
                    phone_state::REPEATED_MESSAGE_STARTED => {
                        if now.wrapping_sub(phone.repeated_message_pickup_start) > 60000 {
                            self.ents.phones.phones[id].state = phone_state::REPEATED_MESSAGE_SHOWN_ONCE;
                        }
                    }
                    phone_state::STATE_9 => {
                        let (scratch, rings) = ((now / 1880) % 2 == 1, (prev / 1880) % 2 == 1);
                        self.shake_phone_box(px, entity, scratch, rings);
                    }
                    _ => {}
                }
                if Vec2::new(cam.x - phone.pos.x, cam.y - phone.pos.y).length_squared() > 100.0 * 100.0 {
                    self.ents.phones.phones[id].visible_to_cam = false;
                }
            } else {
                let seed = self.ent(entity).base.random_seed as u32;
                if self.ents.timer.frame_counter.wrapping_add(seed).is_multiple_of(16) && Vec2::new(cam.x - phone.pos.x, cam.y - phone.pos.y).length_squared() < 60.0 * 60.0 {
                    self.ents.phones.phones[id].visible_to_cam = true;
                }
            }
        }
    }

    /// A ringing phone box rattles and rings; a quiet one stands upright.
    fn shake_phone_box(&mut self, px: &mut Px, entity: EntityRef, scratch: bool, was_ringing: bool) {
        if scratch {
            self.get_mut(entity).expect("phone box").base.matrix.up.z = (px.rng.number() % 1024) as f32 / 16000.0 + 1.0;
            if !was_ringing {
                let pos = self.epos(entity);
                px.hooks.play_one_shot_script_object(crate::audio::script_sound::PAYPHONE_RINGING, pos);
            }
        } else {
            self.get_mut(entity).expect("phone box").base.matrix.up.z = 1.0;
        }
    }

    /// The player is at a ringing phone, facing it: he picks it up.
    fn try_pick_up_phone(&mut self, px: &mut Px, player: EntityRef, id: usize, entity: EntityRef, phone_pos: Vec3, player_pos: Vec3) {
        let dist = Vec2::new(player_pos.x - phone_pos.x, player_pos.y - phone_pos.y);
        if dist.x.abs() >= 1.0 || dist.y.abs() >= 1.0 {
            return;
        }
        let fwd = self.ent(entity).base.matrix.forward;
        if dist.dot(Vec2::new(fwd.x, fwd.y)) / dist.length() >= -0.85 {
            return;
        }
        let obj = self.epos(entity);
        let to_obj = Vec2::new(player_pos.x - obj.x, player_pos.y - obj.y);
        let mut angle = atan_of_xy(to_obj.x, to_obj.y) + FRAC_PI_2;
        if angle > TAU {
            angle -= TAU;
        }
        {
            let pd = self.ped_mut(player);
            pd.rotation_cur = angle;
            pd.rotation_dest = angle;
            pd.ped_state = state::MAKE_CALL;
        }
        self.get_mut(player).expect("player").base.set_heading(angle);
        px.hooks.phone_controls(false);
        px.hooks.camera_widescreen(true);
        self.make_player_safe(px, true);
        if let Some(i) = self.ped_blend_anim(player, assoc_group::STD, anim::STD_PHONE_IN, 4.0)
            && let Some(a) = self.ped_assoc_at(player, i)
        {
            a.set_finish_callback(PHONE_PICK_UP_BASE + id as u64);
        }
        self.ents.phones.picking_up_phone = true;
        self.ents.phones.call_back_ped = Some(player);
    }

    /// `PhonePickUpCB`: the messages start to show and the caller stands talking.
    pub fn phone_pick_up_cb(&mut self, px: &mut Px, ped: EntityRef, phone: usize) {
        let now = self.now();
        let mut display_time = 0;
        let messages: Vec<String> = self.ents.phones.phones[phone].messages.iter().flatten().cloned().collect();
        for m in &messages {
            px.hooks.add_message(m, 3000);
            display_time += 3000;
        }
        let info = &mut self.ents.phones;
        info.picking_up_phone = false;
        info.displaying_phone_message = true;
        info.phone_displaying_messages = Some(phone);
        info.phone_enable_controls_timer = now + display_time;
        let p = &mut info.phones[phone];
        if p.state == phone_state::ONETIME_MESSAGE_SET {
            p.state = phone_state::ONETIME_MESSAGE_STARTED;
        } else {
            p.state = phone_state::REPEATED_MESSAGE_STARTED;
            p.repeated_message_pickup_start = now;
        }
        let caller = self.ents.phones.call_back_ped.take().unwrap_or(ped);
        self.ped_mut(caller).set_move_state(move_state::STILL);
        self.ped_blend_anim(caller, assoc_group::STD, anim::STD_IDLE, 8.0);
        if self.ped_assoc(caller, anim::STD_PHONE_IN).is_none_or(|a| a.blend_amount > 0.5) {
            self.ped_blend_anim(caller, assoc_group::STD, anim::STD_PHONE_TALK, 8.0);
        }
    }

    /// `PhonePutDownCB`: the call is over and the player may move again.
    pub fn phone_put_down_cb(&mut self, px: &mut Px, ped: EntityRef) {
        px.hooks.phone_controls(true);
        let blend = self.ped_assoc(ped, anim::STD_PHONE_OUT).map_or(0.0, |a| a.blend_amount);
        if let Some(a) = self.ped_assoc_mut(ped, anim::STD_PHONE_OUT) {
            a.flags |= aflags::DELETE_FADED_OUT;
            a.blend_delta = -1000.0;
        }
        if blend > 0.5 {
            self.ped_mut(ped).flags.update_anim_heading = true;
        }
        if self.ped(ped).ped_state == state::MAKE_CALL {
            self.ped_mut(ped).ped_state = state::IDLE;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(positions: &[(f32, f32)]) -> PhoneInfo {
        let phones = positions.iter().map(|&(x, y)| Phone { pos: Vec3::new(x, y, 0.0), ..Default::default() }).collect::<Vec<_>>();
        PhoneInfo { max: phones.len(), phones, ..Default::default() }
    }

    fn msgs(first: Option<&str>) -> [Option<String>; 6] {
        [first.map(str::to_owned), None, None, None, None, None]
    }

    #[test]
    fn the_nearest_free_phone_within_60_metres_is_found() {
        let mut p = info(&[(10.0, 0.0), (30.0, 0.0), (100.0, 0.0)]);
        assert_eq!(p.find_nearest_free_phone(Vec3::ZERO), Some(0));
        p.phones[0].state = phone_state::REPORTING_CRIME;
        assert_eq!(p.find_nearest_free_phone(Vec3::ZERO), Some(1));
        p.phones[1].state = phone_state::MESSAGE_REMOVED;
        // the third is out of range
        assert_eq!(p.find_nearest_free_phone(Vec3::ZERO), None);
        assert!(p.phone_at_this_position(Vec3::new(30.0, 0.0, 99.0)) && !p.phone_at_this_position(Vec3::new(31.0, 0.0, 0.0)));
    }

    #[test]
    fn grabbing_moves_the_phone_to_the_script_part_of_the_list() {
        let mut p = info(&[(0.0, 0.0), (50.0, 50.0), (200.0, 0.0)]);
        assert_eq!(p.grab_phone(49.0, 49.0), Some(0));
        assert_eq!((p.script_phones_max, p.phones[0].pos.x, p.phones[0].state), (1, 50.0, phone_state::MESSAGE_REMOVED));
        // the one that was first took its place; nothing else is near this spot
        assert_eq!(p.phones[1].pos.x, 0.0);
        assert_eq!(p.grab_phone(1000.0, 1000.0), None);
    }

    #[test]
    fn messages_set_the_state_and_an_empty_set_removes_the_call() {
        let mut p = info(&[(0.0, 0.0)]);
        p.set_phone_message_just_once(0, msgs(Some("HELLO")));
        assert_eq!((p.phones[0].state, p.phones[0].messages[0].as_deref()), (phone_state::ONETIME_MESSAGE_SET, Some("HELLO")));
        p.set_phone_message_repeatedly(0, msgs(Some("AGAIN")));
        assert_eq!(p.phones[0].state, phone_state::REPEATED_MESSAGE_SET);
        p.set_phone_message_repeatedly(0, msgs(None));
        assert_eq!(p.phones[0].state, phone_state::MESSAGE_REMOVED);
        p.phones[0].state = phone_state::ONETIME_MESSAGE_STARTED;
        assert!(p.has_message_been_displayed(0));
        p.displaying_phone_message = true;
        assert!(!p.has_message_been_displayed(0));
        p.phone_displaying_messages = Some(0);
        assert!(p.is_message_being_displayed(0));
    }
}
