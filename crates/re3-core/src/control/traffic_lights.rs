//! `TrafficLights.cpp`: the light cycles and which cars have to stop for them.
use super::pathfind::PathFind;
use crate::entity::{status, EntityRef, Kind};
use crate::math::atan_of_xy;
use crate::vehicle::base::AutoPilot;
use crate::world::{World, DUMMIES};
use glam::Vec2;

/// `eCarLights`.
pub mod car_light {
    pub const GREEN: u8 = 0;
    pub const YELLOW: u8 = 1;
    pub const RED: u8 = 2;
}

/// `ePedLights`.
pub mod ped_light {
    pub const DONT_WALK: u8 = 0;
    pub const WALK: u8 = 1;
    pub const WALK_BLINK: u8 = 2;
}

/// The unknown flag the original puts on a link's light type when the light faces the other way.
pub const SOME_FLAG: u8 = 0x80;

/// `LightForPeds`.
pub fn light_for_peds(time_ms: u32) -> u8 {
    let period = time_ms % 16384;
    if period < 12000 {
        ped_light::DONT_WALK
    } else if period < 16384 - 1000 {
        ped_light::WALK
    } else {
        ped_light::WALK_BLINK
    }
}

/// `LightForCars1`.
pub fn light_for_cars1(time_ms: u32) -> u8 {
    let period = time_ms % 16384;
    if period < 5000 {
        car_light::GREEN
    } else if period < 5000 + 1000 {
        car_light::YELLOW
    } else {
        car_light::RED
    }
}

/// `LightForCars2`.
pub fn light_for_cars2(time_ms: u32) -> u8 {
    let period = time_ms % 16384;
    if period < 6000 {
        car_light::RED
    } else if period < 12000 - 1000 {
        car_light::GREEN
    } else if period < 12000 {
        car_light::YELLOW
    } else {
        car_light::RED
    }
}

/// `FindTrafficLightType`: which of the two cycles a light on the given heading follows.
pub fn find_traffic_light_type(forward: Vec2) -> u8 {
    let orientation = atan_of_xy(forward.x, forward.y).to_degrees();
    if (orientation > 60.0 && orientation < 60.0 + 90.0) || (orientation > 240.0 && orientation < 240.0 + 90.0) {
        1
    } else {
        2
    }
}

/// One of the three checks of `ShouldCarStopForLight`: the link, the route node it is measured with, the direction, and how close counts.
#[allow(clippy::too_many_arguments)] // the locals of the original
fn stops_for_link(paths: &PathFind, link: usize, route_node: i32, direction: i8, pos: Vec2, always_stop: bool, time_ms: u32, near: f32) -> bool {
    let l = paths.car_path_links[link];
    let ty = l.traffic_light_type;
    if ty == 0 {
        return false;
    }
    let flagged = ty & SOME_FLAG != 0;
    let facing = l.path_node_index as i32 == route_node;
    if !((flagged || facing) && (!flagged || !facing)) {
        return false;
    }
    let base = ty & !SOME_FLAG;
    if !(always_stop || (base == 1 && light_for_cars1(time_ms) != car_light::GREEN) || (base == 2 && light_for_cars2(time_ms) != car_light::GREEN)) {
        return false;
    }
    let dist = (pos - l.pos).dot(l.dir);
    if direction == -1 { dist > 0.0 && dist < near } else { dist < 0.0 && dist > -near }
}

/// `ShouldCarStopForLight`: whether the car's links put a red light just ahead of it.
pub fn should_car_stop_for_light(paths: &PathFind, ap: &AutoPilot, car_status: u8, pos: Vec2, always_stop: bool, time_ms: u32) -> bool {
    if stops_for_link(paths, ap.next_path_node_info as usize, ap.next_route_node, ap.next_direction, pos, always_stop, time_ms, 8.0) {
        return true;
    }
    if stops_for_link(paths, ap.current_path_node_info as usize, ap.current_route_node, ap.current_direction, pos, always_stop, time_ms, 8.0) {
        return true;
    }
    car_status == status::PHYSICS && stops_for_link(paths, ap.previous_path_node_info as usize, ap.prev_route_node, ap.previous_direction, pos, always_stop, time_ms, 6.0)
}

/// `ShouldCarStopForBridge`: the next link is under the bridge lights and the current one is not.
pub fn should_car_stop_for_bridge(paths: &PathFind, ap: &AutoPilot) -> bool {
    paths.car_path_links[ap.next_path_node_info as usize].bridge_lights && !paths.car_path_links[ap.current_path_node_info as usize].bridge_lights
}

impl World {
    /// `CTrafficLights::ShouldCarStopForLight`.
    pub fn should_car_stop_for_light(&self, r: EntityRef, always_stop: bool) -> bool {
        let (Some(v), Some(e)) = (self.try_veh(r), self.get(r)) else { return false };
        let p = e.base.position();
        should_car_stop_for_light(&self.ents.paths, &v.auto_pilot, e.base.status, Vec2::new(p.x, p.y), always_stop, self.now())
    }

    /// `CTrafficLights::ShouldCarStopForBridge`.
    pub fn should_car_stop_for_bridge(&self, r: EntityRef) -> bool {
        self.try_veh(r).is_some_and(|v| should_car_stop_for_bridge(&self.ents.paths, &v.auto_pilot))
    }

    /// `CTrafficLights::ScanForLightsOnMap`: marks the road links and the pedestrian crossings the map's traffic lights control.
    pub fn scan_for_lights_on_map(&mut self) {
        let light_model = self.ents.indices.traffic_lights();
        let g = self.grid;
        let mut lights: Vec<(Vec2, Vec2, Vec2, f32)> = vec![];
        for y in 0..g.num {
            for x in 0..g.num {
                let Some(s) = self.sector(x, y) else { continue };
                for &d in &s.lists[DUMMIES] {
                    let Some(e) = self.get(d) else { continue };
                    if e.base.model_index != light_model || !matches!(e.kind, Kind::Dummy) {
                        continue;
                    }
                    let (p, m) = (e.base.position(), e.base.matrix);
                    lights.push((Vec2::new(p.x, p.y), Vec2::new(m.forward.x, m.forward.y), Vec2::new(m.right.x, m.right.y), p.z));
                }
            }
        }
        let paths = &mut self.ents.paths;
        for (pos, fwd, right, z) in lights {
            // cars
            for i in 0..paths.num_car_path_links {
                let d = paths.car_path_links[i].pos - pos;
                let dot_y = d.dot(fwd).abs();
                let dot_x = d.dot(right);
                if dot_x < 0.0 && dot_x > -15.0 && dot_y < 3.0 {
                    let dz = paths.nodes[paths.car_path_links[i].path_node_index as usize].pos.z - z;
                    if dz < 15.0 {
                        paths.car_path_links[i].traffic_light_type = find_traffic_light_type(fwd);
                        // the two nodes whose connections use this link
                        let (mut n1, mut n2) = (-1i32, -1i32);
                        for j in 0..paths.num_path_nodes {
                            for l in 0..paths.nodes[j].num_links as usize {
                                if paths.car_path_connections[paths.nodes[j].first_link as usize + l] as usize == i {
                                    if n1 == -1 {
                                        n1 = j as i32;
                                    } else {
                                        n2 = j as i32;
                                    }
                                }
                            }
                        }
                        // the original indexes with -1 when a link has fewer than two nodes
                        if n1 >= 0 && n2 >= 0 {
                            if paths.nodes[n1 as usize].num_links <= paths.nodes[n2 as usize].num_links {
                                n1 = n2;
                            }
                            if paths.car_path_links[i].path_node_index as i32 != n1 {
                                paths.car_path_links[i].traffic_light_type |= SOME_FLAG;
                            }
                        }
                    }
                }
            }
            // peds
            for i in paths.num_car_path_nodes..paths.num_path_nodes {
                let n = paths.nodes[i];
                let dist1 = (n.pos.x - pos.x).abs() + (n.pos.y - pos.y).abs();
                if dist1 < 50.0 {
                    for l in 0..n.num_links as usize {
                        let j = n.first_link as usize + l;
                        if paths.connection_crosses_road(j) {
                            let k = paths.connected_node(j);
                            let dist2 = (paths.nodes[k].pos.x - pos.x).abs() + (paths.nodes[k].pos.y - pos.y).abs();
                            if dist1 < 15.0 || dist2 < 15.0 {
                                paths.connection_set_traffic_light(j);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::pathfind::CarPathLink;

    #[test]
    fn the_cycles_repeat_every_16_seconds_and_the_two_directions_alternate() {
        assert_eq!((light_for_cars1(0), light_for_cars1(5500), light_for_cars1(8000)), (car_light::GREEN, car_light::YELLOW, car_light::RED));
        assert_eq!((light_for_cars2(0), light_for_cars2(7000), light_for_cars2(11500), light_for_cars2(13000)), (car_light::RED, car_light::GREEN, car_light::YELLOW, car_light::RED));
        assert_eq!(light_for_cars1(16384), car_light::GREEN);
        // when one way is green the other is red
        assert!(light_for_cars1(2000) == car_light::GREEN && light_for_cars2(2000) == car_light::RED);
        assert_eq!((light_for_peds(0), light_for_peds(12500), light_for_peds(16000)), (ped_light::DONT_WALK, ped_light::WALK, ped_light::WALK_BLINK));
    }

    #[test]
    fn a_light_picks_its_cycle_from_its_heading() {
        // the angle counts counter-clockwise from +x: 60..150 and 240..330 degrees are cycle 1
        assert_eq!(find_traffic_light_type(Vec2::new(0.0, 1.0)), 1);
        assert_eq!(find_traffic_light_type(Vec2::new(0.0, -1.0)), 1);
        assert_eq!(find_traffic_light_type(Vec2::new(1.0, 0.0)), 2);
        assert_eq!(find_traffic_light_type(Vec2::new(-1.0, 0.0)), 2);
    }

    fn road_with_light(light_type: u8) -> PathFind {
        let mut p = crate::control::car_nodes::test_road_line();
        p.car_path_links[1].traffic_light_type = light_type;
        p
    }

    #[test]
    fn a_car_stops_for_a_red_light_just_ahead_but_not_a_green_one() {
        let p = road_with_light(1);
        let mut ap = AutoPilot::new(0);
        ap.next_path_node_info = 1;
        ap.current_path_node_info = 0;
        ap.next_route_node = 1; // the node of the link: the light faces the car
        ap.next_direction = 1;
        // the link sits at (0,75) heading +y; 5 m before it
        let near = Vec2::new(0.0, 70.0);
        assert!(should_car_stop_for_light(&p, &ap, status::SIMPLE, near, false, 8000), "cycle 1 is red at 8 s");
        assert!(!should_car_stop_for_light(&p, &ap, status::SIMPLE, near, false, 1000), "and green at 1 s");
        assert!(should_car_stop_for_light(&p, &ap, status::SIMPLE, near, true, 1000), "unless it always stops");
        // far from the light, or already past it
        assert!(!should_car_stop_for_light(&p, &ap, status::SIMPLE, Vec2::new(0.0, 50.0), false, 8000));
        assert!(!should_car_stop_for_light(&p, &ap, status::SIMPLE, Vec2::new(0.0, 80.0), false, 8000));
        // a link with no light never stops anything
        let none = road_with_light(0);
        assert!(!should_car_stop_for_light(&none, &ap, status::SIMPLE, near, true, 8000));
    }

    #[test]
    fn the_bridge_lights_stop_a_car_entering_the_bridge() {
        let mut p = crate::control::car_nodes::test_road_line();
        let mut ap = AutoPilot::new(0);
        (ap.next_path_node_info, ap.current_path_node_info) = (1, 0);
        assert!(!should_car_stop_for_bridge(&p, &ap));
        p.car_path_links[1] = CarPathLink { bridge_lights: true, ..p.car_path_links[1] };
        assert!(should_car_stop_for_bridge(&p, &ap));
        p.car_path_links[0] = CarPathLink { bridge_lights: true, ..p.car_path_links[0] };
        assert!(!should_car_stop_for_bridge(&p, &ap));
    }
}
