//! Planes in the world: `CPlane::ProcessControl` and the manager (`InitPlanes`, `UpdatePlanes`, ...).
// The original tests `!(a < b)`; with a NaN that differs from `a >= b`.
#![allow(clippy::neg_cmp_op_on_partial_ord)]
use super::{Ent, Kinds, World};
use crate::entity::{status, Entity, EntityRef, EntityType, Kind};
use crate::general::Rng;
use crate::math::{cross, magnitude, normalise};
use crate::modelinfo::indices_gta3::mi;
use crate::vehicle::base::VehicleSub;
use crate::vehicle::env::{CarEnv, CarEvent, Particle, ParticleKind};
use crate::vehicle::plane::*;
use glam::Vec3;

const COLOURS_AIRTRAIN: [[u8; 4]; 8] = [[0, 0, 0, 255], [224, 230, 238, 255], [224, 230, 238, 255], [0, 0, 0, 255], [224, 230, 238, 255], [0, 0, 0, 255], [0, 0, 0, 255], [224, 230, 238, 255]];
const COLOURS_CESNA: [[u8; 4]; 8] = [[0, 0, 0, 255], [224, 230, 238, 255], [224, 230, 238, 255], [0, 0, 0, 255], [252, 66, 66, 255], [0, 0, 0, 255], [0, 0, 0, 255], [252, 66, 66, 255]];

impl World {
    fn plane_data(&self, r: EntityRef) -> Option<&PlaneData> {
        match &self.get(r)?.kind {
            Kind::Vehicle(v) => match &v.sub {
                VehicleSub::Plane(p) => Some(p),
                _ => None,
            },
            _ => None,
        }
    }

    fn plane_data_mut(&mut self, r: EntityRef) -> Option<&mut PlaneData> {
        match &mut self.get_mut(r)?.kind {
            Kind::Vehicle(v) => match &mut v.sub {
                VehicleSub::Plane(p) => Some(p),
                _ => None,
            },
            _ => None,
        }
    }

    /// `CPlane::CPlane`. Planes live in the big-building lists and do not collide.
    pub fn new_plane(&mut self, model_index: i16, created_by: u8, rng: &mut Rng) -> Option<EntityRef> {
        let time_ms = self.ents.timer.time_in_ms;
        let info = self.ents.models.get(model_index as i32)?;
        let crate::modelinfo::Kind::Vehicle(mi_data) = &info.kind else { return None };
        let np = new_plane(time_ms, created_by, mi_data.handling_id as usize, rng);
        let mut base = Entity::new(EntityType::Vehicle, rng);
        base.model_index = model_index;
        base.status = status::PLANE;
        base.flags.uses_collision = false;
        base.flags.is_big_building = true;
        base.level = 0;
        self.insert(Ent::new(base, Some(np.physical), Kind::Vehicle(Box::new(np.vehicle))))
    }

    fn spawn_path_plane(&mut self, model: i16, id: i16, rng: &mut Rng) -> Option<EntityRef> {
        let r = self.new_plane(model, crate::vehicle::base::PERMANENT_VEHICLE, rng)?;
        let e = self.get_mut(r)?;
        e.base.matrix = crate::math::Matrix::from_translation(Vec3::ZERO);
        e.base.status = status::ABANDONED;
        if let Kind::Vehicle(v) = &mut e.kind {
            v.flags.is_locked = true;
        }
        let d = self.plane_data_mut(r)?;
        d.plane_id = id;
        d.cur_path_node = 0;
        let _ = self.add(r);
        Some(r)
    }

    /// `InitPlanes` after the path files are read (`paths[i]` are the files in order:
    /// flight, flight2, flight3, flight4). Puts the jumbo's nodes on the ground and creates
    /// the three jumbos and three Dodos.
    pub fn init_planes(&mut self, paths: [Vec<[f32; 3]>; 4], rng: &mut Rng) {
        self.ents.planes.cesna_status = CesnaStatus::None;
        let mut jumbo = FlightPath::load(&paths[0], true);
        let ground = Kinds { buildings: true, dummies: true, ..Kinds::NONE };
        for n in &mut jumbo.nodes {
            // the original looks up from 1000 units below... (it casts upward to find the roof)
            match self.process_vertical_line(n.p, 1000.0, ground, false, None) {
                Some(h) => {
                    n.p.z = h.point.point.z;
                    n.on_ground = true;
                }
                None => n.on_ground = false,
            }
        }
        self.ents.planes.paths[0] = jumbo;
        self.ents.planes.paths[1] = FlightPath::load(&paths[1], true);
        self.ents.planes.paths[2] = FlightPath::load(&paths[2], false);
        self.ents.planes.paths[3] = FlightPath::load(&paths[3], false);
        self.ents.planes.setup_jumbo_timetable();
        for id in 0..3 {
            self.spawn_path_plane(mi::AIRTRAIN, id, rng);
        }
        for id in 0..3 {
            self.spawn_path_plane(mi::DEADDODO, id, rng);
        }
    }

    /// `CPlane::UpdatePlanes`.
    pub fn update_planes(&mut self, replay_playing: bool) {
        if replay_playing {
            return;
        }
        let t = self.ents.timer.time_in_ms;
        self.ents.planes.update(t);
    }

    /// `CPlane::ProcessControl`.
    pub fn process_control_plane(&mut self, r: EntityRef, env: &mut CarEnv, rng: &mut Rng) {
        let Some(pd) = self.plane_data(r) else { return };
        let mut d = pd.clone();
        let Some(e) = self.get(r) else { return };
        let (mut m, model, random_seed) = (e.base.matrix, e.base.model_index, e.base.random_seed as u32);
        let timer = self.ents.timer.clone();
        let mut events = std::mem::take(&mut env.events);
        let mut destroy = false;
        let mut scorched = false;

        // the explosion sequence after a hit
        if d.has_been_hit {
            let frm = timer.frame_counter.wrapping_sub(d.frame_when_hit) as i32;
            let airtrain = model == mi::AIRTRAIN;
            if frm == 20 {
                events.push(CarEvent::HeliExplosion { pos: m.pos, bomb: false, culprit: env.player.ped });
                let colours = if airtrain { &COLOURS_AIRTRAIN } else { &COLOURS_CESNA };
                for _ in 0..40 {
                    let dir = Vec3::new(rng.range_f32(-2.0, 2.0), rng.range_f32(-2.0, 2.0), rng.range_f32(0.0, 2.0));
                    let mut rot = if airtrain { rng.range_i32(10, 30) } else { rng.range_f32(30.0, 20.0) as i32 };
                    if rng.number() & 1 != 0 {
                        rot = -rot;
                    }
                    self.ents.planes.frame_gen += 1;
                    let fg = self.ents.planes.frame_gen;
                    let mut p = Particle::new(ParticleKind::HeliDebris, m.pos, dir, rng.range_f32(0.1, 1.0));
                    p.colour = Some(colours[(fg & 7) as usize]);
                    p.rotation_speed = rot as f32;
                    p.frame = fg & 3;
                    events.push(CarEvent::Particle(p));
                }
            }
            let (lo, hi) = if airtrain { (40, 80) } else { (40, 60) };
            if frm >= lo && frm <= hi && frm & 1 != 0 {
                let k = if airtrain { 0.2 } else { 0.1 };
                let x = ((rng.number() & 0x3F) as i32 - 32) as f32 * k;
                let z = ((rng.number() & 0x3F) as i32 - 32) as f32 * k;
                let y = if airtrain { (frm - 40) as f32 } else { (frm - 40) as f32 * 0.3 };
                events.push(CarEvent::HeliExplosion { pos: m.transform_point(Vec3::new(x, y, z)), bomb: false, culprit: env.player.ped });
            }
            if (airtrain && frm == 60) || (!airtrain && frm == 30) {
                scorched = true;
            }
            if airtrain && frm == 82 {
                events.push(CarEvent::CameraFlash { rgb: [255, 255, 255] });
                destroy = true;
            }
            if !airtrain && frm == 62 {
                events.push(CarEvent::CameraFlash { rgb: [200, 200, 200] });
                if d.is_drug_run_cesna {
                    self.ents.planes.cesna_status = CesnaStatus::Destroyed;
                    self.ents.planes.drug_run_cesna = None;
                }
                if d.is_drop_off_cesna {
                    self.ents.planes.drop_off_status = CesnaStatus::Destroyed;
                    self.ents.planes.drop_off_cesna = None;
                }
                destroy = true;
            }
        }

        // update position and speed (far-away planes only every eighth frame)
        let mut speed_vec = None;
        if model == mi::AIRTRAIN || !d.is_far_away || (timer.frame_counter.wrapping_add(random_seed)) & 7 == 0 {
            let up_z = if model == mi::AIRTRAIN { 4.3 } else { 1.0 };
            let (fwd, pos, spd, new_far, node) = if model == mi::AIRTRAIN {
                let pm = &self.ents.planes;
                let path = &pm.paths[0];
                let id = d.plane_id as usize;
                let mut rear = pm.path_position[id] - 30.0;
                if rear < 0.0 {
                    rear += path.total_length;
                }
                let position = rear + 30.0;
                let mut pitch = 0.0f32;
                let since_take_off = position - pm.take_off_point;
                if since_take_off <= 0.0 && since_take_off > -70.0 {
                    pitch = 1.0 - since_take_off / -70.0;
                } else if (0.0..100.0).contains(&since_take_off) {
                    pitch = 1.0 - since_take_off / 100.0;
                }
                let since_landing = position - pm.landing_point;
                if since_landing <= 0.0 && since_landing > -200.0 {
                    pitch = 1.0 - since_landing / -200.0;
                } else if (0.0..70.0).contains(&since_landing) {
                    pitch = 1.0 - since_landing / 70.0;
                }
                let rear_pt = path.point_at(d.cur_path_node, rear);
                if pm.path_position[id] >= pm.landing_point && pm.old_path_position[id] < pm.landing_point {
                    events.push(CarEvent::PlaneOnGround);
                }
                let mut front = rear + 60.0;
                if front > path.total_length {
                    front -= path.total_length;
                }
                let front_pt = path.point_at(rear_pt.cur, front);
                let mut front2 = front + 60.0;
                if front2 > path.total_length {
                    front2 -= path.total_length;
                }
                let front2_pt = path.point_at(rear_pt.cur, front2);
                let centre = (rear_pt.pos + front_pt.pos) / 2.0;
                let mut fwd = normalise(front_pt.pos - rear_pt.pos);
                if pitch != 0.0 {
                    fwd.z += 0.4 * pitch;
                    fwd = normalise(fwd);
                }
                let fwd2 = normalise(front2_pt.pos - rear_pt.pos);
                let roll = cross(fwd, fwd2);
                let mut right = cross(fwd, Vec3::Z);
                if !rear_pt.both_on_ground_segment {
                    right.z += 3.0 * roll.z;
                }
                let spd = pm.path_speed[id] / 60.0;
                let dx = front_pt.pos.x - env.camera_pos.x;
                let dy = front_pt.pos.y - env.camera_pos.y;
                (fwd, (centre, right), spd, !(dx * dx + dy * dy < 300.0 * 300.0), rear_pt.cur)
            } else {
                if d.is_drug_run_cesna && self.ents.planes.cesna_status == CesnaStatus::Landed {
                    self.ents.planes.drug_run_cesna = None;
                    destroy = true;
                } else if d.is_drop_off_cesna && self.ents.planes.drop_off_status == CesnaStatus::Landed {
                    self.ents.planes.drop_off_cesna = None;
                    destroy = true;
                }
                let pm = &self.ents.planes;
                let (position, path_idx, spd) = if d.is_drug_run_cesna {
                    (pm.path3_position, 2, pm.path3_speed)
                } else if d.is_drop_off_cesna {
                    (pm.path4_position, 3, pm.path4_speed)
                } else {
                    (pm.path2_position[d.plane_id as usize], 1, pm.path2_speed[d.plane_id as usize])
                };
                let path = &pm.paths[path_idx];
                let mut rear = position - 10.0;
                if rear < 0.0 {
                    rear += path.total_length;
                }
                let rear_pt = path.point_at(d.cur_path_node, rear);
                let mut front = rear + 20.0;
                if front > path.total_length {
                    front -= path.total_length;
                }
                let front_pt = path.point_at(rear_pt.cur, front);
                let mut front2 = front + 30.0;
                if front2 > path.total_length {
                    front2 -= path.total_length;
                }
                let front2_pt = path.point_at(rear_pt.cur, front2);
                let centre = (rear_pt.pos + front_pt.pos) / 2.0;
                let fwd = normalise(front_pt.pos - rear_pt.pos);
                let fwd2 = normalise(front2_pt.pos - rear_pt.pos);
                let roll = cross(fwd, fwd2);
                let mut right = cross(fwd, Vec3::Z);
                right.z += 3.0 * roll.z;
                let dx = front_pt.pos.x - env.camera_pos.x;
                let dy = front_pt.pos.y - env.camera_pos.y;
                (fwd, (centre, right), spd / 60.0, !(dx * dx + dy * dy < 300.0 * 300.0), rear_pt.cur)
            };
            let (centre, right) = pos;
            let right = normalise(right);
            let up = cross(right, fwd);
            m.pos = centre;
            m.pos.z += up_z;
            m.right = right;
            m.up = up;
            m.forward = fwd;
            d.cur_path_node = node;
            d.speed = spd;
            d.is_far_away = new_far;
            speed_vec = Some(fwd * spd);
        }

        env.events = events;
        if let Some(pd) = self.plane_data_mut(r) {
            *pd = d;
        }
        if let Some(e) = self.get_mut(r) {
            e.base.matrix = m;
            if let Some(v) = speed_vec {
                let p = e.phys.as_mut().unwrap();
                p.move_speed = v;
                p.turn_speed = Vec3::ZERO;
            }
            e.base.flags.is_in_safe_position = true;
            if scorched {
                e.base.flags.render_scorched = true;
            }
            if destroy {
                e.base.flags.remove_from_world = true;
            }
        }
        let _ = (magnitude(Vec3::ZERO), mi::DEADDODO);
    }

    /// `CPlane::TestRocketCollision`: a rocket within 25 units downs a Dodo/Airtrain.
    pub fn test_plane_rocket_collision(&mut self, rocket: Vec3) -> bool {
        let frame = self.ents.timer.frame_counter;
        let pool = &self.ents.pools.vehicles;
        let hit: Vec<(usize, i32)> = pool.iter().filter_map(|(i, e)| {
            let Kind::Vehicle(v) = &e.kind else { return None };
            let VehicleSub::Plane(p) = &v.sub else { return None };
            let model_ok = e.base.model_index == mi::AIRTRAIN || e.base.model_index == mi::DEADDODO;
            (model_ok && !p.has_been_hit && magnitude(rocket - e.base.position()) < 25.0).then_some((i, 0))
        }).collect();
        // the original scans from the last slot and stops at the first plane it hits
        let Some(&(slot, _)) = hit.last() else { return false };
        let r = EntityRef { pool: crate::entity::PoolKind::Vehicle, handle: self.ents.pools.vehicles.handle_of(slot) };
        let pos = self.get(r).map(|e| e.base.position()).unwrap_or(Vec3::ZERO);
        if let Some(p) = self.plane_data_mut(r) {
            p.frame_when_hit = frame;
            p.has_been_hit = true;
        }
        self.ents.events.push(super::WorldEvent::Car(CarEvent::CrimeDestroyedCessna { pos, id: slot as i32 + 1 + 1983 - 1 }));
        true
    }

    /// `CPlane::CreateIncomingCesna`.
    pub fn create_incoming_cesna(&mut self, rng: &mut Rng) {
        if self.ents.planes.cesna_status == CesnaStatus::Flying
            && let Some(r) = self.ents.planes.drug_run_cesna.take()
        {
            self.delete(r);
        }
        if let Some(r) = self.spawn_path_plane(mi::DEADDODO, 0, rng) {
            if let Some(d) = self.plane_data_mut(r) {
                d.is_drug_run_cesna = true;
            }
            self.ents.planes.drug_run_cesna = Some(r);
        }
        self.ents.planes.cesna_status = CesnaStatus::Flying;
        self.ents.planes.cesna_start_time = self.ents.timer.time_in_ms;
    }

    /// `CPlane::CreateDropOffCesna`.
    pub fn create_drop_off_cesna(&mut self, rng: &mut Rng) {
        if self.ents.planes.drop_off_status == CesnaStatus::Flying
            && let Some(r) = self.ents.planes.drop_off_cesna.take()
        {
            self.delete(r);
        }
        if let Some(r) = self.spawn_path_plane(mi::DEADDODO, 0, rng) {
            if let Some(d) = self.plane_data_mut(r) {
                d.is_drop_off_cesna = true;
            }
            self.ents.planes.drop_off_cesna = Some(r);
        }
        self.ents.planes.drop_off_status = CesnaStatus::Flying;
        self.ents.planes.drop_off_start_time = self.ents.timer.time_in_ms;
    }
}
