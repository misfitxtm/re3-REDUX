//! `Ped.cpp`: reacting to nearby events, looking at things worth looking at, and sitting in a car.
use super::ctx::{EventEntity, Px};
use super::pending::Investigate;
use super::data::*;
use super::types::ped_type;
use crate::anim::tables_gta3::anim;
use crate::audio::sound;
use crate::entity::EntityRef;
use crate::general::radian_angle_between_points;
use crate::math::{heading, Matrix};
use crate::world::{Kinds, World, BUILDINGS, OBJECTS, VEHICLES};
use glam::{Vec2, Vec3};
use re3_formats::datafile::EffectKind;
use std::f32::consts::FRAC_PI_2;

/// `CAR_POS_FRONTSEAT` and `CAR_POS_BACKSEAT` in the vehicle model's position table.
const CAR_POS_FRONTSEAT: usize = 2;
const CAR_POS_BACKSEAT: usize = 3;
const ATTRACTOR_ICECREAM: i32 = 0;
const ATTRACTOR_STARE: i32 = 1;

impl World {
    /// `CPed::CheckForExplosions`: the area of the closest explosion or fire.
    pub fn check_for_explosions(&mut self, px: &mut Px, r: EntityRef) -> Option<Vec2> {
        let pos = self.epos(r);
        if let Some(ev) = px.hooks.find_closest_event(event_type::EXPLOSION, pos) {
            let actual = match ev.entity {
                EventEntity::Ped(e) | EventEntity::Vehicle(e) | EventEntity::Object(e) => Some(e),
                EventEntity::None => None,
            };
            let pd = self.ped_mut(r);
            if let Some(e) = actual {
                pd.event_entity = Some(e);
            }
            pd.flags.gonna_investigate_event = actual.is_some();
            px.hooks.clear_event(ev.index);
            return Some(Vec2::new(ev.pos.x, ev.pos.y));
        }
        if let Some(ev) = px.hooks.find_closest_event(event_type::FIRE, pos) {
            px.hooks.clear_event(ev.index);
            self.ped_mut(r).flags.gonna_investigate_event = false;
            return Some(Vec2::new(ev.pos.x, ev.pos.y));
        }
        self.ped_mut(r).flags.gonna_investigate_event = false;
        None
    }

    /// `CPed::CheckForGunShots`: the ped that fired the closest shot.
    pub fn check_for_gun_shots(&mut self, px: &mut Px, r: EntityRef) -> Option<EntityRef> {
        let pos = self.epos(r);
        // peds don't walk towards the shots, so investigating stays off
        self.ped_mut(r).flags.gonna_investigate_event = false;
        match px.hooks.find_closest_event(event_type::GUNSHOT, pos)?.entity {
            EventEntity::Ped(p) => Some(p),
            _ => None,
        }
    }

    /// `CPed::CheckForDeadPeds`.
    pub fn check_for_dead_peds(&mut self, px: &mut Px, r: EntityRef) -> Option<EntityRef> {
        let pos = self.epos(r);
        let dead = match px.hooks.find_closest_event(event_type::DEAD_PED, pos).map(|e| e.entity) {
            Some(EventEntity::Ped(p)) => Some(p),
            _ => None,
        };
        self.ped_mut(r).flags.gonna_investigate_event = dead.is_some();
        dead
    }

    /// `CPed::LookForSexyPeds`.
    pub fn look_for_sexy_peds(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let pd = self.ped(r);
        if (!pd.is_ped_in_control() && pd.ped_state != state::DRIVING) || pd.look_timer >= now || (!pd.is_gang_member() && pd.ped_type != ped_type::CIVMALE) {
            return;
        }
        let our_sexiness = self.ents.ped_stats.0[pd.ped_stats].sexiness;
        for near in pd.near_peds.iter().take(pd.num_near_peds as usize).flatten().copied().collect::<Vec<_>>() {
            if !self.can_see_entity(r, near, CAN_SEE_ENTITY_ANGLE_THRESHOLD) || (self.epos(r) - self.epos(near)).length() >= 10.0 {
                continue;
            }
            let np = self.ped(near);
            // FIX_BUGS: prostitutes count too
            if self.ents.ped_stats.0[np.ped_stats].sexiness > our_sexiness && matches!(np.ped_type, ped_type::CIVFEMALE | ped_type::PROSTITUTE) {
                self.set_look_flag_entity(r, near, true);
                self.ped_mut(r).look_timer = now.wrapping_add(4000);
                self.say(px, r, sound::PED_CHAT_SEXY);
                return;
            }
        }
        self.ped_mut(r).look_timer = now.wrapping_add(10000);
    }

    /// `CPed::LookForSexyCars`.
    pub fn look_for_sexy_cars(&mut self, r: EntityRef) {
        let now = self.now();
        let pd = self.ped(r);
        if (!pd.is_ped_in_control() && pd.ped_state != state::DRIVING) || pd.look_timer >= now {
            return;
        }
        let (my_vehicle, pos) = (pd.my_vehicle, self.epos(r));
        let kinds = Kinds { vehicles: true, ..Kinds::NONE };
        let vehicles = self.find_objects_in_range(pos, 10.0, true, 6, kinds);
        let (mut found, mut best_price) = (0, 0);
        for (i, &v) in vehicles.iter().enumerate() {
            let value = self.try_veh(v).map_or(0, |d| self.ents.handling.get(d.handling).monetary_value);
            if Some(v) != my_vehicle && best_price < value {
                found = i;
                best_price = value;
            }
        }
        if !vehicles.is_empty() && best_price > 40000 {
            self.set_look_flag_entity(r, vehicles[found], false);
        }
        self.ped_mut(r).look_timer = now.wrapping_add(10000);
    }

    /// `CPed::LookForInterestingNodes`: an ice cream van or a graffiti to stop at; true if something was found.
    pub fn look_for_interesting_nodes(&mut self, px: &mut Px, r: EntityRef) -> bool {
        let now = self.now();
        let seed = self.ent(r).base.random_seed as u32;
        if self.ents.timer.frame_counter.wrapping_add(seed % 256) & 7 != 0 || now <= self.ped(r).chat_timer {
            return false;
        }
        let rand_val = (px.rng.number() % 256) as i32;
        let pos = self.epos(r);
        let g = self.grid;
        let (min_x, min_y) = (g.index_x(pos.x - CHECK_NEARBY_THINGS_MAX_DIST).max(0), g.index_y(pos.y - CHECK_NEARBY_THINGS_MAX_DIST).max(0));
        let (max_x, max_y) = (g.index_x(pos.x + CHECK_NEARBY_THINGS_MAX_DIST).min(g.num - 1), g.index_y(pos.y + CHECK_NEARBY_THINGS_MAX_DIST).min(g.num - 1));
        let mut found: Option<(Matrix, Vec3, i32, [f32; 3], i32)> = None;
        'search: for y in min_y..=max_y {
            for x in min_x..=max_x {
                let Some(sector) = self.sector(x, y) else { continue };
                for list in [VEHICLES, OBJECTS, BUILDINGS, BUILDINGS + 1] {
                    for &e in &sector.lists[list] {
                        let Some(ent) = self.get(e) else { continue };
                        let Some(model) = self.ents.models.get(ent.base.model_index as i32) else { continue };
                        for fx in &model.effects {
                            let EffectKind::Attractor { kind, dir, probability } = fx.kind else { continue };
                            if probability < rand_val {
                                continue;
                            }
                            let fx_pos = Vec3::from(fx.pos);
                            if (ent.base.matrix.transform_point(fx_pos) - pos).length_squared() < 8.0 * 8.0 {
                                found = Some((ent.base.matrix, fx_pos, kind, dir, probability));
                                break 'search;
                            }
                        }
                    }
                }
            }
        }
        let Some((m, fx_pos, kind, dir, probability)) = found else { return false };
        let front = m.right * dir[0] + m.forward * dir[1] + m.up * dir[2];
        let angle_to_face = radian_angle_between_points(front.x, front.y, 0.0, 0.0);
        let rand_val = px.rng.number() % 256;
        if rand_val as u32 <= seed % 256 {
            self.ped_mut(r).chat_timer = now.wrapping_add(2000);
            self.set_look_flag_direction(r, angle_to_face, true);
            self.set_look_timer(r, 1000);
            return false;
        }
        let at = m.transform_point(fx_pos);
        let at = Vec2::new(at.x, at.y);
        match kind {
            ATTRACTOR_ICECREAM => self.set_investigate_event(px, r, Investigate { event: event_type::ICECREAM, pos: at, distance_to_count_done: 0.1, time: 15000, angle: angle_to_face }),
            ATTRACTOR_STARE => {
                let time = px.rng.range_i32(8000, 10 * probability + 8500);
                self.set_investigate_event(px, r, Investigate { event: event_type::SHOPSTALL, pos: at, distance_to_count_done: 1.0, time, angle: angle_to_face });
            }
            _ => {}
        }
        true
    }

    /// `CPed::SetPedPositionInCar`: put the ped on its seat.
    pub fn set_ped_position_in_car(&mut self, r: EntityRef) {
        let Some(veh) = self.ped(r).my_vehicle else { return };
        if self.ped(r).flags.changed_seat {
            let waiting = [
                anim::STD_CAR_GET_IN_LHS,
                anim::STD_CAR_GET_IN_LO_LHS,
                anim::STD_CAR_CLOSE_DOOR_LHS,
                anim::STD_CAR_CLOSE_DOOR_LO_LHS,
                anim::STD_CAR_SHUFFLE_RHS,
                anim::STD_CAR_SHUFFLE_LO_RHS,
                anim::STD_VAN_CLOSE_DOOR_REAR_LHS,
                anim::STD_VAN_CLOSE_DOOR_REAR_RHS,
                anim::STD_VAN_GET_IN_REAR_LHS,
                anim::STD_VAN_GET_IN_REAR_RHS,
                anim::STD_COACH_GET_IN_LHS,
                anim::STD_COACH_GET_IN_RHS,
            ]
            .into_iter()
            .any(|a| self.ped_assoc(r, a).is_some());
            if waiting {
                self.line_up_ped_with_car(r, line_up::TO_CAR_START);
                self.ped_mut(r).flags.changed_seat = false;
                return;
            }
        }
        let model = self.ent(veh).base.model_index as i32;
        let Some(positions) = self.ents.models.get(model).and_then(|m| m.vehicle()).map(|v| v.positions) else { return };
        let Some(vd) = self.try_veh(veh) else { return };
        let (is_boat, is_bike, is_van) = (vd.is_boat(), vd.is_bike(), vd.flags.is_van);
        let seat = |p: Option<EntityRef>| p == Some(r);
        let mut seat_pos;
        if seat(vd.driver) {
            seat_pos = positions[CAR_POS_FRONTSEAT];
            if !is_boat && !is_bike {
                seat_pos.x = -seat_pos.x;
            }
        } else if seat(vd.passengers[0]) {
            seat_pos = positions[CAR_POS_FRONTSEAT];
        } else if seat(vd.passengers[1]) {
            seat_pos = positions[CAR_POS_BACKSEAT];
            seat_pos.x = -seat_pos.x;
        } else if seat(vd.passengers[2]) {
            seat_pos = positions[CAR_POS_BACKSEAT];
        } else {
            seat_pos = positions[CAR_POS_FRONTSEAT];
        }
        let (second, third) = (seat(vd.passengers[1]), seat(vd.passengers[2]));
        let mut new_mat = self.ent(veh).base.matrix;
        new_mat.pos += new_mat.right * seat_pos.x + new_mat.forward * seat_pos.y + new_mat.up * seat_pos.z;
        let veh_heading = heading(self.ent(veh).base.matrix.forward);
        // rear seats of a van face sideways
        let (rotation, turn) = if !is_van {
            (veh_heading, 0.0)
        } else if second {
            (veh_heading - FRAC_PI_2, -FRAC_PI_2)
        } else if third {
            (veh_heading + FRAC_PI_2, FRAC_PI_2)
        } else {
            (veh_heading, 0.0)
        };
        if turn != 0.0 {
            let mut t = Matrix::IDENTITY;
            t.rotate_z(turn);
            new_mat = new_mat * t;
        }
        self.ped_mut(r).rotation_cur = rotation;
        self.get_mut(r).expect("ped").base.matrix = new_mat;
    }
}
