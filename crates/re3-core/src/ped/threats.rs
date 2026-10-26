//! `Ped.cpp`: looking round for threats, and for things that make a wandering ped stop and do something.
use super::ctx::Px;
use super::data::*;
use super::types::{ped_flag, ped_type};
use crate::audio::sound;
use crate::entity::{status, EntityRef};
use crate::modelinfo::indices_gta3::mi;
use crate::vehicle::automobile::node;
use crate::world::{Kinds, World};

impl World {
    /// `CPed::ScanForThreats`: the flag of the kind of threat found, 0 for none.
    pub fn scan_for_threats(&mut self, px: &mut Px, r: EntityRef) -> u32 {
        let fear_flags = self.ped(r).fear_flags;
        let our_pos = self.epos(r);
        let mut closest_ped_dist = 60.0_f32;
        if fear_flags & ped_flag::EXPLOSION != 0
            && let Some(pos) = self.check_for_explosions(px, r)
        {
            self.ped_mut(r).event_or_threat = pos;
            return ped_flag::EXPLOSION;
        }
        if fear_flags & ped_flag::GUN != 0
            && let Some(shooter) = self.check_for_gun_shots(px, r)
        {
            let (ours, theirs) = (self.ped(r).ped_type, self.ped(shooter).ped_type);
            if ours != theirs || ours == ped_type::CIVMALE || ours == ped_type::CIVFEMALE {
                if !self.ped(r).is_gang_member() {
                    self.ped_mut(r).threat_entity = Some(shooter);
                    return ped_flag::GUN;
                }
                let flag = self.ents.ped_types.flag(theirs);
                if flag & fear_flags != 0 {
                    self.ped_mut(r).threat_entity = Some(shooter);
                    return flag;
                }
            }
        }
        if fear_flags & ped_flag::DEADPEDS != 0 && self.ped(r).char_created_by != MISSION_CHAR {
            let dead = self.check_for_dead_peds(px, r);
            if let Some(d) = dead.filter(|&d| {
                let p = self.epos(d);
                let (dx, dy, dz) = (p.x - our_pos.x, p.y - our_pos.y, p.z - our_pos.z);
                dx * dx + dy * dy + dz * dz < 20.0 * 20.0 && !self.get(d).and_then(|e| e.phys.as_ref()).is_some_and(|p| p.flags.is_in_water)
            }) {
                self.ped_mut(r).event_entity = Some(d);
                return ped_flag::DEADPEDS;
            }
        }
        let mut ped_to_fear: Option<EntityRef> = None;
        let pd = self.ped(r);
        let created_by = pd.char_created_by;
        let near: Vec<EntityRef> = pd.near_peds.iter().take(pd.num_near_peds as usize).flatten().copied().collect();
        for n in near {
            if created_by != RANDOM_CHAR || self.ped(n).char_created_by != MISSION_CHAR || self.ped(n).is_player() {
                let flags = self.ents.ped_types.flag(self.ped(n).ped_type);
                if flags & fear_flags != 0 && self.ped(n).health > 0.0 && self.our_ped_can_see_this_one(r, n) {
                    // FIX_BUGS: the distance to the ped, not to an explosion
                    let p = self.epos(n);
                    let dist_sq = (p.x - our_pos.x).powi(2) + (p.y - our_pos.y).powi(2);
                    if closest_ped_dist * closest_ped_dist > dist_sq {
                        closest_ped_dist = dist_sq.sqrt();
                        ped_to_fear = Some(n);
                    }
                }
            }
        }
        let kinds = Kinds { vehicles: true, ..Kinds::NONE };
        for veh in self.find_objects_in_range(our_pos, 20.0, true, 6, kinds) {
            let Some(d) = self.try_veh(veh).and_then(|v| v.driver) else { continue };
            let flags = self.ents.ped_types.flag(self.ped(d).ped_type);
            if flags & fear_flags != 0 && self.ped(d).health > 0.0 && self.our_ped_can_see_this_one(r, d) {
                let p = self.epos(d);
                let dist_sq = (p.x - our_pos.x).powi(2) + (p.y - our_pos.y).powi(2);
                if closest_ped_dist * closest_ped_dist > dist_sq {
                    closest_ped_dist = dist_sq.sqrt();
                    ped_to_fear = Some(d);
                }
            }
        }
        self.ped_mut(r).threat_entity = ped_to_fear;
        // FIX_BUGS: the flag of the ped found, not of the last one looked at
        ped_to_fear.map_or(0, |p| self.ents.ped_types.flag(self.ped(p).ped_type))
    }

    /// `CPed::ScanForInterestingStuff`: a bored ped picks a car to steal, a ped to mug, a friend to chat to...
    pub fn scan_for_interesting_stuff(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let pd = self.ped(r);
        if !pd.is_ped_in_control() || pd.objective != objective::NONE || pd.char_created_by == MISSION_CHAR {
            return;
        }
        self.look_for_sexy_peds(px, r);
        self.look_for_sexy_cars(r);
        if self.look_for_interesting_nodes(px, r) {
            return;
        }
        let pd = self.ped(r);
        if pd.ped_type == ped_type::CRIMINAL && pd.car_jack_timer < now {
            // steal a car or mug a ped
            if px.rng.number() % 100 < 10 {
                if self.criminal_picks_car(px, r) {
                    return;
                }
                self.ped_mut(r).car_jack_timer = now.wrapping_add(5000);
            } else if pd.objective != objective::MUG_CHAR && px.rng.number() & 7 == 0 {
                let pos = self.epos(r);
                let pd = self.ped(r);
                let near: Vec<EntityRef> = pd.near_peds.iter().take(pd.num_near_peds as usize).flatten().copied().collect();
                let mut to_mug = None;
                for n in near {
                    if (self.epos(n) - pos).length_squared() > 7.0 * 7.0 {
                        break;
                    }
                    let np = self.ped(n);
                    if matches!(np.ped_type, ped_type::CIVFEMALE | ped_type::CIVMALE | ped_type::CRIMINAL | ped_type::UNUSED1 | ped_type::PROSTITUTE)
                        && np.char_created_by != MISSION_CHAR
                        && np.is_ped_shootable()
                        && np.objective != objective::MUG_CHAR
                    {
                        to_mug = Some(n);
                        break;
                    }
                }
                if let Some(t) = to_mug {
                    self.set_objective_entity(px, r, objective::MUG_CHAR, t);
                }
                self.ped_mut(r).car_jack_timer = now.wrapping_add(5000);
            }
        }
        if self.ped(r).ped_state == state::WANDER_PATH && now > self.ped(r).chat_timer {
            let pd = self.ped(r);
            let near: Vec<EntityRef> = pd.near_peds.iter().take(pd.num_near_peds as usize).copied().flatten().collect();
            let pos = self.epos(r);
            // every second ped, as the original does
            for n in near.into_iter().step_by(2) {
                if self.ped(n).ped_state == state::WANDER_PATH && self.will_chat(r, n) {
                    let rolled_out = px.rng.range_i32(0, 100) >= 100;
                    if rolled_out || (pos - self.epos(n)).length() >= 1.8 {
                        self.ped_mut(r).chat_timer = now.wrapping_add(30000);
                    } else if self.can_see_entity(r, n, CAN_SEE_ENTITY_ANGLE_THRESHOLD) {
                        let time = (px.rng.number() % 4000 + 10000) as u32;
                        self.set_chat(r, n, time);
                        self.set_chat(n, r, time);
                        return;
                    }
                }
            }
        }
        // the parts below are elsewhere in Vice City
        let pd = self.ped(r);
        if !px.hooks.no_prostitutes() && pd.ped_type == ped_type::PROSTITUTE && pd.char_created_by != MISSION_CHAR && pd.objective_timer < now && !px.hooks.player_on_mission() {
            let pos = self.epos(r);
            let kinds = Kinds { vehicles: true, ..Kinds::NONE };
            for veh in self.find_objects_in_range(pos, CHECK_NEARBY_THINGS_MAX_DIST, true, 6, kinds) {
                let Some(v) = self.try_veh(veh) else { continue };
                if v.is_vehicle_normal(self.ent(veh).base.model_index, self.ent(veh).base.status) && v.is_car() && (pos - self.epos(veh)).length() < 5.0 && self.room_to_leave_car(veh, node::DOOR_LF as u32, None) {
                    self.set_objective_entity(px, r, objective::SOLICIT_VEHICLE, veh);
                    self.say(px, r, sound::PED_SOLICIT);
                    return;
                }
            }
        }
        if matches!(self.ped(r).ped_type, ped_type::CIVMALE | ped_type::CIVFEMALE) {
            let pos = self.epos(r);
            let kinds = Kinds { vehicles: true, ..Kinds::NONE };
            for veh in self.find_objects_in_range(pos, CHECK_NEARBY_THINGS_MAX_DIST, true, 6, kinds) {
                let e = self.ent(veh);
                if e.base.model_index == mi::MRWHOOP && e.base.status != status::ABANDONED && e.base.status != status::WRECKED && (pos - e.base.position()).length() < 5.0 {
                    self.set_objective_entity(px, r, objective::BUY_ICE_CREAM, veh);
                    return;
                }
            }
        }
    }

    /// The car-stealing half of `ScanForInterestingStuff` for a criminal: true if it set its mind on a car.
    fn criminal_picks_car(&mut self, px: &mut Px, r: EntityRef) -> bool {
        let now = self.now();
        let pos = self.epos(r);
        let kinds = Kinds { vehicles: true, ..Kinds::NONE };
        let (mut best, mut best_value) = (None, 0);
        for veh in self.find_objects_in_range(pos, 10.0, true, 6, kinds) {
            let Some(v) = self.try_veh(veh) else { continue };
            let speed = self.ent(veh).phys.as_ref().map_or(0.0, |p| p.move_speed.length());
            let value = self.ents.handling.get(v.handling).monetary_value;
            if v.created_by != crate::vehicle::base::MISSION_VEHICLE
                && speed <= 0.1
                && v.is_vehicle_normal(self.ent(veh).base.model_index, self.ent(veh).base.status)
                && v.is_car()
                && best_value < value
            {
                best = Some(veh);
                best_value = value;
            }
        }
        if let Some(v) = best.filter(|_| best_value > 2000) {
            self.set_objective_entity(px, r, objective::ENTER_CAR_AS_DRIVER, v);
            self.ped_mut(r).car_jack_timer = now.wrapping_add(5000);
            return true;
        }
        false
    }
}
