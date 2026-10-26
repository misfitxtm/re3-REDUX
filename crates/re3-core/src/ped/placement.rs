//! `PedPlacement.cpp` and the placement helpers of `PedAI.cpp`.
use super::data::*;
use crate::entity::EntityRef;
use crate::world::{Kinds, LineOpts, World};
use glam::Vec3;

impl World {
    /// `CPedPlacement::FindZCoorForPed`: stand on the floor below `pos`.
    pub fn find_z_coor_for_ped(&mut self, pos: &mut Vec3) {
        let start_z = pos.z - 100.0;
        let floor = Kinds { buildings: true, dummies: true, ..Kinds::NONE };
        let mut vec = Vec3::new(pos.x, pos.y, pos.z + 1.0);
        let first = self.process_vertical_line(vec, start_z, floor, false, None).map_or(-100.0, |h| h.point.point.z);
        // a second probe slightly aside
        vec.x += 0.1;
        vec.y += 0.1;
        let second = self.process_vertical_line(vec, start_z, floor, false, None).map_or(-100.0, |h| h.point.point.z);
        let z = first.max(second);
        if z > -99.0 {
            pos.z = FEET_OFFSET + z;
        }
    }

    /// `CPedPlacement::IsPositionClearForPed`.
    pub fn is_position_clear_for_ped(&mut self, pos: Vec3) -> bool {
        let kinds = Kinds { vehicles: true, peds: true, ..Kinds::NONE };
        self.find_objects_kinda_colliding(pos, 0.75, true, 2, kinds).is_empty()
    }

    /// `CPed::PositionPedOutOfCollision`: moves a ped that stepped out of a car into free space nearby.
    pub fn position_ped_out_of_collision(&mut self, r: EntityRef) -> bool {
        let Some(my_vehicle) = self.ped(r).my_vehicle else { return false };
        let veh_pos = self.epos(my_vehicle);
        let our_pos = self.epos(r);
        // the original keeps the best squared distances in ints, so they are truncated
        let (mut best_near, mut best_far) = (999_i32, 999_i32);
        let (mut near_vehicle, mut somewhere): (Option<Vec3>, Option<Vec3>) = (None, None);
        let mut veh_hit = None;
        let mut potential = Vec3::new(0.0, our_pos.y - 3.5, our_pos.z);
        let solid = Kinds { buildings: true, objects: true, ..Kinds::NONE };
        for _ in 0..15 {
            potential.x = our_pos.x - 3.5;
            for _ in 0..15 {
                self.find_z_coor_for_ped(&mut potential);
                let dist_vec = potential - veh_pos;
                let dist = dist_vec.length();
                // the ray is stretched past the spot to leave room for the ped
                let mult = (0.6 + dist) / dist;
                let adjusted = dist_vec * mult + veh_pos;
                if self.is_line_of_sight_clear(veh_pos, adjusted, solid, LineOpts::default()) && self.test_sphere_against_world(potential, 0.6, Some(r), solid, false).is_none() {
                    let change = (potential - our_pos).length_squared();
                    let car = self.test_sphere_against_world(potential, 0.6, Some(r), Kinds { vehicles: true, ..Kinds::NONE }, false);
                    // the original keeps whatever the last test found, even nothing
                    veh_hit = car;
                    if car.is_some() {
                        if change < best_near as f32 {
                            near_vehicle = Some(potential);
                            best_near = change as i32;
                        }
                    } else if change < best_far as f32 {
                        best_far = change as i32;
                        somewhere = Some(potential);
                    }
                }
                potential.x += 0.5;
            }
            potential.y += 0.5;
        }
        // a spot beside a car is not used for the ped itself: it might be on top of the car
        if let Some(p) = somewhere {
            self.get_mut(r).expect("ped").base.set_position(p);
            true
        } else if let (Some(mut p), Some(v)) = (near_vehicle, veh_hit) {
            p.z += self.col_model_of(v).map_or(0.0, |c| c.bounding_box.max.z);
            self.get_mut(r).expect("ped").base.set_position(p);
            true
        } else {
            false
        }
    }
}
