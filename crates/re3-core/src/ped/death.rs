//! `Ped.cpp`: dying, being dead and being removed from the world.
use super::callbacks;
use super::ctx::Px;
use super::data::*;
use crate::anim::{assoc_group, flags as aflags, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::{status, EntityRef};
use crate::weapons::info::weapon_type as wt;
use crate::world::World;

impl World {
    /// `CPed::SetDie`.
    pub fn set_die(&mut self, px: &mut Px, r: EntityRef, anim_id: u32, mut delta: f32, speed: f32) {
        if self.ents.player_ped == Some(r) && self.ped(r).player.as_ref().is_some_and(|p| !p.can_be_damaged) {
            return;
        }
        self.ped_mut(r).threat_entity = None;
        if self.ped(r).dying_or_dead() {
            return;
        }
        if matches!(self.ped(r).ped_state, state::FALL | state::GETUP) {
            delta *= 0.5;
        }
        self.set_stored_state(r);
        self.clear_all(px, r);
        let pd = self.ped_mut(r);
        pd.health = 0.0;
        if pd.ped_state == state::DRIVING {
            if !pd.is_player() {
                self.flag_to_destroy_when_next_processed(r);
            }
        } else if pd.in_vehicle {
            if let Some(id) = pd.vehicle_anim
                && let Some(a) = self.ped_assoc_mut(r, id as u32)
            {
                a.blend_delta = -1000.0;
            }
        } else if pd.entering_car() {
            self.quit_entering_car(px, r);
        }
        self.ped_mut(r).ped_state = state::DIE;
        if anim_id == anim::STD_NUM {
            self.ped_mut(r).flags.is_ped_die_anim_playing = false;
        } else if let Some(i) = self.ped_blend_anim(r, assoc_group::STD, anim_id, delta)
            && let Some(a) = self.ped_assoc_at(r, i)
        {
            if speed > 0.0 {
                a.speed = speed;
            }
            a.flags &= !aflags::FADE_OUT_WHEN_DONE;
            if a.is_running() {
                a.set_finish_callback(callbacks::FINISH_DIE_ANIM);
                self.ped_mut(r).flags.is_ped_die_anim_playing = true;
            }
        }
        self.say(px, r, sound::PED_DEATH);
        if matches!(self.ped(r).last_ped_state, state::ENTER_CAR | state::CARJACK) {
            self.quit_entering_car(px, r);
        }
        if !self.ped(r).in_vehicle {
            self.stop_non_partial_anims(r);
        }
        let now = self.now();
        self.ped_mut(r).bloody_footprint_count_or_death_time = now;
    }

    /// `CPed::FinishDieAnimCB`.
    pub fn finish_die_anim_cb(&mut self, r: EntityRef) {
        self.ped_mut(r).flags.is_ped_die_anim_playing = false;
    }

    /// `CPed::SetDead`.
    pub fn set_dead(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        {
            let e = self.get_mut(r).expect("ped");
            e.base.flags.uses_collision = false;
            if matches!(&e.kind, crate::entity::Kind::Ped(p) if p.ped_state == state::DRIVING) {
                e.base.flags.is_visible = false;
            }
        }
        let pd = self.ped_mut(r);
        pd.health = 0.0;
        pd.ped_state = state::DEAD;
        pd.vehicle_anim = None;
        pd.colliding_entity = None;
        self.remove_weapon_model(r);
        self.ped_mut(r).current_weapon = wt::UNARMED as u8;
        px.hooks.register_event(event_type::INJURED_PED, r, 250);
        if self.ents.player_ped != Some(r) {
            px.hooks.create_dead_ped_pickups(r);
        }
        let pd = self.ped_mut(r);
        pd.bloody_footprint_count_or_death_time = now;
        pd.dead_bleeding = false;
        pd.flags.do_bloody_footprints = false;
        pd.flags.veh_exit_will_be_instant = false;
        px.hooks.register_event(event_type::DEAD_PED, r, 1000);
    }

    /// `CPed::FlagToDestroyWhenNextProcessed`.
    pub fn flag_to_destroy_when_next_processed(&mut self, r: EntityRef) {
        self.get_mut(r).expect("ped").base.flags.remove_from_world = true;
        let pd = self.ped(r);
        let (Some(veh), true) = (pd.my_vehicle, pd.in_vehicle) else { return };
        let player = pd.is_player();
        let mission = pd.char_created_by == MISSION_CHAR;
        if let Some(v) = self.try_veh_mut(veh) {
            if v.driver == Some(r) {
                let cur = self.get(veh).map_or(status::SIMPLE, |e| e.base.status);
                let wrecked = cur == status::WRECKED;
                if let Some(v) = self.try_veh_mut(veh) {
                    v.driver = None;
                }
                if player && !wrecked {
                    self.get_mut(veh).expect("vehicle").base.status = status::ABANDONED;
                }
            } else {
                v.remove_passenger(r);
            }
        }
        let pd = self.ped_mut(r);
        pd.in_vehicle = false;
        pd.my_vehicle = None;
        pd.ped_state = if mission { state::DEAD } else { state::NONE };
        pd.vehicle_anim = None;
    }
}
