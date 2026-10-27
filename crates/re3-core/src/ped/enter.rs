//! `PedAI.cpp`: getting into, out of and being dragged out of cars.
use super::callbacks as cb;
use super::ctx::Px;
use super::data::*;
use super::types::ped_type;
use crate::anim::{assoc_group, flags as aflags, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::physical::GRAVITY;
use crate::entity::{status, EntityRef};
use crate::general::limit_radian_angle;
use crate::math::heading;
use crate::modelinfo::indices_gta3::mi;
use crate::vehicle::automobile::{car_door_flag, door_id, node};
use crate::vehicle::base::MISSION_VEHICLE;
use crate::world::World;
use glam::Vec3;
use std::f32::consts::{FRAC_PI_2, PI};

/// The `eDoors` id behind a `CAR_DOOR_*` node, if it is one.
pub fn door_of_component(component: usize) -> Option<usize> {
    match component {
        node::DOOR_RF => Some(door_id::FRONT_RIGHT),
        node::DOOR_RR => Some(door_id::REAR_RIGHT),
        node::DOOR_LF => Some(door_id::FRONT_LEFT),
        node::DOOR_LR => Some(door_id::REAR_LEFT),
        _ => None,
    }
}

impl World {
    /// Starts an animation on the ped and keeps it as the vehicle animation (`m_pVehicleAnim`).
    pub(super) fn set_vehicle_anim(&mut self, r: EntityRef, anim_id: u32, add: bool, delta: f32, finish: Option<u64>) {
        let i = if add { self.ped_add_anim(r, assoc_group::STD, anim_id) } else { self.ped_blend_anim(r, assoc_group::STD, anim_id, delta) };
        if let Some(a) = i.and_then(|i| self.ped_assoc_at(r, i)) {
            if let Some(token) = finish {
                a.set_finish_callback(token);
            }
            self.ped_mut(r).vehicle_anim = Some(anim_id as i32);
        }
    }

    /// The association `m_pVehicleAnim` points at.
    fn vehicle_anim_assoc(&self, r: EntityRef) -> Option<&crate::anim::Association> {
        let id = self.ped(r).vehicle_anim?;
        self.ped_clump(r)?.anim.association(id)
    }

    /// Does the ped play any of these animations?
    fn plays_any(&self, r: EntityRef, ids: &[u32]) -> bool {
        ids.iter().any(|&a| self.ped_assoc(r, a).is_some())
    }

    /// `CPed::LineUpPedWithCar`.
    pub fn line_up_ped_with_car(&mut self, r: EntityRef, phase: u8) {
        if !self.ped(r).flags.changed_seat && phase != line_up::TO_CAR_2 {
            if self.plays_any(r, &[anim::STD_CAR_SIT, anim::STD_CAR_SIT_LO, anim::STD_CAR_SIT_P, anim::STD_CAR_SIT_P_LO]) {
                self.set_ped_position_in_car(r);
                return;
            }
            self.ped_mut(r).flags.changed_seat = true;
        }
        if phase == line_up::TO_CAR_START {
            self.zero_ped_move_speed(r);
        }
        let Some(veh) = self.ped(r).my_vehicle else { return };
        let vm = self.ent(veh).base.matrix;
        let upside_down = vm.up.z <= -0.8;
        let (door, is_bus) = (self.ped(r).veh_door as usize, self.try_veh(veh).is_some_and(|v| v.flags.is_bus));
        let veh_heading = heading(vm.forward);
        // not quite right: IsUpsideDown tests for <= -0.9
        let dest = match door {
            node::DOOR_LF | node::DOOR_LR => {
                if upside_down {
                    veh_heading
                } else if is_bus {
                    -0.5 * PI + veh_heading
                } else {
                    veh_heading
                }
            }
            node::DOOR_RF | node::DOOR_RR => {
                if upside_down {
                    -PI + veh_heading
                } else if is_bus {
                    0.5 * PI + veh_heading
                } else {
                    veh_heading
                }
            }
            _ => {
                // boats, probably
                if upside_down {
                    veh_heading
                } else if is_bus {
                    0.5 * PI + veh_heading
                } else {
                    veh_heading
                }
            }
        };
        self.ped_mut(r).rotation_dest = dest;
        let mut seat_pos_mult = if self.ped(r).in_vehicle { 0.0 } else { 1.0 };
        let veh_anim = self.vehicle_anim_assoc(r).map(|a| (a.anim_id as u32, a.current_time, a.hierarchy.total_length, a.time_left(), a.time_step));
        if let Some((id, time, total, left, _)) = veh_anim {
            match id {
                anim::STD_JACKEDCAR_RHS
                | anim::STD_JACKEDCAR_LO_RHS
                | anim::STD_JACKEDCAR_LHS
                | anim::STD_JACKEDCAR_LO_LHS
                | anim::STD_VAN_GET_IN_REAR_LHS
                | anim::STD_VAN_GET_IN_REAR_RHS
                | anim::STD_QUICKJACKED
                | anim::STD_GETOUT_LHS
                | anim::STD_GETOUT_LO_LHS
                | anim::STD_GETOUT_RHS
                | anim::STD_GETOUT_LO_RHS
                | anim::STD_CRAWLOUT_LHS
                | anim::STD_CRAWLOUT_RHS
                | anim::STD_VAN_GET_OUT_REAR_LHS
                | anim::STD_VAN_GET_OUT_REAR_RHS => seat_pos_mult = time / total,
                anim::STD_CAR_GET_IN_RHS | anim::STD_CAR_GET_IN_LHS | anim::STD_QUICKJACK | anim::STD_CAR_GET_IN_LO_LHS | anim::STD_CAR_GET_IN_LO_RHS | anim::STD_BOAT_DRIVE => seat_pos_mult = left / total,
                anim::STD_CAR_CLOSE_DOOR_LHS
                | anim::STD_CAR_CLOSE_DOOR_LO_LHS
                | anim::STD_CAR_CLOSE_DOOR_RHS
                | anim::STD_CAR_CLOSE_DOOR_LO_RHS
                | anim::STD_CAR_SHUFFLE_RHS
                | anim::STD_CAR_SHUFFLE_LO_RHS => seat_pos_mult = 0.0,
                anim::STD_CAR_CLOSE_LHS
                | anim::STD_CAR_CLOSE_RHS
                | anim::STD_COACH_OPEN_LHS
                | anim::STD_COACH_OPEN_RHS
                | anim::STD_COACH_GET_IN_LHS
                | anim::STD_COACH_GET_IN_RHS
                | anim::STD_COACH_GET_OUT_LHS => seat_pos_mult = 1.0,
                _ => {}
            }
        }

        let pos = self.epos(r);
        let mut needed_pos = if phase == line_up::TO_CAR_2 { pos } else { self.get_position_to_open_car_door_offset(veh, door as u32, seat_pos_mult) };
        let mut auto_z_pos = needed_pos;
        let veh_in_water = self.get(veh).and_then(|e| e.phys.as_ref()).is_some_and(|p| p.flags.is_in_water);
        if veh_in_water {
            if self.try_veh(veh).is_some_and(|v| v.is_boat()) && vm.up.z <= -0.9 {
                auto_z_pos.z += 1.0;
            }
        } else {
            self.find_z_coor_for_ped(&mut auto_z_pos);
        }

        if phase == line_up::TO_CAR_END || phase == line_up::TO_CAR_2 {
            needed_pos.z = pos.z;
            // getting out
            if !is_bus || vm.up.z <= -0.9 {
                let step = self.time_step();
                let move_speed = self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
                let next_z_speed = move_speed.z - GRAVITY * step;
                // if the ped is not on the ground next step, the animation moves it
                if needed_pos.z + next_z_speed >= auto_z_pos.z {
                    self.ped_move_speed_z(r, next_z_speed);
                    self.apply_ped_move_speed(r);
                    // removing this breaks the animation
                    needed_pos.z = self.epos(r).z;
                } else {
                    needed_pos.z = auto_z_pos.z;
                    self.zero_ped_move_speed(r);
                }
            }
        }

        const VAN_GET_IN_OR_CLOSE: [u32; 4] = [anim::STD_VAN_GET_IN_REAR_LHS, anim::STD_VAN_CLOSE_DOOR_REAR_LHS, anim::STD_VAN_CLOSE_DOOR_REAR_RHS, anim::STD_VAN_GET_IN_REAR_RHS];
        if auto_z_pos.z > needed_pos.z {
            let current_z = self.epos(r).z;
            match veh_anim {
                Some((id, ..)) if !VAN_GET_IN_OR_CLOSE.contains(&id) => {
                    needed_pos.z = auto_z_pos.z;
                    self.zero_ped_move_speed(r);
                }
                Some((id, _, _, left, step)) if needed_pos.z < current_z && id != anim::STD_VAN_CLOSE_DOOR_REAR_LHS && id != anim::STD_VAN_CLOSE_DOOR_REAR_RHS => {
                    // smoothly change the ped's height
                    needed_pos.z = current_z - (current_z - needed_pos.z) / (left / step.max(0.1));
                }
                _ => {}
            }
        } else if phase == line_up::TO_CAR_START {
            let current_z = self.epos(r).z;
            if needed_pos.z > current_z {
                const RAISING: [u32; 7] =
                    [anim::STD_CAR_GET_IN_RHS, anim::STD_CAR_GET_IN_LO_RHS, anim::STD_CAR_GET_IN_LHS, anim::STD_CAR_GET_IN_LO_LHS, anim::STD_QUICKJACK, anim::STD_VAN_GET_IN_REAR_LHS, anim::STD_VAN_GET_IN_REAR_RHS];
                match veh_anim {
                    Some((id, _, _, left, step)) if RAISING.contains(&id) => {
                        needed_pos.z = (needed_pos.z - current_z) / (left / step.max(0.1)) + current_z;
                    }
                    _ if self.ped(r).entering_car() => needed_pos.z = current_z.max(auto_z_pos.z),
                    _ => {}
                }
            }
        }

        let now = self.now();
        let mut still_getting_in_out = false;
        if now < self.ped(r).ped_state_timer {
            still_getting_in_out = !self.try_veh(veh).is_some_and(|v| v.is_boat()) || self.ped(r).flags.on_boat;
        }
        if !still_getting_in_out {
            let pd = self.ped_mut(r);
            pd.rotation_cur = pd.rotation_dest;
        } else {
            let pd = self.ped_mut(r);
            let mut limited_dest = limit_radian_angle(pd.rotation_dest);
            let time_until_state_change = pd.ped_state_timer.wrapping_sub(now) as f32 / 600.0;
            if time_until_state_change <= 0.0 {
                pd.offset_seek.x = 0.0;
                pd.offset_seek.y = 0.0;
            }
            pd.offset_seek.z = 0.0;
            needed_pos -= time_until_state_change * pd.offset_seek;
            if PI + pd.rotation_cur < limited_dest {
                limited_dest -= 2.0 * PI;
            } else if pd.rotation_cur - PI > limited_dest {
                limited_dest += 2.0 * PI;
            }
            pd.rotation_cur -= (pd.rotation_cur - limited_dest) * (1.0 - time_until_state_change);
        }

        if seat_pos_mult > 0.2 || upside_down {
            let rot = self.ped(r).rotation_cur;
            let e = self.get_mut(r).expect("ped");
            e.base.set_position(needed_pos);
            e.base.set_heading(rot);
        } else {
            let mut door_mat = vm;
            let local = self.get_local_position_to_open_car_door(veh, door as u32, 0.0);
            door_mat.pos += door_mat.right * local.x + door_mat.forward * local.y + door_mat.up * local.z;
            // (Vice City fixes the couch animations here)
            self.get_mut(r).expect("ped").base.matrix = door_mat;
        }
    }

    /// `m_vecMoveSpeed = CVector(0, 0, 0)`.
    pub(super) fn zero_ped_move_speed(&mut self, r: EntityRef) {
        if let Some(p) = self.get_mut(r).and_then(|e| e.phys.as_mut()) {
            p.move_speed = Vec3::ZERO;
        }
    }

    fn ped_move_speed_z(&mut self, r: EntityRef, z: f32) {
        if let Some(p) = self.get_mut(r).and_then(|e| e.phys.as_mut()) {
            p.move_speed.z = z;
        }
    }

    /// `CPhysical::ApplyMoveSpeed` on a ped.
    fn apply_ped_move_speed(&mut self, r: EntityRef) {
        let step = self.time_step();
        let e = self.get_mut(r).expect("ped");
        if let Some(p) = e.phys.as_ref() {
            let mut m = e.base.matrix;
            p.apply_move_speed(&mut m, step);
            e.base.matrix = m;
        }
    }

    /// `CPed::SetCarJack`.
    pub fn set_car_jack(&mut self, px: &mut Px, r: EntityRef, car: EntityRef) {
        let Some(v) = self.try_veh(car) else { return };
        if v.is_boat() {
            return;
        }
        let pd = self.ped(r);
        let door = pd.veh_door as usize;
        let (driver, passengers) = (v.driver, v.passengers);
        let mut ped_in_seat = match door {
            node::DOOR_RF => passengers[0].or_else(|| (pd.ped_type == ped_type::COP).then_some(driver).flatten()),
            node::DOOR_RR => passengers[2],
            node::DOOR_LF => driver,
            node::DOOR_LR => passengers[1],
            _ => None,
        };
        if v.flags.is_bus {
            ped_in_seat = driver;
        }
        let door_flag = car_door_flag(door);
        let eligible = pd.health > 0.0
            && (pd.is_player() || matches!(pd.objective, objective::KILL_CHAR_ON_FOOT | objective::KILL_CHAR_ANY_MEANS) || (v.created_by != MISSION_VEHICLE && self.ent(car).base.model_index != mi::DODO));
        let looking_aside = px.hooks.camera().looking_left_or_right;
        let Some(seat) = ped_in_seat.filter(|&s| eligible && !self.is_ped_doing_drive_by_shooting(s, looking_aside) && self.ped(s).ped_state == state::DRIVING) else { return };
        let _ = seat;
        let door_ok = match door_of_component(door) {
            Some(d) => self.car_ref(car).is_none_or(|c| c.is_door_ready(d, v.getting_in_flags) || c.is_door_fully_open(d)),
            None => true,
        };
        if pd.ped_state != state::CARJACK && pd.vehicle_anim.is_none() && door_ok && !v.flags.is_being_car_jacked && door_flag & v.getting_in_flags == 0 && door_flag & v.getting_out_flags == 0 {
            self.set_car_jack_all_clear(px, r, car, door as u32, door_flag);
        }
    }

    /// `CPed::SetCarJack_AllClear`.
    fn set_car_jack_all_clear(&mut self, px: &mut Px, r: EntityRef, car: EntityRef, _door_node: u32, door_flag: i8) {
        self.remove_weapon_when_entering_vehicle(r);
        if self.ped(r).ped_state != state::SEEK_CAR {
            self.set_stored_state(r);
        }
        let pd = self.ped_mut(r);
        pd.seek_target = Some(car);
        pd.ped_state = state::CARJACK;
        pd.my_vehicle = Some(car);
        let cop = pd.ped_type == ped_type::COP;
        if let Some(v) = self.try_veh_mut(car) {
            v.flags.is_being_car_jacked = true;
            v.num_getting_in += 1;
        }
        self.say(px, r, if cop { sound::PED_ARREST_COP } else { sound::PED_CAR_JACKING });
        let door = self.ped(r).veh_door as u32;
        let enter_pos = self.get_position_to_open_car_door(car, door);
        if let Some(v) = self.try_veh_mut(car) {
            v.getting_in_flags |= door_flag;
        }
        let (pos, now) = (self.epos(r), self.now());
        let pd = self.ped_mut(r);
        pd.offset_seek = enter_pos - pos;
        pd.ped_state_timer = now.wrapping_add(600);
        let z_diff = (enter_pos.z - pos.z).max(0.0);
        self.get_mut(r).expect("ped").base.flags.uses_collision = false;
        let high = z_diff > 4.4;
        let align = if matches!(door as usize, node::DOOR_LF | node::DOOR_LR) {
            if high { anim::STD_CAR_ALIGNHI_DOOR_LHS } else { anim::STD_CAR_ALIGN_DOOR_LHS }
        } else if high {
            anim::STD_CAR_ALIGNHI_DOOR_RHS
        } else {
            anim::STD_CAR_ALIGN_DOOR_RHS
        };
        self.set_vehicle_anim(r, align, false, 4.0, Some(cb::PED_ANIM_ALIGN));
    }

    /// `CPed::SetBeingDraggedFromCar`.
    pub fn set_being_dragged_from_car(&mut self, px: &mut Px, r: EntityRef, veh: EntityRef, enter_type: u32, quick_jack: bool) {
        if self.ped(r).ped_state == state::DRAG_FROM_CAR {
            return;
        }
        self.get_mut(r).expect("ped").base.flags.uses_collision = false;
        self.zero_ped_move_speed(r);
        let pd = self.ped_mut(r);
        pd.last_ped_state = state::IDLE;
        pd.set_move_state(move_state::STILL);
        pd.seek_target = Some(veh);
        pd.veh_door = enter_type as u16;
        if enter_type as usize == node::DOOR_LF {
            let driver_is_player = self.try_veh(veh).and_then(|v| v.driver).is_some_and(|d| self.ped(d).is_player());
            self.get_mut(veh).expect("vehicle").base.status = if driver_is_player { status::PLAYER_DISABLED } else { status::ABANDONED };
        }
        self.remove_in_car_anims(r);
        self.ped_mut(r).set_move_state(move_state::NONE);
        self.line_up_ped_with_car(r, line_up::TO_CAR_START);
        let pd = self.ped_mut(r);
        pd.vehicle_anim = None;
        pd.ped_state = state::DRAG_FROM_CAR;
        pd.flags.changed_seat = false;
        pd.flags.will_be_quick_jacked = quick_jack;
        let rot = pd.rotation_cur;
        self.get_mut(r).expect("ped").base.set_heading(rot);
        self.say(px, r, sound::PED_CAR_JACKED);
        self.set_radio_station(px, r);
        if let Some(v) = self.try_veh_mut(veh) {
            v.getting_out_flags |= car_door_flag(enter_type as usize);
        }
    }

    /// `CPed::BeingDraggedFromCar`.
    pub fn being_dragged_from_car(&mut self, r: EntityRef) {
        let line_up_type = match self.vehicle_anim_assoc(r).map(|a| a.current_time) {
            None => {
                let group = self.ped(r).anim_group as usize;
                self.ped_blend_anim(r, group, anim::STD_IDLE, 100.0);
                for sit in [anim::STD_CAR_SIT, anim::STD_CAR_SIT_LO, anim::STD_CAR_SIT_P, anim::STD_CAR_SIT_P_LO] {
                    if let Some(a) = self.ped_assoc_mut(r, sit) {
                        a.blend_delta = -1000.0;
                        break;
                    }
                }
                let pd = self.ped(r);
                let low = pd.my_vehicle.and_then(|v| self.try_veh(v)).is_some_and(|v| v.flags.low_vehicle);
                let enter = match pd.veh_door as usize {
                    node::DOOR_LF | node::DOOR_LR => {
                        if pd.flags.will_be_quick_jacked {
                            Some(anim::STD_QUICKJACKED)
                        } else if low {
                            Some(anim::STD_JACKEDCAR_LO_LHS)
                        } else {
                            Some(anim::STD_JACKEDCAR_LHS)
                        }
                    }
                    node::DOOR_RF | node::DOOR_RR => Some(if low { anim::STD_JACKEDCAR_LO_RHS } else { anim::STD_JACKEDCAR_RHS }),
                    _ => None,
                };
                if let Some(a) = enter {
                    self.set_vehicle_anim(r, a, true, 0.0, Some(cb::PED_SET_DRAGGED_OUT_CAR));
                }
                line_up::TO_CAR_START
            }
            Some(t) if t <= 1.4 => {
                self.zero_ped_move_speed(r);
                line_up::TO_CAR_START
            }
            Some(_) => line_up::TO_CAR_2,
        };
        self.line_up_ped_with_car(r, line_up_type);
    }

    /// `CPed::SetEnterCar`.
    pub fn set_enter_car(&mut self, px: &mut Px, r: EntityRef, car: EntityRef, _unused: u32) {
        if self.is_car_carried_by_crane(car) {
            self.restore_previous_state(px, r);
            self.restore_previous_objective(px, r);
            return;
        }
        let door = self.ped(r).veh_door as usize;
        let door_flag = car_door_flag(door);
        let Some(v) = self.try_veh(car) else { return };
        let pd = self.ped(r);
        let door_not_ready = door_of_component(door).filter(|_| door_flag != 0).is_some_and(|d| self.car_ref(car).is_some_and(|c| !c.is_door_ready(d, v.getting_in_flags) && !c.is_door_fully_open(d)));
        if !pd.is_ped_in_control()
            || pd.health <= 0.0
            || door_flag & v.getting_in_flags != 0
            || door_flag & v.getting_out_flags != 0
            || v.flags.is_being_car_jacked
            || pd.vehicle_anim.is_some()
            || door_not_ready
        {
            self.ped_mut(r).set_move_state(move_state::STILL);
        } else {
            self.set_enter_car_all_clear(px, r, car, door as u32, door_flag);
        }
    }

    /// `CPed::SetEnterCar_AllClear`.
    fn set_enter_car_all_clear(&mut self, px: &mut Px, r: EntityRef, car: EntityRef, door_node: u32, door_flag: i8) {
        self.remove_weapon_when_entering_vehicle(r);
        if let Some(v) = self.try_veh_mut(car) {
            v.getting_in_flags |= door_flag;
        }
        self.ped_mut(r).flags.veh_enter_door_is_blocked = false;
        if !matches!(self.ped(r).ped_state, state::SEEK_CAR | state::SEEK_IN_BOAT) {
            self.set_stored_state(r);
        }
        let pd = self.ped_mut(r);
        pd.seek_target = Some(car);
        pd.veh_door = door_node as u16;
        pd.ped_state = state::ENTER_CAR;
        let driver_objective = pd.objective == objective::ENTER_CAR_AS_DRIVER;
        let not_bike = !self.try_veh(car).is_some_and(|v| v.is_bike());
        if door_node as usize == node::DOOR_RF && driver_objective && not_bike {
            self.try_veh_mut(car).expect("vehicle").flags.is_being_car_jacked = true;
        }
        self.ped_mut(r).my_vehicle = Some(car);
        let is_bus = self.try_veh(car).is_some_and(|v| v.flags.is_bus);
        if let Some(v) = self.try_veh_mut(car) {
            v.num_getting_in += 1;
        }
        self.get_mut(r).expect("ped").base.flags.uses_collision = false;
        let door_open_pos = self.get_position_to_open_car_door(car, door_node);
        let pos = self.epos(r);
        // buses have stairs
        let z_diff = if is_bus { 0.0 } else { (door_open_pos.z - pos.z).max(0.0) };
        let now = self.now();
        let pd = self.ped_mut(r);
        pd.offset_seek = door_open_pos - pos;
        pd.ped_state_timer = now.wrapping_add(600);
        if self.try_veh(car).is_some_and(|v| v.is_boat()) {
            // FIX_BUGS: the boat's own driver animation; enter at once so the key press is not handled twice
            let driver_anim = self.driver_anim(car);
            self.set_vehicle_anim(r, driver_anim, false, 100.0, None);
            self.ped_set_in_car_cb(px, r, None);
            self.ped_mut(r).flags.veh_exit_will_be_instant = true;
        } else {
            let high = z_diff > 4.4;
            let right = matches!(door_node as usize, node::DOOR_RF | node::DOOR_RR);
            let align = match (right, high) {
                (true, true) => anim::STD_CAR_ALIGNHI_DOOR_RHS,
                (true, false) => anim::STD_CAR_ALIGN_DOOR_RHS,
                (false, true) => anim::STD_CAR_ALIGNHI_DOOR_LHS,
                (false, false) => anim::STD_CAR_ALIGN_DOOR_LHS,
            };
            self.set_vehicle_anim(r, align, false, 4.0, Some(cb::PED_ANIM_ALIGN));
            self.try_veh_mut(car).expect("vehicle").auto_pilot.cruise_speed = 0;
        }
    }

    /// `CPed::EnterCar`.
    pub fn enter_car(&mut self, px: &mut Px, r: EntityRef) {
        if self.is_not_in_wrecked_vehicle(r) && self.ped(r).health > 0.0 {
            let Some(veh) = self.ped(r).seek_target else { return };
            let is_player = self.ped(r).is_player();
            if self.try_veh(veh).is_some_and(|v| v.can_ped_open_locks(is_player))
                && self.ped(r).veh_door != 0
                && let Some((id, time)) = self.vehicle_anim_assoc(r).map(|a| (a.anim_id as u32, a.current_time))
            {
                let door = self.ped(r).veh_door as usize;
                self.process_open_door(veh, door, id, time);
            }
            self.ped_mut(r).flags.is_in_the_air = false;
            self.line_up_ped_with_car(r, line_up::TO_CAR_START);
        } else {
            self.quit_entering_car(px, r);
            self.set_die(px, r, anim::STD_KO_FRONT, 4.0, 0.0);
        }
    }

    /// `CPed::QuitEnteringCar`.
    pub fn quit_entering_car(&mut self, px: &mut Px, r: EntityRef) {
        let veh = self.ped(r).my_vehicle;
        if let Some(a) = self.vehicle_anim_id(r).and_then(|id| self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(id))) {
            a.blend_delta = -1000.0;
        }
        self.restart_non_partial_anims(r);
        if self.ped_assoc(r, anim::STD_IDLE).is_none() {
            let group = self.ped(r).anim_group as usize;
            self.ped_blend_anim(r, group, anim::STD_IDLE, 100.0);
        }
        if let Some(veh) = veh {
            let pd = self.ped(r);
            let (driver_objective, jack, door) = (pd.objective == objective::ENTER_CAR_AS_DRIVER, pd.ped_state == state::CARJACK, pd.veh_door as usize);
            if let Some(v) = self.try_veh_mut(veh) {
                if driver_objective || jack {
                    v.flags.is_being_car_jacked = false;
                }
                if v.num_getting_in != 0 {
                    v.num_getting_in -= 1;
                }
                v.getting_in_flags &= !car_door_flag(door);
            }
        }
        self.get_mut(r).expect("ped").base.flags.uses_collision = true;
        self.replace_weapon_when_exiting_vehicle(r);
        if self.ped(r).dying_or_dead() {
            let flags_id = self.vehicle_anim_id(r);
            if let Some(a) = flags_id.and_then(|id| self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(id))) {
                a.blend_delta = -4.0;
                a.flags |= aflags::DELETE_FADED_OUT;
                a.flags &= !aflags::RUNNING;
            }
        } else {
            self.set_idle(px, r);
        }
        self.ped_mut(r).vehicle_anim = None;
        if let Some(v) = veh.and_then(|v| self.try_veh_mut(v))
            && v.auto_pilot.cruise_speed == 0
        {
            v.auto_pilot.cruise_speed = 17;
        }
    }

    fn vehicle_anim_id(&self, r: EntityRef) -> Option<i32> {
        self.ped(r).vehicle_anim
    }

    /// `CPed::SetExitBoat` (without VC_PED_PORTS).
    pub fn set_exit_boat(&mut self, px: &mut Px, r: EntityRef, boat: EntityRef) {
        self.ped_mut(r).ped_state = state::IDLE;
        let mut new_pos = self.epos(r);
        self.remove_in_car_anims(r);
        let m = self.ent(boat).base.matrix;
        if m.up.z <= -0.9 {
            let min_z = self.col_model_of(boat).map_or(0.0, |c| c.bounding_box.min.z);
            new_pos = m.transform_point(Vec3::new(0.0, 0.0, min_z));
            new_pos.z += 1.0;
        }
        self.ped_mut(r).veh_door = node::DOOR_RF as u16;
        self.ped_set_out_car_cb(px, r, None);
        let pd = self.ped_mut(r);
        pd.flags.is_standing = true;
        pd.cur_surface = Some(boat);
        pd.current_phys_surface = Some(boat);
        if m.up.z > -0.9 {
            let kinds = crate::world::Kinds { vehicles: true, ..crate::world::Kinds::NONE };
            if let Some(hit) = self.process_vertical_line(new_pos, new_pos.z - 1.4, kinds, false, None) {
                new_pos.z = FEET_OFFSET + hit.point.point.z;
            }
        }
        self.get_mut(r).expect("ped").base.set_position(new_pos);
        self.ped_mut(r).set_move_state(move_state::STILL);
        let boat_speed = self.ent(boat).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
        if let Some(p) = self.get_mut(r).and_then(|e| e.phys.as_mut()) {
            p.move_speed = boat_speed;
        }
    }

    /// `CPed::SetExitCar`; a door of 0 lets the function choose.
    pub fn set_exit_car(&mut self, px: &mut Px, r: EntityRef, veh: EntityRef, wanted_door: u32) {
        let mut opted = wanted_door as usize;
        let mut teleport_needed = false;
        let Some(v) = self.try_veh(veh) else { return };
        let low = v.flags.low_vehicle;
        let can_exit = self.get(veh).is_some_and(|e| e.phys.as_ref().is_some_and(|p| crate::vehicle::base::VehicleData::can_ped_exit_car(&e.base.matrix, p)));
        if !can_exit {
            if v.driver.is_some_and(|d| !self.ped(d).is_player()) {
                let v = self.try_veh_mut(veh).expect("vehicle");
                v.auto_pilot.cruise_speed = 0;
                v.auto_pilot.car_mission = crate::vehicle::base::car_mission::NONE;
            }
            return;
        }
        if matches!(self.ped(r).ped_state, state::EXIT_CAR | state::DRAG_FROM_CAR) {
            return;
        }
        self.zero_ped_move_speed(r);
        if let Some(p) = self.get_mut(r).and_then(|e| e.phys.as_mut()) {
            p.turn_speed = Vec3::ZERO;
        }
        if wanted_door == 0 {
            opted = node::DOOR_LF;
            let v = self.try_veh(veh).expect("vehicle");
            if !v.flags.is_bus {
                if v.driver == Some(r) {
                    opted = node::DOOR_LF;
                } else if v.passengers[0] == Some(r) {
                    opted = node::DOOR_RF;
                } else if v.passengers[1] == Some(r) {
                    opted = node::DOOR_LR;
                } else if v.passengers[2] == Some(r) {
                    opted = node::DOOR_RR;
                } else if let Some(i) = (3..v.num_max_passengers as usize).find(|&i| v.passengers[i] == Some(r)) {
                    opted = if i & 1 != 0 { node::DOOR_RR } else { node::DOOR_LR };
                }
            }
        }
        let v = self.try_veh(veh).expect("vehicle");
        let (getting_in, getting_out) = (v.getting_in_flags, v.getting_out_flags);
        let flag = car_door_flag(opted);
        let someone_enters = getting_in & flag != 0;
        let someone_exits = getting_out & flag != 0;
        if someone_enters && self.ped(r).objective == objective::LEAVE_CAR {
            self.restore_previous_objective(px, r);
            return;
        }
        let cop = self.ped(r).ped_type == ped_type::COP;
        let bus = self.try_veh(veh).is_some_and(|v| v.flags.is_bus);
        if someone_exits && !(cop && bus) {
            return;
        }
        let there_is_room = self.room_to_leave_car(veh, opted as u32, None);
        let on_its_side = self.get(veh).is_some_and(|e| crate::vehicle::base::VehicleData::is_on_its_side(&e.base.matrix));
        if on_its_side {
            teleport_needed = true;
        } else if !there_is_room {
            let v = self.try_veh(veh).expect("vehicle");
            let gi = |n: usize| v.getting_in_flags & car_door_flag(n) != 0;
            // (try the seat beside: its occupant, the door that frees up)
            let mut try_side_seat = false;
            let mut ped_on_side_seat = None;
            match opted {
                node::DOOR_RF => {
                    if v.driver.is_some() || gi(node::DOOR_LF) {
                        ped_on_side_seat = v.driver;
                        try_side_seat = true;
                    } else {
                        opted = node::DOOR_LF;
                    }
                }
                node::DOOR_RR => {
                    if v.passengers[1].is_some() || gi(node::DOOR_LR) {
                        ped_on_side_seat = v.passengers[1];
                        try_side_seat = true;
                    } else {
                        opted = node::DOOR_LR;
                    }
                }
                node::DOOR_LF => {
                    if v.passengers[0].is_some() || gi(node::DOOR_RF) {
                        ped_on_side_seat = v.passengers[0];
                        try_side_seat = true;
                    } else {
                        opted = node::DOOR_RF;
                    }
                }
                node::DOOR_LR => {
                    if v.passengers[2].is_some() || gi(node::DOOR_RR) {
                        ped_on_side_seat = v.passengers[2];
                        try_side_seat = true;
                    } else {
                        opted = node::DOOR_RR;
                    }
                }
                _ => {}
            }
            let pd = self.ped(r);
            let player_like = pd.is_player() || pd.char_created_by == MISSION_CHAR;
            if try_side_seat {
                if ped_on_side_seat.is_none() || !player_like {
                    return;
                }
                opted = match opted {
                    node::DOOR_RF => node::DOOR_LF,
                    node::DOOR_RR => node::DOOR_LR,
                    node::DOOR_LF => node::DOOR_RF,
                    node::DOOR_LR => node::DOOR_RR,
                    other => other,
                };
            }
            if !self.room_to_leave_car(veh, opted as u32, None) {
                if !player_like {
                    return;
                }
                teleport_needed = true;
            }
        }
        if self.ped(r).ped_state == state::FLEE_POS {
            let pd = self.ped_mut(r);
            pd.last_ped_state = state::FLEE_POS;
            pd.prev_move_state = move_state::RUN as i32;
            pd.set_move_state(move_state::SPRINT);
        } else {
            let pd = self.ped_mut(r);
            pd.last_ped_state = state::IDLE;
            pd.prev_move_state = move_state::STILL as i32;
            pd.set_move_state(move_state::STILL);
        }
        self.replace_weapon_when_exiting_vehicle(r);
        self.get_mut(r).expect("ped").base.flags.uses_collision = false;
        let pd = self.ped_mut(r);
        pd.seek_target = Some(veh);
        pd.veh_door = opted as u16;
        pd.ped_state = state::EXIT_CAR;
        if let Some(a) = self.vehicle_anim_id(r).and_then(|id| self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(id)))
            && a.flags & aflags::PARTIAL != 0
        {
            a.blend_delta = -1000.0;
        }
        self.ped_mut(r).set_move_state(move_state::NONE);
        let group = self.ped(r).anim_group as usize;
        self.ped_blend_anim(r, group, anim::STD_IDLE, 100.0);
        self.remove_in_car_anims(r);
        self.try_veh_mut(veh).expect("vehicle").auto_pilot.cruise_speed = 0;
        if teleport_needed {
            self.ped_set_out_car_cb(px, r, None);
            // the same probe as FindZCoorForPed, starting 1.5 up and also hitting vehicles
            let mut vec = self.epos(r);
            vec.z += 1.5;
            let start_z = self.epos(r).z - 100.0;
            let kinds = crate::world::Kinds { buildings: true, vehicles: true, dummies: true, ..crate::world::Kinds::NONE };
            let first = self.process_vertical_line(vec, start_z, kinds, false, None).map_or(-100.0, |h| h.point.point.z);
            vec.x += 0.1;
            vec.y += 0.1;
            let second = self.process_vertical_line(vec, start_z, kinds, false, None).map_or(-100.0, |h| h.point.point.z);
            let z = first.max(second);
            if z > -99.0 {
                let mut p = self.epos(r);
                p.z = FEET_OFFSET + z;
                self.get_mut(r).expect("ped").base.set_position(p);
            }
        } else if self.ent(veh).base.matrix.up.z > -0.8 {
            self.start_exit_animation(r, veh, low);
        } else {
            let door = self.ped(r).veh_door as usize;
            let crawl = if matches!(door, node::DOOR_RF | node::DOOR_RR) { Some(anim::STD_CRAWLOUT_RHS) } else if matches!(door, node::DOOR_LF | node::DOOR_LR) { Some(anim::STD_CRAWLOUT_LHS) } else { None };
            if let Some(a) = crawl {
                self.set_vehicle_anim(r, a, true, 0.0, Some(cb::PED_SET_OUT_CAR));
            }
        }
        self.ped_mut(r).flags.changed_seat = false;
        if bus {
            self.ped_mut(r).flags.render_ped_in_car = true;
        }
        self.set_radio_station(px, r);
        if self.try_veh(veh).and_then(|v| v.driver) == Some(r) {
            self.get_mut(veh).expect("vehicle").base.status = if self.ped(r).is_player() { status::PLAYER_DISABLED } else { status::ABANDONED };
        }
    }

    /// The step-out animation of `SetExitCar` and the door flag of whoever is getting out.
    fn start_exit_animation(&mut self, r: EntityRef, veh: EntityRef, low: bool) {
        let door = self.ped(r).veh_door as usize;
        let (bus, van) = self.try_veh(veh).map_or((false, false), |v| (v.flags.is_bus, v.flags.is_van));
        let yardie = self.ent(veh).base.model_index == mi::YARDIE;
        let a = match door {
            node::DOOR_RF => {
                if bus {
                    Some(anim::STD_COACH_GET_OUT_LHS)
                } else {
                    if yardie {
                        self.add_yardie_door_smoke(veh, node::DOOR_RF);
                    }
                    Some(if low { anim::STD_GETOUT_LO_RHS } else { anim::STD_GETOUT_RHS })
                }
            }
            node::DOOR_RR => Some(if van {
                anim::STD_VAN_GET_OUT_REAR_RHS
            } else if low {
                anim::STD_GETOUT_LO_RHS
            } else {
                anim::STD_GETOUT_RHS
            }),
            node::DOOR_LF => {
                if bus {
                    Some(anim::STD_COACH_GET_OUT_LHS)
                } else {
                    if yardie {
                        self.add_yardie_door_smoke(veh, node::DOOR_LF);
                    }
                    Some(if low { anim::STD_GETOUT_LO_LHS } else { anim::STD_GETOUT_LHS })
                }
            }
            node::DOOR_LR => Some(if van {
                anim::STD_VAN_GET_OUT_REAR_LHS
            } else if low {
                anim::STD_GETOUT_LO_LHS
            } else {
                anim::STD_GETOUT_LHS
            }),
            _ => None,
        };
        if let Some(a) = a {
            self.set_vehicle_anim(r, a, true, 0.0, Some(cb::PED_ANIM_STEP_OUT_CAR));
        }
        if !self.ped(r).flags.bus_jacked
            && let Some(v) = self.try_veh_mut(veh)
        {
            v.getting_out_flags |= car_door_flag(door);
        }
    }

    /// `AddYardieDoorSmoke`: a particle at the door; the particles are the renderer's.
    fn add_yardie_door_smoke(&mut self, _veh: EntityRef, _component: usize) {
        // shortcut: particles are not simulated, so nothing is spawned
    }

    /// `CPed::ExitCar`.
    pub fn exit_car(&mut self, px: &mut Px, r: EntityRef) {
        let Some((exit_anim, anim_time)) = self.vehicle_anim_assoc(r).map(|a| (a.anim_id as u32, a.current_time)) else { return };
        let Some(veh) = self.ped(r).my_vehicle else { return };
        let door = self.ped(r).veh_door as usize;
        self.process_open_door(veh, door, exit_anim, anim_time);
        if self.ped(r).seek_target.is_some() {
            let up_z = self.ent(veh).base.matrix.up.z;
            if up_z > -0.8 {
                let dodo = self.ent(veh).base.model_index == mi::DODO;
                if exit_anim == anim::STD_CAR_CLOSE_RHS || exit_anim == anim::STD_CAR_CLOSE_LHS || anim_time > 0.3 {
                    self.line_up_ped_with_car(r, line_up::TO_CAR_END);
                } else {
                    self.line_up_ped_with_car(r, if dodo { line_up::TO_CAR_END } else { line_up::TO_CAR_START });
                }
            } else {
                self.line_up_ped_with_car(r, line_up::TO_CAR_END);
            }
        }
        // someone in front of the door falls over as the ped gets out
        if self.ped(r).ped_state == state::EXIT_CAR {
            let pos = self.epos(r);
            let pd = self.ped(r);
            let found = pd.near_peds.iter().take(pd.num_near_peds as usize).flatten().copied().find(|&n| {
                let d = self.epos(n) - pos;
                d.x * d.x + d.y * d.y < 0.04
            });
            if let Some(f) = found.filter(|&f| anim_time > 0.4 && self.ped(f).is_ped_in_control()) {
                self.set_fall(px, f, 1000, anim::STD_HIGHIMPACT_FRONT, true);
            }
        }
    }

    /// `CPed::RemoveInCarAnims`.
    pub fn remove_in_car_anims(&mut self, r: EntityRef) {
        if !self.ped(r).is_player() {
            return;
        }
        let low = self.ped(r).my_vehicle.and_then(|v| self.try_veh(v)).is_some_and(|v| v.flags.low_vehicle);
        let list: [u32; 5] = if low {
            [anim::STD_CAR_DRIVE_LEFT_LO, anim::STD_CAR_DRIVE_RIGHT_LO, anim::STD_CAR_DRIVEBY_LEFT, anim::STD_CAR_DRIVEBY_RIGHT, anim::STD_CAR_LOOKBEHIND]
        } else {
            [anim::STD_CAR_DRIVE_LEFT, anim::STD_CAR_DRIVE_RIGHT, anim::STD_CAR_DRIVEBY_LEFT, anim::STD_CAR_DRIVEBY_RIGHT, anim::STD_CAR_LOOKBEHIND]
        };
        for id in list {
            if let Some(a) = self.ped_assoc_mut(r, id) {
                a.blend_delta = -1000.0;
            }
        }
    }

    /// `FRAC_PI_2` used by the exit callbacks.
    pub(super) fn bus_exit_angle(&self, veh: EntityRef, door: usize) -> f32 {
        let h = heading(self.ent(veh).base.matrix.forward);
        if door == node::DOOR_LF { FRAC_PI_2 + h } else { h - FRAC_PI_2 }
    }
}
