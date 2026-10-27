//! `Remote.cpp`: the radio-controlled car the player drives from afar.
use crate::entity::{status, EntityRef};
use crate::math::Matrix;
use crate::ped::ctx::Px;
use crate::vehicle::base::{car_mission as m, driving_style, temp_action, MISSION_VEHICLE, RANDOM_VEHICLE};
use crate::world::World;
use glam::Vec3;

impl World {
    /// `CRemote::GivePlayerRemoteControlledCar`: makes the car at the spot and hands the player its controls.
    pub fn give_player_remote_controlled_car(&mut self, px: &mut Px, x: f32, y: f32, z: f32, rot: f32, model: i16) -> Option<EntityRef> {
        let car = self.new_car(model, MISSION_VEHICLE, px.rng)?;
        let (ground, _) = self.find_ground_z_for_3d_coord(Vec3::new(x, y, z + 2.0));
        let base_height = self.col_model_of(car).map_or(0.0, |c| -c.bounding_box.min.z);
        {
            let e = self.get_mut(car).expect("car");
            let mut matrix = Matrix::rotate_z_only(rot);
            matrix.pos = Vec3::new(x, y, ground + base_height);
            e.base.matrix = matrix;
            e.base.status = status::PLAYER_REMOTE;
        }
        self.try_veh_mut(car).expect("car").flags.is_locked = true;
        self.join_car_with_road_system(px, car);
        {
            let v = self.try_veh_mut(car).expect("car");
            v.auto_pilot.car_mission = m::NONE;
            v.auto_pilot.temp_action = temp_action::NONE;
            v.auto_pilot.driving_style = driving_style::STOP_FOR_CARS;
            v.auto_pilot.cruise_speed = 9;
            v.auto_pilot.max_traffic_speed = 9.0;
            v.auto_pilot.next_lane = 0;
            v.auto_pilot.current_lane = 0;
            v.flags.engine_on = true;
        }
        let _ = self.add(car);
        if let Some(pv) = self.find_player_vehicle() {
            self.get_mut(pv).expect("vehicle").base.status = status::PLAYER_DISABLED;
        }
        self.ents.player_info.remote_vehicle = Some(car);
        px.hooks.camera_take_control_behind_car(car);
        Some(car)
    }

    /// `CRemote::TakeRemoteControlledCarFromPlayer`: the car turns into ordinary traffic and is removed.
    pub fn take_remote_controlled_car_from_player(&mut self) {
        let now = self.now();
        let Some(rc) = self.ents.player_info.remote_vehicle else { return };
        if let Some(v) = self.try_veh_mut(rc) {
            v.created_by = RANDOM_VEHICLE;
            v.flags.is_locked = false;
            self.ents.car_ctrl.num_mission_cars -= 1;
            self.ents.car_ctrl.num_random_cars += 1;
        }
        if let Some(e) = self.get_mut(rc) {
            e.base.flags.remove_from_world = true;
        }
        let info = &mut self.ents.player_info;
        info.time_lost_remote_car = now;
        info.in_remote_mode = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::physical::Physical;
    use crate::entity::{Ent, Entity, EntityType, Kind};
    use crate::general::Rng;
    use crate::vehicle::base::{VehicleData, VehicleType};

    #[test]
    fn taking_the_buggy_back_makes_it_ordinary_traffic_to_be_removed() {
        let mut w = World::new(crate::modelinfo::ModelInfoStore::new(re3_formats::datafile::Game::Gta3), crate::temp_col::TempColModels::new(re3_formats::datafile::Game::Gta3));
        let mut rng = Rng::default();
        let mut v = VehicleData::new(MISSION_VEHICLE, 0, VehicleType::Car, &mut rng);
        v.flags.is_locked = true;
        let car = w.insert(Ent::new(Entity::new(EntityType::Vehicle, &mut rng), Some(Physical::default()), Kind::Vehicle(Box::new(v)))).unwrap();
        w.ents.player_info.remote_vehicle = Some(car);
        w.ents.timer.time_in_ms = 4000;
        w.take_remote_controlled_car_from_player();
        let v = w.try_veh(car).unwrap();
        assert_eq!((v.created_by, v.flags.is_locked), (RANDOM_VEHICLE, false));
        assert!(w.ent(car).base.flags.remove_from_world);
        let info = &w.ents.player_info;
        assert_eq!((info.in_remote_mode, info.time_lost_remote_car), (true, 4000));
        assert_eq!((w.ents.car_ctrl.num_mission_cars, w.ents.car_ctrl.num_random_cars), (0, 1));
    }
}
