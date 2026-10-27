//! `CarAI.cpp`: what a car does about its mission each frame.
use super::car_ctrl::{GAME_SPEED_TO_CARAI_SPEED, TIME_COPS_WAIT_TO_EXIT_AFTER_STOPPING};
use crate::entity::{status, EntityRef};
use crate::modelinfo::indices_gta3::mi;
use crate::ped::ctx::Px;
use crate::ped::data::{objective, state};
use crate::vehicle::base::{car_mission as m, driving_style, temp_action, VehicleData, RANDOM_VEHICLE, MISSION_VEHICLE};
use crate::vehicle::env::{CarEvent, CarSound};
use crate::world::{World, WorldEvent};
use glam::{Vec2, Vec3};

const DISTANCE_TO_SWITCH_DISTANCE_GOTO: f32 = 20.0;

/// `FindSwitchDistanceClose`.
pub fn find_switch_distance_close() -> f32 {
    30.0
}

/// `FindSwitchDistanceFarNormalVehicle`.
pub fn find_switch_distance_far_normal_vehicle() -> f32 {
    find_switch_distance_close() + 5.0
}

/// `FindSwitchDistanceFar`.
pub fn find_switch_distance_far(law_enforcer: bool) -> f32 {
    if law_enforcer { 50.0 } else { find_switch_distance_far_normal_vehicle() }
}

fn flat(a: Vec3, b: Vec3) -> f32 {
    Vec2::new(a.x - b.x, a.y - b.y).length()
}

impl World {
    /// `CCarAI::CarHasReasonToStop`: a car with a reason to wait is not stuck, so it won't try to back out.
    pub fn car_has_reason_to_stop(&mut self, r: EntityRef) {
        let now = self.now();
        if let Some(v) = self.try_veh_mut(r) {
            v.auto_pilot.anti_reverse_timer = now;
        }
    }

    /// `CVehicle::SetUpDriver`: a random driver sits in a random car.
    pub fn set_up_driver(&mut self, px: &mut Px, r: EntityRef) -> Option<EntityRef> {
        let v = self.try_veh(r)?;
        if v.driver.is_some() {
            return v.driver;
        }
        if v.created_by != RANDOM_VEHICLE {
            return None;
        }
        let bus = v.flags.is_bus;
        let ped = self.add_ped_in_car(px, r)?;
        self.try_veh_mut(r)?.driver = Some(ped);
        self.seat_ped(ped, r, bus);
        Some(ped)
    }

    /// `CVehicle::SetupPassenger`.
    pub fn setup_passenger(&mut self, px: &mut Px, r: EntityRef, n: usize) -> Option<EntityRef> {
        let v = self.try_veh(r)?;
        if let Some(p) = v.passengers[n] {
            return Some(p);
        }
        let bus = v.flags.is_bus;
        let ped = self.add_ped_in_car(px, r)?;
        let v = self.try_veh_mut(r)?;
        v.passengers[n] = Some(ped);
        v.num_passengers += 1;
        self.seat_ped(ped, r, bus);
        Some(ped)
    }

    fn seat_ped(&mut self, ped: EntityRef, car: EntityRef, bus: bool) {
        let pd = self.ped_mut(ped);
        pd.my_vehicle = Some(car);
        pd.in_vehicle = true;
        pd.ped_state = state::DRIVING;
        if bus {
            pd.flags.render_ped_in_car = false;
        }
    }

    /// `CCarAI::AddPoliceCarOccupants`.
    pub fn add_police_car_occupants(&mut self, px: &mut Px, r: EntityRef) {
        let Some(v) = self.try_veh_mut(r) else { return };
        if v.flags.occupants_have_been_generated {
            return;
        }
        v.flags.occupants_have_been_generated = true;
        let level = self.player_wanted().map_or(0, |w| w.level);
        match self.ent(r).base.model_index {
            mi::FBICAR | mi::ENFORCER => {
                self.set_up_driver(px, r);
                for i in 0..3 {
                    self.setup_passenger(px, r, i);
                }
            }
            mi::POLICE | mi::RHINO | mi::BARRACKS => {
                self.set_up_driver(px, r);
                if level > 1 {
                    self.setup_passenger(px, r, 0);
                }
            }
            _ => {}
        }
    }

    /// `CCarAI::AddAmbulanceOccupants`.
    pub fn add_ambulance_occupants(&mut self, px: &mut Px, r: EntityRef) {
        self.set_up_driver(px, r);
        self.setup_passenger(px, r, 1);
    }

    /// `CCarAI::AddFiretruckOccupants`.
    pub fn add_firetruck_occupants(&mut self, px: &mut Px, r: EntityRef) {
        self.set_up_driver(px, r);
        self.setup_passenger(px, r, 0);
    }

    /// `CCarAI::TellOccupantsToLeaveCar`.
    pub fn tell_occupants_to_leave_car(&mut self, px: &mut Px, r: EntityRef) {
        let Some(v) = self.try_veh(r) else { return };
        let (driver, passengers, n) = (v.driver, v.passengers, v.num_max_passengers as usize);
        let model = self.ent(r).base.model_index;
        if let Some(d) = driver {
            self.set_objective_entity(px, d, objective::LEAVE_CAR, r);
            if model == mi::AMBULAN {
                self.say(px, d, crate::audio::sound::PED_LEAVE_VEHICLE);
            }
        }
        for p in passengers[..n].iter().flatten() {
            self.set_objective_entity(px, *p, objective::LEAVE_CAR, r);
        }
    }

    /// `CCarAI::TellCarToRamOtherCar`.
    pub fn tell_car_to_ram_other_car(&mut self, r: EntityRef, target: EntityRef) {
        self.tell_car_after_other(r, target, m::RAMCAR_FARAWAY);
    }

    /// `CCarAI::TellCarToBlockOtherCar`.
    pub fn tell_car_to_block_other_car(&mut self, r: EntityRef, target: EntityRef) {
        self.tell_car_after_other(r, target, m::BLOCKCAR_FARAWAY);
    }

    fn tell_car_after_other(&mut self, r: EntityRef, target: EntityRef, mission: u8) {
        let Some(v) = self.try_veh_mut(r) else { return };
        v.auto_pilot.target_car = Some(target);
        v.auto_pilot.car_mission = mission;
        v.flags.engine_on = true;
        v.auto_pilot.cruise_speed = v.auto_pilot.cruise_speed.max(6);
    }

    /// `CCarAI::GetCarToGoToCoors`: sends the car to `target`; how far it still has to go.
    pub fn get_car_to_go_to_coors(&mut self, px: &mut Px, r: EntityRef, target: Vec3) -> f32 {
        let mission = self.try_veh(r).map_or(0, |v| v.auto_pilot.car_mission);
        let now = self.now();
        if mission != m::GOTOCOORDS && mission != m::GOTOCOORDS_STRAIGHT {
            {
                let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
                ap.driving_style = driving_style::AVOID_CARS;
                ap.temp_action = temp_action::NONE;
                ap.cruise_speed = 20;
                ap.anti_reverse_timer = now;
            }
            self.get_mut(r).expect("vehicle").base.status = status::PHYSICS;
            let none_found = self.join_car_with_road_system_goto_coors(px, r, target, false);
            self.try_veh_mut(r).expect("vehicle").auto_pilot.car_mission = if none_found { m::GOTOCOORDS_STRAIGHT } else { m::GOTOCOORDS };
        } else {
            let dest = self.try_veh(r).expect("vehicle").auto_pilot.destination_coors;
            if (target.x - dest.x).abs() > 2.0 || (target.y - dest.y).abs() > 2.0 {
                self.try_veh_mut(r).expect("vehicle").auto_pilot.destination_coors = target;
            }
        }
        flat(self.epos(r), target)
    }

    /// `CCarAI::FindPoliceCarMissionForWantedLevel`.
    pub fn find_police_car_mission_for_wanted_level(&self, px: &mut Px) -> u8 {
        let level = self.player_wanted().map_or(0, |w| w.level);
        let pick = |px: &mut Px, from: u16| if px.rng.number() & 3 >= from { m::RAMPLAYER_FARAWAY } else { m::BLOCKPLAYER_FARAWAY };
        match level {
            0 | 1 => m::BLOCKPLAYER_FARAWAY,
            2 => pick(px, 3),
            3 => pick(px, 2),
            4..=6 => pick(px, 1),
            _ => m::BLOCKPLAYER_FARAWAY,
        }
    }

    /// `CCarAI::FindPoliceCarSpeedForWantedLevel`.
    pub fn find_police_car_speed_for_wanted_level(&self, px: &mut Px, r: EntityRef) -> i32 {
        let level = self.player_wanted().map_or(0, |w| w.level);
        let max_velocity = self.try_veh(r).map_or(0.0, |v| self.ents.handling.get(v.handling).transmission.max_velocity);
        let scaled = |k: f32| (GAME_SPEED_TO_CARAI_SPEED * max_velocity * k) as i32;
        match level {
            0 => px.rng.range_i32(12, 16),
            1 => 25,
            2 => 34,
            3 => scaled(0.9),
            4 => scaled(1.2),
            5 => scaled(1.25),
            6 => scaled(1.3),
            _ => 0,
        }
    }

    /// `CCarAI::MellowOutChaseSpeed`: cops ease off when the player is far or on foot at a low wanted level.
    pub fn mellow_out_chase_speed(&mut self, r: EntityRef) {
        let level = self.player_wanted().map_or(0, |w| w.level);
        if level != 1 && level != 2 {
            return;
        }
        let dist = (self.epos(r) - self.player_info_pos()).length();
        let in_car = self.find_player_vehicle().is_some();
        let speed = match (level, in_car) {
            (1, true) => if dist < 10.0 { 15 } else if dist < 20.0 { 22 } else { 25 },
            (1, false) => if dist < 20.0 { 5 } else if dist < 40.0 { 13 } else { 25 },
            (_, true) => if dist < 10.0 { 27 } else if dist < 20.0 { 30 } else { 34 },
            (_, false) => if dist < 20.0 { 5 } else if dist < 40.0 { 18 } else { 34 },
        };
        self.try_veh_mut(r).expect("vehicle").auto_pilot.cruise_speed = speed;
    }

    /// `CCarAI::MakeWayForCarWithSiren`: other traffic ahead of an emergency car swerves or waits.
    pub fn make_way_for_car_with_siren(&mut self, r: EntityRef) {
        let speed = self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
        let flat_speed = Vec2::new(speed.x, speed.y).length();
        if flat_speed < 0.1 {
            return;
        }
        let forward = Vec2::new(speed.x, speed.y) / flat_speed;
        let projection = flat_speed * 45.0 + 20.0;
        let me = self.epos(r);
        let now = self.now();
        let pool = &self.ents.pools.vehicles;
        let refs: Vec<EntityRef> = pool.iter().map(|(i, _)| EntityRef { pool: crate::entity::PoolKind::Vehicle, handle: pool.handle_of(i) }).collect();
        for v in refs.into_iter().rev() {
            let Some(e) = self.get(v) else { continue };
            let Some(d) = self.try_veh(v) else { continue };
            if v == r || (self.car_ref(v).is_none() && !d.is_bike()) || !matches!(e.base.status, status::SIMPLE | status::PHYSICS) || d.created_by != RANDOM_VEHICLE {
                continue;
            }
            if d.flags.is_law_enforcer || d.flags.is_ambulance_on_duty || d.flags.is_fire_truck_on_duty || (me.z - e.base.position().z).abs() >= 5.0 {
                continue;
            }
            let p = e.base.position();
            let distance = Vec2::new(p.x - me.x, p.y - me.y);
            let other_speed = e.phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
            if distance.length() >= projection || Vec2::new(other_speed.x, other_speed.y).length() <= 0.05 {
                continue;
            }
            let correlation = forward.dot(distance) / distance.length();
            if correlation <= 0.0 {
                continue;
            }
            let other_fwd = Vec2::new(e.base.matrix.forward.x, e.base.matrix.forward.y);
            if correlation > 0.8 && forward.dot(other_fwd) > 0.7 {
                let ap = &mut self.try_veh_mut(v).expect("vehicle").auto_pilot;
                if ap.temp_action != temp_action::SWERVELEFT && ap.temp_action != temp_action::SWERVERIGHT {
                    ap.temp_action = if distance.x * forward.y - distance.y * forward.x > 0.0 { temp_action::SWERVELEFT } else { temp_action::SWERVERIGHT };
                    ap.time_temp_action = now + 2000;
                }
                self.get_mut(v).expect("vehicle").base.status = status::PHYSICS;
            } else if Vec2::new(other_speed.x, other_speed.y).dot(distance) < 0.0 {
                let ap = &mut self.try_veh_mut(v).expect("vehicle").auto_pilot;
                if ap.temp_action != temp_action::WAIT {
                    ap.temp_action = temp_action::WAIT;
                    ap.time_temp_action = now + 2000;
                }
            }
        }
    }

    /// `CVehicle::PlayCarHorn`.
    pub fn play_car_horn(&mut self, px: &mut Px, r: EntityRef) {
        let Some(v) = self.try_veh_mut(r) else { return };
        if v.car_horn_timer != 0 {
            return;
        }
        let roll = px.rng.number() & 7;
        let has_driver = v.driver.is_some();
        if roll < 2 {
            v.car_horn_timer = 45;
        } else if roll < 4 {
            v.car_horn_timer = 45;
            if has_driver {
                self.ents.events.push(WorldEvent::Car(CarEvent::Sound { sound: CarSound::AnnoyedDriver, volume: 0.0 }));
            }
        } else if has_driver {
            self.ents.events.push(WorldEvent::Car(CarEvent::Sound { sound: CarSound::AnnoyedDriver, volume: 0.0 }));
        }
    }

    /// The tail all the police missions share: when nobody is after the player, the car goes back to normal traffic.
    fn police_stand_down(&mut self, px: &mut Px, r: EntityRef) -> bool {
        let w = self.player_wanted().map(|w| (w.ignored_by_everyone, w.level, w.ignored_by_cops)).unwrap_or_default();
        let law = self.try_veh(r).is_some_and(|v| v.flags.is_law_enforcer);
        let no_police = px.hooks.no_police_zone();
        if w.0 || (law && (w.1 == 0 || w.2 || no_police)) {
            self.join_car_with_road_system(px, r);
            let v = self.try_veh_mut(r).expect("vehicle");
            v.auto_pilot.car_mission = if no_police { m::NONE } else { m::CRUISE };
            v.auto_pilot.driving_style = driving_style::STOP_FOR_CARS;
            v.siren_or_alarm = false;
            return true;
        }
        false
    }

    /// Whether a police car chasing on foot has been stuck at the player long enough to get out.
    fn cops_get_out_when_stuck_near_player(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let step = self.ents.timer.time_step_in_milliseconds() as u16;
        let pv = self.find_player_vehicle();
        let player_slow = pv.is_some_and(|p| self.ent(p).phys.as_ref().is_some_and(|ph| ph.move_speed.length() < 0.05));
        let v = self.try_veh_mut(r).expect("vehicle");
        v.time_blocked = if player_slow { v.time_blocked.saturating_add(step) } else { 0 };
        let blocked = v.time_blocked as u32;
        let law = v.flags.is_law_enforcer;
        let seed = self.ent(r).base.random_seed;
        let upside_down = pv.is_some_and(|p| VehicleData::is_upside_down(&self.ent(p).base.matrix));
        let near = flat(self.epos(r), self.player_info_pos()) < 10.0;
        let rhino = self.ent(r).base.model_index == mi::RHINO;
        if (pv.is_none() || upside_down || (player_slow && blocked > TIME_COPS_WAIT_TO_EXIT_AFTER_STOPPING)) && law && (!rhino || seed > 10000) && near {
            self.tell_occupants_to_leave_car(px, r);
            let v = self.try_veh_mut(r).expect("vehicle");
            v.auto_pilot.cruise_speed = 0;
            v.auto_pilot.car_mission = m::NONE;
            if self.player_wanted().is_some_and(|w| w.level <= 1) {
                self.try_veh_mut(r).expect("vehicle").siren_or_alarm = false;
            }
        }
        let _ = now;
    }

    /// `CCarAI::UpdateCarAI`: the mission's decisions and the unstuck logic of one car.
    pub fn update_car_ai(&mut self, px: &mut Px, r: EntityRef) {
        let Some(v) = self.try_veh(r) else { return };
        let law = v.flags.is_law_enforcer;
        let mission = v.auto_pilot.car_mission;
        let now = self.now();
        if law && matches!(mission, m::BLOCKCAR_FARAWAY | m::RAMPLAYER_FARAWAY | m::BLOCKPLAYER_CLOSE | m::RAMPLAYER_CLOSE) {
            let s = self.find_police_car_speed_for_wanted_level(px, r);
            self.try_veh_mut(r).expect("vehicle").auto_pilot.cruise_speed = s as u8;
        }
        let st = self.ent(r).base.status;
        match st {
            status::SIMPLE | status::PHYSICS => self.update_mission(px, r),
            status::ABANDONED | status::WRECKED => {
                let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
                ap.car_mission = m::NONE;
                ap.cruise_speed = 0;
            }
            _ => {}
        }
        let speed = self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
        let flat_speed = speed.x * speed.x + speed.y * speed.y;
        if flat_speed > 0.018 * 0.018 {
            let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
            ap.time_to_start_mission = now;
            ap.anti_reverse_timer = now;
        }
        self.unstick(px, r, flat_speed);
        self.panic_when_idle(px, r);
    }

    /// The part of `UpdateCarAI` after the stuck check: long idleness, flipped cars, chasing cars waiting for a fast player.
    fn panic_when_idle(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let seed = self.ent(r).base.random_seed;
        let ap = self.try_veh(r).expect("vehicle").auto_pilot;
        if seed & 7 == 0
            && now.wrapping_sub(ap.time_to_start_mission) > 30000
            && self.ents.timer.previous_time_in_ms.wrapping_sub(ap.time_to_start_mission) <= 30000
            && ap.car_mission == m::CRUISE
            && !self.should_car_stop_for_bridge(r)
        {
            self.get_mut(r).expect("vehicle").base.status = status::PHYSICS;
            self.switch_vehicle_to_real_physics(r);
            let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
            ap.driving_style = driving_style::AVOID_CARS;
            ap.temp_action = temp_action::REVERSE;
            ap.time_temp_action = now + 400;
        }
        if self.ent(r).base.matrix.up.z < -0.7 {
            let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
            ap.temp_action = temp_action::WAIT;
            ap.time_temp_action = now + 1000;
        }
        let ap = self.try_veh(r).expect("vehicle").auto_pilot;
        if ap.temp_action == temp_action::NONE && matches!(ap.car_mission, m::RAMPLAYER_FARAWAY | m::RAMPLAYER_CLOSE | m::BLOCKPLAYER_FARAWAY | m::BLOCKPLAYER_CLOSE) {
            #[allow(clippy::collapsible_if)] // the three nested tests mirror the original
            if let Some(pv) = self.find_player_vehicle() {
                let (ps, my_speed) = (self.find_player_speed(), self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed));
                if ps.length() > my_speed.length() && ps.length() > 0.1 {
                    let (pf, mf) = (self.ent(pv).base.matrix.forward, self.ent(r).base.matrix.forward);
                    if Vec2::new(pf.x, pf.y).dot(Vec2::new(mf.x, mf.y)) > 0.0 {
                        let pc = self.player_info_pos();
                        let dist = Vec2::new(self.epos(r).x - pc.x, self.epos(r).y - pc.y);
                        let sp = Vec2::new(ps.x, ps.y);
                        if 0.5 * dist.length() * sp.length() < dist.dot(sp) && (pc - self.epos(r)).length() > 12.0 {
                            let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
                            ap.temp_action = temp_action::WAIT;
                            ap.time_temp_action = now + 500;
                        }
                    }
                }
            }
        }
        let driver = self.try_veh(r).and_then(|v| v.driver);
        if driver.is_some_and(|d| self.ped(d).objective == objective::KILL_CHAR_ANY_MEANS) && (self.epos(r) - self.player_info_pos()).length() < 15.0 {
            let pv = self.find_player_vehicle();
            let collided = pv.is_some_and(|p| self.ent(r).phys.as_ref().is_some_and(|ph| ph.has_collided_with(self.ent(r).base.flags.use_collision_records, p)));
            if pv.is_none() || collided {
                let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
                ap.temp_action = temp_action::WAIT;
                ap.time_temp_action = now + 3000;
            }
        }
        if self.try_veh(r).expect("vehicle").siren_or_alarm && (seed ^ px.rng.number()) as u8 == 0xAD {
            self.try_veh_mut(r).expect("vehicle").car_horn_timer = 45;
        }
    }

    /// The reverse-when-stuck logic of `UpdateCarAI`.
    fn unstick(&mut self, px: &mut Px, r: EntityRef, flat_speed: f32) {
        let now = self.now();
        let ap = self.try_veh(r).expect("vehicle").auto_pilot;
        let created_by = self.try_veh(r).expect("vehicle").created_by;
        if self.ent(r).base.status != status::PHYSICS || ap.temp_action != temp_action::NONE || ap.car_mission == m::NONE {
            return;
        }
        if ap.car_mission == m::STOP_FOREVER || ap.cruise_speed == 0 || (created_by == RANDOM_VEHICLE && ap.car_mission == m::CRUISE) || ap.driving_style == driving_style::STOP_FOR_CARS {
            return;
        }
        let last_collided = self.ent(r).phys.as_ref().map_or(0, |p| p.last_time_collided);
        if now.wrapping_sub(last_collided) > 500 {
            self.try_veh_mut(r).expect("vehicle").auto_pilot.anti_reverse_timer = now;
        }
        let ap = self.try_veh(r).expect("vehicle").auto_pilot;
        if flat_speed < 0.018 * 0.018 && now.wrapping_sub(ap.anti_reverse_timer) > 2000 {
            let long = (ap.car_mission != m::NONE && ap.car_mission != m::CRUISE) || created_by == MISSION_VEHICLE;
            let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
            ap.temp_action = temp_action::REVERSE;
            ap.time_temp_action = now + if long { 1500 } else { 750 };
            ap.anti_reverse_timer = now;
            if created_by == RANDOM_VEHICLE {
                ap.driving_style = ap.driving_style.max(driving_style::AVOID_CARS);
            }
            self.play_car_horn(px, r);
        }
    }

    /// The per-mission decisions of `UpdateCarAI` for cars on the road or driving by physics.
    fn update_mission(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let frame = self.ents.timer.frame_counter;
        let ap = self.try_veh(r).expect("vehicle").auto_pilot;
        let law = self.try_veh(r).expect("vehicle").flags.is_law_enforcer;
        let (me, player) = (self.epos(r), self.player_info_pos());
        let near_player = flat(player, me);
        let uses_siren = VehicleData::uses_siren(self.ent(r).base.model_index);
        let ignore = ap.ignore_pathfinding;
        match ap.car_mission {
            m::RAMPLAYER_FARAWAY | m::BLOCKPLAYER_FARAWAY => {
                let (close, siren_on) = if ap.car_mission == m::RAMPLAYER_FARAWAY { (m::RAMPLAYER_CLOSE, true) } else { (m::BLOCKPLAYER_CLOSE, true) };
                if find_switch_distance_close() > near_player || ignore {
                    let v = self.try_veh_mut(r).expect("vehicle");
                    v.auto_pilot.car_mission = close;
                    if uses_siren {
                        v.siren_or_alarm = siren_on;
                    }
                }
                self.police_stand_down(px, r);
            }
            m::RAMPLAYER_CLOSE | m::BLOCKPLAYER_CLOSE => {
                let ramming = ap.car_mission == m::RAMPLAYER_CLOSE;
                if find_switch_distance_far(law) >= near_player || ignore {
                    if ramming {
                        self.ram_player_reverse_on_contact(r, now);
                    }
                    self.cops_get_out_when_stuck_near_player(px, r);
                } else if !self.join_car_with_road_system_goto_coors(px, r, player, true) {
                    let v = self.try_veh_mut(r).expect("vehicle");
                    v.auto_pilot.car_mission = if ramming { m::RAMPLAYER_FARAWAY } else { m::BLOCKPLAYER_FARAWAY };
                    v.siren_or_alarm = false;
                    v.car_horn_timer = 0;
                }
                let stood_down = self.police_stand_down(px, r);
                // the ram mission mellows out only when the car did not stand down, the block mission always
                if law && (!stood_down || !ramming) {
                    self.mellow_out_chase_speed(r);
                }
            }
            m::GOTOCOORDS => {
                if flat(ap.destination_coors, me) < DISTANCE_TO_SWITCH_DISTANCE_GOTO || ignore {
                    self.try_veh_mut(r).expect("vehicle").auto_pilot.car_mission = m::GOTOCOORDS_STRAIGHT;
                }
            }
            m::GOTOCOORDS_STRAIGHT => {
                let distance = flat(ap.destination_coors, me);
                let v = self.try_veh(r).expect("vehicle");
                let on_duty = v.flags.is_ambulance_on_duty || v.flags.is_fire_truck_on_duty;
                if on_duty && distance < 20.0 {
                    self.try_veh_mut(r).expect("vehicle").auto_pilot.car_mission = m::EMERGENCYVEHICLE_STOP;
                }
                if distance < 5.0 {
                    let a = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
                    a.car_mission = m::NONE;
                    a.temp_action = temp_action::NONE;
                } else if distance > find_switch_distance_far_normal_vehicle() && !ignore && frame & 7 == 0 {
                    self.try_veh_mut(r).expect("vehicle").auto_pilot.temp_action = temp_action::NONE;
                    let none = self.join_car_with_road_system_goto_coors(px, r, ap.destination_coors, true);
                    self.try_veh_mut(r).expect("vehicle").auto_pilot.car_mission = if none { m::GOTOCOORDS_STRAIGHT } else { m::GOTOCOORDS };
                }
            }
            m::EMERGENCYVEHICLE_STOP => self.emergency_vehicle_stop(px, r),
            m::GOTOCOORDS_ACCURATE => {
                if flat(ap.destination_coors, me) < 20.0 || ignore {
                    self.try_veh_mut(r).expect("vehicle").auto_pilot.car_mission = m::GOTO_COORDS_STRAIGHT_ACCURATE;
                }
            }
            m::GOTO_COORDS_STRAIGHT_ACCURATE => {
                let distance = flat(ap.destination_coors, me);
                if distance < 1.0 {
                    let a = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
                    a.car_mission = m::NONE;
                    a.temp_action = temp_action::NONE;
                } else if distance > find_switch_distance_far_normal_vehicle() && !ignore && frame & 7 == 0 {
                    self.try_veh_mut(r).expect("vehicle").auto_pilot.temp_action = temp_action::NONE;
                    let none = self.join_car_with_road_system_goto_coors(px, r, ap.destination_coors, true);
                    self.try_veh_mut(r).expect("vehicle").auto_pilot.car_mission = if none { m::GOTO_COORDS_STRAIGHT_ACCURATE } else { m::GOTOCOORDS_ACCURATE };
                }
            }
            m::RAMCAR_FARAWAY => {
                let v = self.try_veh_mut(r).expect("vehicle");
                match ap.target_car {
                    Some(t) => {
                        let tp = self.epos(t);
                        if flat(me, tp) < find_switch_distance_close() || ignore {
                            self.try_veh_mut(r).expect("vehicle").auto_pilot.car_mission = m::RAMCAR_CLOSE;
                        }
                    }
                    None => v.auto_pilot.car_mission = m::NONE,
                }
            }
            m::RAMCAR_CLOSE => match ap.target_car {
                Some(t) => {
                    let w = self.player_wanted().map(|w| (w.ignored_by_everyone, w.level, w.ignored_by_cops)).unwrap_or_default();
                    let no_police = px.hooks.no_police_zone();
                    // FIX_BUGS: only when the target is the player's car
                    if self.find_player_vehicle() == Some(t) && (w.0 || (law && (w.1 == 0 || w.2 || no_police))) {
                        self.join_car_with_road_system(px, r);
                        let v = self.try_veh_mut(r).expect("vehicle");
                        v.auto_pilot.car_mission = if no_police { m::NONE } else { m::CRUISE };
                        v.auto_pilot.driving_style = driving_style::STOP_FOR_CARS;
                        v.siren_or_alarm = false;
                    }
                    let tp = self.epos(t);
                    if flat(tp, me) <= find_switch_distance_far(law) || ignore {
                        let collided = self.ent(r).phys.as_ref().is_some_and(|p| p.has_collided_with(self.ent(r).base.flags.use_collision_records, t));
                        let speed = self.ent(r).phys.as_ref().map_or(0.0, |p| p.move_speed.length());
                        if collided && speed < 0.04 {
                            let a = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
                            a.temp_action = temp_action::REVERSE;
                            a.time_temp_action = now + 800;
                        }
                    } else {
                        self.try_veh_mut(r).expect("vehicle").auto_pilot.car_mission = m::RAMCAR_FARAWAY;
                        self.join_car_with_road_system(px, r);
                    }
                }
                None => self.try_veh_mut(r).expect("vehicle").auto_pilot.car_mission = m::NONE,
            },
            m::BLOCKCAR_FARAWAY => match ap.target_car {
                Some(t) => {
                    if flat(self.epos(t), me) < find_switch_distance_close() || ignore {
                        let v = self.try_veh_mut(r).expect("vehicle");
                        v.auto_pilot.car_mission = m::BLOCKCAR_CLOSE;
                        if uses_siren {
                            v.siren_or_alarm = true;
                        }
                    }
                }
                None => self.try_veh_mut(r).expect("vehicle").auto_pilot.car_mission = m::NONE,
            },
            m::BLOCKCAR_CLOSE => match ap.target_car {
                Some(t) => {
                    if flat(self.epos(t), me) > find_switch_distance_far(law) && !ignore {
                        let v = self.try_veh_mut(r).expect("vehicle");
                        v.auto_pilot.car_mission = m::BLOCKCAR_FARAWAY;
                        v.siren_or_alarm = false;
                        v.car_horn_timer = 0;
                        self.join_car_with_road_system(px, r);
                    }
                }
                None => self.try_veh_mut(r).expect("vehicle").auto_pilot.car_mission = m::NONE,
            },
            _ => {
                let level = self.player_wanted().map_or(0, |w| w.level);
                if law && level > 0 && !px.hooks.no_police_zone() {
                    if (player.x - me.x).abs() > 10.0 || (player.y - me.y).abs() > 10.0 {
                        let s = self.find_police_car_speed_for_wanted_level(px, r);
                        let mission = self.find_police_car_mission_for_wanted_level(px);
                        self.get_mut(r).expect("vehicle").base.status = status::PHYSICS;
                        let a = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
                        a.cruise_speed = s as u8;
                        a.car_mission = mission;
                        a.temp_action = temp_action::NONE;
                        a.driving_style = driving_style::AVOID_CARS;
                    } else if ap.car_mission == m::CRUISE {
                        self.get_mut(r).expect("vehicle").base.status = status::PHYSICS;
                        self.tell_occupants_to_leave_car(px, r);
                        let v = self.try_veh_mut(r).expect("vehicle");
                        v.auto_pilot.cruise_speed = 0;
                        v.auto_pilot.car_mission = m::NONE;
                        if level <= 1 {
                            v.siren_or_alarm = false;
                        }
                    }
                }
            }
        }
    }

    /// A ramming car that touched the player's car backs off, briefly if the player is moving.
    fn ram_player_reverse_on_contact(&mut self, r: EntityRef, now: u32) {
        let Some(pv) = self.find_player_vehicle() else { return };
        let collided = self.ent(r).phys.as_ref().is_some_and(|p| p.has_collided_with(self.ent(r).base.flags.use_collision_records, pv));
        let ta = self.try_veh(r).expect("vehicle").auto_pilot.temp_action;
        if collided && ta != temp_action::TURNLEFT && ta != temp_action::TURNRIGHT {
            let slow = self.ent(pv).phys.as_ref().is_some_and(|p| p.move_speed.length() < 0.05);
            let a = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
            a.temp_action = temp_action::REVERSE;
            a.time_temp_action = now + if slow { 800 } else { 50 };
        }
    }

    /// `MISSION_EMERGENCYVEHICLE_STOP`: a stopped ambulance or fire truck waits for work, or goes back to the traffic.
    fn emergency_vehicle_stop(&mut self, px: &mut Px, r: EntityRef) {
        let speed = self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
        if Vec2::new(speed.x, speed.y).length() >= 0.01 {
            return;
        }
        let dest = self.try_veh(r).expect("vehicle").auto_pilot.destination_coors;
        if self.try_veh(r).expect("vehicle").flags.is_ambulance_on_duty {
            let (found, distance) = self.find_nearest_accident(dest);
            if found.is_some() && distance < 30.0 {
                self.tell_occupants_to_leave_car(px, r);
                self.try_veh_mut(r).expect("vehicle").auto_pilot.car_mission = m::STOP_FOREVER;
            } else {
                self.join_car_with_road_system(px, r);
                let v = self.try_veh_mut(r).expect("vehicle");
                v.auto_pilot.car_mission = m::CRUISE;
                v.siren_or_alarm = false;
                v.auto_pilot.cruise_speed = 17;
                self.end_emergency_duty(r);
            }
        }
        if self.try_veh(r).expect("vehicle").flags.is_fire_truck_on_duty {
            let (found, distance) = self.ents.fires.find_nearest(dest);
            if found.is_some() && distance < 30.0 {
                self.tell_occupants_to_leave_car(px, r);
                self.try_veh_mut(r).expect("vehicle").auto_pilot.car_mission = m::STOP_FOREVER;
            } else {
                self.join_car_with_road_system(px, r);
                let v = self.try_veh_mut(r).expect("vehicle");
                v.auto_pilot.car_mission = m::CRUISE;
                v.siren_or_alarm = false;
                v.auto_pilot.cruise_speed = 17;
                self.end_emergency_duty(r);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::{Box3, ColModel, Sphere};
    use crate::entity::{Ent, Entity, EntityType, Kind};
    use crate::general::Rng;
    use crate::ped::ctx::NoPedHooks;
    use crate::ped::types::ped_type;
    use crate::vehicle::base::VehicleType;

    fn world() -> (World, EntityRef) {
        let mut models = crate::modelinfo::ModelInfoStore::new(re3_formats::datafile::Game::Gta3);
        let mi = models.add_simple_model(10).unwrap();
        let b = Box3 { min: Vec3::new(-1.0, -2.0, -1.0), max: Vec3::new(1.0, 2.0, 1.0), surface: 0, piece: 0 };
        mi.col_model = Some(std::sync::Arc::new(ColModel { bounding_sphere: Sphere::new(2.5, Vec3::ZERO, 0, 0), bounding_box: b, boxes: vec![b], ..Default::default() }));
        let mut w = World::new(models, crate::temp_col::TempColModels::new(re3_formats::datafile::Game::Gta3));
        let mut rng = Rng::default();
        let mut hooks = NoPedHooks;
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let p = w.create_ped(&mut px, ped_type::PLAYER1, 10, None).unwrap();
        w.ped_mut(p).player = Some(Default::default());
        w.add(p).unwrap();
        w.ents.player_ped = Some(p);
        let mut v = VehicleData::new(RANDOM_VEHICLE, 0, VehicleType::Car, &mut rng);
        v.auto_pilot.cruise_speed = 10;
        let mut base = Entity::new(EntityType::Vehicle, &mut rng);
        base.model_index = 10;
        base.flags.uses_collision = true;
        base.status = status::PHYSICS;
        base.set_position(Vec3::new(100.0, 100.0, 0.0));
        let r = w.insert(Ent::new(base, Some(crate::entity::physical::Physical::default()), Kind::Vehicle(Box::new(v)))).unwrap();
        w.add(r).unwrap();
        (w, r)
    }

    fn set_level(w: &mut World, level: i32) {
        w.player_wanted_mut().unwrap().level = level;
    }

    #[test]
    fn police_speed_and_mission_follow_the_wanted_level() {
        let (mut w, r) = world();
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        set_level(&mut w, 1);
        assert_eq!((w.find_police_car_speed_for_wanted_level(&mut px, r), w.find_police_car_mission_for_wanted_level(&mut px)), (25, m::BLOCKPLAYER_FARAWAY));
        set_level(&mut w, 2);
        assert_eq!(w.find_police_car_speed_for_wanted_level(&mut px, r), 34);
        set_level(&mut w, 0);
        assert!((12..16).contains(&w.find_police_car_speed_for_wanted_level(&mut px, r)));
        set_level(&mut w, 7);
        assert_eq!(w.find_police_car_speed_for_wanted_level(&mut px, r), 0);
    }

    #[test]
    fn a_wrecked_car_has_no_mission() {
        let (mut w, r) = world();
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        w.get_mut(r).unwrap().base.status = status::WRECKED;
        w.try_veh_mut(r).unwrap().auto_pilot.car_mission = m::CRUISE;
        w.update_car_ai(&mut px, r);
        let ap = w.try_veh(r).unwrap().auto_pilot;
        assert_eq!((ap.car_mission, ap.cruise_speed), (m::NONE, 0));
    }

    #[test]
    fn a_car_going_to_coordinates_goes_straight_when_close_and_stops_when_there() {
        let (mut w, r) = world();
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        {
            let ap = &mut w.try_veh_mut(r).unwrap().auto_pilot;
            ap.car_mission = m::GOTOCOORDS;
            ap.destination_coors = Vec3::new(110.0, 100.0, 0.0);
        }
        w.update_car_ai(&mut px, r);
        assert_eq!(w.try_veh(r).unwrap().auto_pilot.car_mission, m::GOTOCOORDS_STRAIGHT);
        w.try_veh_mut(r).unwrap().auto_pilot.destination_coors = Vec3::new(102.0, 100.0, 0.0);
        w.update_car_ai(&mut px, r);
        assert_eq!(w.try_veh(r).unwrap().auto_pilot.car_mission, m::NONE);
    }

    #[test]
    fn a_stuck_car_that_avoids_traffic_reverses_after_two_seconds() {
        let (mut w, r) = world();
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        {
            let ap = &mut w.try_veh_mut(r).unwrap().auto_pilot;
            ap.car_mission = m::GOTOCOORDS_STRAIGHT;
            ap.destination_coors = Vec3::new(400.0, 100.0, 0.0);
            ap.ignore_pathfinding = true;
            ap.driving_style = driving_style::AVOID_CARS;
            ap.anti_reverse_timer = 0;
        }
        w.ents.timer.time_in_ms = 5000;
        w.get_mut(r).unwrap().phys.as_mut().unwrap().last_time_collided = 5000;
        w.update_car_ai(&mut px, r);
        let ap = w.try_veh(r).unwrap().auto_pilot;
        assert_eq!(ap.temp_action, temp_action::REVERSE);
        assert_eq!(ap.time_temp_action, 5000 + 1500);
    }

    #[test]
    fn cops_ease_off_when_the_player_is_far_or_on_foot() {
        let (mut w, r) = world();
        set_level(&mut w, 1);
        w.mellow_out_chase_speed(r);
        // the player on foot, 140 m away
        assert_eq!(w.try_veh(r).unwrap().auto_pilot.cruise_speed, 25);
        w.ent_mut_for_test(w.ents.player_ped.unwrap()).base.set_position(Vec3::new(100.0, 110.0, 0.0));
        w.mellow_out_chase_speed(r);
        assert_eq!(w.try_veh(r).unwrap().auto_pilot.cruise_speed, 5);
    }
}
