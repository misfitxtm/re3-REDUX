//! `PedAI.cpp`: giving a ped an objective, clearing it and following a leader.
use super::ctx::Px;
use super::data::*;
use super::types::ped_type;
use crate::entity::{status, EntityRef};
use crate::vehicle::base::VehicleType;
use crate::world::World;
use glam::Vec3;

impl World {
    /// `CPed::SetObjective(objective)`.
    pub fn set_objective(&mut self, px: &mut Px, r: EntityRef, new: u8) {
        if self.ped(r).dying_or_dead() {
            return;
        }
        let pd = self.ped(r);
        if new == objective::NONE {
            if matches!(pd.objective, objective::LEAVE_CAR | objective::ENTER_CAR_AS_DRIVER | objective::ENTER_CAR_AS_PASSENGER) && !pd.is_ped_in_control() {
                self.ped_mut(r).flags.start_wander_path_on_foot = true;
                return;
            }
            let pd = self.ped_mut(r);
            pd.objective = objective::NONE;
            pd.prev_objective = objective::NONE;
        } else if pd.prev_objective != new || pd.prev_objective == objective::NONE {
            let now = self.now();
            let pd = self.ped_mut(r);
            pd.set_objective_timer(0, now);
            if pd.objective == new {
                return;
            }
            if PedData::is_temporary_objective(pd.objective) {
                pd.prev_objective = new;
            } else {
                pd.set_stored_objective();
                pd.objective = new;
            }
            pd.flags.objective_completed = false;
            if new == objective::HAIL_TAXI {
                pd.wait_timer = 0;
                self.set_idle(px, r);
                self.ped_mut(r).set_move_state(move_state::STILL);
            }
        }
    }

    /// The part `SetObjective(objective, entity)` runs before it looks at what the objective is; false to stop.
    fn begin_objective_entity(&mut self, r: EntityRef, new: u8, entity: Option<EntityRef>) -> bool {
        let pd = self.ped(r);
        if pd.dying_or_dead() || (pd.prev_objective == new && pd.prev_objective != objective::NONE) || entity == Some(r) {
            return false;
        }
        let now = self.now();
        let pd = self.ped_mut(r);
        pd.set_objective_timer(0, now);
        if pd.objective == new {
            match new {
                objective::KILL_CHAR_ON_FOOT
                | objective::KILL_CHAR_ANY_MEANS
                | objective::GOTO_CHAR_ON_FOOT
                | objective::FOLLOW_CHAR_IN_FORMATION
                | objective::GOTO_AREA_ANY_MEANS
                | objective::GUARD_ATTACK => {
                    if pd.ped_in_objective == entity {
                        return false;
                    }
                }
                objective::LEAVE_CAR | objective::FLEE_CAR => return false,
                objective::ENTER_CAR_AS_PASSENGER | objective::ENTER_CAR_AS_DRIVER | objective::DESTROY_CAR | objective::SOLICIT_VEHICLE | objective::BUY_ICE_CREAM => {
                    if pd.car_in_objective == entity {
                        return false;
                    }
                }
                objective::SET_LEADER if pd.leader == entity => return false,
                _ => {}
            }
        } else if new == objective::LEAVE_CAR && !pd.in_vehicle {
            return false;
        }
        pd.flags.objective_completed = false;
        if PedData::is_temporary_objective(pd.objective) && !PedData::is_temporary_objective(new) {
            pd.prev_objective = new;
        } else {
            if pd.objective != new {
                if PedData::is_temporary_objective(new) {
                    pd.force_stored_objective(new);
                } else {
                    pd.set_stored_objective();
                }
            }
            pd.objective = new;
        }
        true
    }

    /// `CPed::SetObjective(objective, int)` for `OBJECTIVE_WAIT_IN_CAR_THEN_GET_OUT`: wait `time` ms.
    pub fn set_objective_wait_in_car(&mut self, r: EntityRef, new: u8, time: i32) {
        if self.begin_objective_entity(r, new, None) && new == objective::WAIT_IN_CAR_THEN_GET_OUT {
            let now = self.now();
            self.ped_mut(r).set_objective_timer(time, now);
        }
    }

    /// `CPed::SetObjective(objective, entity)`.
    pub fn set_objective_entity(&mut self, px: &mut Px, r: EntityRef, new: u8, entity: EntityRef) {
        if !self.begin_objective_entity(r, new, Some(entity)) {
            return;
        }
        let now = self.now();
        match new {
            objective::KILL_CHAR_ON_FOOT | objective::KILL_CHAR_ANY_MEANS | objective::MUG_CHAR => {
                let pd = self.ped_mut(r);
                pd.next_path_node = None;
                pd.flags.use_ped_node_seek = false;
                pd.seek_pos = Vec3::ZERO;
                pd.ped_in_objective = Some(entity);
                pd.look_target = Some(entity);
            }
            objective::FLEE_CHAR_ON_FOOT_TILL_SAFE | objective::FLEE_CHAR_ON_FOOT_ALWAYS | objective::GOTO_CHAR_ON_FOOT | objective::GUARD_ATTACK => {
                let pd = self.ped_mut(r);
                pd.seek_pos = Vec3::ZERO;
                pd.ped_in_objective = Some(entity);
            }
            objective::FOLLOW_CHAR_IN_FORMATION => {
                let pd = self.ped_mut(r);
                pd.ped_in_objective = Some(entity);
                pd.ped_formation = formation::REAR;
            }
            objective::LEAVE_CAR | objective::FLEE_CAR => {
                self.ped_mut(r).car_in_objective = Some(entity);
                let leave_timer = self.ped(r).leave_car_timer;
                if let Some(v) = self.try_veh(entity).filter(|v| v.flags.is_bus && leave_timer == 0)
                    && let Some(i) = (0..v.num_max_passengers as usize).find(|&i| v.passengers[i] == Some(r))
                {
                    self.ped_mut(r).leave_car_timer = now.wrapping_add(1200 * i as u32);
                }
            }
            objective::ENTER_CAR_AS_PASSENGER | objective::ENTER_CAR_AS_DRIVER => {
                if self.ped(r).move_state == move_state::STILL {
                    self.ped_mut(r).set_move_state(move_state::RUN);
                }
                if self.try_veh(entity).is_some_and(|v| v.veh_type == VehicleType::Boat) && !self.ped(r).is_player() {
                    self.restore_previous_objective(px, r);
                    return;
                }
                self.objective_car_target(px, r, new, entity, now);
            }
            objective::DESTROY_CAR | objective::SOLICIT_VEHICLE | objective::BUY_ICE_CREAM => self.objective_car_target(px, r, new, entity, now),
            objective::SET_LEADER => {
                self.set_leader(r, Some(entity));
                self.restore_previous_objective(px, r);
            }
            _ => {}
        }
    }

    /// The shared tail of the car objectives of `SetObjective(objective, entity)`.
    fn objective_car_target(&mut self, px: &mut Px, r: EntityRef, new: u8, car: EntityRef, now: u32) {
        let pd = self.ped_mut(r);
        pd.car_in_objective = Some(car);
        pd.seek_target = Some(car);
        pd.seek_pos = Vec3::ZERO;
        let disabled = self.ent(car).base.status == status::PLAYER_DISABLED || px.hooks.player_controls_disabled();
        let pd = self.ped_mut(r);
        if new == objective::SOLICIT_VEHICLE {
            pd.objective_timer = now.wrapping_add(10000);
        } else if pd.objective == objective::ENTER_CAR_AS_PASSENGER && pd.char_created_by == MISSION_CHAR && disabled {
            pd.set_objective_timer(14000, now);
        } else {
            pd.objective_timer = 0;
        }
    }

    /// `CPed::SetObjective(objective, dest, safeDist)`.
    pub fn set_objective_area(&mut self, r: EntityRef, new: u8, dest: Vec3, safe_dist: f32) {
        let pd = self.ped(r);
        if pd.dying_or_dead() || (pd.prev_objective != objective::NONE && pd.prev_objective == new) {
            return;
        }
        let now = self.now();
        let pd = self.ped_mut(r);
        pd.set_objective_timer(0, now);
        if pd.objective == new {
            let same_area = matches!(new, objective::GOTO_AREA_ANY_MEANS | objective::GOTO_AREA_ON_FOOT | objective::RUN_TO_AREA) && pd.next_route_point_pos == dest && pd.distance_to_count_seek_done == safe_dist;
            let same_spot = new == objective::GUARD_SPOT && pd.seek_pos_ex == dest && pd.distance_to_count_seek_done_ex == safe_dist;
            if same_area || same_spot {
                return;
            }
        }
        pd.flags.objective_completed = false;
        if PedData::is_temporary_objective(pd.objective) {
            pd.prev_objective = new;
        } else {
            if pd.objective != new {
                pd.set_stored_objective();
            }
            pd.objective = new;
        }
        if new == objective::GUARD_SPOT {
            pd.seek_pos_ex = dest;
            pd.distance_to_count_seek_done_ex = safe_dist;
        } else if matches!(new, objective::GOTO_AREA_ANY_MEANS | objective::GOTO_AREA_ON_FOOT | objective::RUN_TO_AREA) {
            pd.next_path_node = None;
            pd.next_route_point_pos = dest;
            pd.seek_pos = dest;
            pd.flags.use_ped_node_seek = true;
        }
    }

    /// `CPed::SetObjective(objective, routePoint, routeType)`: only the follow-route opcode uses it.
    pub fn set_objective_route(&mut self, r: EntityRef, new: u8, route_point: i16, route_type: i16) {
        let pd = self.ped(r);
        if pd.dying_or_dead() || (pd.prev_objective == new && pd.prev_objective != objective::NONE) {
            return;
        }
        let now = self.now();
        let pd = self.ped_mut(r);
        pd.set_objective_timer(0, now);
        if pd.objective == new && new == objective::FOLLOW_ROUTE && pd.route_last_point == route_point && pd.route_type == route_type {
            return;
        }
        pd.flags.objective_completed = false;
        if PedData::is_temporary_objective(pd.objective) {
            pd.prev_objective = new;
        } else {
            if pd.objective != new {
                pd.set_stored_objective();
            }
            pd.objective = new;
        }
        if new == objective::FOLLOW_ROUTE {
            self.set_follow_route(r, route_point, route_type);
        }
    }

    /// `CPed::SetObjective(objective, dest)`.
    pub fn set_objective_pos(&mut self, r: EntityRef, new: u8, dest: Vec3) {
        let pd = self.ped(r);
        if pd.dying_or_dead() || (pd.prev_objective != objective::NONE && pd.prev_objective == new) {
            return;
        }
        let now = self.now();
        let pos = self.epos(r);
        let pd = self.ped_mut(r);
        pd.set_objective_timer(0, now);
        if pd.objective == new {
            let same_area = matches!(new, objective::GOTO_AREA_ANY_MEANS | objective::GOTO_AREA_ON_FOOT | objective::RUN_TO_AREA) && pd.next_route_point_pos == dest;
            if same_area || (new == objective::GUARD_SPOT && pd.seek_pos_ex == dest) {
                return;
            }
        }
        pd.flags.objective_completed = false;
        match new {
            objective::GUARD_SPOT => {
                pd.seek_pos_ex = dest;
                pd.distance_to_count_seek_done_ex = 5.0;
                pd.set_move_state(move_state::STILL);
            }
            objective::GOTO_AREA_ANY_MEANS | objective::GOTO_AREA_ON_FOOT | objective::RUN_TO_AREA => {
                pd.flags.is_running = new == objective::RUN_TO_AREA;
                pd.next_path_node = None;
                pd.next_route_point_pos = dest;
                pd.seek_pos = dest;
                pd.distance_to_count_seek_done = 0.5;
                pd.flags.use_ped_node_seek = true;
                let (dx, dy) = (dest.x - pos.x, dest.y - pos.y);
                // already there: the objective is not taken at all
                if pd.distance_to_count_seek_done * pd.distance_to_count_seek_done > dx * dx + dy * dy {
                    return;
                }
            }
            _ => {}
        }
        if PedData::is_temporary_objective(pd.objective) {
            pd.prev_objective = new;
        } else {
            if pd.objective != new {
                pd.set_stored_objective();
            }
            pd.objective = new;
        }
    }

    /// `CPed::ClearObjective`.
    pub fn clear_objective(&mut self, px: &mut Px, r: EntityRef) {
        let pd = self.ped(r);
        if !(pd.is_ped_in_control() || pd.ped_state == state::DRIVING) {
            self.ped_mut(r).flags.clear_objective = true;
            return;
        }
        let (driving, vehicle) = (pd.ped_state == state::DRIVING, pd.my_vehicle);
        self.ped_mut(r).objective = objective::NONE;
        if let (true, Some(veh)) = (driving, vehicle) {
            if self.try_veh(veh).and_then(|v| v.driver) != Some(r) {
                if !self.ped(r).is_player() {
                    self.ped_mut(r).flags.wander_path_after_exiting_car = true;
                }
                self.set_objective_entity(px, r, objective::LEAVE_CAR, veh);
            }
        } else {
            self.set_idle(px, r);
            self.ped_mut(r).set_move_state(move_state::STILL);
        }
    }

    /// `CPed::ClearLeader`.
    pub fn clear_leader(&mut self, px: &mut Px, r: EntityRef) {
        if self.ped(r).leader.is_none() {
            return;
        }
        self.ped_mut(r).leader = None;
        if self.ped(r).is_ped_in_control() {
            self.set_objective(px, r, objective::NONE);
            if self.ped(r).char_created_by == MISSION_CHAR {
                self.set_idle(px, r);
            } else {
                let dir = px.rng.range_i32(0, 8) as i8;
                self.set_wander_path(px, r, dir);
            }
        } else if self.ped(r).objective != objective::NONE {
            self.ped_mut(r).flags.clear_objective = true;
        }
    }

    /// `CPed::RestorePreviousObjective`.
    pub fn restore_previous_objective(&mut self, px: &mut Px, r: EntityRef) {
        let pd = self.ped(r);
        if pd.objective == objective::NONE {
            return;
        }
        // FIX_BUGS: a ped being car jacked keeps who it is after
        if !matches!(pd.objective, objective::LEAVE_CAR | objective::ENTER_CAR_AS_PASSENGER | objective::ENTER_CAR_AS_DRIVER) && pd.ped_state != state::CARJACK {
            self.ped_mut(r).ped_in_objective = None;
        }
        if self.ped(r).objective == objective::WAIT_IN_CAR_THEN_GET_OUT {
            self.ped_mut(r).objective = objective::NONE;
            if let Some(veh) = self.ped(r).my_vehicle {
                self.set_objective_entity(px, r, objective::LEAVE_CAR, veh);
            }
        } else {
            let pd = self.ped_mut(r);
            pd.objective = pd.prev_objective;
            pd.prev_objective = objective::NONE;
        }
        self.ped_mut(r).flags.objective_completed = false;
    }

    /// `CPed::UpdateFromLeader`.
    pub fn update_from_leader(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        if now <= self.ped(r).objective_timer {
            return;
        }
        let Some(leader) = self.ped(r).leader else { return };
        let (leader_vehicle, leader_in_vehicle) = (self.ped(leader).my_vehicle, self.ped(leader).in_vehicle_now());
        let leader_pos = if leader_in_vehicle { self.epos(leader_vehicle.expect("in vehicle")) } else { self.epos(leader) };
        if (leader_pos - self.epos(r)).length() > 30.0 {
            if self.ped(r).is_ped_in_control() {
                self.set_objective(px, r, objective::NONE);
                self.set_idle(px, r);
                self.ped_mut(r).set_move_state(move_state::STILL);
            }
            self.set_leader(r, None);
            return;
        }
        let in_control = self.ped(r).is_ped_in_control();
        if in_control {
            self.update_from_leader_on_foot(px, r, leader);
        } else if self.ped(r).in_vehicle {
            self.update_from_leader_in_car(px, r, leader);
        }
    }

    /// `UpdateFromLeader` for a ped that can act.
    fn update_from_leader_on_foot(&mut self, px: &mut Px, r: EntityRef, leader: EntityRef) {
        if self.ped(r).wait_state == wait_state::PLAYANIM_TAXI {
            self.warp_ped_to_near_leader_off_screen(px, r);
        }
        if self.ped(leader).ped_state == state::DEAD {
            self.set_leader(r, None);
            self.set_objective(px, r, objective::FLEE_ON_FOOT_TILL_SAFE);
            return;
        }
        let (l_in_vehicle, l_objective) = (self.ped(leader).in_vehicle, self.ped(leader).objective);
        if !l_in_vehicle {
            if l_objective != objective::ENTER_CAR_AS_DRIVER {
                if self.ped(r).in_vehicle {
                    let pd = self.ped(r);
                    if pd.objective != objective::WAIT_IN_CAR_THEN_GET_OUT && pd.objective != objective::LEAVE_CAR
                        && let Some(veh) = pd.my_vehicle
                    {
                        self.set_objective_entity(px, r, objective::LEAVE_CAR, veh);
                    }
                    return;
                }
                if self.ped(r).objective == objective::ENTER_CAR_AS_PASSENGER {
                    self.restore_previous_objective(px, r);
                    self.restore_previous_state(px, r);
                }
            }
            let pd = self.ped(r);
            if pd.ped_type == ped_type::PROSTITUTE && pd.char_created_by == RANDOM_CHAR {
                self.set_leader(r, None);
                return;
            }
        }
        let (l_state, l_vehicle) = (self.ped(leader).ped_state, self.ped(leader).my_vehicle);
        if !self.ped(r).in_vehicle && self.ped(leader).in_vehicle && l_state == state::DRIVING {
            let pd = self.ped(r);
            if pd.objective != objective::ENTER_CAR_AS_PASSENGER && pd.objective != objective::ENTER_CAR_AS_DRIVER
                && let Some(veh) = l_vehicle
                && self.try_veh(veh).is_some_and(|v| v.num_passengers < v.num_max_passengers)
            {
                self.set_objective_entity(px, r, objective::ENTER_CAR_AS_PASSENGER, veh);
            }
        } else if l_objective == objective::NONE
            || (self.ped(leader).is_player() && l_objective == objective::WAIT_ON_FOOT)
            || self.ped(r).objective == l_objective
        {
            if l_state == state::ATTACK {
                let target = self.ped(leader).look_target;
                if let Some(t) = target.filter(|&t| self.ped(r).objective != objective::KILL_CHAR_ON_FOOT && self.is_ped(t) && t != r) {
                    self.set_objective_entity(px, r, objective::KILL_CHAR_ON_FOOT, t);
                    let now = self.now();
                    self.ped_mut(r).set_objective_timer(8000, now);
                    self.set_look_flag_entity(r, t, false);
                    self.set_look_timer(r, 500);
                }
            } else {
                if self.ped(r).is_ped_in_control() && self.ped(r).ped_state != state::ATTACK {
                    self.set_objective_entity(px, r, objective::GOTO_CHAR_ON_FOOT, leader);
                    let now = self.now();
                    self.ped_mut(r).set_objective_timer(0, now);
                }
                if self.ped(r).ped_state == state::IDLE && self.ped(leader).is_player() && self.scan_for_threats(px, r) != 0
                    && let Some(threat) = self.ped(r).threat_entity
                {
                    self.ped_mut(r).look_target = Some(threat);
                    self.turn_body(r);
                    let pd = self.ped(r);
                    if pd.attack_timer < self.now() && !pd.weapons[pd.current_weapon as usize].is_type_melee() {
                        self.ped_mut(r).point_gun_at = Some(threat);
                        self.set_attack(px, r, threat);
                    }
                }
            }
        } else {
            self.copy_leader_objective(px, r, leader, l_objective);
        }
    }

    /// The `switch (m_leader->m_objective)` of `UpdateFromLeader`: take on what the leader is doing.
    fn copy_leader_objective(&mut self, px: &mut Px, r: EntityRef, leader: EntityRef, l_objective: u8) {
        let l_timer = self.ped(leader).objective_timer;
        match l_objective {
            objective::WAIT_ON_FOOT | objective::FLEE_ON_FOOT_TILL_SAFE | objective::WAIT_IN_CAR | objective::FOLLOW_ROUTE => {
                self.set_objective(px, r, l_objective);
                self.ped_mut(r).objective_timer = l_timer;
            }
            objective::GUARD_SPOT => {
                let spot = self.ped(leader).seek_pos_ex;
                self.set_objective_pos(r, objective::GUARD_SPOT, spot);
                self.ped_mut(r).objective_timer = l_timer;
            }
            objective::KILL_CHAR_ON_FOOT | objective::KILL_CHAR_ANY_MEANS | objective::GOTO_CHAR_ON_FOOT => {
                if let Some(target) = self.ped(leader).ped_in_objective {
                    self.set_objective_entity(px, r, l_objective, target);
                    self.ped_mut(r).objective_timer = l_timer;
                }
            }
            objective::ENTER_CAR_AS_PASSENGER | objective::ENTER_CAR_AS_DRIVER => {
                if let Some(car) = self.ped(leader).car_in_objective {
                    self.set_objective_entity(px, r, objective::ENTER_CAR_AS_PASSENGER, car);
                }
            }
            objective::GUARD_ATTACK => {}
            objective::HAIL_TAXI => {
                self.ped_mut(r).leader = None;
                self.set_objective(px, r, objective::NONE);
            }
            _ => {
                self.set_objective_entity(px, r, objective::GOTO_CHAR_ON_FOOT, leader);
                let now = self.now();
                self.ped_mut(r).set_objective_timer(0, now);
            }
        }
    }

    /// `UpdateFromLeader` for a ped sitting in a car whose leader got out.
    fn update_from_leader_in_car(&mut self, px: &mut Px, r: EntityRef, leader: EntityRef) {
        let l = self.ped(leader);
        if !((!l.in_vehicle || l.ped_state == state::EXIT_CAR) && self.ped(r).objective != objective::WAIT_IN_CAR_THEN_GET_OUT) {
            return;
        }
        let (l_objective, l_my, l_car) = (l.objective, l.my_vehicle, l.car_in_objective);
        let mine = self.ped(r).my_vehicle;
        if matches!(l_objective, objective::ENTER_CAR_AS_PASSENGER | objective::ENTER_CAR_AS_DRIVER) && (mine == l_my || mine == l_car) {
            return;
        }
        if let Some(veh) = mine.filter(|_| self.ped(r).objective != objective::LEAVE_CAR) {
            self.set_objective_entity(px, r, objective::LEAVE_CAR, veh);
        }
    }
}
