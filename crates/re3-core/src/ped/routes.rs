//! `PedRoutes.cpp` and the route and formation parts of `PedAI.cpp`.
use super::data::*;
use crate::entity::EntityRef;
use crate::world::World;
use glam::Vec3;

/// `NUMPEDROUTES`.
pub const NUM_PED_ROUTES: usize = 200;

/// `gaRoutes`: every route's points one after the other.
#[derive(Debug, Clone)]
pub struct PedRoutes {
    /// The route each point belongs to, -1 for unused.
    pub route: [i16; NUM_PED_ROUTES],
    pub pos: [Vec3; NUM_PED_ROUTES],
}

impl Default for PedRoutes {
    fn default() -> Self {
        Self { route: [-1; NUM_PED_ROUTES], pos: [Vec3::ZERO; NUM_PED_ROUTES] }
    }
}

impl PedRoutes {
    /// `CRouteNode::GetRouteThisPointIsOn`.
    pub fn route_of_point(&self, point: i16) -> i16 {
        self.route.get(point as usize).copied().unwrap_or(-1)
    }

    /// `CRouteNode::GetRouteStart`: the first point of `route`.
    pub fn route_start(&self, route: i16) -> i16 {
        self.route.iter().position(|&r| r == route).map_or(-1, |i| i as i16)
    }

    /// `CRouteNode::GetPointPosition`.
    pub fn point_position(&self, point: i16) -> Vec3 {
        self.pos.get(point as usize).copied().unwrap_or(Vec3::ZERO)
    }

    /// `CRouteNode::AddRoutePoint`.
    pub fn add_point(&mut self, route: i16, pos: Vec3) {
        if let Some(i) = self.route.iter().position(|&r| r == -1) {
            self.route[i] = route;
            self.pos[i] = pos;
        }
    }

    /// `CRouteNode::RemoveRoute`.
    pub fn remove_route(&mut self, route: i16) {
        let Some(first) = self.route.iter().position(|&r| r == route) else { return };
        let last = (first..NUM_PED_ROUTES).find(|&i| self.route[i] != route).unwrap_or(NUM_PED_ROUTES);
        let diff = last - first;
        for i in first..NUM_PED_ROUTES - diff {
            self.route[i] = self.route[i + diff];
            self.pos[i] = self.pos[i + diff];
        }
        for i in NUM_PED_ROUTES - diff..NUM_PED_ROUTES {
            self.route[i] = -1;
        }
    }
}

impl World {
    /// `CPed::SetFollowRoute`.
    pub fn set_follow_route(&mut self, r: EntityRef, current_point: i16, route_type: i16) {
        let start = self.ents.ped_routes.route_start(current_point);
        let pd = self.ped_mut(r);
        pd.route_last_point = current_point;
        pd.route_start_point = start as u16;
        pd.route_points_passed = 0;
        pd.route_type = route_type;
        pd.route_points_being_passed = 1;
        pd.objective = objective::FOLLOW_ROUTE;
        let next = self.get_next_point_on_route(r);
        let pos = self.ents.ped_routes.point_position(next as i16);
        self.ped_mut(r).next_route_point_pos = pos;
    }

    /// `CPed::GetNextPointOnRoute`.
    pub fn get_next_point_on_route(&mut self, r: EntityRef) -> i32 {
        let pd = self.ped(r);
        let mut next = i32::from(pd.route_points_being_passed) + i32::from(pd.route_points_passed) + i32::from(pd.route_start_point);
        let on_route = next >= 0 && next <= NUM_PED_ROUTES as i32 && pd.route_last_point == self.ents.ped_routes.route_of_point(next as i16);
        if !on_route {
            let pd = self.ped_mut(r);
            match pd.route_type {
                route_type::STOP_WHEN_DONE => next = -1,
                route_type::GO_BACKWARD_WHEN_DONE => {
                    pd.route_points_being_passed = -pd.route_points_being_passed;
                    next = i32::from(pd.route_points_being_passed) + i32::from(pd.route_points_passed) + i32::from(pd.route_start_point);
                }
                route_type::GO_TO_START_WHEN_DONE => {
                    pd.route_points_passed = -1;
                    next = i32::from(pd.route_points_being_passed) + i32::from(pd.route_points_passed) + i32::from(pd.route_start_point);
                }
                _ => {}
            }
        }
        next
    }

    /// `CPed::HaveReachedNextPointOnRoute`.
    pub fn have_reached_next_point_on_route(&mut self, r: EntityRef, dist_to_count_reached: f32) -> bool {
        let (pos, next) = (self.epos(r), self.ped(r).next_route_point_pos);
        if Vec3::new(next.x - pos.x, next.y - pos.y, 0.0).length() >= dist_to_count_reached {
            return false;
        }
        let pd = self.ped_mut(r);
        pd.route_points_passed += pd.route_points_being_passed;
        true
    }

    /// `CPed::SetFormation`.
    pub fn set_formation(&mut self, r: EntityRef, kind: u8) {
        self.ped_mut(r).ped_formation = kind;
    }

    /// `CPed::GetFormationPosition`: where this ped stands relative to the one it follows.
    pub fn get_formation_position(&mut self, r: EntityRef) -> Vec3 {
        let Some(mut leader) = self.ped(r).ped_in_objective else { return self.epos(r) };
        if self.ped(leader).ped_state == state::DEAD {
            match self.ped(leader).ped_in_objective {
                None => {
                    self.ped_mut(r).ped_in_objective = None;
                    return self.epos(r);
                }
                Some(next) => leader = next,
            }
            self.ped_mut(r).ped_in_objective = Some(leader);
        }
        let offset = match self.ped(r).ped_formation {
            formation::REAR => Vec3::new(0.0, -1.5, 0.0),
            formation::REAR_LEFT => Vec3::new(-1.5, -1.5, 0.0),
            formation::REAR_RIGHT => Vec3::new(1.5, -1.5, 0.0),
            formation::FRONT_LEFT => Vec3::new(-1.5, 1.5, 0.0),
            formation::FRONT_RIGHT => Vec3::new(1.5, 1.5, 0.0),
            formation::LEFT => Vec3::new(-1.5, 0.0, 0.0),
            formation::RIGHT => Vec3::new(1.5, 0.0, 0.0),
            formation::FRONT => Vec3::new(0.0, 1.5, 0.0),
            _ => Vec3::ZERO,
        };
        offset + self.epos(leader)
    }
}
