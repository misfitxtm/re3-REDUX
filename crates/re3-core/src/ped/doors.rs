//! `PedAI.cpp`: where a ped stands to open a car door and which door is the closest.
use super::data::*;
use crate::anim::tables_gta3::anim;
use crate::entity::EntityRef;
use crate::vehicle::automobile::{car_door_flag, node};
use crate::world::World;
use glam::{Vec2, Vec3};

/// `CAR_POS_FRONTSEAT` and `CAR_POS_BACKSEAT`.
const CAR_POS_FRONTSEAT: usize = 2;
const CAR_POS_BACKSEAT: usize = 3;

/// The translation at the end of the enter and exit animations (`SetAnimOffsetForEnterOrExitVehicle`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AnimOffsets {
    pub dragged_out: Vec3,
    pub car_door: Vec3,
    pub car_door_lo: Vec3,
    pub quick_dragged_out: Vec3,
    pub van_rear_door: Vec3,
    pub train_door: Vec3,
}

impl World {
    /// `CPed::SetAnimOffsetForEnterOrExitVehicle`: reads the offsets off the loaded animations.
    pub fn set_anim_offset_for_enter_or_exit_vehicle(&mut self) {
        use crate::anim::assoc_group::STD;
        let end = |w: &World, id: u32| -> Option<Vec3> {
            let a = w.ents.anim.assoc(STD, id as usize)?;
            let seq = a.hierarchy.sequences.first()?;
            let last = seq.keyframes.last()?;
            Some(if seq.has_translation { last.translation } else { Vec3::ZERO })
        };
        let o = &mut self.ents.ped_anim_offsets.clone();
        for (field, id) in [
            (&mut o.dragged_out, anim::STD_JACKEDCAR_LHS),
            (&mut o.car_door, anim::STD_CAR_GET_IN_LHS),
            (&mut o.car_door_lo, anim::STD_CAR_GET_IN_LO_LHS),
            (&mut o.quick_dragged_out, anim::STD_QUICKJACKED),
            (&mut o.van_rear_door, anim::STD_VAN_GET_IN_REAR_LHS),
            (&mut o.train_door, anim::STD_TRAIN_GETOUT),
        ] {
            if let Some(v) = end(self, id) {
                *field = v;
            }
        }
        self.ents.ped_anim_offsets = *o;
    }

    /// `CVehicle::GetDriverAnim`.
    pub fn driver_anim(&self, veh: EntityRef) -> u32 {
        let model = self.ent(veh).base.model_index;
        match self.try_veh(veh) {
            Some(v) if v.is_car() && v.flags.low_vehicle => anim::STD_CAR_SIT_LO,
            Some(v) if v.is_boat() && model != crate::modelinfo::indices_gta3::mi::SPEEDER => anim::STD_BOAT_DRIVE,
            _ => anim::STD_CAR_SIT,
        }
    }

    /// `CPed::GetLocalPositionToOpenCarDoor`.
    pub fn get_local_position_to_open_car_door(&self, veh: EntityRef, component: u32, seat_pos_mult: f32) -> Vec3 {
        let model = self.ent(veh).base.model_index as i32;
        let Some(positions) = self.ents.models.get(model).and_then(|m| m.vehicle()).map(|v| v.positions) else { return Vec3::ZERO };
        let Some(vd) = self.try_veh(veh) else { return Vec3::ZERO };
        let offsets = &self.ents.ped_anim_offsets;
        let (seat_offset, mut door_offset) = if vd.flags.is_van && matches!(component as usize, node::DOOR_LR | node::DOOR_RR) {
            (0.0, offsets.van_rear_door)
        } else {
            let seat = self.ents.handling.get(vd.handling).seat_offset_distance * seat_pos_mult;
            (seat, if vd.flags.low_vehicle { offsets.car_door_lo } else { offsets.car_door })
        };
        let mut pos;
        match component as usize {
            node::DOOR_RF => {
                pos = positions[CAR_POS_FRONTSEAT];
                pos.x += seat_offset;
                door_offset.x = -door_offset.x;
            }
            node::DOOR_RR => {
                pos = positions[CAR_POS_BACKSEAT];
                pos.x += seat_offset;
                door_offset.x = -door_offset.x;
            }
            node::DOOR_LF => {
                pos = positions[CAR_POS_FRONTSEAT];
                pos.x = -(pos.x + seat_offset);
            }
            node::DOOR_LR => {
                pos = positions[CAR_POS_BACKSEAT];
                pos.x = -(pos.x + seat_offset);
            }
            _ => {
                pos = positions[CAR_POS_FRONTSEAT];
                door_offset = Vec3::ZERO;
            }
        }
        pos - door_offset
    }

    /// `CPed::GetPositionToOpenCarDoor(veh, component, offset)`.
    pub fn get_position_to_open_car_door_offset(&self, veh: EntityRef, component: u32, offset: f32) -> Vec3 {
        let m = self.ent(veh).base.matrix;
        let local = self.get_local_position_to_open_car_door(veh, component, offset);
        self.epos(veh) + m.right * local.x + m.forward * local.y + m.up * local.z
    }

    /// `CPed::GetPositionToOpenCarDoor(veh, component)`: with the seat offset in full.
    pub fn get_position_to_open_car_door(&self, veh: EntityRef, component: u32) -> Vec3 {
        self.get_position_to_open_car_door_offset(veh, component, 1.0)
    }

    /// `IsRoomForPedToLeaveCar` for the door `component`, with the ped's exit offset when given.
    pub fn room_to_leave_car(&mut self, veh: EntityRef, component: u32, offset: Option<Vec3>) -> bool {
        let door_pos = self.get_position_to_open_car_door(veh, component);
        self.is_room_for_ped_to_leave_car(veh, component as usize, door_pos, offset)
    }

    /// `CPed::GetNearestDoor`: the closer of the two front doors that has room; `pos_to_open` is left alone if neither does.
    pub fn get_nearest_door(&mut self, r: EntityRef, veh: EntityRef, pos_to_open: &mut Vec3) -> bool {
        let Some(vd) = self.try_veh(veh) else { return false };
        let (driver, passengers, getting_in) = (vd.driver, vd.passengers, vd.getting_in_flags);
        let mission_vehicle = vd.created_by == crate::vehicle::base::MISSION_VEHICLE;
        let pd = self.ped(r);
        let occupied = match pd.veh_door as usize {
            node::DOOR_LF => driver.is_some(),
            node::DOOR_RF => passengers[0].is_some(),
            node::DOOR_LR => passengers[1].is_some(),
            node::DOOR_RR => passengers[2].is_some(),
            _ => false,
        };
        let enter_offset = occupied.then_some(self.ents.ped_anim_offsets.quick_dragged_out);
        let objective = pd.objective;
        let pos = self.epos(r);
        let lf = self.get_position_to_open_car_door(veh, node::DOOR_LF as u32);
        let rf = self.get_position_to_open_car_door(veh, node::DOOR_RF as u32);
        let dist2d = |a: Vec3| Vec2::new(a.x - pos.x, a.y - pos.y).length_squared();
        let (lf_door, rf_door) = (node::DOOR_LF as u32, node::DOOR_RF as u32);
        if dist2d(lf) < dist2d(rf) {
            if self.room_to_leave_car(veh, lf_door, enter_offset) {
                self.ped_mut(r).veh_door = lf_door as u16;
                *pos_to_open = lf;
            } else if self.room_to_leave_car(veh, rf_door, enter_offset) {
                self.ped_mut(r).veh_door = rf_door as u16;
                *pos_to_open = rf;
            }
        } else if self.room_to_leave_car(veh, rf_door, enter_offset) {
            let rf_passenger = passengers[0];
            let friendly = rf_passenger.is_some_and(|p| self.ped(p).leader == Some(r) || self.ped(p).flags.dont_drag_me_out_car)
                || (mission_vehicle && objective == super::data::objective::ENTER_CAR_AS_DRIVER);
            let rf_busy = getting_in & car_door_flag(node::DOOR_RF) != 0;
            if ((rf_passenger.is_some() && friendly) && self.room_to_leave_car(veh, lf_door, enter_offset)) || (rf_busy && self.room_to_leave_car(veh, lf_door, enter_offset)) {
                self.ped_mut(r).veh_door = lf_door as u16;
                *pos_to_open = lf;
            } else {
                self.ped_mut(r).veh_door = rf_door as u16;
                *pos_to_open = rf;
            }
        } else if self.room_to_leave_car(veh, lf_door, enter_offset) {
            self.ped_mut(r).veh_door = lf_door as u16;
            *pos_to_open = lf;
        }
        true
    }

    /// `CPed::GetNearestPassengerDoor`: true if some door has room.
    pub fn get_nearest_passenger_door(&mut self, r: EntityRef, veh: EntityRef, pos_to_open: &mut Vec3) -> bool {
        let model = self.ent(veh).base.model_index;
        if model == crate::modelinfo::indices_gta3::mi::BUS {
            self.ped_mut(r).veh_door = node::DOOR_RF as u16;
            *pos_to_open = self.get_position_to_open_car_door(veh, node::DOOR_RF as u32);
            return true;
        }
        let Some(vd) = self.try_veh(veh) else { return false };
        let (passengers, getting_in) = (vd.passengers, vd.getting_in_flags);
        let four_doors = self.ents.models.get(model as i32).and_then(|m| m.vehicle()).is_some_and(|v| v.num_doors == 4);
        let pos = self.epos(r);
        let far = Vec2::new(999.0, 999.0);
        let (mut rf_pos, mut lr_pos, mut rr_pos) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
        let (mut rf_dist, mut lr_dist, mut rr_dist) = (far, far, far);
        let mut can_enter = false;
        let flag = |n: usize| getting_in & car_door_flag(n) != 0;
        let d2 = |a: Vec3| Vec2::new(a.x - pos.x, a.y - pos.y);
        if passengers[0].is_none() && !flag(node::DOOR_RF) && self.room_to_leave_car(veh, node::DOOR_RF as u32, None) {
            rf_pos = self.get_position_to_open_car_door(veh, node::DOOR_RF as u32);
            can_enter = true;
            rf_dist = d2(rf_pos);
        }
        if four_doors {
            if passengers[1].is_none() && !flag(node::DOOR_LR) && self.room_to_leave_car(veh, node::DOOR_LR as u32, None) {
                lr_pos = self.get_position_to_open_car_door(veh, node::DOOR_LR as u32);
                can_enter = true;
                lr_dist = d2(lr_pos);
            }
            if passengers[2].is_none() && !flag(node::DOOR_RR) && self.room_to_leave_car(veh, node::DOOR_RR as u32, None) {
                rr_pos = self.get_position_to_open_car_door(veh, node::DOOR_RR as u32);
                can_enter = true;
                rr_dist = d2(rr_pos);
            }
            // the door is blocked by something: make room by shuffling the passengers
            if !can_enter {
                self.shuffle_passengers_to_make_space(veh);
            }
        }
        let (mut best, mut door, mut dest) = (rf_dist, node::DOOR_RF, rf_pos);
        if lr_dist.length_squared() < best.length_squared() {
            (best, door, dest) = (lr_dist, node::DOOR_LR, lr_pos);
        }
        if rr_dist.length_squared() < best.length_squared() {
            (door, dest) = (node::DOOR_RR, rr_pos);
        }
        self.ped_mut(r).veh_door = door as u16;
        *pos_to_open = dest;
        can_enter
    }

    /// `CVehicle::ShufflePassengersToMakeSpace`: a passenger whose door has room moves to a free seat.
    pub fn shuffle_passengers_to_make_space(&mut self, veh: EntityRef) -> bool {
        let Some(v) = self.try_veh(veh) else { return false };
        if v.num_passengers >= v.num_max_passengers {
            return false;
        }
        let flag = |w: &World, n: usize| w.try_veh(veh).is_some_and(|v| v.getting_in_flags & car_door_flag(n) != 0);
        // (seat, its door, [(target seat, its door)] in the order they are tried)
        let moves = [
            (1, node::DOOR_LR, [(2, node::DOOR_RR), (0, node::DOOR_RF)]),
            (2, node::DOOR_RR, [(1, node::DOOR_LR), (0, node::DOOR_RF)]),
            (0, node::DOOR_RF, [(1, node::DOOR_LR), (2, node::DOOR_RR)]),
        ];
        for (seat, door, targets) in moves {
            let occupant = self.try_veh(veh).and_then(|v| v.passengers[seat]);
            if occupant.is_none() || flag(self, door) || !self.room_to_leave_car(veh, door as u32, None) {
                continue;
            }
            for (target, target_door) in targets {
                let free = self.try_veh(veh).is_some_and(|v| v.passengers[target].is_none()) && !flag(self, target_door);
                if free {
                    let ped = occupant.expect("checked");
                    let v = self.try_veh_mut(veh).expect("vehicle");
                    v.passengers[target] = Some(ped);
                    v.passengers[seat] = None;
                    self.ped_mut(ped).veh_door = target_door as u16;
                    return true;
                }
            }
            return false;
        }
        false
    }

    /// `CPed::GoToNearestDoor`.
    pub fn go_to_nearest_door(&mut self, px: &mut super::ctx::Px, r: EntityRef, veh: EntityRef) {
        let mut pos_to_open = Vec3::ZERO;
        self.get_nearest_door(r, veh, &mut pos_to_open);
        self.set_seek_pos(px, r, pos_to_open, 0.5);
        self.ped_mut(r).set_move_state(move_state::RUN);
    }

    /// `CPed::IsRoomToBeCarJacked`.
    pub fn is_room_to_be_car_jacked(&mut self, r: EntityRef) -> bool {
        let Some(veh) = self.ped(r).my_vehicle else { return false };
        let low = self.try_veh(veh).is_some_and(|v| v.flags.low_vehicle);
        let mut offset = if low || self.ped(r).ped_type == super::types::ped_type::COP { self.ents.ped_anim_offsets.dragged_out } else { self.ents.ped_anim_offsets.quick_dragged_out };
        offset.z = 0.0;
        self.room_to_leave_car(veh, node::DOOR_LF as u32, Some(offset))
    }

    /// `CVehicle::SetDriver(ped)`.
    pub fn vehicle_set_driver(&mut self, veh: EntityRef, ped: EntityRef) {
        let (ped_mass, ped_pos) = (self.ent(ped).phys.as_ref().map_or(70.0, |p| p.mass), self.epos(ped));
        let Some(e) = self.ents.pools.get_mut(veh) else { return };
        let (m, crate::entity::Kind::Vehicle(v), Some(phys)) = (e.base.matrix, &mut e.kind, e.phys.as_mut()) else { return };
        v.set_driver(ped, phys, &m, ped_mass, ped_pos);
    }

    /// `CVehicle::AddPassenger(ped)` or `AddPassenger(ped, seat)`.
    pub fn vehicle_add_passenger(&mut self, veh: EntityRef, ped: EntityRef, seat: Option<u8>) -> bool {
        let (ped_mass, ped_pos) = (self.ent(ped).phys.as_ref().map_or(70.0, |p| p.mass), self.epos(ped));
        let Some(e) = self.ents.pools.get_mut(veh) else { return false };
        let (m, crate::entity::Kind::Vehicle(v), Some(phys)) = (e.base.matrix, &mut e.kind, e.phys.as_mut()) else { return false };
        match seat {
            Some(n) => v.add_passenger_at(ped, n, phys, &m, ped_mass, ped_pos),
            None => v.add_passenger(ped, phys, &m, ped_mass, ped_pos),
        }
    }
}
