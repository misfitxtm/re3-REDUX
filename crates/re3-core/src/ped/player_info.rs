//! `PlayerInfo.cpp`: the per-frame player upkeep, getting in and out of cars, dying and being arrested.
use super::ctx::Px;
use super::data::*;
use crate::entity::{status, EntityRef};
use crate::math::atan_of_xy;
use crate::modelinfo::indices_gta3::mi;
use crate::player_info::wb_state;
use crate::vehicle::base::CarLock;
use crate::world::World;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

/// What the remote-control ending does to the camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteCamera {
    /// Start fading to black.
    FadeOut,
    /// Jump back to the player, fade in and load what the new view needs.
    Restore,
}

impl World {
    /// `CPlayerInfo::GetPos`: where the player is, in the car if in one.
    pub fn player_info_pos(&self) -> Vec3 {
        let Some(p) = self.ents.player_ped else { return Vec3::ZERO };
        self.find_player_vehicle().map_or_else(|| self.epos(p), |v| self.epos(v))
    }

    /// `FindPlayerSpeed`.
    pub fn find_player_speed(&self) -> Vec3 {
        let ent = self.find_player_vehicle().or(self.ents.player_ped);
        ent.and_then(|r| self.get(r)).and_then(|e| e.phys.as_ref()).map_or(Vec3::ZERO, |p| p.move_speed)
    }

    /// `FindPlayerHeading`.
    pub fn find_player_heading(&self) -> f32 {
        let ent = self.ents.player_info.remote_vehicle.or(self.find_player_vehicle()).or(self.ents.player_ped);
        ent.and_then(|r| self.get(r)).map_or(0.0, |e| crate::math::heading(e.base.matrix.forward))
    }

    /// `FindPlayerEntity`: the car if the player is in one.
    pub fn find_player_entity(&self) -> Option<EntityRef> {
        self.find_player_vehicle().or(self.ents.player_ped)
    }

    /// `CPlayerInfo::KillPlayer`.
    pub fn kill_player(&mut self, px: &mut Px) {
        if self.ents.player_info.wb_state != wb_state::PLAYING {
            return;
        }
        self.ents.player_info.wb_state = wb_state::WASTED;
        self.ents.player_info.wb_time = self.now();
        self.reset_darkel_on_player_death(px);
        px.hooks.player_wasted_or_busted(false);
    }

    /// `CPlayerInfo::ArrestPlayer`.
    pub fn arrest_player_info(&mut self, px: &mut Px) {
        if self.ents.player_info.wb_state != wb_state::PLAYING {
            return;
        }
        self.ents.player_info.wb_state = wb_state::BUSTED;
        self.ents.player_info.wb_time = self.now();
        self.reset_darkel_on_player_death(px);
        px.hooks.player_wasted_or_busted(true);
    }

    /// `CPlayerInfo::PlayerFailedCriticalMission`.
    pub fn player_failed_critical_mission(&mut self) {
        if self.ents.player_info.wb_state != wb_state::PLAYING {
            return;
        }
        self.ents.player_info.wb_state = wb_state::FAILED_CRITICAL_MISSION;
        self.ents.player_info.wb_time = self.now();
    }

    /// `CPlayerInfo::CancelPlayerEnteringCars`: for `car`, or for any car with `None`.
    pub fn cancel_player_entering_cars(&mut self, px: &mut Px, car: Option<EntityRef>) {
        let Some(p) = self.ents.player_ped else { return };
        if (car.is_none() || car == self.ped(p).my_vehicle) && self.ped(p).entering_car() {
            self.quit_entering_car(px, p);
        }
        if matches!(self.ped(p).objective, objective::ENTER_CAR_AS_PASSENGER | objective::ENTER_CAR_AS_DRIVER) {
            self.clear_objective(px, p);
        }
    }

    /// `CPlayerInfo::MakePlayerSafe`.
    pub fn make_player_safe(&mut self, px: &mut Px, toggle: bool) {
        let Some(p) = self.ents.player_ped else { return };
        if toggle {
            if let Some(w) = self.player_wanted_mut() {
                w.ignored_by_everyone = true;
            }
            self.stop_all_law_enforcers_in_their_tracks();
            px.hooks.set_player_safe_controls(true);
            let f = &mut self.get_mut(p).expect("player").base.flags;
            (f.bullet_proof, f.fire_proof, f.collision_proof, f.melee_proof, f.only_damaged_by_player, f.explosion_proof) = (true, true, true, true, true, true);
            if let Some(d) = self.ped_mut(p).player.as_deref_mut() {
                d.can_be_damaged = false;
            }
            self.clear_adrenaline(px);
            self.cancel_player_entering_cars(px, None);
            let pos = self.player_info_pos();
            self.extinguish_fires_at(px, pos, 4000.0);
            self.ents.explosions.remove_in_area(pos, 4000.0);
            px.hooks.remove_all_projectiles();
            self.set_all_cars_can_be_damaged(false);
            self.extinguish_all_car_fires_in_area(pos, 4000.0);
        } else if px.hooks.can_make_player_unsafe() {
            if let Some(w) = self.player_wanted_mut() {
                w.ignored_by_everyone = false;
            }
            px.hooks.set_player_safe_controls(false);
            let f = &mut self.get_mut(p).expect("player").base.flags;
            (f.bullet_proof, f.fire_proof, f.collision_proof, f.melee_proof, f.only_damaged_by_player, f.explosion_proof) = (false, false, false, false, false, false);
            if let Some(d) = self.ped_mut(p).player.as_deref_mut() {
                d.can_be_damaged = true;
            }
            self.set_all_cars_can_be_damaged(true);
        }
    }

    /// `CPlayerInfo::BlowUpRCBuggy`.
    pub fn blow_up_rc_buggy(&mut self) {
        let Some(rc) = self.ents.player_info.remote_vehicle else { return };
        if self.get(rc).is_none_or(|e| e.base.flags.remove_from_world) {
            return;
        }
        self.take_remote_controlled_car_from_player();
        let culprit = self.ents.player_ped;
        self.blow_up_car_entity(rc, culprit);
    }

    /// `CPlayerInfo::AwardMoneyForExplosion`: money for a wreck, more for chains of them.
    pub fn award_money_for_explosion(&mut self, px: &mut Px, wrecked: EntityRef) {
        let now = self.now();
        let info = &mut self.ents.player_info;
        if now.wrapping_sub(info.previous_time_rewarded_for_explosion) < 6000 {
            info.explosions_since_last_reward += 1;
        } else {
            info.explosions_since_last_reward = 1;
        }
        info.previous_time_rewarded_for_explosion = now;
        let value = self.try_veh(wrecked).map_or(0, |v| self.ents.handling.get(v.handling).monetary_value);
        let award = (value as f32 * 0.002) as i32;
        let pos = self.epos(wrecked);
        px.hooks.money_message(pos + Vec3::new(0.0, 0.0, 2.0), award);
        let info = &mut self.ents.player_info;
        info.money += award;
        for _ in 1..info.explosions_since_last_reward {
            // the original draws a number here and drops it
            px.rng.number();
            self.ents.player_info.money += award;
        }
    }

    /// `FindClosestCarSectorList` over the sectors around the player, as the enter-car key does it.
    fn find_closest_car(&mut self, player: EntityRef) -> Option<EntityRef> {
        let pos = self.epos(player);
        let mut closest = None;
        let mut last_closeness = 0.0;
        for v in self.vehicles_in_sector_box(pos.x - 10.0, pos.y - 10.0, pos.x + 10.0, pos.y + 10.0) {
            let Some(e) = self.get(v) else { continue };
            if !e.base.flags.uses_collision {
                continue;
            }
            let st = e.base.status;
            let is_bike = self.try_veh(v).is_some_and(|d| d.is_bike());
            if st == status::WRECKED || st == status::TRAIN_MOVING || !(e.base.matrix.up.z > 0.3 || is_bike) {
                continue;
            }
            let centre = self.bound_centre(v);
            if (pos.z - centre.z).abs() >= 2.0 {
                continue;
            }
            let dist = (pos - centre).truncate().length();
            if dist <= 10.0 && !self.ents.cranes.is_car_being_carried(v) {
                // `EvaluateCarPosition`: prefers cars the player already faces
                let to_car = (e.base.position() - pos).truncate();
                let fwd = self.ent(player).base.matrix.forward;
                let mut turn = atan_of_xy(fwd.x, fwd.y) - atan_of_xy(to_car.x, to_car.y);
                while turn >= PI {
                    turn -= TAU;
                }
                while turn < -PI {
                    turn += TAU;
                }
                let closeness = (1.0 - turn.abs() / TAU) * (10.0 - dist);
                if closeness > last_closeness {
                    last_closeness = closeness;
                    closest = Some(v);
                }
            }
        }
        closest
    }

    /// `CPlayerInfo::Process`.
    pub fn process_player_info(&mut self, px: &mut Px) {
        let Some(player) = self.ents.player_ped else { return };
        if self.ents.timer.playing_back {
            return;
        }
        let now = self.now();
        let frame = self.ents.timer.frame_counter;

        // the unused taxi feature: a dollar per second with a passenger
        let mut start_taxi_timer = true;
        if self.ents.player_info.unused_taxi_thing && self.ped(player).in_vehicle {
            let veh = self.ped(player).my_vehicle;
            let ok = veh.is_some_and(|v| {
                matches!(self.ent(v).base.model_index, mi::TAXI | mi::CABBIE | mi::BORGNINE) && self.try_veh(v).is_some_and(|d| d.driver == Some(player) && d.num_passengers != 0)
            });
            if ok {
                let info = &mut self.ents.player_info;
                let mut passed = now.wrapping_sub(info.unused_taxi_timer);
                while passed >= 1000 {
                    passed -= 1000;
                    info.money += 1;
                    info.unused_taxi_timer += 1000;
                }
                start_taxi_timer = false;
            }
        }
        if start_taxi_timer {
            self.ents.player_info.unused_taxi_timer = now;
        }

        // the money counter ticks towards the real amount
        let info = &mut self.ents.player_info;
        if info.visible_money != info.money {
            let diff = info.money - info.visible_money;
            let by = match diff.abs() {
                d if d > 100_000 => 12345,
                d if d > 10_000 => 1234,
                d if d > 1000 => 123,
                d if d > 50 => 42,
                _ => 1,
            };
            info.visible_money += if diff < 0 { -by } else { by };
        }

        if frame & 15 == 0 {
            let pos = self.player_info_pos();
            self.ents.player_info.road_density = self.ents.paths.calc_road_density(pos.x, pos.y);
        }
        let rd = &mut self.ents.player_info.road_density;
        *rd = rd.clamp(0.4, 1.45);

        let pad = px.hooks.pad();
        let enter_or_exit = if self.ped(player).flags.veh_exit_will_be_instant && self.ped(player).in_vehicle { pad.exit_vehicle_just_down } else { pad.exit_vehicle };
        if enter_or_exit && !matches!(self.ped(player).ped_state, state::SNIPER_MODE | state::ROCKET_MODE) {
            if self.ped(player).in_vehicle {
                if self.ents.player_info.remote_vehicle.is_none() {
                    self.player_leaves_vehicle(px, player);
                }
            } else if pad.exit_vehicle_just_down {
                self.player_enters_vehicle(px, player);
            }
        }

        if self.ents.player_info.in_remote_mode {
            let lost = self.ents.player_info.time_lost_remote_car;
            let without = now.wrapping_sub(lost);
            if self.ents.timer.previous_time_in_ms.wrapping_sub(lost) < 1000 && without >= 1000 && self.ents.player_info.wb_state == wb_state::PLAYING {
                px.hooks.remote_mode_camera(RemoteCamera::FadeOut);
            }
            if without > 2000 {
                if self.ents.player_info.wb_state == wb_state::PLAYING {
                    px.hooks.remote_mode_camera(RemoteCamera::Restore);
                }
                self.ents.player_info.in_remote_mode = false;
                self.ents.player_info.remote_vehicle = None;
                if let Some(v) = self.find_player_vehicle() {
                    self.get_mut(v).expect("vehicle").base.status = status::PLAYER;
                }
            }
        }

        if frame & 31 == 0 {
            self.check_player_car_upside_down(player);
        }

        if let Some(v) = self.find_player_vehicle() {
            self.get_mut(v).expect("vehicle").phys.as_mut().expect("physical").zone_level = -1;
            let passengers: Vec<EntityRef> = self.try_veh(v).map(|d| d.passengers.iter().flatten().copied().collect()).unwrap_or_default();
            for p in passengers {
                if let Some(ph) = self.get_mut(p).and_then(|e| e.phys.as_mut()) {
                    ph.zone_level = 0;
                }
            }
            let d = self.get(v).and_then(|e| e.phys.as_ref()).map_or(0.0, |p| p.distance_travelled);
            px.hooks.add_distance_travelled(true, d);
        } else {
            let d = self.get(player).and_then(|e| e.phys.as_ref()).map_or(0.0, |p| p.distance_travelled);
            px.hooks.add_distance_travelled(false, d);
        }
    }

    /// The upside-down check every 32nd frame: a stuck-on-its-roof car loses its engine.
    fn check_player_car_upside_down(&mut self, player: EntityRef) {
        let veh = self.find_player_vehicle().filter(|_| self.ped(player).in_vehicle);
        let upside_down = veh.is_some_and(|v| {
            let (e, d) = (self.ent(v), self.try_veh(v).expect("vehicle"));
            let speed = e.phys.as_ref().map_or(0.0, |p| p.move_speed.length());
            e.base.matrix.up.z < 0.0 && speed < 0.05 && self.car_ref(v).is_some() && !e.phys.as_ref().is_some_and(|p| p.flags.is_in_water) && !d.is_boat()
        });
        let up = veh.map_or(1.0, |v| self.ent(v).base.matrix.up.z);
        let info = &mut self.ents.player_info;
        if upside_down {
            info.upside_down_counter += if up < -0.5 { 2 } else { 1 };
        } else {
            info.upside_down_counter = 0;
        }
        if info.upside_down_counter > 6
            && let Some(v) = veh
            && self.try_veh(v).is_some_and(|d| d.flags.can_be_damaged)
        {
            let d = self.try_veh_mut(v).expect("vehicle");
            d.health = d.health.min(249.0);
            if let Some(car) = self.car_mut(v) {
                car.damage.set_engine_status(225);
                car.set_on_fire_entity = None;
            }
        }
    }

    /// The exit key in a car.
    fn player_leaves_vehicle(&mut self, px: &mut Px, player: EntityRef) {
        let Some(veh) = self.ped(player).my_vehicle else { return };
        let on_bridge = self.try_veh(veh).and_then(|v| v.cur_ground_entity).is_some_and(|s| self.this_is_a_bridge_object_moving_up(self.ent(s).base.model_index));
        if on_bridge {
            return;
        }
        let (is_boat, lock) = self.try_veh(veh).map_or((false, CarLock::Unlocked), |v| (v.is_boat(), v.door_lock));
        if is_boat && lock != CarLock::LockedPlayerInside {
            // the part VC moved into SetExitBoat
            self.set_exit_boat(px, player, veh);
            self.ped_mut(player).flags.trying_to_reach_dry_land = true;
            return;
        }
        let e = self.ent(veh);
        let speed = e.phys.as_ref().map_or(0.0, |p| p.move_speed.length());
        let in_water = e.phys.as_ref().is_some_and(|p| p.flags.is_in_water);
        if !matches!(e.base.status, status::WRECKED | status::TRAIN_MOVING) && lock != CarLock::LockedPlayerInside && speed < 0.17 && self.ents.timer.time_scale >= 0.5 && !in_water {
            self.set_objective_entity(px, player, objective::LEAVE_CAR, veh);
        }
    }

    /// The exit key on foot: the closest car, or the boat the player stands on.
    fn player_enters_vehicle(&mut self, px: &mut Px, player: EntityRef) {
        let mut car_below = None;
        let mut on_boat = false;
        if let Some(s) = self.ped(player).current_phys_surface.filter(|&s| self.is_vehicle_ref(s)) {
            car_below = Some(s);
            if self.try_veh(s).is_some_and(|v| v.is_boat()) {
                on_boat = true;
                self.ped_mut(player).flags.on_boat = true;
                if self.ent(s).base.status != status::WRECKED {
                    self.set_seek_boat_position(player, s);
                }
            }
        }
        if !on_boat {
            car_below = self.find_closest_car(player).or(car_below);
        }
        let Some(car) = car_below.filter(|_| !on_boat) else { return };
        if self.ent(car).base.status == status::TRAIN_NOT_MOVING {
            self.set_objective_entity(px, player, objective::ENTER_CAR_AS_PASSENGER, car);
        } else if self.try_veh(car).is_some_and(|v| v.is_boat()) {
            if self.try_veh(car).is_some_and(|v| v.driver.is_none()) {
                self.ped_mut(player).veh_door = 0;
                self.set_enter_car(px, player, car, 0);
            }
        } else {
            self.set_objective_entity(px, player, objective::ENTER_CAR_AS_DRIVER, car);
        }
    }
}
