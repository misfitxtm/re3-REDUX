//! `CPlane`: the scripted aircraft that follow flight paths (the airport jumbos, the Dodos,
//! the drug-run Cesna). Paths, the animation of the jumbo and the mission state.
use super::base::{VehicleData, VehicleType};
use crate::entity::physical::Physical;
use crate::entity::EntityRef;
use crate::general::Rng;
use glam::Vec3;

pub const CRUISE_SPEED: f32 = 50.0;
pub const TAXI_SPEED: f32 = 5.0;

/// `CPlaneNode`.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlaneNode {
    pub p: Vec3,
    /// xy distance from the start of the path.
    pub t: f32,
    pub on_ground: bool,
}

/// A loaded flight path.
#[derive(Debug, Clone, Default)]
pub struct FlightPath {
    pub nodes: Vec<PlaneNode>,
    pub total_length: f32,
}

/// Where along a path a position falls.
#[derive(Debug, Clone, Copy)]
pub struct PathPoint {
    pub cur: usize,
    pub next: usize,
    pub pos: Vec3,
    pub both_on_ground_segment: bool,
}

impl FlightPath {
    /// `LoadPath`: nodes from the file, the cumulative xy length as `t`. A non-looping path has
    /// no closing segment.
    pub fn load(points: &[[f32; 3]], looped: bool) -> Self {
        let mut nodes: Vec<PlaneNode> = points.iter().map(|p| PlaneNode { p: Vec3::from(*p), t: 0.0, on_ground: false }).collect();
        let n = nodes.len();
        let mut total = 0.0;
        for i in 0..n {
            nodes[i].t = total;
            let d = nodes[(i + 1) % n].p - nodes[i].p;
            let mut l = (d.x * d.x + d.y * d.y).sqrt();
            if !looped && i == n - 1 {
                l = 0.0;
            }
            total += l;
        }
        Self { nodes, total_length: total }
    }

    /// The segment `(cur, next)` containing `position`, searching forward from `start`;
    /// the last node's segment ends at the total length.
    fn segment(&self, start: usize, position: f32) -> (usize, usize, f32, f32) {
        let n = self.nodes.len();
        let bounds = |cur: usize| {
            let next = cur + 1;
            let p1 = self.nodes[cur].t;
            if next < n { (next, p1, self.nodes[next].t) } else { (0, p1, self.total_length) }
        };
        let mut cur = start;
        let (mut next, mut p1, mut p2) = bounds(cur);
        while position < p1 || position > p2 {
            cur = (cur + 1) % n;
            (next, p1, p2) = bounds(cur);
        }
        (cur, next, p1, p2)
    }

    /// Interpolated point at `position`, with the cursor to continue from.
    pub fn point_at(&self, start: usize, position: f32) -> PathPoint {
        let (cur, next, _, _) = self.segment(start, position);
        let mut dist = self.nodes[next].t - self.nodes[cur].t;
        if dist < 0.0 {
            dist += self.total_length;
        }
        let f = (position - self.nodes[cur].t) / dist;
        PathPoint { cur, next, pos: (1.0 - f) * self.nodes[cur].p + f * self.nodes[next].p, both_on_ground_segment: self.nodes[cur].on_ground && self.nodes[next].on_ground }
    }
}

/// `CPlaneInterpolationLine`: one phase of the jumbo's timetable.
#[derive(Debug, Clone, Copy, Default)]
pub struct InterpolationLine {
    pub kind: u8,
    pub time: f32,
    pub position: f32,
    pub speed: f32,
    pub acceleration: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CesnaStatus {
    #[default]
    None,
    Flying,
    Destroyed,
    Landed,
}

/// The statics of `Plane.cpp`.
#[derive(Debug, Clone, Default)]
pub struct PlaneManager {
    /// Jumbo, Dodo, drug-run Cesna and drop-off Cesna paths.
    pub paths: [FlightPath; 4],
    pub durations: [f32; 4],
    pub landing_point: f32,
    pub take_off_point: f32,
    pub line_bits: [InterpolationLine; 6],
    pub path_position: [f32; 3],
    pub old_path_position: [f32; 3],
    pub path_speed: [f32; 3],
    pub path2_position: [f32; 3],
    pub path2_speed: [f32; 3],
    pub path3_position: f32,
    pub path3_speed: f32,
    pub path4_position: f32,
    pub path4_speed: f32,
    pub cesna_status: CesnaStatus,
    pub cesna_start_time: u32,
    pub drug_run_cesna: Option<EntityRef>,
    pub drop_off_status: CesnaStatus,
    pub drop_off_start_time: u32,
    pub drop_off_cesna: Option<EntityRef>,
    pub frame_gen: i32,
}

impl PlaneManager {
    /// Works out the take-off and landing points and the jumbo's timetable from the path
    /// (`InitPlanes`, after the nodes have been put on the ground).
    pub fn setup_jumbo_timetable(&mut self) {
        let path = &self.paths[0];
        let n = path.nodes.len();
        self.landing_point = -1.0;
        self.take_off_point = -1.0;
        let mut last_on_ground = path.nodes[n - 1].on_ground;
        for node in &path.nodes {
            if node.on_ground && !last_on_ground {
                self.landing_point = node.t;
            } else if !node.on_ground && last_on_ground {
                self.take_off_point = node.t;
            }
            last_on_ground = node.on_ground;
        }
        let total = path.total_length;
        let (mut time, mut position) = (0.0f32, 0.0f32);
        let set = |i: usize, kind: u8, time: f32, position: f32, speed: f32, acceleration: f32, bits: &mut [InterpolationLine; 6]| {
            bits[i] = InterpolationLine { kind, time, position, speed, acceleration };
        };
        let mut bits = [InterpolationLine::default(); 6];
        // start on the ground at taxi speed
        set(0, 1, time, position, TAXI_SPEED, 0.0, &mut bits);
        let mut dist = (self.take_off_point - 600.0) - position;
        time += dist / TAXI_SPEED;
        position += dist;
        // accelerate to take off
        set(1, 2, time, position, TAXI_SPEED, 618.75 / 600.0, &mut bits);
        time += 600.0 / ((CRUISE_SPEED + TAXI_SPEED) / 2.0);
        position += 600.0;
        // cruise
        set(2, 1, time, position, CRUISE_SPEED, 0.0, &mut bits);
        dist = self.landing_point - self.take_off_point;
        time += dist / CRUISE_SPEED;
        position += dist;
        // brake after landing
        set(3, 2, time, position, CRUISE_SPEED, -618.75 / 600.0, &mut bits);
        time += 600.0 / ((CRUISE_SPEED + TAXI_SPEED) / 2.0);
        position += 600.0;
        // taxi
        set(4, 1, time, position, TAXI_SPEED, 0.0, &mut bits);
        time += (total - position) / TAXI_SPEED;
        bits[5].time = time;
        self.line_bits = bits;
        self.durations[0] = time;
        self.durations[1] = self.paths[1].total_length / CRUISE_SPEED;
        self.durations[2] = self.paths[2].total_length / CRUISE_SPEED;
        self.durations[3] = self.paths[3].total_length / CRUISE_SPEED;
    }

    /// `UpdatePlanes`: advances the path positions from the clock.
    pub fn update(&mut self, time_ms: u32) {
        let mut time = time_ms;
        for i in 0..3 {
            let t = self.durations[0] * (time & 0x7FFFF) as f32 / 0x80000 as f32;
            let mut j = 0;
            while t > self.line_bits[j + 1].time {
                j += 1;
            }
            self.old_path_position[i] = self.path_position[i];
            let line = self.line_bits[j];
            let delta_t = t - line.time;
            match line.kind {
                0 => {
                    self.path_position[i] = line.position;
                    self.path_speed[i] = 0.0;
                }
                1 => {
                    self.path_position[i] = line.position + line.speed * delta_t;
                    self.path_speed[i] = (self.durations[0] * 1000.0 / 0x80000 as f32) * line.speed;
                }
                _ => {
                    self.path_position[i] = line.position + (line.speed + line.acceleration * delta_t) * delta_t;
                    self.path_speed[i] = (self.durations[0] * 1000.0 / 0x80000 as f32) * line.speed + 2.0 * line.acceleration * delta_t;
                }
            }
            time = time.wrapping_add(0x80000 / 3);
        }

        let time = time_ms;
        let t = self.durations[1] / 0x80000 as f32;
        self.path2_position[0] = CRUISE_SPEED * (time & 0x7FFFF) as f32 * t;
        self.path2_position[1] = CRUISE_SPEED * (time.wrapping_add(0x80000 / 3) & 0x7FFFF) as f32 * t;
        self.path2_position[2] = CRUISE_SPEED * (time.wrapping_add(0x80000 / 3 * 2) & 0x7FFFF) as f32 * t;
        self.path2_speed = [CRUISE_SPEED * t; 3];

        if self.cesna_status == CesnaStatus::Flying {
            self.path3_speed = CRUISE_SPEED * self.durations[2] / 0x20000 as f32;
            let since = time.wrapping_sub(self.cesna_start_time);
            self.path3_position = self.path3_speed * (since & 0x1FFFF) as f32;
            if since >= 128_072 {
                self.cesna_status = CesnaStatus::Landed;
            }
        }
        if self.drop_off_status == CesnaStatus::Flying {
            self.path4_speed = CRUISE_SPEED * self.durations[3] / 0x80000 as f32;
            let since = time.wrapping_sub(self.drop_off_start_time);
            self.path4_position = self.path4_speed * (since & 0x7FFFF) as f32;
            if since >= 521_288 {
                self.drop_off_status = CesnaStatus::Landed;
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlaneData {
    pub plane_id: i16,
    pub is_far_away: bool,
    pub cur_path_node: usize,
    pub speed: f32,
    pub frame_when_hit: u32,
    pub has_been_hit: bool,
    pub is_drug_run_cesna: bool,
    pub is_drop_off_cesna: bool,
}

pub struct NewPlane {
    pub vehicle: VehicleData,
    pub physical: Physical,
}

/// `CPlane::CPlane`.
pub fn new_plane(time_ms: u32, created_by: u8, handling_id: usize, rng: &mut Rng) -> NewPlane {
    let mut v = VehicleData::new(created_by, time_ms, VehicleType::Plane, rng);
    v.handling = handling_id;
    v.sub = super::base::VehicleSub::Plane(Box::default());
    let physical = Physical { mass: 100_000_000.0, turn_mass: 100_000_000.0, air_resistance: 0.9994, elasticity: 0.05, ..Default::default() };
    NewPlane { vehicle: v, physical }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> FlightPath {
        FlightPath::load(&[[0.0, 0.0, 10.0], [100.0, 0.0, 10.0], [100.0, 100.0, 10.0], [0.0, 100.0, 10.0]], true)
    }

    #[test]
    fn loading_measures_the_xy_length() {
        let p = square();
        assert_eq!(p.total_length, 400.0);
        assert_eq!(p.nodes.iter().map(|n| n.t).collect::<Vec<_>>(), vec![0.0, 100.0, 200.0, 300.0]);
        // an open path has no closing segment
        let open = FlightPath::load(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]], false);
        assert_eq!(open.total_length, 20.0);
    }

    #[test]
    fn positions_interpolate_between_nodes_and_wrap() {
        let p = square();
        let a = p.point_at(0, 150.0);
        assert_eq!((a.cur, a.next), (1, 2));
        assert!((a.pos - Vec3::new(100.0, 50.0, 10.0)).length() < 1e-4);
        // the cursor walks forward and around the loop
        let b = p.point_at(3, 20.0);
        assert_eq!((b.cur, b.next), (0, 1));
        assert!((b.pos - Vec3::new(20.0, 0.0, 10.0)).length() < 1e-4);
        // the last segment closes back to node 0
        let c = p.point_at(0, 350.0);
        assert_eq!((c.cur, c.next), (3, 0));
        assert!((c.pos - Vec3::new(0.0, 50.0, 10.0)).length() < 1e-4);
    }

    #[test]
    fn the_jumbo_timetable_runs_taxi_takeoff_cruise_landing() {
        let mut m = PlaneManager::default();
        let mut path = FlightPath::load(&[[0.0, 0.0, 0.0], [1000.0, 0.0, 0.0], [3000.0, 0.0, 50.0], [6000.0, 0.0, 50.0], [8000.0, 0.0, 0.0], [9000.0, 0.0, 0.0]], true);
        for (i, n) in path.nodes.iter_mut().enumerate() {
            n.on_ground = matches!(i, 0 | 1 | 4 | 5);
        }
        m.paths[0] = path;
        m.paths[1] = FlightPath::load(&[[0.0, 0.0, 0.0], [500.0, 0.0, 0.0]], true);
        m.paths[2] = m.paths[1].clone();
        m.paths[3] = m.paths[1].clone();
        m.setup_jumbo_timetable();
        // the first airborne node is at 3000, the next ground node at 8000
        assert_eq!((m.take_off_point, m.landing_point), (3000.0, 8000.0));
        assert!(m.line_bits.windows(2).all(|w| w[0].time <= w[1].time));
        assert_eq!(m.durations[0], m.line_bits[5].time);
        m.update(0);
        let p0 = m.path_position;
        // the three jumbos are a third of a lap apart in time
        assert_eq!(p0[0], 0.0);
        m.update(100_000);
        assert!(m.path_position[0] > p0[0] && m.old_path_position[0] == 0.0);
        // the Dodos fly at cruise speed
        m.update(0x80000 / 2);
        assert!((m.path2_position[0] - CRUISE_SPEED * (0x80000 / 2) as f32 * (m.durations[1] / 0x80000 as f32)).abs() < 1e-2);
        // a Cesna on its mission lands after 128 seconds
        m.cesna_status = CesnaStatus::Flying;
        m.cesna_start_time = 1000;
        m.update(1000 + 128_071);
        assert_eq!(m.cesna_status, CesnaStatus::Flying);
        m.update(1000 + 128_072);
        assert_eq!(m.cesna_status, CesnaStatus::Landed);
    }
}
