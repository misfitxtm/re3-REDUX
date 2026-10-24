//! Trains in the world: `CTrain::ProcessControl`, `InitTrains`, `UpdateTrains`.
#![allow(clippy::neg_cmp_op_on_partial_ord)]
use super::{Kinds, World};
use crate::entity::{status, Ent, Entity, EntityRef, EntityType, Kind};
use crate::general::Rng;
use crate::math::{cross, normalise};
use crate::modelinfo::indices_gta3::mi;
use crate::vehicle::base::VehicleSub;
use crate::vehicle::env::{CarEnv, CarEvent};
use crate::vehicle::train::*;
use glam::Vec3;

/// `eTrainPositions` entries of the model (left, middle and right door).
const TRAIN_POS_LEFT_ENTRY: usize = 2;
const TRAIN_POS_MID_ENTRY: usize = 3;
const TRAIN_POS_RIGHT_ENTRY: usize = 4;

impl World {
    fn train_data(&self, r: EntityRef) -> Option<&TrainData> {
        match &self.get(r)?.kind {
            Kind::Vehicle(v) => match &v.sub {
                VehicleSub::Train(t) => Some(t),
                _ => None,
            },
            _ => None,
        }
    }

    fn train_data_mut(&mut self, r: EntityRef) -> Option<&mut TrainData> {
        match &mut self.get_mut(r)?.kind {
            Kind::Vehicle(v) => match &mut v.sub {
                VehicleSub::Train(t) => Some(t),
                _ => None,
            },
            _ => None,
        }
    }

    /// `CTrain::CTrain`; place it with [`World::add`].
    pub fn new_train(&mut self, model_index: i16, created_by: u8, rng: &mut Rng) -> Option<EntityRef> {
        let time_ms = self.ents.timer.time_in_ms;
        let info = self.ents.models.get(model_index as i32)?;
        let crate::modelinfo::Kind::Vehicle(mi_data) = &info.kind else { return None };
        let nt = new_train(time_ms, created_by, mi_data.handling_id as usize, rng);
        let mut base = Entity::new(EntityType::Vehicle, rng);
        base.model_index = model_index;
        base.status = status::TRAIN_MOVING;
        base.flags.uses_collision = true;
        self.insert(Ent::new(base, Some(nt.physical), Kind::Vehicle(Box::new(nt.vehicle))))
    }

    /// `InitTrains` once the track files are read: builds both tracks (the nodes are put on the
    /// ground) and creates the five El-Train and eight subway wagons.
    pub fn init_trains(&mut self, tracks: [Vec<[f32; 3]>; 2], rng: &mut Rng) {
        let ground = Kinds { buildings: true, dummies: true, ..Kinds::NONE };
        let el = Track::build(&tracks[0], false, &STATION_DIST, |p| self.process_vertical_line(p, p.z - 0.5, ground, false, None).map(|h| h.point.point.z));
        let sub = Track::build(&tracks[1], true, &STATION_DIST_S, |p| self.process_vertical_line(p, p.z - 0.5, ground, false, None).map(|h| h.point.point.z));
        self.ents.trains.el = el;
        self.ents.trains.subway = sub;

        let model = mi::TRAIN;
        let (wp, first, last, group) = ([0.0f32, 20.0, 40.0, 0.0, 20.0], [true, false, false, true, false], [false, false, true, false, true], [0usize, 0, 0, 1, 1]);
        for i in 0..5 {
            self.spawn_wagon(model, wp[i], first[i], last[i], group[i], i as i16, TRACK_ELTRAIN, rng);
        }
        let (wp, first, last, group) = (
            [0.0f32, 20.0, 0.0, 20.0, 0.0, 20.0, 0.0, 20.0],
            [true, false, true, false, true, false, true, false],
            [false, true, false, true, false, true, false, true],
            [0usize, 0, 1, 1, 2, 2, 3, 3],
        );
        for i in 0..8 {
            self.spawn_wagon(model, wp[i], first[i], last[i], group[i], i as i16, TRACK_SUBWAY, rng);
        }
    }

    #[allow(clippy::too_many_arguments)] // the wagon table of InitTrains
    fn spawn_wagon(&mut self, model: i16, position: f32, first: bool, last: bool, group: usize, id: i16, track: u8, rng: &mut Rng) {
        let Some(r) = self.new_train(model, crate::vehicle::base::PERMANENT_VEHICLE, rng) else { return };
        if let Some(e) = self.get_mut(r) {
            e.base.matrix = crate::math::Matrix::from_translation(Vec3::ZERO);
            e.base.status = status::ABANDONED;
            if let Kind::Vehicle(v) = &mut e.kind {
                v.flags.is_locked = true;
            }
        }
        if let Some(t) = self.train_data_mut(r) {
            t.wagon_position = position;
            t.is_first_wagon = first;
            t.is_last_wagon = last;
            t.wagon_group = group;
            t.wagon_id = id;
            t.cur_track_node = 0;
            t.track_id = track;
        }
        let _ = self.add(r);
    }

    /// `CTrain::UpdateTrains`.
    pub fn update_trains(&mut self, camera: Vec3, env: &mut CarEnv) {
        let t = self.ents.timer.time_in_ms;
        for a in self.ents.trains.update(t, camera) {
            env.events.push(CarEvent::TrainAnnouncement { arriving: a.arriving, station: a.station });
        }
    }

    /// `CTrain::ProcessControl`.
    pub fn process_control_train(&mut self, r: EntityRef, env: &mut CarEnv) {
        let Some(td) = self.train_data(r) else { return };
        let mut t = td.clone();
        let timer = self.ents.timer.clone();
        if t.is_far_away && (timer.frame_counter.wrapping_add(t.wagon_id as u32)) & 0xF != 0 {
            return;
        }
        let Some(e) = self.get(r) else { return };
        let model = e.base.model_index;
        let col = self.ents.col_model(model);
        let pm = &self.ents.trains;
        let (track, positions, speeds): (&Track, &[f32], &[f32]) = if t.track_id == TRACK_SUBWAY {
            (&pm.subway, &pm.engine_position_s, &pm.engine_speed_s)
        } else {
            (&pm.el, &pm.engine_position, &pm.engine_speed)
        };
        if track.path.nodes.is_empty() {
            return;
        }
        let total = track.path.total_length;
        let mut rear = positions[t.wagon_group] - t.wagon_position;
        if rear < 0.0 {
            rear += total;
        }
        let rear_pt = track.path.point_at(t.cur_track_node, rear);
        t.cur_track_node = rear_pt.cur;
        let mut front = rear + 20.0;
        if front > total {
            front -= total;
        }
        let front_pt = track.path.point_at(t.cur_track_node, front);
        let engine_speed = speeds[t.wagon_group];

        let pos = (rear_pt.pos + front_pt.pos) / 2.0;
        let fwd = normalise(front_pt.pos - rear_pt.pos);
        let right = normalise(cross(fwd, Vec3::Z));
        let up = cross(right, fwd);
        let move_speed = fwd * engine_speed / 60.0;
        t.speed = engine_speed / 60.0;
        let moving = engine_speed > 0.001;
        if moving {
            t.train_stopping = false;
            t.process_door = true;
        } else {
            t.train_stopping = true;
        }
        let cam = env.camera_pos;
        let dx = front_pt.pos.x - cam.x;
        let dy = front_pt.pos.y - cam.y;
        // the original compares the distance, not its square, with 250 squared
        t.is_far_away = !((dx * dx + dy * dy).sqrt() < 250.0 * 250.0);

        let mut events = std::mem::take(&mut env.events);
        if t.wagon_position == 20.0 && t.speed > 0.0001 && (cam.z - pos.z).abs() < 15.0 {
            events.push(CarEvent::TrainShake { x: pos.x, y: pos.y });
        }

        if t.process_door {
            use crate::vehicle::train::DoorState::*;
            match t.door_state {
                Closed => {
                    if t.train_stopping {
                        t.door_timer = timer.time_in_ms + 1000;
                        t.door_state = Opening;
                        events.push(CarEvent::TrainDoor { opening: false });
                    }
                }
                Opening => {
                    if timer.time_in_ms < t.door_timer {
                        let ratio = 1.0 - (t.door_timer - timer.time_in_ms) as f32 / 1000.0;
                        t.doors[0].open(ratio);
                        t.doors[1].open(ratio);
                    } else {
                        t.doors[0].open(1.0);
                        t.doors[1].open(1.0);
                        t.door_state = Open;
                    }
                }
                Open => {
                    if !t.train_stopping {
                        t.door_timer = timer.time_in_ms + 1000;
                        t.door_state = Closing;
                        events.push(CarEvent::TrainDoor { opening: true });
                    }
                }
                Closing => {
                    if timer.time_in_ms < t.door_timer {
                        let ratio = (t.door_timer - timer.time_in_ms) as f32 / 1000.0;
                        t.doors[0].open(ratio);
                        t.doors[1].open(ratio);
                    } else {
                        t.doors[0].open(0.0);
                        t.doors[1].open(0.0);
                        t.door_state = Closed;
                        t.process_door = false;
                    }
                }
            }
        }
        env.events = events;

        let first_moving = t.is_first_wagon && moving;
        if let Some(td) = self.train_data_mut(r) {
            *td = t;
        }
        if let Some(e) = self.get_mut(r) {
            e.base.matrix.pos = pos;
            e.base.matrix.right = right;
            e.base.matrix.up = up;
            e.base.matrix.forward = fwd;
            let p = e.phys.as_mut().unwrap();
            p.move_speed = move_speed;
            p.turn_speed = Vec3::ZERO;
            e.base.status = if moving { status::TRAIN_MOVING } else { status::TRAIN_NOT_MOVING };
        }
        let _ = self.remove_and_add(r);
        if let Some(e) = self.get_mut(r) {
            e.base.flags.is_stuck = false;
            e.base.flags.is_in_safe_position = true;
            e.base.flags.was_postponed = false;
        }

        // the front of the first wagon knocks things over
        if first_moving {
            let (fwd_len, ms) = (col.map_or(0.0, |c| c.bounding_box.max.y), move_speed);
            let front = pos + fwd * fwd_len + ms * timer.time_step;
            let g = self.grid;
            let (xmin, xmax) = (g.index_x(front.x - 3.0).max(0), g.index_x(front.x + 3.0).min(g.num - 1));
            let (ymin, ymax) = (g.index_y(front.y - 3.0).max(0), g.index_y(front.y + 3.0).min(g.num - 1));
            self.advance_scan_code();
            let me_z = pos.z;
            for y in ymin..=ymax {
                for x in xmin..=xmax {
                    let Some(s) = self.sector(x, y).cloned() else { continue };
                    for list in [super::VEHICLES, super::VEHICLES + 1, super::PEDS, super::PEDS + 1] {
                        for &rb in &s.lists[list] {
                            // the original does not use the scan code in this loop
                            if rb != r
                                && let Some(b) = self.get_mut(rb)
                                && (me_z - b.base.position().z).abs() < 1.5
                                && let Some(bp) = b.phys.as_mut()
                            {
                                bp.flags.hit_by_train = true;
                            }
                        }
                    }
                }
            }
        }
    }

    /// `CTrain::AddPassenger`: a ped takes the seat at its door, or the first free one.
    pub fn train_add_passenger(&mut self, train: EntityRef, ped: EntityRef, door_index: usize) {
        let Some(e) = self.get_mut(train) else { return };
        let Kind::Vehicle(v) = &mut e.kind else { return };
        if matches!(door_index, TRAIN_POS_LEFT_ENTRY | TRAIN_POS_MID_ENTRY | TRAIN_POS_RIGHT_ENTRY) && v.passengers[door_index].is_none() {
            v.passengers[door_index] = Some(ped);
            v.num_passengers += 1;
        } else {
            for i in 0..6 {
                if v.passengers[i].is_none() {
                    v.passengers[i] = Some(ped);
                    v.num_passengers += 1;
                    return;
                }
            }
        }
    }
}
