//! `CarCtrl.cpp`, part 3: how a car on the road system picks the next node and prepares the curve to it.
use super::car_ctrl::LANE_WIDTH;
use super::curves::calc_speed_scale_factor;
use super::pathfind::PathFind;
use super::car_traffic::limit_radian_angle;
use crate::entity::{status, EntityRef, Kind};
use crate::general::Rng;
use crate::math::atan_of_xy;
use crate::ped::ctx::Px;
use crate::vehicle::base::{car_mission as m, AutoPilot, NUM_PATH_NODES_IN_AUTOPILOT};
use crate::world::World;
use glam::{Vec2, Vec3};

pub const PATH_DIRECTION_NONE: u8 = 0;
pub const PATH_DIRECTION_STRAIGHT: u8 = 1;
pub const PATH_DIRECTION_RIGHT: u8 = 2;
pub const PATH_DIRECTION_LEFT: u8 = 4;
pub const ATTEMPTS_TO_FIND_NEXT_NODE: i32 = 15;

/// `FindPathDirection`: which way a car turns going from `prev` over `cur` to `next`.
pub fn find_path_direction(paths: &PathFind, prev: usize, cur: usize, next: usize) -> u8 {
    let p = |i: usize| Vec2::new(paths.nodes[i].pos.x, paths.nodes[i].pos.y);
    let (prev_to_cur, cur_to_next) = (p(cur) - p(prev), p(next) - p(cur));
    let dist_prev_to_cur = prev_to_cur.length();
    if dist_prev_to_cur == 0.0 {
        return PATH_DIRECTION_NONE;
    }
    // the turn is the angle between the two legs: pi/2 + a + b, with a measured from the y axis and b from the x axis
    let (sin_a, cos_a) = (prev_to_cur.x / dist_prev_to_cur, prev_to_cur.y / dist_prev_to_cur);
    let dist_cur_to_next = cur_to_next.length();
    if dist_cur_to_next == 0.0 {
        return PATH_DIRECTION_NONE;
    }
    let (sin_b, cos_b) = (cur_to_next.y / dist_cur_to_next, cur_to_next.x / dist_cur_to_next);
    let sin_direction = sin_a * sin_b - cos_a * cos_b;
    if sin_direction > 0.77 {
        PATH_DIRECTION_LEFT
    } else if sin_direction < -0.77 {
        PATH_DIRECTION_RIGHT
    } else {
        PATH_DIRECTION_STRAIGHT
    }
}

/// How a pick may change lanes after the links are advanced.
enum LaneSwitch {
    /// Random picks: a one in four chance first, then the length of the road decides.
    Random,
    /// Chases and paths: the length decides first; `allowed` is false for the far-away missions.
    Length { allowed: bool },
}

/// The tail all three pickers share: the old next link becomes the current one, the lane and the time on the curve are worked out.
/// `cur_node_pos` and `next_node_pos` are the road nodes the lane switch measures between.
#[allow(clippy::too_many_arguments)] // the locals of the original
fn advance_links(paths: &PathFind, ap: &mut AutoPilot, rng: &mut Rng, cur_node: usize, connection: usize, cur_link_index: usize, next_node_pos: Vec3, lanes: LaneSwitch) {
    let cur_link = paths.car_path_links[cur_link_index];
    let next_link = paths.car_path_links[paths.car_path_connections[connection] as usize];
    ap.time_entered_curve = ap.time_entered_curve.wrapping_add(ap.time_to_spend_on_current_curve as u32);
    ap.previous_path_node_info = ap.current_path_node_info;
    ap.current_path_node_info = ap.next_path_node_info;
    ap.previous_direction = ap.current_direction;
    ap.current_direction = ap.next_direction;
    ap.current_lane = ap.next_lane;
    ap.next_path_node_info = paths.car_path_connections[connection] as u32;
    ap.next_direction = if cur_node as i32 >= ap.next_route_node { 1 } else { -1 };
    let lanes_on_next_node = if ap.next_direction == 1 { next_link.num_left_lanes } else { next_link.num_right_lanes };
    let cur_forward = Vec2::new(ap.current_direction as f32 * cur_link.dir.x, ap.current_direction as f32 * cur_link.dir.y);
    let next_forward = Vec2::new(ap.next_direction as f32 * next_link.dir.x, ap.next_direction as f32 * next_link.dir.y);
    if lanes_on_next_node >= 0 {
        let cur_pos = paths.nodes[cur_node].pos;
        let dist_sq = Vec2::new(next_node_pos.x - cur_pos.x, next_node_pos.y - cur_pos.y).length_squared();
        let switch = match lanes {
            LaneSwitch::Random => rng.number() & 0x600 == 0 && dist_sq >= 14.0 * 14.0,
            LaneSwitch::Length { allowed } => dist_sq >= 7.0 * 7.0 && rng.number() & 0x600 == 0 && allowed,
        };
        if switch {
            if rng.true_false() {
                ap.next_lane += 1;
            } else {
                ap.next_lane -= 1;
            }
        }
        ap.next_lane = ap.next_lane.min(lanes_on_next_node - 1).max(0);
    } else {
        ap.next_lane = ap.current_lane;
    }
    if ap.stay_in_fast_lane {
        ap.next_lane = 0;
    }
    let on_cur = Vec3::new(
        cur_link.pos.x + ((ap.current_lane as f32 + cur_link.one_way_lane_offset()) * LANE_WIDTH) * cur_forward.y,
        cur_link.pos.y - ((ap.current_lane as f32 + cur_link.one_way_lane_offset()) * LANE_WIDTH) * cur_forward.x,
        0.0,
    );
    let on_next = Vec3::new(
        next_link.pos.x + ((ap.next_lane as f32 + next_link.one_way_lane_offset()) * LANE_WIDTH) * next_forward.y,
        next_link.pos.y - ((ap.next_lane as f32 + next_link.one_way_lane_offset()) * LANE_WIDTH) * next_forward.x,
        0.0,
    );
    let dir_cur = Vec2::new(cur_link.dir.x * ap.current_direction as f32, cur_link.dir.y * ap.current_direction as f32);
    let dir_next = Vec2::new(next_link.dir.x * ap.next_direction as f32, next_link.dir.y * ap.next_direction as f32);
    ap.time_to_spend_on_current_curve = ((calc_speed_scale_factor(on_cur, on_next, dir_cur, dir_next) * (1000.0 / ap.max_traffic_speed)) as i32).max(10);
}

/// `PickNextNodeRandomly`: true if the car had to turn round.
pub fn pick_next_node_randomly(paths: &PathFind, ap: &mut AutoPilot, rng: &mut Rng) -> bool {
    let prev_node = ap.current_route_node as usize;
    let cur_node = ap.next_route_node as usize;
    let total_links = paths.nodes[cur_node].num_links as u32;
    let cur_link_index = ap.next_path_node_info as usize;
    let cur_link = paths.car_path_links[cur_link_index];
    // FIX_BUGS: the lanes on the way the car comes from
    let lanes_on_current_path = if cur_link.path_node_index as usize == cur_node { cur_link.num_left_lanes } else { cur_link.num_right_lanes } as i32;
    let mut allowed = PATH_DIRECTION_NONE;
    let next_lane = ap.next_lane as i32;
    if next_lane == 0 {
        // the leftmost lane may always turn left
        allowed |= PATH_DIRECTION_LEFT;
    }
    if next_lane == lanes_on_current_path - 1 {
        allowed |= PATH_DIRECTION_RIGHT;
    }
    if lanes_on_current_path < 3 || allowed == PATH_DIRECTION_NONE {
        allowed |= PATH_DIRECTION_STRAIGHT;
    }
    ap.prev_route_node = ap.current_route_node;
    ap.current_route_node = ap.next_route_node;
    let (prev_pn, cur_pn) = (paths.nodes[prev_node], paths.nodes[cur_node]);
    let first = cur_pn.first_link as usize;
    let against = |l: &super::pathfind::CarPathLink| if l.path_node_index as usize == cur_node { l.num_right_lanes == 0 } else { l.num_left_lanes == 0 };
    let mut next_link = 0usize;
    let mut going_against = false;
    let mut direction = PATH_DIRECTION_NONE;
    let mut attempt = 0;
    while attempt < ATTEMPTS_TO_FIND_NEXT_NODE {
        if attempt != 0 && ap.next_route_node as usize != prev_node && direction & allowed != 0 {
            let n = paths.nodes[ap.next_route_node as usize];
            if (!n.dead_end || prev_pn.dead_end) && (!n.disabled || prev_pn.disabled) && (!n.between_levels || prev_pn.between_levels || !ap.stay_in_current_level) && !going_against {
                break;
            }
        }
        next_link = (rng.number() as u32 % total_links) as usize;
        ap.next_route_node = paths.connected_node(next_link + first) as i32;
        direction = find_path_direction(paths, prev_node, cur_node, ap.next_route_node as usize);
        going_against = against(&paths.car_path_links[paths.car_path_connections[next_link + first] as usize]);
        attempt += 1;
    }
    if attempt >= ATTEMPTS_TO_FIND_NEXT_NODE {
        // nothing fit: dead ends and the lane limit no longer count
        attempt = 0;
        while attempt < ATTEMPTS_TO_FIND_NEXT_NODE {
            if attempt != 0 && ap.next_route_node as usize != prev_node {
                let n = paths.nodes[ap.next_route_node as usize];
                if (!n.disabled || prev_pn.disabled) && (!n.between_levels || prev_pn.between_levels || !ap.stay_in_current_level) && !going_against {
                    break;
                }
            }
            next_link = (rng.number() as u32 % total_links) as usize;
            ap.next_route_node = paths.connected_node(next_link + first) as i32;
            going_against = against(&paths.car_path_links[paths.car_path_connections[next_link + first] as usize]);
            attempt += 1;
        }
    }
    if attempt >= ATTEMPTS_TO_FIND_NEXT_NODE {
        // still nothing: no U-turn limit and no randomness
        let mut found = false;
        for l in 0..total_links as usize {
            ap.next_route_node = paths.connected_node(l + first) as i32;
            going_against = against(&paths.car_path_links[paths.car_path_connections[l + first] as usize]);
            next_link = l;
            if !going_against {
                let n = paths.nodes[ap.next_route_node as usize];
                if (!n.disabled || prev_pn.disabled) && (!n.between_levels || prev_pn.between_levels || !ap.stay_in_current_level) {
                    // the original sets its loop counter to 1000 here and then indexes with it; the link found is kept instead
                    found = true;
                    break;
                }
            }
        }
        if !found {
            // everything failed: turn round
            ap.next_route_node = prev_node as i32;
            next_link = next_link.min(total_links as usize - 1);
        }
    }
    let turned_round = prev_node as i32 == ap.next_route_node;
    let next_node_pos = paths.nodes[ap.next_route_node as usize].pos;
    advance_links(paths, ap, rng, cur_node, next_link + first, cur_link_index, next_node_pos, LaneSwitch::Random);
    turned_round
}

/// `PickNextNodeToChaseCar`: the node that leads towards `target`, or failing that the one pointing most at it.
/// `target_node` is the road node closest to the target if it stands on a road piece, else -1.
pub fn pick_next_node_to_chase_car(paths: &mut PathFind, ap: &mut AutoPilot, rng: &mut Rng, car_pos: Vec3, target: Vec3, target_node: i32) {
    let prev_node = ap.current_route_node as usize;
    let cur_node = ap.next_route_node as usize;
    let cur_pn = paths.nodes[cur_node];
    let (found, _) = paths.do_path_search(0, cur_pn.pos, cur_node as i32, target, 1, None, 999999.9, target_node);
    let first = cur_pn.first_link as usize;
    let (new_next_node, next_link);
    if found.len() != 1 || found[0] == cur_node {
        let current_angle = atan_of_xy(target.x - car_pos.x, target.y - car_pos.y);
        let mut link = 0;
        let mut lowest = 10.0f32;
        let mut node = 0usize;
        for i in 0..cur_pn.num_links as usize {
            let con = paths.connected_node(i + first);
            if con == prev_node && i > 1 {
                continue;
            }
            let t = paths.nodes[con].pos;
            let angle = limit_radian_angle(atan_of_xy(t.x - cur_pn.pos.x, t.y - cur_pn.pos.y) - current_angle).abs();
            if angle < lowest {
                lowest = angle;
                node = con;
                link = i;
            }
        }
        new_next_node = node;
        next_link = link;
    } else {
        new_next_node = found[0];
        next_link = (0..cur_pn.num_links as usize).find(|&i| paths.connected_node(i + first) == new_next_node).unwrap_or(0);
    }
    // the original reads the next node before it is replaced, so the lane switch measures a distance of zero
    let next_node_pos = paths.nodes[ap.next_route_node as usize].pos;
    let cur_link_index = ap.next_path_node_info as usize;
    ap.prev_route_node = ap.current_route_node;
    ap.current_route_node = ap.next_route_node;
    ap.next_route_node = new_next_node as i32;
    let allowed = !matches!(ap.car_mission, m::RAMPLAYER_FARAWAY | m::BLOCKPLAYER_FARAWAY | m::RAMCAR_FARAWAY | m::BLOCKCAR_FARAWAY);
    advance_links(paths, ap, rng, cur_node, next_link + first, cur_link_index, next_node_pos, LaneSwitch::Length { allowed });
}

/// `PickNextNodeToFollowPath`: true if there is no route to the destination.
pub fn pick_next_node_to_follow_path(paths: &mut PathFind, ap: &mut AutoPilot, rng: &mut Rng, car_pos: Vec3) -> bool {
    let cur_node = ap.next_route_node as usize;
    if ap.path_find_nodes_count == 0 {
        let (nodes, _) = paths.do_path_search(0, car_pos, cur_node as i32, ap.destination_coors, NUM_PATH_NODES_IN_AUTOPILOT, None, 999999.9, -1);
        ap.path_find_nodes_count = nodes.len() as i16;
        for (i, n) in nodes.iter().enumerate() {
            ap.path_find_nodes[i] = *n as i32;
        }
        if ap.path_find_nodes_count < 1 {
            return true;
        }
    }
    let next_node_pos = paths.nodes[ap.next_route_node as usize].pos;
    let cur_link_index = ap.next_path_node_info as usize;
    ap.prev_route_node = ap.current_route_node;
    ap.current_route_node = ap.next_route_node;
    ap.next_route_node = ap.path_find_nodes[0];
    ap.remove_one_path_node();
    let cur_pn = paths.nodes[cur_node];
    let first = cur_pn.first_link as usize;
    let next_link = (0..cur_pn.num_links as usize).find(|&i| paths.connected_node(i + first) as i32 == ap.next_route_node).unwrap_or(0);
    advance_links(paths, ap, rng, cur_node, next_link + first, cur_link_index, next_node_pos, LaneSwitch::Length { allowed: true });
    false
}

/// `FindLinksToGoWithTheseNodes`: the road links the car is on and goes to, given the route nodes it has.
pub fn find_links_to_go_with_these_nodes(paths: &PathFind, ap: &mut AutoPilot, rng: &mut Rng) {
    let cur = paths.nodes[ap.current_route_node as usize];
    let first = cur.first_link as usize;
    let next_link = (0..12).find(|&l| paths.connected_node(l + first) as i32 == ap.next_route_node).unwrap_or(12);
    ap.next_path_node_info = paths.car_path_connections[next_link + first] as u32;
    ap.next_direction = if ap.current_route_node >= ap.next_route_node { 1 } else { -1 };
    let (cur_link, cur_connection);
    if cur.num_links == 1 {
        cur_link = 0;
        cur_connection = paths.car_path_connections[first] as u32;
    } else {
        // any link but the one the car goes to
        let (link, connection) = loop {
            let link = (rng.number() as u32 % cur.num_links as u32) as usize;
            let connection = paths.car_path_connections[link + first] as u32;
            if connection != ap.next_path_node_info {
                break (link, connection);
            }
        };
        cur_link = link;
        cur_connection = connection;
    }
    ap.current_path_node_info = cur_connection;
    ap.current_direction = if paths.connected_node(cur_link + first) as i32 >= ap.current_route_node { 1 } else { -1 };
}

impl World {
    /// `PickNextNodeAccordingStrategy`.
    pub fn pick_next_node_according_strategy(&mut self, px: &mut Px, r: EntityRef) -> bool {
        let Some(v) = self.try_veh(r) else { return false };
        let mut ap = v.auto_pilot;
        let car_pos = self.epos(r);
        let mut paths = std::mem::take(&mut self.ents.paths);
        let mut result = false;
        let mut turned = false;
        match ap.car_mission {
            m::RAMPLAYER_FARAWAY | m::BLOCKPLAYER_FARAWAY => {
                let target = self.find_player_vehicle();
                let (pos, node) = self.chase_target(&paths, target.or(self.ents.player_ped));
                pick_next_node_to_chase_car(&mut paths, &mut ap, px.rng, car_pos, pos, node);
            }
            m::RAMCAR_FARAWAY | m::BLOCKCAR_FARAWAY => {
                let (pos, node) = self.chase_target(&paths, ap.target_car);
                pick_next_node_to_chase_car(&mut paths, &mut ap, px.rng, car_pos, pos, node);
            }
            m::GOTOCOORDS | m::GOTOCOORDS_ACCURATE => result = pick_next_node_to_follow_path(&mut paths, &mut ap, px.rng, car_pos),
            _ => turned = pick_next_node_randomly(&paths, &mut ap, px.rng),
        }
        self.ents.paths = paths;
        if let Some(v) = self.try_veh_mut(r) {
            v.auto_pilot = ap;
        }
        if turned {
            // a car that has to turn round can no longer be shifted along the rails
            self.get_mut(r).expect("vehicle").base.status = status::PHYSICS;
            self.switch_vehicle_to_real_physics(r);
        }
        result
    }

    /// `FindLinksToGoWithTheseNodes`.
    pub fn find_links_to_go_with_these_nodes(&mut self, px: &mut Px, r: EntityRef) {
        let Some(v) = self.try_veh(r) else { return };
        let mut ap = v.auto_pilot;
        find_links_to_go_with_these_nodes(&self.ents.paths, &mut ap, px.rng);
        self.try_veh_mut(r).expect("vehicle").auto_pilot = ap;
    }

    /// `JoinCarWithRoadSystem`: puts the car on the road system at the road piece nearest to where it points.
    pub fn join_car_with_road_system(&mut self, px: &mut Px, r: EntityRef) {
        let (pos, fwd) = {
            let e = self.ent(r);
            (e.base.position(), e.base.matrix.forward)
        };
        {
            let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
            ap.prev_route_node = 0;
            ap.current_route_node = 0;
            ap.next_route_node = 0;
            ap.current_path_node_info = 0;
            ap.previous_path_node_info = 0;
            ap.next_path_node_info = 0;
        }
        let paths = &self.ents.paths;
        let node_id = paths.find_node_closest_to_coors_favour_direction(pos, 0, fwd.x, fwd.y);
        if node_id < 0 {
            return;
        }
        let mut node = node_id;
        let n = paths.nodes[node as usize];
        let (mut prev, mut min_dist) = (-1i32, 999999.9f32);
        for i in 0..n.num_links as usize {
            let cand = paths.connected_node(i + n.first_link as usize);
            let d = Vec2::new(paths.nodes[cand].pos.x - n.pos.x, paths.nodes[cand].pos.y - n.pos.y).length();
            if d < min_dist {
                min_dist = d;
                prev = cand as i32;
            }
        }
        if prev < 0 {
            return;
        }
        let mut forward = Vec2::new(fwd.x, fwd.y);
        let prev_pos = paths.nodes[prev as usize].pos;
        if forward == Vec2::ZERO {
            forward.x = 1.0;
        }
        if Vec2::new(n.pos.x - prev_pos.x, n.pos.y - prev_pos.y).dot(forward) < 0.0 {
            std::mem::swap(&mut prev, &mut node);
        }
        let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
        ap.prev_route_node = 0;
        ap.current_route_node = prev;
        ap.next_route_node = node;
        ap.path_find_nodes_count = 0;
        self.find_links_to_go_with_these_nodes(px, r);
        let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
        ap.next_lane = 0;
        ap.current_lane = 0;
    }

    /// `JoinCarWithRoadSystemGotoCoors`: true if no route was found.
    pub fn join_car_with_road_system_goto_coors(&mut self, px: &mut Px, r: EntityRef, target: Vec3, _is_proper_now: bool) -> bool {
        let pos = self.epos(r);
        self.try_veh_mut(r).expect("vehicle").auto_pilot.destination_coors = target;
        let (mut nodes, _) = self.ents.paths.do_path_search(0, pos, -1, target, crate::vehicle::base::NUM_PATH_NODES_IN_AUTOPILOT, None, 999999.9, -1);
        self.ents.paths.remove_bad_start_node(pos, &mut nodes);
        let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
        ap.path_find_nodes_count = nodes.len() as i16;
        for (i, n) in nodes.iter().enumerate() {
            ap.path_find_nodes[i] = *n as i32;
        }
        if nodes.len() < 2 {
            ap.prev_route_node = 0;
            ap.current_route_node = 0;
            ap.next_route_node = 0;
            return true;
        }
        ap.prev_route_node = 0;
        ap.current_route_node = ap.path_find_nodes[0];
        ap.remove_one_path_node();
        ap.next_route_node = ap.path_find_nodes[0];
        ap.remove_one_path_node();
        self.find_links_to_go_with_these_nodes(px, r);
        let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
        ap.next_lane = 0;
        ap.current_lane = 0;
        false
    }

    /// Where a chasing car is heading, and the road node nearest to it if it stands on a road piece.
    fn chase_target(&self, paths: &PathFind, target: Option<EntityRef>) -> (Vec3, i32) {
        let Some(t) = target else { return (Vec3::ZERO, -1) };
        let pos = self.epos(t);
        let ground = self.try_veh(t).and_then(|v| v.cur_ground_entity);
        if let Some(Kind::Treadable(td)) = ground.and_then(|g| self.get(g)).map(|e| &e.kind)
            && td.node_indices[0][0] >= 0
        {
            let mut closest = -1;
            let mut min_dist = 100000.0f32;
            for &n in &td.node_indices[0] {
                if n < 0 {
                    break;
                }
                let d = (paths.nodes[n as usize].pos - pos).length();
                if d < min_dist {
                    min_dist = d;
                    closest = n as i32;
                }
            }
            return (pos, closest);
        }
        (pos, -1)
    }
}

#[cfg(test)]
/// Three nodes in a line along y, 50 apart, a two-lane road each way.
pub(crate) fn test_road_line() -> PathFind {
    let mut p = PathFind::default();
    for (i, y) in [0.0, 50.0, 100.0].into_iter().enumerate() {
        p.nodes[i].pos = glam::Vec3::new(0.0, y, 0.0);
    }
    let layout = [(0, 1), (1, 2), (3, 1)];
    for (i, (first, num)) in layout.into_iter().enumerate() {
        p.nodes[i].first_link = first;
        p.nodes[i].num_links = num;
    }
    p.connections[..4].copy_from_slice(&[1, 0, 2, 1]);
    p.car_path_connections[..4].copy_from_slice(&[0, 0, 1, 1]);
    p.car_path_links[0] = crate::control::pathfind::CarPathLink { pos: glam::Vec2::new(0.0, 25.0), dir: glam::Vec2::Y, path_node_index: 0, num_left_lanes: 2, num_right_lanes: 2, ..Default::default() };
    p.car_path_links[1] = crate::control::pathfind::CarPathLink { pos: glam::Vec2::new(0.0, 75.0), dir: glam::Vec2::Y, path_node_index: 1, num_left_lanes: 2, num_right_lanes: 2, ..Default::default() };
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pilot(prev: i32, next: i32, link: u32) -> AutoPilot {
        let mut ap = AutoPilot::new(0);
        (ap.current_route_node, ap.next_route_node, ap.next_path_node_info) = (prev, next, link);
        ap.max_traffic_speed = 10.0;
        ap
    }

    #[test]
    fn a_car_carries_on_along_the_road_it_is_on() {
        let p = test_road_line();
        let mut ap = pilot(0, 1, 0);
        let turned = pick_next_node_randomly(&p, &mut ap, &mut Rng::default());
        assert!(!turned);
        assert_eq!((ap.prev_route_node, ap.current_route_node, ap.next_route_node, ap.next_path_node_info), (0, 1, 2, 1));
        assert!(ap.time_to_spend_on_current_curve >= 10);
        assert_eq!(ap.current_path_node_info, 0);
    }

    #[test]
    fn a_dead_end_turns_the_car_round() {
        let p = test_road_line();
        let mut ap = pilot(1, 2, 1);
        assert!(pick_next_node_randomly(&p, &mut ap, &mut Rng::default()));
        assert_eq!((ap.current_route_node, ap.next_route_node), (2, 1));
    }

    #[test]
    fn a_path_is_followed_node_by_node() {
        let mut p = test_road_line();
        let mut ap = pilot(0, 1, 0);
        ap.destination_coors = Vec3::new(0.0, 100.0, 0.0);
        ap.path_find_nodes_count = 1;
        ap.path_find_nodes[0] = 2;
        assert!(!pick_next_node_to_follow_path(&mut p, &mut ap, &mut Rng::default(), Vec3::new(0.0, 40.0, 0.0)));
        assert_eq!((ap.current_route_node, ap.next_route_node, ap.path_find_nodes_count), (1, 2, 0));
    }

    #[test]
    fn turns_are_told_from_the_angle_between_the_legs() {
        let mut p = test_road_line();
        p.nodes[2].pos = Vec3::new(50.0, 50.0, 0.0);
        assert_eq!(find_path_direction(&p, 0, 1, 2), PATH_DIRECTION_RIGHT);
        p.nodes[2].pos = Vec3::new(-50.0, 50.0, 0.0);
        assert_eq!(find_path_direction(&p, 0, 1, 2), PATH_DIRECTION_LEFT);
        p.nodes[2].pos = Vec3::new(0.0, 100.0, 0.0);
        assert_eq!(find_path_direction(&p, 0, 1, 2), PATH_DIRECTION_STRAIGHT);
        assert_eq!(find_path_direction(&p, 1, 1, 2), PATH_DIRECTION_NONE);
    }
}
