//! `Ped.cpp`: walking to a car, soliciting, buying ice cream, warping into a seat and picking a radio station.
use super::ctx::Px;
use super::data::*;
use super::types::ped_type;
use crate::anim::{assoc_group, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::{status, EntityRef};
use crate::general::{limit_radian_angle, radian_angle_between_points};
use crate::math::heading;
use crate::modelinfo::indices_gta3::mi;
use crate::vehicle::automobile::{car_door_flag, node};
use crate::vehicle::base::MISSION_VEHICLE;
use crate::world::{Kinds, World};
use glam::Vec3;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// `eRadioStation`.
mod radio {
    pub const HEAD_RADIO: u8 = 0;
    pub const DOUBLE_CLEF: u8 = 1;
    pub const JAH_RADIO: u8 = 2;
    pub const RISE_FM: u8 = 3;
    pub const LIPS_106: u8 = 4;
    pub const GAME_FM: u8 = 5;
    pub const MSX_FM: u8 = 6;
    pub const FLASHBACK: u8 = 7;
    pub const CHATTERBOX: u8 = 8;
    pub const USERTRACK: u8 = 9;
}

const RADIOS_PER_CATEGORY: [[u8; 4]; 10] = [
    [radio::JAH_RADIO, radio::RISE_FM, radio::GAME_FM, radio::MSX_FM],
    [radio::HEAD_RADIO, radio::DOUBLE_CLEF, radio::LIPS_106, radio::FLASHBACK],
    [radio::RISE_FM, radio::GAME_FM, radio::MSX_FM, radio::FLASHBACK],
    [radio::HEAD_RADIO, radio::RISE_FM, radio::LIPS_106, radio::MSX_FM],
    [radio::HEAD_RADIO, radio::RISE_FM, radio::MSX_FM, radio::FLASHBACK],
    [radio::JAH_RADIO, radio::RISE_FM, radio::LIPS_106, radio::FLASHBACK],
    [radio::HEAD_RADIO, radio::RISE_FM, radio::LIPS_106, radio::FLASHBACK],
    [radio::HEAD_RADIO, radio::JAH_RADIO, radio::LIPS_106, radio::FLASHBACK],
    [radio::HEAD_RADIO, radio::DOUBLE_CLEF, radio::LIPS_106, radio::FLASHBACK],
    [radio::CHATTERBOX, radio::HEAD_RADIO, radio::LIPS_106, radio::GAME_FM],
];

/// `CPed::GetPedRadioCategory` (GTA III models): the stations a ped model likes.
pub fn ped_radio_category(model: i16) -> usize {
    match model {
        mi::MALE01 | mi::FEMALE03 | mi::PROSTITUTE2 | mi::WORKER1 | mi::MOD_MAN | mi::MOD_WOM | mi::ST_WOM | mi::FAN_WOM => 3,
        mi::TAXI_D | mi::PIMP | mi::MALE02 | mi::FEMALE02 | mi::FATFEMALE01 | mi::FATFEMALE02 | mi::DOCKER1 | mi::WORKER2 | mi::FAN_MAN2 => 9,
        mi::GANG01 | mi::GANG02 | mi::SCUM_MAN | mi::SCUM_WOM | mi::HOS_WOM | mi::CONST1 => 1,
        mi::GANG03 | mi::GANG04 | mi::GANG07 | mi::GANG08 | mi::CT_MAN2 | mi::CT_WOM2 | mi::B_MAN3 | mi::SHOPPER3 => 4,
        mi::GANG05 | mi::GANG06 | mi::GANG11 | mi::GANG12 | mi::CRIMINAL02 | mi::B_WOM2 | mi::ST_MAN | mi::HOS_MAN => 5,
        mi::FATMALE01 | mi::LI_MAN2 | mi::SHOPPER1 | mi::CAS_MAN => 6,
        mi::PROSTITUTE | mi::P_WOM2 | mi::LI_WOM2 | mi::B_WOM3 | mi::CAS_WOM => 2,
        mi::P_WOM1 | mi::DOCKER2 | mi::STUD_MAN => 7,
        mi::CT_MAN1 | mi::CT_WOM1 | mi::LI_MAN1 | mi::LI_WOM1 | mi::B_MAN1 | mi::B_MAN2 | mi::B_WOM1 | mi::SHOPPER2 | mi::STUD_WOM => 8,
        _ => 0,
    }
}

/// `m_nPathDir`-style heading of `v`'s forward axis.
fn forward_heading(w: &World, r: EntityRef) -> f32 {
    heading(w.ent(r).base.matrix.forward)
}

impl World {
    /// `CPed::SetSeekCar`.
    pub fn set_seek_car(&mut self, r: EntityRef, car: EntityRef, door_node: u32) {
        if self.ped(r).ped_state == state::SEEK_CAR {
            return;
        }
        self.set_stored_state(r);
        let pd = self.ped_mut(r);
        pd.seek_target = Some(car);
        pd.car_in_objective = Some(car);
        pd.my_vehicle = Some(car);
        pd.veh_door = door_node as u16;
        pd.distance_to_count_seek_done = 0.5;
        pd.ped_state = state::SEEK_CAR;
    }

    /// `CPed::SeekCar`.
    pub fn seek_car(&mut self, px: &mut Px, r: EntityRef) {
        let Some(veh) = self.ped(r).car_in_objective else {
            self.restore_previous_state(px, r);
            return;
        };
        let now = self.now();
        let mut dest = Vec3::ZERO;
        let pd = self.ped(r);
        let door = pd.veh_door as u32;
        if pd.objective != objective::ENTER_CAR_AS_PASSENGER {
            if pd.veh_door != 0 && pd.objective != objective::ENTER_CAR_AS_DRIVER {
                if self.is_room_to_be_car_jacked(r) {
                    dest = self.get_position_to_open_car_door(veh, door);
                } else if self.ped(r).ped_type == ped_type::COP {
                    dest = self.get_position_to_open_car_door(veh, node::DOOR_RF as u32);
                } else {
                    self.ped_mut(r).set_move_state(move_state::STILL);
                }
            } else {
                self.get_nearest_door(r, veh, &mut dest);
            }
        } else {
            if pd.car_jack_timer > now {
                self.ped_mut(r).set_move_state(move_state::STILL);
                return;
            }
            let is_train = self.try_veh(veh).is_some_and(|v| v.is_train());
            if self.ent(veh).base.model_index == mi::COACH {
                self.get_nearest_door(r, veh, &mut dest);
            } else if is_train {
                if self.ent(veh).base.status != status::TRAIN_NOT_MOVING || !self.get_nearest_train_door(r, veh, &mut dest) {
                    self.restore_previous_objective(px, r);
                    self.restore_previous_state(px, r);
                    return;
                }
            } else {
                if !self.get_nearest_passenger_door(r, veh, &mut dest) {
                    let full = self.try_veh(veh).is_some_and(|v| v.num_passengers == v.num_max_passengers);
                    if full {
                        self.restore_previous_objective(px, r);
                        self.restore_previous_state(px, r);
                    } else {
                        self.ped_mut(r).set_move_state(move_state::STILL);
                    }
                    self.ped_mut(r).flags.veh_enter_door_is_blocked = true;
                    return;
                }
                self.ped_mut(r).flags.veh_enter_door_is_blocked = false;
            }
        }

        if dest.x == 0.0 && dest.y == 0.0 {
            let pd = self.ped(r);
            let (player, mission) = (pd.is_player(), pd.char_created_by == MISSION_CHAR);
            let v = self.try_veh(veh);
            let keep_out = (!player && !mission)
                || v.is_none_or(|v| v.created_by != MISSION_VEHICLE || v.driver.is_some() || !v.can_ped_open_locks(player));
            if keep_out {
                self.restore_previous_state(px, r);
                if player {
                    self.clear_objective(px, r);
                } else if self.ped(r).char_created_by == RANDOM_CHAR {
                    self.ped_mut(r).car_jack_timer = now.wrapping_add(30000);
                }
                self.ped_mut(r).set_move_state(move_state::STILL);
                px.hooks.clear_player_weapon_mode();
                px.hooks.remove_from_interesting_vehicle_list(veh);
                return;
            }
            dest = self.epos(veh);
            if self.ped(r).flags.collided_with_my_vehicle {
                if let Some(mine) = self.ped(r).my_vehicle {
                    self.warp_ped_into_car(px, r, mine);
                }
                return;
            }
        }
        let found_better = self.possibly_find_better_pos_to_seek_car(r, &mut dest, veh);
        self.ped_mut(r).seek_pos = dest;
        let dist_sq = (dest - self.epos(r)).length_squared();
        let pd = self.ped_mut(r);
        if pd.flags.is_running {
            pd.set_move_state(move_state::RUN);
        } else if dist_sq < 4.0 {
            pd.set_move_state(move_state::WALK);
        }
        let max_x = self.col_model_of(veh).map_or(0.0, |c| c.bounding_box.max.x);
        let pd = self.ped_mut(r);
        if dist_sq >= 1.0 {
            pd.flags.can_ped_enter_seeked_car = false;
        } else if 2.0 * max_x > dist_sq {
            pd.flags.can_ped_enter_seeked_car = true;
        }
        let getting_in = self.try_veh(veh).map_or(0, |v| v.getting_in_flags);
        self.ped_mut(r).flags.veh_enter_door_is_blocked = getting_in & car_door_flag(door as usize) != 0;

        // arrived at the car
        if !self.seek(px, r) || found_better {
            return;
        }
        let z = self.epos(r).z;
        if !(1.5 + z > dest.z && z - 0.5 < dest.z) {
            return;
        }
        if self.try_veh(veh).is_some_and(|v| v.is_train()) {
            self.set_enter_train(px, r, veh, door);
            return;
        }
        let pd = self.ped_mut(r);
        pd.rotation_cur = pd.rotation_dest;
        if pd.flags.veh_enter_door_is_blocked {
            pd.set_move_state(move_state::STILL);
            return;
        }
        self.get_mut(veh).expect("vehicle").base.flags.is_static = false;
        match self.ped(r).objective {
            objective::SOLICIT_VEHICLE => self.set_solicit(px, r, 1000),
            objective::BUY_ICE_CREAM => self.set_buy_ice_cream(r),
            _ => self.enter_sought_car(px, r, veh, door),
        }
    }

    /// The part of `SeekCar` that picks between entering and jacking once the ped stands at the door.
    fn enter_sought_car(&mut self, px: &mut Px, r: EntityRef, veh: EntityRef, door: u32) {
        let e = self.ent(veh);
        let can_enter = e.phys.as_ref().is_some_and(|p| crate::vehicle::base::VehicleData::can_ped_enter_car(&e.base.matrix, p));
        let Some(v) = self.try_veh(veh) else { return };
        if !((v.num_getting_in as i32) < v.num_max_passengers as i32 + 1 && can_enter) {
            self.restore_previous_state(px, r);
            return;
        }
        let (is_bus, driver, passengers) = (v.flags.is_bus, v.driver, v.passengers);
        let leader = self.ped(r).leader;
        let door_taken = |d: u32| match d as usize {
            node::DOOR_LF => driver.is_some(),
            node::DOOR_RF => passengers[0].is_some(),
            node::DOOR_LR => passengers[1].is_some(),
            node::DOOR_RR => passengers[2].is_some(),
            _ => false,
        };
        match self.ent(veh).base.status {
            status::PLAYER | status::SIMPLE | status::PHYSICS | status::PLAYER_DISABLED => {
                if !is_bus && (leader.is_none() || leader != driver) && door_taken(door) {
                    self.set_car_jack(px, r, veh);
                    if self.ped(r).objective == objective::ENTER_CAR_AS_DRIVER && door as usize != node::DOOR_LF && let Some(d) = driver {
                        self.ped_mut(d).flags.flee_after_exiting_car = true;
                    }
                } else {
                    self.set_enter_car(px, r, veh, door);
                }
            }
            status::ABANDONED => {
                if door as usize == node::DOOR_RF && let Some(p) = passengers[0] {
                    if self.ped(p).flags.dont_drag_me_out_car {
                        if self.ped(r).is_player() {
                            self.set_enter_car(px, r, veh, door);
                        }
                    } else {
                        self.set_car_jack(px, r, veh);
                    }
                } else {
                    self.set_enter_car(px, r, veh, door);
                }
            }
            status::WRECKED => self.set_idle(px, r),
            _ => {}
        }
    }

    /// `CPed::SetSolicit`.
    pub fn set_solicit(&mut self, px: &mut Px, r: EntityRef, time: u32) {
        let pd = self.ped(r);
        let Some(car) = pd.car_in_objective else { return };
        if pd.ped_state == state::SOLICIT || !pd.is_ped_in_control() {
            return;
        }
        let now = self.now();
        let getting_in = self.try_veh(car).map_or(0, |v| v.num_getting_in);
        if pd.char_created_by != MISSION_CHAR && getting_in == 0 && now < pd.objective_timer {
            let side = if pd.veh_door as usize == node::DOOR_LF { -FRAC_PI_2 } else { FRAC_PI_2 };
            let dest = forward_heading(self, car) + side;
            self.ped_mut(r).rotation_dest = dest;
            if (dest - self.ped(r).rotation_cur).abs() < FRAC_PI_2 {
                self.ped_mut(r).chat_timer = now.wrapping_add(time);
                let (van, bus) = self.try_veh(car).map_or((false, false), |v| (v.flags.is_van, v.flags.is_bus));
                if !van && !bus {
                    let _ = self.ped_blend_anim(r, assoc_group::STD, anim::STD_CAR_HOOKERTALK, 4.0);
                    self.ped_mut(r).vehicle_anim = Some(anim::STD_CAR_HOOKERTALK as i32);
                }
                self.ped_mut(r).ped_state = state::SOLICIT;
            }
        }
        let _ = px;
    }

    /// `CPed::Solicit`.
    pub fn solicit(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let pd = self.ped(r);
        let (car, door) = (pd.car_in_objective, pd.veh_door as u32);
        if let Some(car) = car.filter(|_| pd.chat_timer >= now) {
            let door_pos = self.get_position_to_open_car_door_offset(car, door, 0.0);
            self.ped_mut(r).set_move_state(move_state::STILL);
            let pos = self.epos(r);
            let mut dest = radian_angle_between_points(door_pos.x, door_pos.y, pos.x, pos.y);
            if dest < 0.0 {
                dest += TAU;
            } else if dest > TAU {
                dest -= TAU;
            }
            self.ped_mut(r).rotation_dest = dest;
            if (pos - door_pos).length_squared() <= 1.0 {
                return;
            }
            if let Some(a) = self.ped_assoc_mut(r, anim::STD_CAR_HOOKERTALK) {
                a.blend_delta = -1000.0;
                a.flags |= crate::anim::flags::DELETE_FADED_OUT;
            }
            self.restore_previous_state(px, r);
            self.restore_previous_objective(px, r);
            self.ped_mut(r).set_objective_timer(10000, now);
        } else if car.is_none() {
            self.restore_previous_state(px, r);
            self.restore_previous_objective(px, r);
            self.ped_mut(r).set_objective_timer(10000, now);
        } else if self.ents.player_info.money <= 100 {
            self.ped_mut(r).car_in_objective = None;
        } else {
            self.ped_mut(r).vehicle_anim = None;
            let driver = car.and_then(|c| self.try_veh(c)).and_then(|v| v.driver);
            self.set_leader(r, driver);
        }
    }

    /// `CPed::SetBuyIceCream`.
    pub fn set_buy_ice_cream(&mut self, r: EntityRef) {
        let pd = self.ped(r);
        let Some(car) = pd.car_in_objective else { return };
        if pd.ped_state == state::BUY_ICECREAM || !pd.is_ped_in_control() {
            return;
        }
        // the side of the ice cream van
        let dest = forward_heading(self, car) - FRAC_PI_2;
        self.ped_mut(r).rotation_dest = dest;
        if (dest - self.ped(r).rotation_cur).abs() < FRAC_PI_2 {
            let now = self.now();
            let pd = self.ped_mut(r);
            pd.chat_timer = now.wrapping_add(3000);
            pd.ped_state = state::BUY_ICECREAM;
        }
    }

    /// `CPed::PossiblyFindBetterPosToSeekCar`: walk round another car that stands between the ped and the door.
    pub fn possibly_find_better_pos_to_seek_car(&mut self, r: EntityRef, pos: &mut Vec3, veh: EntityRef) -> bool {
        let mut helper_pos = self.epos(r);
        helper_pos.z = pos.z - 0.5;
        let mut found_pos = *pos;
        found_pos.z -= 0.5;
        let kinds = Kinds { vehicles: true, ..Kinds::NONE };
        if self.test_sphere_against_world((found_pos + helper_pos) / 2.0, 0.25, Some(veh), kinds, false).is_none() {
            return false;
        }
        let Some(col) = self.col_model_of(veh) else { return false };
        let (min, max) = (col.bounding_box.min, col.bounding_box.max);
        let m = self.ent(veh).base.matrix;
        let left_rear = m.transform_point(Vec3::new(min.x - 0.5, min.y - 0.5, 0.0));
        let right_rear = m.transform_point(Vec3::new(0.5 + max.x, min.y - 0.5, 0.0));
        let left_front = m.transform_point(Vec3::new(min.x - 0.5, 0.5 + max.y, 0.0));
        let right_front = m.transform_point(Vec3::new(0.5 + max.x, 0.5 + max.y, 0.0));
        // the ped's offset from the car, moved into the car's frame
        helper_pos -= m.pos;
        helper_pos = m.transform_point(helper_pos);
        let veh_heading = heading(m.forward);
        let needed = |corner: Vec3| limit_radian_angle((corner.x - helper_pos.x).atan2(corner.y - helper_pos.y) - veh_heading);
        let (lr, rr, lf, rf) = (needed(left_rear), needed(right_rear), needed(left_front), needed(right_front));
        let can_lr = lr <= -PI || lr >= -FRAC_PI_2;
        let can_rr = rr <= FRAC_PI_2 || rr >= PI;
        let can_lf = lf >= 0.0 || lf <= -FRAC_PI_2;
        let can_rf = rf <= 0.0 || rf >= FRAC_PI_2;
        // only the order of preference differs between the doors
        let order: [(bool, Vec3); 4] = match self.ped(r).veh_door as usize {
            node::DOOR_RR => [(can_rr, right_rear), (can_rf, right_front), (can_lr, left_rear), (can_lf, left_front)],
            node::DOOR_RF => [(can_rf, right_front), (can_rr, right_rear), (can_lf, left_front), (can_lr, left_rear)],
            node::DOOR_LF => [(can_lf, left_front), (can_lr, left_rear), (can_rf, right_front), (can_rr, right_rear)],
            node::DOOR_LR => [(can_lr, left_rear), (can_lf, left_front), (can_rr, right_rear), (can_rf, right_front)],
            _ => return false,
        };
        let Some(&(_, found)) = order.iter().find(|(can, _)| *can) else { return false };
        let mut to_found = self.epos(r) - found;
        to_found.z = 0.0;
        if to_found.length_squared() <= 0.5 * 0.5 {
            return false;
        }
        pos.x = found.x;
        pos.y = found.y;
        true
    }

    /// `CPed::SetLeader`.
    pub fn set_leader(&mut self, r: EntityRef, leader: Option<EntityRef>) {
        self.ped_mut(r).leader = leader;
    }

    /// `CPed::CanPedDriveOff`.
    pub fn can_ped_drive_off(&mut self, r: EntityRef) -> bool {
        let now = self.now();
        let pd = self.ped(r);
        if pd.ped_state != state::DRIVING || pd.look_timer > now {
            return false;
        }
        let waiting = pd.near_peds.iter().take(pd.num_near_peds as usize).flatten().any(|&n| {
            let np = self.ped(n);
            np.ped_type == pd.ped_type && np.objective == objective::ENTER_CAR_AS_PASSENGER && np.car_in_objective == pd.car_in_objective
        });
        if waiting {
            self.ped_mut(r).look_timer = now.wrapping_add(1000);
        }
        !waiting
    }

    /// `CPed::SetRadioStation`: a driver picks a station that suits its model.
    pub fn set_radio_station(&mut self, px: &mut Px, r: EntityRef) {
        let pd = self.ped(r);
        let Some(veh) = pd.my_vehicle else { return };
        if pd.is_player() || self.try_veh(veh).and_then(|v| v.driver) != Some(r) {
            return;
        }
        let category = ped_radio_category(self.ent(r).base.model_index);
        let mp3 = px.hooks.mp3_radio_available();
        let current = self.try_veh(veh).map_or(0, |v| v.radio_station);
        let mut liked = RADIOS_PER_CATEGORY[category].iter().position(|&s| s == current);
        if mp3 && px.rng.number() & 15 == 0 {
            self.try_veh_mut(veh).expect("vehicle").radio_station = radio::USERTRACK;
            liked = Some(0);
        }
        // the original leaves the index at 0 when the MP3 branch was taken without a search
        if liked.is_none() {
            let station = if mp3 && px.rng.number() & 15 == 0 { radio::USERTRACK } else { RADIOS_PER_CATEGORY[category][(px.rng.number() & 3) as usize] };
            self.try_veh_mut(veh).expect("vehicle").radio_station = station;
        }
    }

    /// `CPed::RemoveWeaponWhenEnteringVehicle`.
    pub fn remove_weapon_when_entering_vehicle(&mut self, r: EntityRef) {
        use crate::weapons::info::weapon_type as wt;
        let pd = self.ped(r);
        if pd.is_player() && pd.has_weapon(wt::UZI) && pd.weapons[wt::UZI].ammo_total > 0 {
            let current = pd.weapons[pd.current_weapon as usize].ty;
            if pd.stored_weapon == wt::UNIDENTIFIED {
                self.ped_mut(r).stored_weapon = current;
            }
            self.set_current_weapon(r, wt::UZI);
        } else {
            self.remove_weapon_model(r);
        }
    }

    /// `CPed::ReplaceWeaponWhenExitingVehicle`.
    pub fn replace_weapon_when_exiting_vehicle(&mut self, r: EntityRef) {
        use crate::weapons::info::weapon_type as wt;
        let pd = self.ped(r);
        let weapon = pd.weapons[pd.current_weapon as usize].ty;
        if pd.is_player() && weapon == wt::UZI {
            if pd.stored_weapon != wt::UNIDENTIFIED {
                let stored = pd.stored_weapon;
                self.set_current_weapon(r, stored);
                self.ped_mut(r).stored_weapon = wt::UNIDENTIFIED;
            }
        } else {
            let model = self.ents.weapon_infos.get(weapon).model_id;
            self.add_weapon_model(r, model);
        }
    }

    /// `CPed::WarpPedIntoCar`.
    pub fn warp_ped_into_car(&mut self, px: &mut Px, r: EntityRef, car: EntityRef) {
        let pd = self.ped_mut(r);
        pd.in_vehicle = true;
        pd.my_vehicle = Some(car);
        pd.car_in_objective = Some(car);
        pd.ped_state = state::DRIVING;
        pd.flags.is_in_the_air = false;
        pd.flags.veh_exit_will_be_instant = true;
        self.get_mut(r).expect("ped").base.flags.uses_collision = false;
        let objective = self.ped(r).objective;
        match objective {
            objective::ENTER_CAR_AS_DRIVER => self.vehicle_set_driver(car, r),
            objective::ENTER_CAR_AS_PASSENGER => {
                for i in 0..4 {
                    if self.try_veh(car).is_some_and(|v| v.passengers[i].is_none()) {
                        self.try_veh_mut(car).expect("vehicle").passengers[i] = Some(r);
                        break;
                    }
                }
            }
            _ => return,
        }
        if self.ped(r).is_player() {
            self.get_mut(car).expect("vehicle").base.status = status::PLAYER;
            px.hooks.player_got_in_car(car);
        } else {
            self.get_mut(car).expect("vehicle").base.status = status::PHYSICS;
        }
        self.remove(r);
        let car_pos = self.epos(car);
        self.get_mut(r).expect("ped").base.set_position(car_pos);
        self.add(r).ok();
        self.end_emergency_duty(car);
        if let Some(v) = self.try_veh_mut(car).filter(|v| !v.flags.engine_on) {
            v.flags.engine_on = true;
            px.hooks.play_one_shot(car, sound::CAR_ENGINE_START, 1.0);
        }
        let is_boat = self.try_veh(car).is_some_and(|v| v.is_boat());
        let is_bus = self.try_veh(car).is_some_and(|v| v.flags.is_bus);
        let driver_anim = self.driver_anim(car);
        if is_boat {
            // FIX_BUGS: the boat's own driver animation
            self.ped_blend_anim(r, assoc_group::STD, driver_anim, 100.0);
            self.remove_weapon_model(r);
        } else {
            self.remove_weapon_when_entering_vehicle(r);
            self.ped_blend_anim(r, assoc_group::STD, driver_anim, 100.0);
        }
        self.ped_mut(r).vehicle_anim = Some(driver_anim as i32);
        self.stop_non_partial_anims(r);
        if is_bus {
            self.ped_mut(r).flags.render_ped_in_car = false;
        }
        self.ped_mut(r).flags.changed_seat = true;
    }
}
