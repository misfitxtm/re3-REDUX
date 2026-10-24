//! `CTrain`: the El-Train and the subway, driven along track files by a global timetable.
#![allow(clippy::needless_range_loop)]
use super::base::{VehicleData, VehicleType};
use super::door::TrainDoor;
use super::plane::{FlightPath, InterpolationLine};
use crate::entity::physical::Physical;
use crate::general::Rng;
use crate::math::{cross, normalise};
use glam::Vec3;

pub const TRACK_ELTRAIN: u8 = 0;
pub const TRACK_SUBWAY: u8 = 1;

pub const STATION_DIST: [f32; 3] = [873.0, 1522.0, 2481.0];
pub const STATION_DIST_S: [f32; 4] = [55.0, 1388.0, 2337.0, 3989.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DoorState {
    #[default]
    Closed,
    Opening,
    Open,
    Closing,
}

/// A track: nodes with their distance along it, the stop-and-go timetable and its duration.
#[derive(Debug, Clone, Default)]
pub struct Track {
    pub path: FlightPath,
    pub total_duration: f32,
    pub lines: Vec<InterpolationLine>,
}

impl Track {
    /// `ReadAndInterpretTrackFile` without the file and the world: `points` are the file's nodes
    /// (one rail); they are moved to the track centre, measured, put on the ground by
    /// `ground_z` and given a timetable that stops at each station.
    pub fn build(points: &[[f32; 3]], right_rail: bool, station_dists: &[f32], mut ground_z: impl FnMut(Vec3) -> Option<f32>) -> Self {
        let n = points.len();
        let to_centre = if right_rail { 0.9 } else { -0.9 };
        let mut pts: Vec<Vec3> = points.iter().map(|p| Vec3::from(*p)).collect();
        let orig = pts.clone();
        for i in 0..n {
            let fwd = if i == n - 1 { orig[0] - orig[i] } else { orig[i + 1] - orig[i] };
            let right = normalise(cross(fwd, Vec3::Z));
            pts[i] -= right * to_centre;
        }
        let raw: Vec<[f32; 3]> = pts.iter().map(|p| p.to_array()).collect();
        let mut path = FlightPath::load(&raw, true);
        for node in &mut path.nodes {
            let mut p = node.p;
            p.z += 1.0;
            if let Some(z) = ground_z(p) {
                node.p.z = z;
            }
            node.p.z += 0.2;
        }

        let (mut position, mut time) = (0.0f32, 0.0f32);
        let mut lines = vec![];
        let line = |kind, time, position, speed, acceleration| InterpolationLine { kind, time, position, speed, acceleration };
        for &station in station_dists {
            // full speed up to 40 units before the station
            lines.push(line(1, time, position, 15.0, 0.0));
            let dist = (station - 40.0) - position;
            time += dist / 15.0;
            position += dist;
            // slow down over the last 40 units
            lines.push(line(2, time, position, 15.0, -45.0 / 32.0));
            time += 80.0 / 15.0;
            position += 40.0;
            // stopped for 25 seconds
            lines.push(line(0, time, position, 0.0, 0.0));
            time += 25.0;
            // accelerate again
            lines.push(line(2, time, position, 0.0, 45.0 / 32.0));
            time += 80.0 / 15.0;
            position += 40.0;
        }
        lines.push(line(1, time, position, 15.0, 0.0));
        let total_duration = time + (path.total_length - position) / 15.0;
        lines.push(InterpolationLine { time: total_duration, ..Default::default() });
        Self { path, total_duration, lines }
    }

    /// Position and speed of an engine at timetable time `t`; `duration_for_speed` and
    /// `period` convert to per-frame units (the subway uses the El-Train's duration, as the original).
    fn engine_state(&self, t: f32, duration_for_speed: f32, period: f32) -> (f32, f32) {
        let mut j = 0;
        while t > self.lines[j + 1].time {
            j += 1;
        }
        let l = self.lines[j];
        let delta = t - l.time;
        match l.kind {
            0 => (l.position, 0.0),
            1 => (l.position + l.speed * delta, (duration_for_speed * 1000.0 / period) * l.speed),
            _ => (l.position + (l.speed + l.acceleration * delta) * delta, (duration_for_speed * 1000.0 / period) * l.speed + 2.0 * l.acceleration * delta),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TrainManager {
    pub el: Track,
    pub subway: Track,
    pub engine_position: [f32; 2],
    pub engine_speed: [f32; 2],
    pub engine_position_s: [f32; 4],
    pub engine_speed_s: [f32; 4],
    pub arrival_announced: [bool; 3],
}

/// An announcement the audio should play (`SCRIPT_SOUND_TRAIN_ANNOUNCEMENT_1/2`) at a station.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Announcement {
    pub arriving: bool,
    pub station: usize,
}

impl TrainManager {
    /// `UpdateTrains`. `camera` is the camera position; the El-Train only runs near the line.
    pub fn update(&mut self, time_ms: u32, camera: Vec3) -> Vec<Announcement> {
        let mut out = vec![];
        if camera.x > 200.0 && camera.x < 1600.0 && camera.y > -1000.0 && camera.y < 500.0 && !self.el.lines.is_empty() {
            let mut time = time_ms;
            for i in 0..2 {
                let t = self.el.total_duration * (time & 0x1FFFF) as f32 / 0x20000 as f32;
                (self.engine_position[i], self.engine_speed[i]) = self.el.engine_state(t, self.el.total_duration, 0x20000 as f32);
                time = time.wrapping_add(0x20000 / 2);
            }
            out = self.announcements();
        }
        if !self.subway.lines.is_empty() {
            let mut time = time_ms;
            for i in 0..4 {
                let t = self.subway.total_duration * (time & 0x3FFFF) as f32 / 0x40000 as f32;
                // the original scales the subway's speed with the El-Train's duration
                (self.engine_position_s[i], self.engine_speed_s[i]) = self.subway.engine_state(t, self.el.total_duration, 0x40000 as f32);
                time = time.wrapping_add(0x40000 / 4);
            }
        }
        out
    }

    /// `ProcessTrainAnnouncements`.
    fn announcements(&mut self) -> Vec<Announcement> {
        let mut out = vec![];
        let total = self.el.path.total_length;
        for i in 0..STATION_DIST.len() {
            for j in 0..2 {
                let pos = self.engine_position[j];
                if !self.arrival_announced[i] {
                    let mut pre = STATION_DIST[i] - 100.0;
                    if pre < 0.0 {
                        pre += total;
                    }
                    if pos > pre && pos < STATION_DIST[i] {
                        self.arrival_announced[i] = true;
                        out.push(Announcement { arriving: true, station: i });
                        break;
                    }
                } else {
                    // FIX_BUGS: wrap past the end of the track
                    let mut post = STATION_DIST[i] + 10.0;
                    if post > total {
                        post -= total;
                    }
                    if pos > STATION_DIST[i] && pos < post {
                        self.arrival_announced[i] = false;
                        out.push(Announcement { arriving: false, station: i });
                        break;
                    }
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct TrainData {
    pub wagon_position: f32,
    pub wagon_id: i16,
    pub is_far_away: bool,
    pub cur_track_node: usize,
    pub wagon_group: usize,
    pub speed: f32,
    pub process_door: bool,
    pub train_stopping: bool,
    pub is_first_wagon: bool,
    pub is_last_wagon: bool,
    pub track_id: u8,
    pub door_timer: u32,
    pub door_state: DoorState,
    pub doors: [TrainDoor; 2],
}

pub struct NewTrain {
    pub vehicle: VehicleData,
    pub physical: Physical,
}

/// `CTrain::CTrain`.
pub fn new_train(time_ms: u32, created_by: u8, handling_id: usize, rng: &mut Rng) -> NewTrain {
    let mut v = VehicleData::new(created_by, time_ms, VehicleType::Train, rng);
    v.handling = handling_id;
    v.num_max_passengers = 5;
    let mut doors = [TrainDoor::default(); 2];
    doors[0].init(0.8, 0.0, 1, 0);
    doors[1].init(-0.8, 0.0, 0, 0);
    v.sub = super::base::VehicleSub::Train(Box::new(TrainData {
        wagon_position: 0.0,
        wagon_id: 0,
        is_far_away: true,
        cur_track_node: 0,
        wagon_group: 0,
        speed: 0.0,
        process_door: true,
        train_stopping: false,
        is_first_wagon: false,
        is_last_wagon: false,
        track_id: TRACK_ELTRAIN,
        door_timer: time_ms,
        door_state: DoorState::Closed,
        doors,
    }));
    let physical = Physical { mass: 100_000_000.0, turn_mass: 100_000_000.0, air_resistance: 0.9994, elasticity: 0.05, ..Default::default() };
    NewTrain { vehicle: v, physical }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A straight 6000 unit line along x.
    fn straight(right: bool, stations: &[f32]) -> Track {
        let pts: Vec<[f32; 3]> = (0..7).map(|i| [i as f32 * 1000.0, 0.0, 0.0]).collect();
        Track::build(&pts, right, stations, |_| Some(5.0))
    }

    #[test]
    fn nodes_are_centred_between_the_rails_and_put_on_the_ground() {
        let t = straight(false, &[]);
        // the rail data runs along +x: "right" of it is -y, so the left rail's centre is shifted
        assert!((t.path.nodes[1].p.y.abs() - 0.9).abs() < 1e-5, "{:?}", t.path.nodes[1].p);
        assert!((t.path.nodes[1].p.z - 5.2).abs() < 1e-5);
        let r = straight(true, &[]);
        assert!((t.path.nodes[1].p.y + r.path.nodes[1].p.y).abs() < 1e-5);
    }

    #[test]
    fn the_timetable_stops_at_every_station_for_25_seconds() {
        let t = straight(false, &STATION_DIST);
        // 4 keyframes per station plus the last one and the end marker
        assert_eq!(t.lines.len(), 3 * 4 + 2);
        let stops: Vec<_> = t.lines[..t.lines.len() - 1].iter().filter(|l| l.kind == 0).collect();
        assert_eq!(stops.len(), 3);
        // the first stop is at the station and lasts 25 s
        assert_eq!(stops[0].position, STATION_DIST[0]);
        let i = t.lines.iter().position(|l| l.kind == 0).unwrap();
        assert!((t.lines[i + 1].time - stops[0].time - 25.0).abs() < 1e-4);
        assert!(t.total_duration > t.lines[t.lines.len() - 2].time);
        assert_eq!(t.lines.last().unwrap().time, t.total_duration);
    }

    #[test]
    fn engines_slow_into_a_station_stand_still_and_pull_out() {
        let mut m = TrainManager { el: straight(false, &STATION_DIST), ..Default::default() };
        let cam = Vec3::new(500.0, 0.0, 0.0);
        let d = m.el.total_duration;
        // find the clock time (within one 0x20000 ms lap) that is in the first stop
        let stop = *m.el.lines.iter().find(|l| l.kind == 0).unwrap();
        let ms = ((stop.time + 5.0) / d * 0x20000 as f32) as u32;
        m.update(ms, cam);
        assert_eq!(m.engine_speed[0], 0.0);
        assert_eq!(m.engine_position[0], STATION_DIST[0]);
        // and a bit later it is moving again
        let ms2 = ((stop.time + 40.0) / d * 0x20000 as f32) as u32;
        m.update(ms2, cam);
        assert!(m.engine_speed[0] > 0.0 && m.engine_position[0] > STATION_DIST[0]);
        // far from the line the El-Train does not update
        let before = m.engine_position;
        m.update(ms, Vec3::new(5000.0, 5000.0, 0.0));
        assert_eq!(m.engine_position, before);
    }

    #[test]
    fn arrival_is_announced_once_then_departure() {
        let mut m = TrainManager { el: straight(false, &STATION_DIST), ..Default::default() };
        m.engine_position = [STATION_DIST[0] - 50.0, 0.0];
        let a = m.announcements();
        assert_eq!(a, vec![Announcement { arriving: true, station: 0 }]);
        assert!(m.announcements().is_empty());
        m.engine_position = [STATION_DIST[0] + 5.0, 0.0];
        assert_eq!(m.announcements(), vec![Announcement { arriving: false, station: 0 }]);
    }
}
