//! `PedAI.cpp` and `Ped.cpp`: the animation callbacks of getting into and out of cars and trains.
use super::callbacks as cb;
use super::ctx::Px;
use super::data::*;
use super::types::ped_type;
use crate::anim::{assoc_group, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::{status, EntityRef};
use crate::math::heading;
use crate::vehicle::automobile::{car_door_flag, door_id, node};
use crate::vehicle::base::{car_mission, driving_style, VehicleSub, MISSION_VEHICLE, PARKED_VEHICLE, RANDOM_VEHICLE};
use crate::vehicle::damage::door_status;
use crate::world::World;
use glam::Vec3;
use std::f32::consts::FRAC_PI_2;

use super::enter::door_of_component;

/// The three train entries are the seats 2, 3 and 4 of the model's position table (and of the passenger list).
const TRAIN_POS_LEFT_ENTRY: usize = 2;
const TRAIN_POS_MID_ENTRY: usize = 3;
const TRAIN_POS_RIGHT_ENTRY: usize = 4;

impl World {
    /// `animAssoc->blendDelta = -1000.0f` for a callback that may have no animation.
    fn fade_out_assoc(&mut self, r: EntityRef, id: Option<i32>) {
        if let Some(a) = id.and_then(|id| self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(id))) {
            a.blend_delta = -1000.0;
        }
    }

    /// Sets a car door's damage status (`Damage.SetDoorStatus`) if the vehicle is a car.
    fn set_car_door_status(&mut self, veh: EntityRef, door: usize, status: u8) {
        if let Some(c) = self.car_mut(veh) {
            c.damage.set_door_status(door, status);
        }
    }

    fn door_missing(&self, veh: EntityRef, door: usize) -> bool {
        self.car_ref(veh).is_some_and(|c| c.is_door_missing(door))
    }

    /// `CPed::PedAnimAlignCB`: the ped has lined up with the door.
    pub fn ped_anim_align_cb(&mut self, px: &mut Px, r: EntityRef, anim_id: Option<i32>) {
        self.fade_out_assoc(r, anim_id);
        if !self.is_not_in_wrecked_vehicle(r) {
            return;
        }
        if !self.ped(r).entering_car() || self.ped(r).health == 0.0 {
            self.quit_entering_car(px, r);
            return;
        }
        let veh = self.ped(r).my_vehicle.expect("checked");
        let Some(v) = self.try_veh(veh) else { return };
        let (mut is_van, is_bus, is_low) = (v.flags.is_van, v.flags.is_bus, v.flags.low_vehicle);
        let door_node = self.ped(r).veh_door as usize;
        let (enter_door, left_side) = match door_node {
            node::DOOR_RF => {
                is_van = false;
                (door_id::FRONT_RIGHT, false)
            }
            node::DOOR_RR => (door_id::REAR_RIGHT, false),
            node::DOOR_LF => {
                is_van = false;
                (door_id::FRONT_LEFT, true)
            }
            node::DOOR_LR => (door_id::REAR_LEFT, true),
            _ => return,
        };
        let is_player = self.ped(r).is_player();
        let (door_missing, door_open) = self.car_ref(veh).map_or((false, false), |c| (c.is_door_missing(enter_door), c.is_door_fully_open(enter_door)));
        if door_missing || door_open {
            self.try_veh_mut(veh).expect("vehicle").auto_pilot.cruise_speed = 0;
            if self.ped(r).ped_state == state::CARJACK {
                self.ped_anim_door_open_cb(px, r, None);
                return;
            }
            let a = match (left_side, is_van, is_bus, is_low) {
                (false, true, _, _) => anim::STD_VAN_GET_IN_REAR_RHS,
                (false, _, true, _) => anim::STD_COACH_GET_IN_RHS,
                (false, _, _, true) => anim::STD_CAR_GET_IN_LO_RHS,
                (false, ..) => anim::STD_CAR_GET_IN_RHS,
                (true, true, ..) => anim::STD_VAN_GET_IN_REAR_LHS,
                (true, _, true, _) => anim::STD_COACH_GET_IN_LHS,
                (true, _, _, true) => anim::STD_CAR_GET_IN_LO_LHS,
                (true, ..) => anim::STD_CAR_GET_IN_LHS,
            };
            self.set_vehicle_anim(r, a, true, 0.0, Some(cb::PED_ANIM_GET_IN));
        } else if self.try_veh(veh).is_some_and(|v| v.can_ped_open_locks(is_player)) {
            self.try_veh_mut(veh).expect("vehicle").auto_pilot.cruise_speed = 0;
            let open = match (left_side, is_van, is_bus) {
                (false, true, _) => Some(anim::STD_VAN_OPEN_DOOR_REAR_RHS),
                (false, _, true) => Some(anim::STD_COACH_OPEN_RHS),
                (false, ..) => Some(anim::STD_CAR_OPEN_DOOR_RHS),
                (true, true, _) => Some(anim::STD_VAN_OPEN_DOOR_REAR_LHS),
                (true, _, true) => Some(anim::STD_COACH_OPEN_LHS),
                (true, ..) => None,
            };
            if let Some(a) = open {
                self.set_vehicle_anim(r, a, true, 0.0, Some(cb::PED_ANIM_DOOR_OPEN));
            } else {
                let driver = self.try_veh(veh).and_then(|v| v.driver);
                if self.ped(r).objective == objective::ENTER_CAR_AS_DRIVER
                    && let Some(d) = driver
                {
                    let dp = self.ped(d);
                    if !is_low && dp.char_created_by != MISSION_CHAR && dp.ped_state == state::DRIVING {
                        self.set_vehicle_anim(r, anim::STD_QUICKJACK, true, 0.0, Some(cb::PED_ANIM_GET_IN));
                        let door = self.ped(r).veh_door as u32;
                        self.set_being_dragged_from_car(px, d, veh, door, true);
                        if self.ped(d).is_gang_member() {
                            self.register_threat_with_gang_peds(d, Some(r));
                        }
                        return;
                    }
                }
                self.set_vehicle_anim(r, anim::STD_CAR_OPEN_DOOR_LHS, true, 0.0, Some(cb::PED_ANIM_DOOR_OPEN));
            }
        } else {
            let locked = if left_side { anim::STD_CARDOOR_LOCKED_LHS } else { anim::STD_CARDOOR_LOCKED_RHS };
            self.set_vehicle_anim(r, locked, true, 0.0, Some(cb::PED_ANIM_DOOR_OPEN));
            self.ped_mut(r).flags.cancel_entering_car = true;
        }
    }

    /// `CPed::PedAnimDoorOpenCB`: the door is open, now get in or drag the driver out.
    pub fn ped_anim_door_open_cb(&mut self, px: &mut Px, r: EntityRef, anim_id: Option<i32>) {
        self.fade_out_assoc(r, anim_id);
        if !self.is_not_in_wrecked_vehicle(r) {
            return;
        }
        if !self.ped(r).entering_car() {
            self.quit_entering_car(px, r);
            return;
        }
        let veh = self.ped(r).my_vehicle.expect("checked");
        let door_node = self.ped(r).veh_door as usize;
        let Some(v) = self.try_veh(veh) else { return };
        let (passengers, driver) = (v.passengers, v.driver);
        let (door, ped_in_seat) = match door_node {
            node::DOOR_RF => (door_id::FRONT_RIGHT, passengers[0]),
            node::DOOR_RR => (door_id::REAR_RIGHT, passengers[2]),
            node::DOOR_LF => (door_id::FRONT_LEFT, driver),
            node::DOOR_LR => (door_id::REAR_LEFT, passengers[1]),
            _ => return,
        };
        if self.ped(r).health == 0.0 || (px.hooks.player_controls_disabled() && ped_in_seat.is_some_and(|s| self.ped(s).is_player())) {
            self.quit_entering_car(px, r);
            return;
        }
        let (mut is_van, is_bus, is_low) = (v.flags.is_van, v.flags.is_bus, v.flags.low_vehicle);
        let veh_matrix = self.ent(veh).base.matrix;
        let upside_down = crate::vehicle::base::VehicleData::is_upside_down(&veh_matrix);
        let is_player = self.ped(r).is_player();
        if self.ped(r).flags.cancel_entering_car {
            if is_player && driver.is_some_and(|d| self.ped(d).ped_type == ped_type::COP) {
                self.set_wanted_level_no_drop(1);
            }
            // CANCELLABLE_CAR_ENTER
            let can_open = self.try_veh(veh).is_some_and(|v| v.can_ped_open_locks(is_player));
            if !self.door_missing(veh, door) && can_open && self.car_ref(veh).is_some() {
                self.set_car_door_status(veh, door, door_status::SWINGING);
            }
            self.quit_entering_car(px, r);
            self.restore_previous_objective(px, r);
            self.ped_mut(r).flags.cancel_entering_car = false;
            return;
        }
        if !self.door_missing(veh, door) && self.car_ref(veh).is_some() {
            self.set_car_door_status(veh, door, door_status::SWINGING);
        }
        let speed = self.ent(veh).phys.as_ref().map_or(0.0, |p| p.move_speed.length());
        if speed > 0.2 {
            self.quit_entering_car(px, r);
            let fall = if !matches!(door_node, node::DOOR_LF | node::DOOR_LR) { anim::STD_HIGHIMPACT_LEFT } else { anim::STD_HIGHIMPACT_RIGHT };
            self.set_fall(px, r, 1000, fall, false);
            return;
        }
        self.process_open_door(veh, door_node, anim::STD_CAR_OPEN_DOOR_LHS, 1.0);
        if matches!(door_node, node::DOOR_LF | node::DOOR_RF) {
            is_van = false;
        }
        let (jacking, left_side) = (self.ped(r).ped_state == state::CARJACK, matches!(door_node, node::DOOR_LF | node::DOOR_LR));
        if !jacking || is_bus {
            let a = match (left_side, is_van, is_bus, is_low) {
                (false, true, _, _) => anim::STD_VAN_GET_IN_REAR_RHS,
                (false, _, true, _) => anim::STD_COACH_GET_IN_RHS,
                (false, _, _, true) => anim::STD_CAR_GET_IN_LO_RHS,
                (false, ..) => anim::STD_CAR_GET_IN_RHS,
                (true, true, ..) => anim::STD_VAN_GET_IN_REAR_LHS,
                (true, _, true, _) => anim::STD_COACH_GET_IN_LHS,
                (true, _, _, true) => anim::STD_CAR_GET_IN_LO_LHS,
                (true, ..) => anim::STD_CAR_GET_IN_LHS,
            };
            self.set_vehicle_anim(r, a, true, 0.0, Some(cb::PED_ANIM_GET_IN));
        } else {
            let mut to_drag_out = ped_in_seat;
            if upside_down {
                self.quit_entering_car(px, r);
                if self.ped(r).ped_type == ped_type::COP
                    && let Some(t) = self.ped(r).ped_in_objective
                {
                    self.set_arrest_player(px, r, t);
                }
            }
            if !left_side {
                if to_drag_out.is_some_and(|d| !self.ped(d).flags.dont_drag_me_out_car) {
                    let d = to_drag_out.expect("checked");
                    if self.ped(d).ped_state != state::DRIVING {
                        self.quit_entering_car(px, r);
                        to_drag_out = None;
                    } else {
                        let pull = if is_low { anim::STD_CAR_PULL_OUT_PED_LO_RHS } else { anim::STD_CAR_PULL_OUT_PED_RHS };
                        self.set_vehicle_anim(r, pull, true, 0.0, Some(cb::PED_ANIM_PULL_PED_OUT));
                    }
                } else if self.ped(r).ped_type == ped_type::COP {
                    self.quit_entering_car(px, r);
                    let target = self.ped(r).ped_in_objective;
                    if let Some(t) = target.filter(|&t| self.ped(t).ped_state == state::DRIVING) {
                        self.get_mut(veh).expect("vehicle").base.status = status::PLAYER_DISABLED;
                        self.set_arrest_player(px, r, t);
                    } else if !self.door_missing(veh, door_id::FRONT_RIGHT) {
                        self.set_car_door_status(veh, door_id::FRONT_RIGHT, door_status::SWINGING);
                    }
                } else {
                    // (the original may sit on top of the passenger if it is flagged not to be dragged)
                    let a = if is_low { anim::STD_CAR_GET_IN_LO_LHS } else { anim::STD_CAR_GET_IN_LHS };
                    self.set_vehicle_anim(r, a, true, 0.0, Some(cb::PED_ANIM_GET_IN));
                }
            } else if let Some(d) = to_drag_out {
                if self.ped(d).ped_state != state::DRIVING || self.ped(d).flags.dont_drag_me_out_car {
                    // the player freezes here: the objective is not restored (an unfinished feature, used in Vice City)
                    self.quit_entering_car(px, r);
                    to_drag_out = None;
                } else {
                    let pull = if is_low { anim::STD_CAR_PULL_OUT_PED_LO_LHS } else { anim::STD_CAR_PULL_OUT_PED_LHS };
                    self.set_vehicle_anim(r, pull, true, 0.0, Some(cb::PED_ANIM_PULL_PED_OUT));
                }
            } else {
                let a = if is_low { anim::STD_CAR_GET_IN_LO_LHS } else { anim::STD_CAR_GET_IN_LHS };
                self.set_vehicle_anim(r, a, true, 0.0, Some(cb::PED_ANIM_GET_IN));
            }
            if let Some(d) = to_drag_out {
                let door = self.ped(r).veh_door as u32;
                self.set_being_dragged_from_car(px, d, veh, door, false);
                if self.ped(d).is_gang_member() {
                    self.register_threat_with_gang_peds(d, Some(r));
                }
            }
        }
        if let Some(d) = self.try_veh(veh).and_then(|v| v.driver) {
            self.set_look_flag_entity(d, r, true);
            self.set_look_timer(d, 1000);
        }
    }

    /// `CPed::PedAnimPullPedOutCB`: the other ped is out, take the seat.
    pub fn ped_anim_pull_ped_out_cb(&mut self, px: &mut Px, r: EntityRef, anim_id: Option<i32>) {
        self.fade_out_assoc(r, anim_id);
        if !self.ped(r).entering_car() {
            self.quit_entering_car(px, r);
            return;
        }
        if !self.is_not_in_wrecked_vehicle(r) {
            return;
        }
        if self.ped(r).flags.cancel_entering_car {
            self.quit_entering_car(px, r);
            self.restore_previous_objective(px, r);
            self.ped_mut(r).flags.cancel_entering_car = false;
            return;
        }
        let veh = self.ped(r).my_vehicle.expect("checked");
        let is_low = self.try_veh(veh).is_some_and(|v| v.flags.low_vehicle);
        if self.ped(r).is_player() && !px.hooks.player_controls_disabled() && px.hooks.player_pad_active() {
            self.quit_entering_car(px, r);
            self.restore_previous_objective(px, r);
            return;
        }
        if self.ped(r).objective == objective::ENTER_CAR_AS_DRIVER {
            let door = self.ped(r).veh_door as usize;
            let left = matches!(door, node::DOOR_LF | node::DOOR_LR);
            let a = match (left, is_low) {
                (false, true) => anim::STD_CAR_GET_IN_LO_RHS,
                (false, false) => anim::STD_CAR_GET_IN_RHS,
                (true, true) => anim::STD_CAR_GET_IN_LO_LHS,
                (true, false) => anim::STD_CAR_GET_IN_LHS,
            };
            self.set_vehicle_anim(r, a, true, 0.0, Some(cb::PED_ANIM_GET_IN));
        } else {
            self.quit_entering_car(px, r);
        }
    }

    /// `CPed::PedAnimGetInCB`.
    pub fn ped_anim_get_in_cb(&mut self, px: &mut Px, r: EntityRef, anim_id: Option<i32>) {
        self.fade_out_assoc(r, anim_id);
        if !self.is_not_in_wrecked_vehicle(r) || self.ped(r).dying_or_dead() {
            return;
        }
        if !self.ped(r).entering_car() {
            self.quit_entering_car(px, r);
            return;
        }
        let veh = self.ped(r).my_vehicle.expect("checked");
        let pd = self.ped(r);
        if pd.is_player() && pd.flags.gonna_kill_the_car_jacker && pd.player.as_ref().is_some_and(|p| p.arresting_cop.is_some()) {
            self.ped_set_in_car_cb(px, r, None);
            let pd = self.ped_mut(r);
            pd.last_ped_state = pd.ped_state;
            pd.ped_state = state::ARRESTED;
            pd.flags.gonna_kill_the_car_jacker = false;
            if let Some(v) = self.try_veh_mut(veh) {
                v.num_getting_in = 0;
                v.getting_in_flags = 0;
                v.flags.is_handbrake_on = true;
            }
            self.get_mut(veh).expect("vehicle").base.status = status::PLAYER_DISABLED;
            return;
        }
        let door_node = self.ped(r).veh_door as usize;
        if self.ped(r).is_player() && door_node == node::DOOR_LF && px.hooks.player_pressing_gas_or_brake() && self.car_ref(veh).is_some() {
            if !self.door_missing(veh, door_id::FRONT_LEFT) {
                self.set_car_door_status(veh, door_id::FRONT_LEFT, door_status::SWINGING);
            }
            self.ped_set_in_car_cb(px, r, None);
            return;
        }
        let v = self.try_veh(veh).expect("vehicle");
        let (mut is_van, is_bus, is_low) = (v.flags.is_van, v.flags.is_bus, v.flags.low_vehicle);
        let enter_door = match door_node {
            node::DOOR_RF => {
                is_van = false;
                door_id::FRONT_RIGHT
            }
            node::DOOR_RR => door_id::REAR_RIGHT,
            node::DOOR_LF => {
                is_van = false;
                door_id::FRONT_LEFT
            }
            _ => door_id::REAR_LEFT,
        };
        if !self.door_missing(veh, enter_door) && self.car_ref(veh).is_some() {
            self.set_car_door_status(veh, enter_door, door_status::SWINGING);
        }
        let driver = self.try_veh(veh).and_then(|v| v.driver);
        let pd = self.ped(r);
        if let Some(d) = driver.filter(|_| pd.objective == objective::ENTER_CAR_AS_DRIVER || pd.ped_state == state::CARJACK) {
            if is_bus {
                self.set_objective_entity(px, d, objective::LEAVE_CAR, veh);
                if self.ped(d).is_player() {
                    let v = self.try_veh_mut(veh).expect("vehicle");
                    v.flags.is_handbrake_on = true;
                    self.get_mut(veh).expect("vehicle").base.status = status::PLAYER_DISABLED;
                }
                self.ped_mut(d).flags.bus_jacked = true;
                self.try_veh_mut(veh).expect("vehicle").flags.is_being_car_jacked = false;
                self.ped_set_in_car_cb(px, r, None);
                let pd = self.ped(r);
                if pd.ped_type == ped_type::COP || matches!(pd.objective, objective::KILL_CHAR_ON_FOOT | objective::KILL_CHAR_ANY_MEANS) {
                    self.set_objective_entity(px, r, objective::LEAVE_CAR, veh);
                }
                let now = self.now();
                self.ped_mut(r).leave_car_timer = now.wrapping_add(400);
                return;
            }
            if d != r && door_node != node::DOOR_LF {
                if !self.ped(d).is_player() {
                    let dp = self.ped(d);
                    let stats = &self.ents.ped_stats.0[dp.ped_stats];
                    let flees = stats.temper <= stats.fear || dp.char_created_by == MISSION_CHAR || self.try_veh(veh).is_some_and(|v| v.created_by == MISSION_VEHICLE);
                    let dm = self.ped_mut(d);
                    dm.flags.use_ped_node_seek = true;
                    dm.last_path_node = None;
                    if flees {
                        dm.flags.flee_after_exiting_car = true;
                    } else {
                        dm.flags.gonna_kill_the_car_jacker = true;
                        self.set_objective_entity(px, d, objective::KILL_CHAR_ON_FOOT, r);
                        if self.ped(d).ped_type == ped_type::COP && self.ped(r).is_player() {
                            self.set_wanted_level_no_drop(1);
                        }
                    }
                }
                let (ours, theirs) = (self.ped(r).ped_type, self.ped(d).ped_type);
                if (ours != ped_type::EMERGENCY || theirs != ped_type::EMERGENCY) && (ours != ped_type::COP || theirs != ped_type::COP) {
                    self.set_objective_entity(px, d, objective::LEAVE_CAR, veh);
                    self.say(px, d, sound::PED_CAR_JACKED);
                } else {
                    self.ped_mut(r).objective = objective::ENTER_CAR_AS_PASSENGER;
                }
            }
        }
        if self.door_missing(veh, enter_door) || is_bus {
            self.ped_anim_door_close_cb(px, r, None);
        } else {
            let a = if !matches!(enter_door, door_id::FRONT_LEFT | door_id::REAR_LEFT) {
                if is_van {
                    anim::STD_VAN_CLOSE_DOOR_REAR_RHS
                } else if is_low {
                    anim::STD_CAR_CLOSE_DOOR_LO_RHS
                } else {
                    anim::STD_CAR_CLOSE_DOOR_RHS
                }
            } else if is_van {
                anim::STD_VAN_CLOSE_DOOR_REAR_LHS
            } else if is_low {
                anim::STD_CAR_CLOSE_DOOR_LO_LHS
            } else {
                anim::STD_CAR_CLOSE_DOOR_LHS
            };
            self.set_vehicle_anim(r, a, true, 0.0, Some(cb::PED_ANIM_DOOR_CLOSE));
        }
    }

    /// `CPed::PedAnimDoorCloseCB`.
    pub fn ped_anim_door_close_cb(&mut self, px: &mut Px, r: EntityRef, anim_id: Option<i32>) {
        if !self.is_not_in_wrecked_vehicle(r) || self.ped(r).dying_or_dead() {
            return;
        }
        if !self.ped(r).entering_car() {
            self.quit_entering_car(px, r);
            return;
        }
        let veh = self.ped(r).my_vehicle.expect("checked");
        let Some(v) = self.try_veh(veh) else { return };
        let (is_low, is_bus) = (v.flags.low_vehicle, v.flags.is_bus);
        let door_node = self.ped(r).veh_door as usize;
        if !is_bus {
            self.process_open_door(veh, door_node, anim::STD_CAR_CLOSE_DOOR_LHS, 1.0);
        }
        let Some(door) = door_of_component(door_node) else { return };
        if self.car_ref(veh).is_some_and(|c| c.damage.door_status(door) == door_status::SWINGING) {
            self.set_car_door_status(veh, door, door_status::OK);
        }
        let v = self.try_veh(veh).expect("vehicle");
        let (getting_in, driver) = (v.getting_in_flags, v.driver);
        if door == door_id::FRONT_LEFT || self.ped(r).objective == objective::ENTER_CAR_AS_PASSENGER || is_bus {
            self.ped_set_in_car_cb(px, r, None);
        } else if door_node == node::DOOR_RF && (getting_in & car_door_flag(node::DOOR_LF) != 0 || driver.is_some_and(|d| self.ped(d).objective != objective::LEAVE_CAR || !self.room_to_leave_car(veh, node::DOOR_LF as u32, None))) {
            if self.ped(r).objective == objective::ENTER_CAR_AS_DRIVER || self.ped(r).ped_state == state::CARJACK {
                self.try_veh_mut(veh).expect("vehicle").flags.is_being_car_jacked = false;
            }
            self.ped_mut(r).objective = objective::ENTER_CAR_AS_PASSENGER;
            self.ped_set_in_car_cb(px, r, None);
            self.set_objective_entity(px, r, objective::LEAVE_CAR, veh);
            let pd = self.ped_mut(r);
            if !pd.is_player() {
                pd.flags.flee_after_exiting_car = true;
            }
            pd.flags.use_ped_node_seek = true;
            pd.next_path_node = None;
        } else {
            self.fade_out_assoc(r, anim_id);
            let a = if is_low { anim::STD_CAR_SHUFFLE_LO_RHS } else { anim::STD_CAR_SHUFFLE_RHS };
            self.set_vehicle_anim(r, a, true, 0.0, Some(cb::PED_SET_IN_CAR));
        }
    }

    /// `CPed::PedAnimStepOutCarCB`.
    pub fn ped_anim_step_out_car_cb(&mut self, px: &mut Px, r: EntityRef, anim_id: Option<i32>) {
        self.fade_out_assoc(r, anim_id);
        let Some(veh) = self.ped(r).my_vehicle else {
            self.ped_set_out_car_cb(px, r, None);
            return;
        };
        if let Some(e) = self.get_mut(veh)
            && let Some(p) = e.phys.as_mut()
        {
            p.static_frames = 0;
            p.move_speed += Vec3::splat(0.001);
            p.turn_speed += Vec3::splat(0.001);
        }
        let door_node = self.ped(r).veh_door as usize;
        let is_bus = self.try_veh(veh).is_some_and(|v| v.flags.is_bus);
        if !is_bus {
            self.process_open_door(veh, door_node, anim::STD_GETOUT_LHS, 1.0);
        }
        let door = door_of_component(door_node);
        let mut close_door = door.is_some_and(|d| !self.door_missing(veh, d));
        if self.ped(r).is_player() {
            let engine_intact = self.car_ref(veh).is_some_and(|c| c.damage.engine_status >= 225);
            let v = self.try_veh(veh).expect("vehicle");
            let locked_shut = v.door_lock == crate::vehicle::base::CarLock::ForceShutDoors;
            if (!px.hooks.player_controls_disabled() && !locked_shut && px.hooks.player_pad_active()) || is_bus || v.car_fire.is_some() || engine_intact {
                close_door = false;
            }
        }
        if !close_door {
            if let Some(d) = door.filter(|&d| !self.door_missing(veh, d) && !is_bus) {
                self.set_car_door_status(veh, d, door_status::SWINGING);
            }
            self.ped_set_out_car_cb(px, r, None);
            return;
        }
        let door = door.expect("checked");
        let pd = self.ped(r);
        if pd.flags.flee_after_exiting_car || pd.flags.gonna_kill_the_car_jacker {
            // FIX_BUGS
            if !self.door_missing(veh, door) {
                self.set_car_door_status(veh, door, door_status::SWINGING);
            }
            self.ped_set_out_car_cb(px, r, None);
            return;
        }
        let close = match door {
            door_id::FRONT_LEFT | door_id::REAR_LEFT => anim::STD_CAR_CLOSE_LHS,
            _ => anim::STD_CAR_CLOSE_RHS,
        };
        self.set_vehicle_anim(r, close, true, 0.0, Some(cb::PED_SET_OUT_CAR));
    }

    /// `CPed::PedSetOutCarCB`: the ped stands beside the car.
    pub fn ped_set_out_car_cb(&mut self, px: &mut Px, r: EntityRef, anim_id: Option<i32>) {
        let mut veh = self.ped(r).my_vehicle;
        let mut started_to_run = false;
        let pd = self.ped_mut(r);
        pd.action_x = 0.0;
        pd.action_y = 0.0;
        pd.flags.veh_exit_will_be_instant = false;
        self.get_mut(r).expect("ped").base.flags.uses_collision = true;
        if veh.is_some_and(|v| self.try_veh(v).is_some_and(|d| d.is_boat())) {
            let step = self.time_step();
            if let Some(e) = self.get_mut(r) {
                let m = e.base.matrix;
                if let Some(p) = e.phys.as_ref() {
                    let mut m = m;
                    p.apply_move_speed(&mut m, step);
                    e.base.matrix = m;
                }
            }
        }
        if self.ped(r).objective == objective::LEAVE_CAR {
            self.restore_previous_objective(px, r);
        }
        self.ped_mut(r).in_vehicle = false;
        if let Some(v) = veh.filter(|&v| self.try_veh(v).is_some_and(|d| d.is_car())) {
            let door = self.ped(r).veh_door as u32;
            if !self.room_to_leave_car(v, door, None) {
                self.position_ped_out_of_collision(r);
            }
        }
        if self.ped(r).ped_state == state::EXIT_CAR {
            if self.ped(r).ped_type == ped_type::COP {
                self.set_idle(px, r);
            } else {
                self.restore_previous_state(px, r);
            }
            veh = self.ped(r).my_vehicle;
            let pd = self.ped(r);
            if let (true, Some(v)) = (pd.flags.flee_after_exiting_car, veh) {
                self.ped_mut(r).flags.flee_after_exiting_car = false;
                let p = self.epos(v);
                self.set_flee_pos(r, glam::Vec2::new(p.x, p.y), 12000);
                let pd = self.ped_mut(r);
                pd.flags.use_ped_node_seek = true;
                pd.next_path_node = None;
                let fear = self.ents.ped_stats.0[self.ped(r).ped_stats].fear;
                if px.rng.number() & 1 != 0 || fear > 70 {
                    self.ped_mut(r).set_move_state(move_state::SPRINT);
                    self.say(px, r, sound::PED_FLEE_SPRINT);
                } else {
                    self.ped_mut(r).set_move_state(move_state::RUN);
                    self.say(px, r, sound::PED_FLEE_RUN);
                }
                started_to_run = true;
                // not a good way to do this
                self.ped_mut(r).last_ped_state = state::WANDER_PATH;
            } else if pd.flags.wander_path_after_exiting_car {
                let dir = px.rng.range_i32(0, 8) as i8;
                self.set_wander_path(px, r, dir);
                let pd = self.ped_mut(r);
                pd.flags.wander_path_after_exiting_car = false;
                if pd.ped_type == ped_type::PROSTITUTE {
                    let now = self.now();
                    self.ped_mut(r).set_objective_timer(30000, now);
                }
                self.ped_mut(r).last_ped_state = state::NONE;
            } else if pd.flags.gonna_kill_the_car_jacker {
                // the kill objective is already given at this point
                self.ped_mut(r).flags.gonna_kill_the_car_jacker = false;
                if let Some(target) = self.ped(r).ped_in_objective {
                    let player_on_mission = px.hooks.player_on_mission();
                    if px.rng.number() & 1 == 0 && self.ped(r).ped_type != ped_type::COP && (!self.ped(target).is_player() || !player_on_mission) {
                        self.clear_objective(px, r);
                        if let Some(v) = veh {
                            self.set_objective_entity(px, r, objective::ENTER_CAR_AS_DRIVER, v);
                        }
                    }
                    let now = self.now();
                    self.ped_mut(r).leave_car_timer = now.wrapping_add(1500);
                }
                self.set_wait_state(px, r, wait_state::PLAYANIM_COWER, Some(1500));
                self.ped_mut(r).set_move_state(move_state::RUN);
                started_to_run = true;
            } else if pd.objective == objective::NONE && pd.char_created_by != MISSION_CHAR && pd.ped_state == state::IDLE && !pd.is_player() {
                let dir = px.rng.range_i32(0, 8) as i8;
                self.set_wander_path(px, r, dir);
            }
        }
        self.fade_out_assoc(r, anim_id);
        self.restart_non_partial_anims(r);
        self.ped_mut(r).vehicle_anim = None;
        let mut pos = self.epos(r);
        self.find_z_coor_for_ped(&mut pos);
        self.zero_ped_move_speed(r);
        self.get_mut(r).expect("ped").base.set_position(pos);
        veh = self.ped(r).my_vehicle;
        if let Some(v) = veh {
            if self.ped(r).ped_type == ped_type::PROSTITUTE {
                let driver_is_player = self.try_veh(v).and_then(|d| d.driver).is_some_and(|d| self.ped(d).is_player());
                if driver_is_player && self.ped(r).char_created_by == RANDOM_CHAR {
                    self.ents.player_info.next_sex_frequency_update_time = 0;
                    self.ents.player_info.hooker = None;
                }
            }
            let door = self.ped(r).veh_door as usize;
            let driver = self.try_veh(v).and_then(|d| d.driver);
            let status_now = self.ent(v).base.status;
            let cop = self.ped(r).ped_type == ped_type::COP;
            if let Some(d) = self.try_veh_mut(v) {
                d.getting_out_flags &= !car_door_flag(door);
            }
            if driver == Some(r) {
                let new_status = self.try_veh_mut(v).expect("vehicle").remove_driver(status_now);
                self.get_mut(v).expect("vehicle").base.status = new_status;
                let d = self.try_veh_mut(v).expect("vehicle");
                if d.door_lock == crate::vehicle::base::CarLock::LockedInitially {
                    d.door_lock = crate::vehicle::base::CarLock::Unlocked;
                }
                let model = self.ent(v).base.model_index;
                if cop && crate::vehicle::base::VehicleData::is_law_enforcement_vehicle(model) {
                    self.try_veh_mut(v).expect("vehicle").flags.is_law_enforcer = false;
                    px.hooks.law_enforcer_changed(v, false);
                }
            } else if let Some(d) = self.try_veh_mut(v) {
                d.remove_passenger(r);
            }
            let m = self.ent(v).base.matrix;
            let bus = self.try_veh(v).is_some_and(|d| d.flags.is_bus);
            if bus && !crate::vehicle::base::VehicleData::is_upside_down(&m) && !crate::vehicle::base::VehicleData::is_on_its_side(&m) {
                let angle = self.bus_exit_angle(v, door);
                let pd = self.ped_mut(r);
                pd.rotation_dest = angle;
                pd.rotation_cur = angle;
                self.get_mut(r).expect("ped").base.set_heading(angle);
                if !self.ped(r).flags.bus_jacked {
                    self.ped_mut(r).set_move_state(move_state::WALK);
                }
            }
            if px.hooks.point_in_garage(self.epos(r)) {
                self.try_veh_mut(v).expect("vehicle").flags.lights_on = false;
            }
        }
        if self.ped(r).is_player() {
            px.hooks.player_left_car();
        }
        self.replace_weapon_when_exiting_vehicle(r);
        self.ped_mut(r).flags.on_boat = false;
        if self.ped(r).flags.bus_jacked {
            self.set_fall(px, r, 1500, anim::STD_HIGHIMPACT_BACK, false);
            self.ped_mut(r).flags.bus_jacked = false;
        }
        self.ped_mut(r).stored_move_state = move_state::NONE as i32;
        let pd = self.ped(r);
        // FIX_BUGS: the test was inverted in the original
        if !pd.is_player() && pd.char_created_by == MISSION_CHAR && !started_to_run {
            self.ped_mut(r).set_move_state(move_state::WALK);
        }
    }

    /// `CPed::PedSetDraggedOutCarCB`.
    pub fn ped_set_dragged_out_car_cb(&mut self, px: &mut Px, r: EntityRef, drag_anim: Option<i32>) {
        let quick_jacked = self.ped_assoc(r, anim::STD_QUICKJACKED).is_some();
        if self.ped(r).ped_state != state::ARRESTED {
            self.ped_mut(r).last_ped_state = state::NONE;
            self.fade_out_assoc(r, drag_anim);
        }
        self.restart_non_partial_anims(r);
        let pd = self.ped_mut(r);
        pd.vehicle_anim = None;
        pd.seek_target = None;
        let vehicle = pd.my_vehicle;
        if let Some(v) = vehicle {
            let door = self.ped(r).veh_door as usize;
            let driver = self.try_veh(v).and_then(|d| d.driver);
            let status_now = self.ent(v).base.status;
            if let Some(d) = self.try_veh_mut(v) {
                d.getting_out_flags &= !car_door_flag(door);
            }
            if driver == Some(r) {
                let new_status = self.try_veh_mut(v).expect("vehicle").remove_driver(status_now);
                self.get_mut(v).expect("vehicle").base.status = new_status;
                let d = self.try_veh_mut(v).expect("vehicle");
                if d.door_lock == crate::vehicle::base::CarLock::LockedInitially {
                    d.door_lock = crate::vehicle::base::CarLock::Unlocked;
                }
                let model = self.ent(v).base.model_index;
                if self.ped(r).ped_type == ped_type::COP && crate::vehicle::base::VehicleData::is_law_enforcement_vehicle(model) {
                    self.try_veh_mut(v).expect("vehicle").flags.is_law_enforcer = false;
                    px.hooks.law_enforcer_changed(v, false);
                }
            } else if let Some(d) = self.try_veh_mut(v) {
                d.remove_passenger(r);
            }
        }
        self.ped_mut(r).in_vehicle = false;
        if self.ped(r).is_player() {
            px.hooks.player_left_car();
        }
        let token = if quick_jacked { cb::PED_SET_QUICK_DRAGGED_OUT_CAR_POSITION } else { cb::PED_SET_DRAGGED_OUT_CAR_POSITION };
        if let Some(a) = drag_anim.and_then(|id| self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(id))) {
            a.set_delete_callback(token);
        }
        if !quick_jacked && self.ped(r).can_set_ped_state() {
            self.ped_blend_anim(r, assoc_group::STD, anim::STD_GET_UP, 1000.0);
        }
        self.replace_weapon_when_exiting_vehicle(r);
        let pd = self.ped_mut(r);
        pd.stored_move_state = move_state::NONE as i32;
        pd.flags.veh_exit_will_be_instant = false;
    }

    /// Where a dragged-out ped ends up: the end of the animation's translation, turned to the ped's heading.
    fn dragged_out_position(&mut self, r: EntityRef, mut offset: Vec3, nudge_up: bool) -> Vec3 {
        if matches!(self.ped(r).veh_door as usize, node::DOOR_RF | node::DOOR_RR) {
            offset.x = -offset.x;
        }
        let m = self.ent(r).base.matrix;
        let mut pos = m.right * offset.x + m.forward * offset.y + m.up * offset.z + self.epos(r);
        if nudge_up {
            pos.z += 1.0;
        }
        self.find_z_coor_for_ped(&mut pos);
        pos
    }

    /// What a ped does once it has been thrown out of a car (shared by both position callbacks).
    fn after_dragged_out(&mut self, px: &mut Px, r: EntityRef, quick: bool) {
        let Some(veh) = self.ped(r).my_vehicle else {
            if !quick {
                self.set_idle(px, r);
                self.set_get_up(px, r);
            }
            return;
        };
        let driver = self.try_veh(veh).and_then(|v| v.driver);
        let player_on_mission = px.hooks.player_on_mission();
        let veh_pos = self.epos(veh);
        let mission_vehicle = self.try_veh(veh).is_some_and(|v| v.created_by == MISSION_VEHICLE);
        let stats = self.ents.ped_stats.0[self.ped(r).ped_stats].clone();
        let angry = stats.temper > stats.fear && self.ped(r).char_created_by != MISSION_CHAR && !mission_vehicle;
        let (flee_time, flee_find_time) = if quick { (14000, 14000) } else { (4000, 4000) };
        if !quick && self.ped(r).is_player() {
            self.set_idle(px, r);
        } else if self.ped(r).flags.flee_after_exiting_car {
            self.ped_mut(r).flags.flee_after_exiting_car = false;
            self.set_flee_pos(r, glam::Vec2::new(veh_pos.x, veh_pos.y), flee_time);
        } else if self.ped(r).flags.wander_path_after_exiting_car {
            let dir = px.rng.range_i32(0, 8) as i8;
            self.set_wander_path(px, r, dir);
            self.ped_mut(r).flags.wander_path_after_exiting_car = false;
        } else if self.ped(r).flags.gonna_kill_the_car_jacker {
            // the kill objective is already set at this point
            self.ped_mut(r).flags.gonna_kill_the_car_jacker = false;
            let target = self.ped(r).ped_in_objective;
            let coin = px.rng.number() & 1 != 0;
            if quick {
                if let Some(t) = target.filter(|_| coin) {
                    if self.ped(r).objective != objective::KILL_CHAR_ON_FOOT {
                        self.set_objective_entity(px, r, objective::KILL_CHAR_ON_FOOT, t);
                    }
                } else {
                    if driver.is_none() || driver == Some(r) || driver.is_some_and(|d| self.ped(d).is_player() && player_on_mission) {
                        self.set_flee_pos(r, glam::Vec2::new(veh_pos.x, veh_pos.y), flee_find_time);
                    } else {
                        self.clear_objective(px, r);
                        self.set_objective_entity(px, r, objective::ENTER_CAR_AS_DRIVER, veh);
                    }
                    let pd = self.ped_mut(r);
                    pd.flags.use_ped_node_seek = true;
                    pd.next_path_node = None;
                    self.say(px, r, sound::PED_FLEE_RUN);
                }
            } else if target.is_none() || px.rng.number() & 1 == 0 {
                if driver.is_none() || driver == Some(r) || driver.is_some_and(|d| self.ped(d).is_player() && player_on_mission) {
                    let pd = self.ped_mut(r);
                    pd.ped_state = state::NONE;
                    pd.last_ped_state = state::NONE;
                    self.set_flee_pos(r, glam::Vec2::new(veh_pos.x, veh_pos.y), flee_find_time);
                } else {
                    self.clear_objective(px, r);
                    self.set_objective_entity(px, r, objective::ENTER_CAR_AS_DRIVER, veh);
                }
            }
        } else if angry && driver.is_some_and(|d| self.ped(d).is_player()) && !player_on_mission {
            // (without VC_PED_PORTS the ped may go for the driver instead)
            if px.rng.number() < px.rng.max() / 2 {
                self.set_objective_entity(px, r, objective::KILL_CHAR_ON_FOOT, driver.expect("checked"));
            } else {
                self.set_objective_entity(px, r, objective::ENTER_CAR_AS_DRIVER, veh);
            }
        } else if quick {
            self.set_find_path_and_flee_pos(r, glam::Vec2::new(veh_pos.x, veh_pos.y), 10000, false);
            let fear = stats.fear;
            if px.rng.number() & 1 != 0 || fear > 70 {
                self.ped_mut(r).set_move_state(move_state::SPRINT);
                self.say(px, r, sound::PED_FLEE_SPRINT);
            } else {
                self.say(px, r, sound::PED_FLEE_RUN);
            }
        } else {
            let pd = self.ped_mut(r);
            pd.ped_state = state::NONE;
            pd.last_ped_state = state::NONE;
            self.set_find_path_and_flee_pos(r, glam::Vec2::new(veh_pos.x, veh_pos.y), 10000, false);
        }
    }

    /// `CPed::PedSetQuickDraggedOutCarPositionCB`.
    pub fn ped_set_quick_dragged_out_car_position_cb(&mut self, px: &mut Px, r: EntityRef) {
        let veh = self.ped(r).my_vehicle;
        self.get_mut(r).expect("ped").base.flags.uses_collision = true;
        self.restart_non_partial_anims(r);
        let offset = self.ents.ped_anim_offsets.quick_dragged_out;
        let final_pos = self.dragged_out_position(r, offset, false);
        self.zero_ped_move_speed(r);
        self.get_mut(r).expect("ped").base.set_position(final_pos);
        if let Some(v) = veh {
            let angle = heading(self.ent(v).base.matrix.forward) - FRAC_PI_2;
            let pd = self.ped_mut(r);
            pd.rotation_dest = angle;
            pd.rotation_cur = angle;
            self.calculate_new_orientation(r);
            let door = self.ped(r).veh_door as u32;
            if !self.room_to_leave_car(v, door, Some(offset)) {
                self.position_ped_out_of_collision(r);
            }
        }
        if !self.ped(r).can_set_ped_state() {
            return;
        }
        self.set_idle(px, r);
        if veh.is_some() {
            self.after_dragged_out(px, r, true);
        }
        if self.ped(r).last_ped_state == state::IDLE {
            self.ped_mut(r).last_ped_state = state::WANDER_PATH;
        }
    }

    /// `CPed::PedSetDraggedOutCarPositionCB`.
    pub fn ped_set_dragged_out_car_position_cb(&mut self, px: &mut Px, r: EntityRef) {
        self.get_mut(r).expect("ped").base.flags.uses_collision = true;
        self.restart_non_partial_anims(r);
        let offset = self.ents.ped_anim_offsets.dragged_out;
        let pos = self.dragged_out_position(r, offset, true);
        self.zero_ped_move_speed(r);
        self.get_mut(r).expect("ped").base.set_position(pos);
        if let Some(v) = self.ped(r).my_vehicle {
            let door = self.ped(r).veh_door as u32;
            if !self.room_to_leave_car(v, door, Some(offset)) {
                self.position_ped_out_of_collision(r);
            }
        }
        if !self.ped(r).can_set_ped_state() {
            return;
        }
        self.after_dragged_out(px, r, false);
        self.set_get_up(px, r);
    }

    /// `CPed::PedSetInCarCB`: the ped sits down.
    pub fn ped_set_in_car_cb(&mut self, px: &mut Px, r: EntityRef, _anim_id: Option<i32>) {
        let Some(veh) = self.ped(r).my_vehicle else { return };
        if !self.is_not_in_wrecked_vehicle(r) || self.ped(r).dying_or_dead() {
            return;
        }
        let (is_player, ped_kind) = (self.ped(r).is_player(), self.ped(r).ped_type);
        self.ped_mut(r).in_vehicle = true;
        let driver = self.try_veh(veh).and_then(|v| v.driver);
        if ped_kind == ped_type::PROSTITUTE && driver.is_some_and(|d| self.ped(d).is_player()) && self.ped(r).char_created_by == RANDOM_CHAR {
            let now = self.now();
            let info = &mut self.ents.player_info;
            info.sex_frequency = 1000;
            info.next_sex_frequency_update_time = now + 3000;
            info.hooker = Some(r);
        }
        let jacking = self.ped(r).ped_state == state::CARJACK;
        let driver_objective = self.ped(r).objective == objective::ENTER_CAR_AS_DRIVER;
        if driver_objective || jacking {
            self.try_veh_mut(veh).expect("vehicle").flags.is_being_car_jacked = false;
        }
        if let Some(v) = self.try_veh_mut(veh)
            && v.num_getting_in != 0
        {
            v.num_getting_in -= 1;
        }
        if is_player {
            self.clear_adrenaline(px);
        }
        if self.try_veh(veh).is_some_and(|v| v.is_boat()) {
            if is_player {
                px.hooks.register_vehicle_of_interest(veh);
                if self.ent(veh).base.status == status::SIMPLE
                    && let Some(p) = self.get_mut(veh).and_then(|e| e.phys.as_mut())
                {
                    p.move_speed = Vec3::new(0.0, 0.0, -0.00001);
                    p.turn_speed = Vec3::ZERO;
                }
                self.get_mut(veh).expect("vehicle").base.status = status::PLAYER;
                px.hooks.player_got_in_car(veh);
            }
            self.vehicle_set_driver(veh, r);
            self.try_veh_mut(veh).expect("vehicle").flags.engine_on = true;
            self.ped_mut(r).ped_state = state::DRIVING;
            self.stop_non_partial_anims(r);
            return;
        }
        if let Some(a) = self.ped(r).vehicle_anim.and_then(|id| self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(id))) {
            a.blend_delta = -1000.0;
        }
        self.ped_mut(r).flags.do_bloody_footprints = false;
        let v = self.try_veh_mut(veh).expect("vehicle");
        if v.alarm_state == -1 {
            v.alarm_state = 15000;
        }
        let stop_vehicle = |w: &mut World| {
            if let Some(p) = w.get_mut(veh).and_then(|e| e.phys.as_mut()) {
                p.move_speed = Vec3::ZERO;
                p.turn_speed = Vec3::ZERO;
            }
        };
        if is_player {
            if driver_objective {
                if self.ent(veh).base.status == status::SIMPLE {
                    stop_vehicle(self);
                }
                self.get_mut(veh).expect("vehicle").base.status = status::PLAYER;
            }
            px.hooks.player_got_in_car(veh);
        } else if driver_objective {
            if self.ent(veh).base.status == status::SIMPLE {
                stop_vehicle(self);
            }
            self.get_mut(veh).expect("vehicle").base.status = status::PHYSICS;
        }
        if driver_objective {
            let v = self.try_veh(veh).expect("vehicle");
            let seats: Vec<EntityRef> = v.passengers.iter().take(v.num_max_passengers as usize).flatten().copied().collect();
            for p in seats {
                if self.ped(p).char_created_by == RANDOM_CHAR {
                    self.set_objective_entity(px, p, objective::LEAVE_CAR, veh);
                }
            }
        }
        if driver_objective || jacking {
            self.vehicle_set_driver(veh, r);
            let v = self.try_veh_mut(veh).expect("vehicle");
            if v.created_by == PARKED_VEHICLE {
                v.created_by = RANDOM_VEHICLE;
                px.hooks.car_became_random(veh);
            }
            if v.flags.is_ambulance_on_duty {
                v.flags.is_ambulance_on_duty = false;
                px.hooks.ambulance_off_duty();
            }
            if v.flags.is_fire_truck_on_duty {
                v.flags.is_fire_truck_on_duty = false;
                px.hooks.firetruck_off_duty();
            }
            let model = self.ent(veh).base.model_index;
            if ped_kind == ped_type::COP && crate::vehicle::base::VehicleData::is_law_enforcement_vehicle(model) {
                self.try_veh_mut(veh).expect("vehicle").flags.is_law_enforcer = true;
                px.hooks.law_enforcer_changed(veh, true);
            }
            let v = self.try_veh_mut(veh).expect("vehicle");
            if !v.flags.engine_on {
                v.flags.engine_on = true;
                px.hooks.play_one_shot(r, sound::CAR_ENGINE_START, 1.0);
            }
            let pd = self.ped(r);
            if driver_objective && pd.char_created_by == RANDOM_CHAR && self.ents.player_ped != Some(r) && pd.ped_type != ped_type::EMERGENCY {
                px.hooks.join_car_with_road_system(veh);
                let ap = &mut self.try_veh_mut(veh).expect("vehicle").auto_pilot;
                ap.car_mission = car_mission::CRUISE;
                ap.temp_action = 0;
                ap.driving_style = driving_style::AVOID_CARS;
                ap.cruise_speed = 25;
            }
            self.ped_mut(r).ped_state = state::DRIVING;
            if driver_objective {
                let pd = self.ped_mut(r);
                if matches!(pd.prev_objective, objective::RUN_TO_AREA | objective::GOTO_CHAR_ON_FOOT | objective::KILL_CHAR_ON_FOOT) {
                    pd.prev_objective = objective::NONE;
                }
                self.restore_previous_objective(px, r);
            }
        } else if self.ped(r).objective == objective::ENTER_CAR_AS_PASSENGER {
            let door = self.ped(r).veh_door as usize;
            let bus = self.try_veh(veh).is_some_and(|v| v.flags.is_bus);
            let seat = if bus {
                None
            } else {
                match door {
                    node::DOOR_RF => Some(0),
                    node::DOOR_RR => Some(2),
                    node::DOOR_LR => Some(1),
                    _ => None,
                }
            };
            self.vehicle_add_passenger(veh, r, seat);
            self.ped_mut(r).ped_state = state::DRIVING;
            let pd = self.ped_mut(r);
            if matches!(pd.prev_objective, objective::RUN_TO_AREA | objective::GOTO_CHAR_ON_FOOT | objective::KILL_CHAR_ON_FOOT) {
                pd.prev_objective = objective::NONE;
            }
            self.restore_previous_objective(px, r);
        }
        let door = self.ped(r).veh_door as usize;
        let v = self.try_veh_mut(veh).expect("vehicle");
        v.getting_in_flags &= !car_door_flag(door);
        let (bus, getting_in) = (v.flags.is_bus, v.getting_in_flags);
        if bus && getting_in == 0 {
            let now = self.now();
            if let Some(c) = self.car_mut(veh) {
                c.set_bus_door_timer(now, 1000, 1);
            }
        }
        if !matches!(self.ped(r).objective, objective::KILL_CHAR_ON_FOOT | objective::KILL_CHAR_ANY_MEANS | objective::LEAVE_CAR | objective::FOLLOW_CAR_IN_CAR | objective::GOTO_AREA_ANY_MEANS | objective::GOTO_AREA_ON_FOOT | objective::RUN_TO_AREA) {
            self.set_objective(px, r, objective::NONE);
        }
        let v = self.try_veh(veh).expect("vehicle");
        let seat_anim = if v.driver == Some(r) {
            if v.flags.low_vehicle { anim::STD_CAR_SIT_LO } else { anim::STD_CAR_SIT }
        } else if v.flags.low_vehicle {
            anim::STD_CAR_SIT_P_LO
        } else {
            anim::STD_CAR_SIT_P
        };
        let bus = v.flags.is_bus;
        self.set_vehicle_anim(r, seat_anim, false, 100.0, None);
        self.stop_non_partial_anims(r);
        if bus {
            self.ped_mut(r).flags.render_ped_in_car = false;
        }
        // FIX_BUGS: always (re)register the vehicle, it also refreshes the time
        if is_player && self.try_veh(veh).is_some_and(|v| v.created_by != MISSION_VEHICLE) {
            px.hooks.register_vehicle_of_interest(veh);
            if self.try_veh(veh).is_some_and(|v| !v.flags.has_been_owned_by_player) {
                px.hooks.register_steal_car(veh, r);
            }
            self.try_veh_mut(veh).expect("vehicle").flags.has_been_owned_by_player = true;
        }
        self.ped_mut(r).flags.changed_seat = true;
    }

    /// `CPed::PedAnimDoorCloseRollingCB`.
    pub fn ped_anim_door_close_rolling_cb(&mut self, r: EntityRef, anim_id: Option<i32>) {
        self.fade_out_assoc(r, anim_id);
        let Some(veh) = self.ped(r).my_vehicle else { return };
        let low = self.try_veh(veh).is_some_and(|v| v.flags.low_vehicle);
        let a = if low { anim::STD_CAR_CLOSE_DOOR_ROLLING_LO_LHS } else { anim::STD_CAR_CLOSE_DOOR_ROLLING_LHS };
        self.process_open_door(veh, node::DOOR_LF, a, 1.0);
        if let Some(v) = self.try_veh_mut(veh) {
            v.getting_out_flags &= !car_door_flag(node::DOOR_LF);
        }
        if self.car_ref(veh).is_some_and(|c| c.damage.door_status(door_id::FRONT_LEFT) == door_status::SWINGING) {
            self.set_car_door_status(veh, door_id::FRONT_LEFT, door_status::OK);
        }
    }

    // ---- trains ----

    /// `CPed::GetNearestTrainPedPosition`: the free train entry nearest to the ped, and its position.
    pub fn get_nearest_train_ped_position(&mut self, r: EntityRef, train: EntityRef) -> Vec3 {
        let model = self.ent(train).base.model_index as i32;
        let positions = self.ents.models.get(model).and_then(|m| m.vehicle()).map(|v| v.positions).unwrap_or_default();
        let passengers = self.try_veh(train).map_or([None; 8], |v| v.passengers);
        let m = self.ent(train).base.matrix;
        let pos = self.epos(r);
        let step = Vec3::new(1.5, 0.0, 0.0);
        let entry = |seat: usize| -> Option<(Vec3, f32)> {
            if passengers[seat].is_some() {
                return None;
            }
            let local = positions[seat] - step;
            let p = m.right * local.x + m.forward * local.y + m.up * local.z + m.pos;
            Some((p, (p - pos).length()))
        };
        let (left, mid, right) = (entry(TRAIN_POS_LEFT_ENTRY), entry(TRAIN_POS_MID_ENTRY), entry(TRAIN_POS_RIGHT_ENTRY));
        let dist = |e: Option<(Vec3, f32)>| e.map_or(999.0, |x| x.1);
        let spot = |e: Option<(Vec3, f32)>| e.map_or(Vec3::ZERO, |x| x.0);
        let (dl, dm, dr) = (dist(left), dist(mid), dist(right));
        let (door, p) = if dm < dl {
            if dm < dr { (TRAIN_POS_MID_ENTRY, spot(mid)) } else { (TRAIN_POS_RIGHT_ENTRY, spot(right)) }
        } else if dr < dl {
            (TRAIN_POS_RIGHT_ENTRY, spot(right))
        } else {
            (TRAIN_POS_LEFT_ENTRY, spot(left))
        };
        self.ped_mut(r).veh_door = door as u16;
        p
    }

    /// `CPed::GetNearestTrainDoor`.
    pub fn get_nearest_train_door(&mut self, r: EntityRef, train: EntityRef, door_pos: &mut Vec3) -> bool {
        *door_pos = self.get_nearest_train_ped_position(r, train);
        true
    }

    /// `CPed::SetEnterTrain`.
    pub fn set_enter_train(&mut self, px: &mut Px, r: EntityRef, train: EntityRef, _unused: u32) {
        let door_open = self.try_veh(train).is_some_and(|v| matches!(&v.sub, VehicleSub::Train(t) if t.doors[0].is_fully_open()));
        if self.ped(r).ped_state == state::ENTER_TRAIN || !door_open {
            return;
        }
        let angle = heading(self.ent(train).base.matrix.forward) - FRAC_PI_2;
        let pd = self.ped_mut(r);
        pd.rotation_cur = angle;
        pd.my_vehicle = Some(train);
        pd.ped_state = state::ENTER_TRAIN;
        self.set_vehicle_anim(r, anim::STD_TRAIN_GETIN, false, 4.0, Some(cb::PED_SET_IN_TRAIN));
        self.get_mut(r).expect("ped").base.flags.uses_collision = false;
        self.line_up_ped_with_train(r);
        if self.ped(r).is_player() {
            self.clear_adrenaline(px);
        }
    }

    /// `CPed::EnterTrain` and `SetPedPositionInTrain`.
    pub fn enter_train(&mut self, r: EntityRef) {
        self.line_up_ped_with_train(r);
    }

    /// `CPed::LineUpPedWithTrain`.
    pub fn line_up_ped_with_train(&mut self, r: EntityRef) {
        let Some(train) = self.ped(r).my_vehicle else { return };
        let model = self.ent(train).base.model_index as i32;
        let positions = self.ents.models.get(model).and_then(|m| m.vehicle()).map(|v| v.positions).unwrap_or_default();
        let enter_offset = Vec3::new(1.5, 0.0, -0.2);
        self.zero_ped_move_speed(r);
        let m = self.ent(train).base.matrix;
        let angle = heading(m.forward) - FRAC_PI_2;
        let pd = self.ped_mut(r);
        pd.rotation_cur = angle;
        pd.rotation_dest = angle;
        let mut line_up_pos;
        if !self.ped(r).in_vehicle {
            let mut p = Vec3::ZERO;
            self.get_nearest_train_door(r, train, &mut p);
            p.z += 0.2;
            line_up_pos = p;
        } else {
            let passengers = self.try_veh(train).map_or([None; 8], |v| v.passengers);
            line_up_pos = Vec3::ZERO;
            for seat in [TRAIN_POS_LEFT_ENTRY, TRAIN_POS_MID_ENTRY, TRAIN_POS_RIGHT_ENTRY] {
                if passengers[seat] == Some(r) {
                    line_up_pos = positions[seat] - enter_offset;
                    break;
                }
            }
            line_up_pos = m.right * line_up_pos.x + m.forward * line_up_pos.y + m.up * line_up_pos.z + m.pos;
        }
        if let Some(a) = self.ped(r).vehicle_anim.and_then(|id| self.ped_clump(r).and_then(|c| c.anim.association(id))) {
            let left = a.time_left() / a.hierarchy.total_length;
            line_up_pos += (self.epos(r) - line_up_pos) * left;
        }
        let e = self.get_mut(r).expect("ped");
        e.base.set_position(line_up_pos);
        e.base.set_heading(angle);
    }

    /// `CPed::SetExitTrain`.
    pub fn set_exit_train(&mut self, _px: &mut Px, r: EntityRef, train: EntityRef) {
        let door_open = self.try_veh(train).is_some_and(|v| matches!(&v.sub, VehicleSub::Train(t) if t.doors[0].is_fully_open()));
        if self.ped(r).ped_state == state::EXIT_TRAIN || self.ent(train).base.status != status::TRAIN_NOT_MOVING || !door_open {
            return;
        }
        self.ped_mut(r).ped_state = state::EXIT_TRAIN;
        self.set_vehicle_anim(r, anim::STD_TRAIN_GETOUT, false, 4.0, Some(cb::PED_SET_OUT_TRAIN));
        self.get_mut(r).expect("ped").base.flags.uses_collision = false;
        self.line_up_ped_with_train(r);
    }

    /// `CPed::ExitTrain`.
    pub fn exit_train(&mut self, r: EntityRef) {
        self.line_up_ped_with_train(r);
    }

    /// `CPed::PedSetInTrainCB`.
    pub fn ped_set_in_train_cb(&mut self, px: &mut Px, r: EntityRef) {
        let Some(train) = self.ped(r).my_vehicle else { return };
        let pd = self.ped_mut(r);
        pd.in_vehicle = true;
        pd.ped_state = state::DRIVING;
        self.restore_previous_objective(px, r);
        self.ped_mut(r).set_move_state(move_state::STILL);
        self.vehicle_add_passenger(train, r, None);
    }

    /// `CPed::PedSetOutTrainCB`.
    pub fn ped_set_out_train_cb(&mut self, px: &mut Px, r: EntityRef) {
        let train = self.ped(r).my_vehicle;
        if let Some(a) = self.ped(r).vehicle_anim.and_then(|id| self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(id))) {
            a.blend_delta = -1000.0;
        }
        self.get_mut(r).expect("ped").base.flags.uses_collision = true;
        let pd = self.ped_mut(r);
        pd.vehicle_anim = None;
        pd.in_vehicle = false;
        pd.ped_state = state::IDLE;
        self.restore_previous_objective(px, r);
        self.ped_mut(r).set_move_state(move_state::STILL);
        let Some(train) = train else { return };
        let m = self.ent(r).base.matrix;
        let angle = FRAC_PI_2 + heading(self.ent(train).base.matrix.forward);
        let pd = self.ped_mut(r);
        pd.rotation_cur = angle;
        pd.rotation_dest = angle;
        let offset = self.ents.ped_anim_offsets.train_door;
        let mut pos = m.right * offset.x + m.forward * offset.y + m.up * offset.z + self.epos(r);
        self.find_z_coor_for_ped(&mut pos);
        self.zero_ped_move_speed(r);
        let e = self.get_mut(r).expect("ped");
        e.base.set_position(pos);
        e.base.set_heading(angle);
        if let Some(v) = self.try_veh_mut(train) {
            v.remove_passenger(r);
        }
    }
}
