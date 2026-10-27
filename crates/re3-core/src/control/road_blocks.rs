//! `RoadBlocks.cpp`: police cars across the road ahead of a player who is being chased.
use super::pathfind::{OBJECT_EAST_WEST, USE_IN_ROADBLOCK};
use crate::entity::{status, EntityRef};
use crate::math::Matrix;
use crate::modelinfo::indices_gta3::mi;
use crate::ped::cop::cop_type;
use crate::ped::ctx::Px;
use crate::vehicle::base::{car_mission as m, temp_action, CarLock, VehicleData, RANDOM_VEHICLE};
use crate::weapons::info::weapon_type as wt;
use crate::world::{Kinds, World};
use glam::{Vec2, Vec3};
use std::f32::consts::FRAC_PI_2;

pub const NUMROADBLOCKS: usize = 600;
const ROADBLOCKDIST: f32 = 80.0;

/// The offsets (in units of the police car's size) cops stand at behind a roadblock car.
const ROADBLOCK_OFFSETS: [Vec3; 6] = [
    Vec3::new(-1.5, 1.8, 0.0),
    Vec3::new(-1.5, -1.8, 0.0),
    Vec3::new(1.5, 1.8, 0.0),
    Vec3::new(1.5, -1.8, 0.0),
    Vec3::new(-1.5, 0.0, 0.0),
    Vec3::new(1.5, 0.0, 0.0),
];

/// `CRoadBlocks`' statics.
#[derive(Debug, Clone, Default)]
pub struct RoadBlocks {
    /// `RoadBlockObjects`: the road pieces (indices into the map objects) a roadblock can stand on.
    pub objects: Vec<i16>,
    /// `InOrOut`: whether the player was within range of each one the last time.
    pub in_or_out: Vec<bool>,
}

impl RoadBlocks {
    /// `CRoadBlocks::Init`: every road piece flagged for roadblocks, up to the limit.
    pub fn init(&mut self, object_flags: &[u8], num_map_objects: usize) {
        self.objects.clear();
        self.in_or_out.clear();
        for (i, &f) in object_flags.iter().enumerate().take(num_map_objects) {
            if f & USE_IN_ROADBLOCK != 0 {
                if self.objects.len() >= NUMROADBLOCKS {
                    // there is no room for more
                    return;
                }
                self.objects.push(i as i16);
                self.in_or_out.push(true);
            }
        }
    }
}

impl World {
    fn model_radius(&self, model: i16) -> f32 {
        self.ents.models.get(model as i32).and_then(|m| m.col_model.as_ref()).map_or(0.0, |c| c.bounding_sphere.radius)
    }

    /// `CRoadBlocks::GenerateRoadBlockCopsForCar`: two cops stand behind the roadblock car, ready to shoot.
    pub fn generate_road_block_cops_for_car(&mut self, px: &mut Px, r: EntityRef) {
        let Some(v) = self.try_veh(r) else { return };
        let (road_block_type, road_block_node) = (v.roadblock_type as usize, v.roadblock_node);
        let target = self.find_player_vehicle().or(self.ents.player_ped);
        let police_radius = self.model_radius(mi::POLICE);
        let radius = self.bound_radius_of(r) / police_radius;
        let model = self.ent(r).base.model_index;
        let (model_id, kind) = match model {
            mi::FBICAR => (mi::FBI, cop_type::FBI),
            mi::ENFORCER => (mi::SWAT, cop_type::SWAT),
            mi::BARRACKS => (mi::ARMY, cop_type::ARMY),
            _ => (mi::COP, cop_type::STREET),
        };
        for i in 0..2 {
            let index = i + 2 * road_block_type;
            let matrix = self.ent(r).base.matrix;
            let mut pos = matrix.transform_point(radius * ROADBLOCK_OFFSETS[index]);
            let kind = if px.hooks.model_loaded(model_id) { kind } else { cop_type::STREET };
            let clump = px.hooks.load_ped_clump(match kind {
                cop_type::FBI => mi::FBI,
                cop_type::SWAT => mi::SWAT,
                cop_type::ARMY => mi::ARMY,
                _ => mi::COP,
            });
            let Some(cop) = self.create_cop(px, kind, clump) else { return };
            if kind == cop_type::STREET {
                self.set_current_weapon(cop, wt::COLT45);
            }
            self.find_z_coor_for_ped(&mut pos);
            {
                let e = self.get_mut(cop).expect("cop");
                e.base.set_position(pos);
                e.base.set_orientation(0.0, 0.0, -FRAC_PI_2);
            }
            self.ped_mut(cop).cop.as_deref_mut().expect("cop data").is_disabled_cop = true;
            self.set_idle(px, cop);
            {
                let pd = self.ped_mut(cop);
                pd.flags.kinda_stay_in_same_place = true;
                pd.flags.not_allowed_to_duck = false;
                pd.cop.as_deref_mut().expect("cop data").roadblock_node = road_block_node;
                pd.flags.crouch_when_shooting = road_block_type != 2;
            }
            if let Some(t) = target {
                self.set_weapon_lock_on_target(cop, Some(t));
                self.set_attack(px, cop, t);
            }
            let pd = self.ped_mut(cop);
            pd.my_vehicle = Some(r);
            pd.flags.cull_extra_far_away = true;
            pd.clump_alpha = 0;
            let _ = self.add(cop);
        }
    }

    /// `CRoadBlocks::GenerateRoadBlocks`: a sixteenth of the roadblock sites each frame; a site the player comes near gets cars.
    pub fn generate_road_blocks(&mut self, px: &mut Px) {
        let frame = (self.ents.timer.frame_counter & 0xF) as usize;
        let mut node = NUMROADBLOCKS * frame / 16;
        let max_blocks = NUMROADBLOCKS * (frame + 1) / 16;
        let count = self.ents.road_blocks.objects.len();
        let player = self.player_info_pos();
        while node < count.min(max_blocks) {
            let object = self.ents.road_blocks.objects[node] as usize;
            let (obj_model, obj_matrix) = {
                let mo = &self.ents.paths.map_objects[object];
                (mo.model, mo.matrix)
            };
            let dist = Vec2::new(player.x - obj_matrix.pos.x, player.y - obj_matrix.pos.y);
            if dist.x > -ROADBLOCKDIST && dist.x < ROADBLOCKDIST && dist.y > -ROADBLOCKDIST && dist.y < ROADBLOCKDIST && dist.length() < ROADBLOCKDIST {
                if !self.ents.road_blocks.in_or_out[node] {
                    self.ents.road_blocks.in_or_out[node] = true;
                    let w = self.player_wanted().cloned().unwrap_or_default();
                    if self.find_player_vehicle().is_some() && ((px.rng.number() & 0x7F) as i32) < w.roadblock_density as i32 {
                        self.build_road_block(px, node, object, obj_model, obj_matrix, &w);
                    }
                }
            } else {
                self.ents.road_blocks.in_or_out[node] = false;
            }
            node += 1;
        }
    }

    /// The cars of one roadblock site.
    fn build_road_block(&mut self, px: &mut Px, node: usize, object: usize, obj_model: i16, obj_matrix: Matrix, w: &crate::control::wanted::Wanted) {
        let map_radius = 2.0 * self.ents.models.get(obj_model as i32).and_then(|m| m.col_model.as_ref()).map_or(0.0, |c| c.bounding_box.max.x);
        let mut vehicle_id = mi::POLICE;
        if w.are_army_required() {
            vehicle_id = mi::BARRACKS;
        } else if w.are_fbi_required() {
            vehicle_id = mi::FBICAR;
        } else if w.are_swat_required() {
            vehicle_id = mi::ENFORCER;
        }
        if !px.hooks.model_loaded(vehicle_id) {
            vehicle_id = mi::POLICE;
        }
        let model_radius = 2.0 * self.model_radius(vehicle_id) + 0.25;
        let radius = (map_radius / model_radius) as i16;
        if radius >= 6 {
            return;
        }
        let to_camera = px.hooks.camera_position() - obj_matrix.pos;
        let dot = Vec2::new(to_camera.x, to_camera.y).dot(Vec2::new(obj_matrix.forward.x, obj_matrix.forward.y));
        let offset = 0.5 * model_radius * (radius - 1) as f32;
        let east_west = self.ents.paths.object_flags[object] & OBJECT_EAST_WEST != 0;
        for i in 0..radius {
            let mut block_type = (dot < 0.0) as u8;
            let rotation;
            if px.rng.number() & 1 != 0 {
                rotation = ((px.rng.number() & 0xFF) as f32 - 128.0) * 0.003 + FRAC_PI_2;
            } else {
                block_type = (block_type == 0) as u8;
                rotation = ((px.rng.number() & 0xFF) as f32 - 128.0) * 0.003 - FRAC_PI_2;
            }
            let mut offset_matrix = Matrix::rotate_z_only(rotation);
            offset_matrix.pos = if east_west { Vec3::new(0.0, i as f32 * model_radius - offset, 0.6) } else { Vec3::new(i as f32 * model_radius - offset, 0.0, 0.6) };
            let mut vehicle_matrix = obj_matrix * offset_matrix;
            let car_radius = self.model_radius(vehicle_id) - 0.25;
            let kinds = Kinds { vehicles: true, peds: true, ..Kinds::NONE };
            if !self.find_objects_kinda_colliding(vehicle_matrix.pos, car_radius, false, 2, kinds).is_empty() {
                continue;
            }
            let Some(car) = self.new_car(vehicle_id, RANDOM_VEHICLE, px.rng) else { continue };
            self.get_mut(car).expect("car").base.status = status::ABANDONED;
            vehicle_matrix.pos.z += car_radius - 0.6;
            self.get_mut(car).expect("car").base.matrix = vehicle_matrix;
            self.place_car_on_road_properly(car);
            self.get_mut(car).expect("car").base.flags.is_static = false;
            self.try_veh_mut(car).expect("car").door_lock = CarLock::Unlocked;
            self.join_car_with_road_system(px, car);
            {
                let v = self.try_veh_mut(car).expect("car");
                v.flags.is_locked = false;
                v.auto_pilot.car_mission = m::NONE;
                v.auto_pilot.temp_action = temp_action::NONE;
                v.auto_pilot.current_lane = 0;
                v.auto_pilot.next_lane = 0;
                v.auto_pilot.max_traffic_speed = 0.0;
                v.auto_pilot.cruise_speed = 0;
                v.flags.extended_range = true;
            }
            if VehicleData::uses_siren(vehicle_id) && px.rng.number() & 1 != 0 {
                self.try_veh_mut(car).expect("car").siren_or_alarm = true;
            }
            if self.ent(car).base.matrix.up.z > 0.94 {
                self.try_veh_mut(car).expect("car").clump_alpha = 0;
                let _ = self.add(car);
                let v = self.try_veh_mut(car).expect("car");
                v.flags.create_road_block_peds = true;
                v.roadblock_type = block_type;
                v.roadblock_node = node as i16;
            } else {
                self.delete(car);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::general::Rng;
    use crate::ped::ctx::NoPedHooks;
    use crate::ped::types::ped_type;

    #[test]
    fn init_takes_the_flagged_road_pieces_up_to_the_limit() {
        let mut rb = RoadBlocks::default();
        let mut flags = vec![0u8; 700];
        for i in (0..700).step_by(1) {
            flags[i] = USE_IN_ROADBLOCK;
        }
        rb.init(&flags, 700);
        assert_eq!((rb.objects.len(), rb.in_or_out.len()), (NUMROADBLOCKS, NUMROADBLOCKS));
        assert!(rb.in_or_out.iter().all(|&b| b));
        let mut few = vec![0u8; 10];
        few[3] = USE_IN_ROADBLOCK | OBJECT_EAST_WEST;
        few[7] = USE_IN_ROADBLOCK;
        rb.init(&few, 10);
        assert_eq!(rb.objects, vec![3, 7]);
    }

    #[test]
    fn a_site_remembers_whether_the_player_was_near_and_stays_empty_without_a_chase() {
        let mut w = World::new(crate::modelinfo::ModelInfoStore::new(re3_formats::datafile::Game::Gta3), crate::temp_col::TempColModels::new(re3_formats::datafile::Game::Gta3));
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let player = w.create_ped(&mut px, ped_type::PLAYER1, -1, None).unwrap();
        w.ped_mut(player).player = Some(Default::default());
        w.add(player).ok();
        w.ents.player_ped = Some(player);
        let mut m = Matrix::IDENTITY;
        m.pos = Vec3::new(500.0, 0.0, 0.0);
        w.ents.paths.register_map_object(1, m);
        w.ents.road_blocks.objects = vec![0];
        w.ents.road_blocks.in_or_out = vec![true];
        w.generate_road_blocks(&mut px);
        // far away: the site is marked as left
        assert!(!w.ents.road_blocks.in_or_out[0]);
        w.ent_mut_for_test(player).base.set_position(Vec3::new(480.0, 0.0, 0.0));
        w.generate_road_blocks(&mut px);
        assert!(w.ents.road_blocks.in_or_out[0]);
        assert_eq!(w.ents.pools.vehicles.used(), 0);
    }
}
