//! `Garages.cpp`: the doors, the per-type state machines and everything a garage does to the world (GTA III).
use super::garages::*;
use crate::audio::sound;
use crate::entity::{EntityRef, Kind};
use crate::math::{cross, magnitude, Matrix};
use crate::ped::ctx::{Px, StatEvent};
use crate::ped::data::{objective, state as ped_state};
use crate::ped::pending::Hit;
use crate::ped::types::ped_type;
use crate::vehicle::base::{CarLock, MISSION_VEHICLE, RANDOM_VEHICLE};
use crate::vehicle::damage::engine;
use crate::weapons::info::weapon_type as wt;
use crate::weapons::weapon::WeaponState;
use crate::world::{Kinds, World};
use glam::Vec3;
use std::f32::consts::{FRAC_PI_2, PI};

const ROTATED_DOOR_OPEN_SPEED: f32 = 0.015;
const ROTATED_DOOR_CLOSE_SPEED: f32 = 0.02;
const DEFAULT_DOOR_OPEN_SPEED: f32 = 0.035;
const DEFAULT_DOOR_CLOSE_SPEED: f32 = 0.04;
const CRUSHER_CRANE_SPEED: f32 = 0.005;
const HIDEOUT_DOOR_SPEED_COEFFICIENT: f32 = 1.7;

const BOMB_PRICE: i32 = 1000;
const RESPRAY_PRICE: i32 = 1000;

const DISTANCE_TO_CALL_OFF_CHASE: f32 = 10.0;
const DISTANCE_FOR_MRWHOOP_HACK: f32 = 4.0;
const DISTANCE_TO_ACTIVATE_GARAGE: f32 = 8.0;
const DISTANCE_TO_ACTIVATE_KEEPCAR_GARAGE: f32 = 17.0;
const DISTANCE_TO_CLOSE_MISSION_GARAGE: f32 = 30.0;
const DISTANCE_TO_CLOSE_COLLECTSPECIFICCARS_GARAGE: f32 = 25.0;
const DISTANCE_TO_CLOSE_COLLECTCARS_GARAGE: f32 = 40.0;
const DISTANCE_TO_CLOSE_HIDEOUT_GARAGE_ON_FOOT: f32 = 2.2;
const DISTANCE_TO_CLOSE_HIDEOUT_GARAGE_IN_CAR: f32 = 15.0;
const DISTANCE_TO_FORCE_CLOSE_HIDEOUT_GARAGE: f32 = 70.0;
const DISTANCE_TO_OPEN_HIDEOUT_GARAGE_ON_FOOT: f32 = 1.7;
const DISTANCE_TO_OPEN_HIDEOUT_GARAGE_IN_CAR: f32 = 10.0;
const DISTANCE_TO_SHOW_HIDEOUT_MESSAGE: f32 = 5.0;
const DISTANCE_TO_CONSIDER_DOOR_FOR_GARAGE: f32 = 20.0;

const TIME_TO_RESPRAY: u32 = 2000;
const TIME_TO_SETUP_BOMB: u32 = 2000;
const TIME_TO_CRUSH_CAR: u32 = 3000;
const TIME_TO_PROCESS_KEEPCAR_GARAGE: u32 = 2000;
const TIME_BETWEEN_HIDEOUT_MESSAGES: u32 = 18000;

const FREE_RESPRAY_HEALTH_THRESHOLD: f32 = 970.0;
const NUM_PARTICLES_IN_RESPRAY: usize = 200;
const RESPRAY_CENTERING_COEFFICIENT: f32 = 0.75;
const KGS_OF_EXPLOSIVES_IN_BOMB: i32 = 10;

const REWARD_FOR_FIRST_POLICE_CAR: i32 = 5000;
const REWARD_FOR_FIRST_BANK_VAN: i32 = 5000;
const MAX_POLICE_CARS_TO_COLLECT: i32 = 10;
const MAX_BANK_VANS_TO_COLLECT: i32 = 10;
const MAX_SPEED_TO_SHOW_COLLECTED_MESSAGE: f32 = 0.03;
const IMPORT_REWARD: i32 = 1000;
const IMPORT_ALLCARS_REWARD: i32 = 200000;

const CRUSHER_VEHICLE_TEST_SPAN: usize = 8;
const CRUSHER_MIN_REWARD: f32 = 25.0;
const CRUSHER_MAX_REWARD: f32 = 125.0;
const CRUSHER_REWARD_COEFFICIENT: f32 = 1.0 / 500000.0;

/// One step of a door opening: true when it is fully open.
fn open_step(g: &mut Garage, step: f32, speed_scale: f32) -> bool {
    let speed = if g.rotated_door { ROTATED_DOOR_OPEN_SPEED } else { DEFAULT_DOOR_OPEN_SPEED };
    g.door_pos = g.door_height.min(g.door_pos + speed_scale * speed * step);
    g.door_pos == g.door_height
}

/// One step of a door closing: true when it is shut.
fn close_step(g: &mut Garage, step: f32, speed_scale: f32) -> bool {
    let speed = if g.rotated_door { ROTATED_DOOR_CLOSE_SPEED } else { DEFAULT_DOOR_CLOSE_SPEED };
    g.door_pos = (g.door_pos - speed_scale * speed * step).max(0.0);
    g.door_pos == 0.0
}

impl World {
    // ---- helpers ----

    /// The tests of an entity against a garage need its position and where its collision spheres are.
    pub fn garage_footprint(&self, r: EntityRef) -> Footprint {
        self.footprint_with_spheres(r, r)
    }

    /// The position of `r` with the collision spheres of `col_from` placed at `r`'s matrix (the original does this for peds).
    fn footprint_with_spheres(&self, r: EntityRef, col_from: EntityRef) -> Footprint {
        let Some(e) = self.get(r) else { return Footprint::default() };
        let spheres = self.col_model_of(col_from).map_or(vec![], |c| c.spheres.iter().map(|s| (e.base.matrix.transform_point(s.center), s.radius)).collect());
        Footprint { pos: e.base.position(), bound_radius: self.bound_radius_of(r), spheres }
    }

    fn player_vehicle_ref(&self) -> Option<EntityRef> {
        self.find_player_vehicle()
    }

    fn set_garage_controls(px: &mut Px, enabled: bool) {
        px.hooks.garage_controls(enabled);
    }

    fn set_player_ignored_by_cops(&mut self, ignored: bool) {
        if let Some(w) = self.player_wanted_mut() {
            w.ignored_by_cops = ignored;
        }
    }

    fn garage_message(&mut self, text: &str, n1: i32, time: u32, n2: i32) {
        let now = self.now();
        self.ents.garages.trigger_message(text, n1, time, n2, now);
    }

    fn garage_door_sound(px: &mut Px, sound: u16) {
        px.hooks.garage_sound(sound);
    }

    fn stat(px: &mut Px, e: StatEvent) {
        px.hooks.stat(e);
    }

    fn player_money(&mut self) -> &mut i32 {
        &mut self.ents.player_info.money
    }

    /// `CWanted::Reset` of the player: the cops let go, then a clean sheet.
    pub fn reset_player_wanted(&mut self, px: &mut Px) {
        let cops: Vec<EntityRef> = self.player_wanted().map(|w| w.cops.iter().flatten().copied().collect()).unwrap_or_default();
        for c in cops {
            if self.try_ped(c).is_none() {
                continue;
            }
            let dead = {
                let p = self.ped_mut(c);
                if let Some(cd) = p.cop.as_deref_mut() {
                    cd.is_in_pursuit = false;
                }
                p.objective = objective::NONE;
                p.prev_objective = objective::NONE;
                p.last_ped_state = ped_state::NONE;
                p.dying_or_dead()
            };
            if !dead {
                let path = px.rng.range_f32(0.0, 8.0) as i8;
                self.set_wander_path(px, c, path);
            }
        }
        if let Some(w) = self.player_wanted_mut() {
            w.cops = Default::default();
            w.current_cops = 0;
            w.initialise();
        }
    }

    /// `DestroyVehicleAndDriverAndPassengers`.
    pub fn destroy_vehicle_and_driver_and_passengers(&mut self, px: &mut Px, v: EntityRef) {
        let Some(vd) = self.try_veh(v) else { return };
        let occupants: Vec<EntityRef> = vd.driver.into_iter().chain(vd.passengers[..vd.num_max_passengers as usize].iter().flatten().copied()).collect();
        for o in occupants {
            if self.try_ped(o).is_some() {
                self.register_kill_by_player(px, o, wt::UNIDENTIFIED, false);
                self.flag_to_destroy_when_next_processed(o);
            }
        }
        self.delete(v);
    }

    /// `CWorld::CallOffChaseForArea`: police cars and peds going for the player back off from the rectangle.
    pub fn call_off_chase_for_area(&mut self, px: &mut Px, x1: f32, y1: f32, x2: f32, y2: f32) {
        let (sx, sy, ex, ey) = (x1 - 10.0, y1 - 10.0, x2 + 10.0, y2 + 10.0);
        let kinds = Kinds { vehicles: true, peds: true, ..Kinds::NONE };
        let near = self.entities_in_sector_box(sx, sy, ex, ey, kinds);
        let player = self.ents.player_ped;
        let player_vehicle = self.player_vehicle_ref();
        let now = self.now();
        for r in near {
            match self.get(r).map(|e| &e.kind) {
                Some(Kind::Vehicle(_)) => {
                    let p = self.epos(r);
                    let v = self.try_veh(r).unwrap();
                    let m = v.auto_pilot.car_mission;
                    use crate::vehicle::base::car_mission as cm;
                    if Some(r) == player_vehicle || !(p.x > sx && p.x < ex && p.y > sy && p.y < ey) || !v.flags.is_law_enforcer {
                        continue;
                    }
                    if !matches!(m, cm::RAMPLAYER_FARAWAY | cm::RAMPLAYER_CLOSE | cm::BLOCKPLAYER_FARAWAY | cm::BLOCKPLAYER_CLOSE) {
                        continue;
                    }
                    let fp = self.garage_footprint(r);
                    self.try_veh_mut(r).unwrap().auto_pilot.time_temp_action = now + 2000;
                    if fp.spheres.iter().any(|&(c, rad)| c.x + rad > x1 && c.x - rad < x2 && c.y + rad > y1 && c.y - rad < y2)
                        && let Some(ph) = self.get_mut(r).and_then(|e| e.phys.as_mut())
                    {
                        ph.move_speed.x = if p.x <= (x1 + x2) * 0.5 { ph.move_speed.x.min(0.0) } else { ph.move_speed.x.max(0.0) };
                        ph.move_speed.y = if p.y <= (y1 + y2) * 0.5 { ph.move_speed.y.min(0.0) } else { ph.move_speed.y.max(0.0) };
                    }
                }
                Some(Kind::Ped(_)) => {
                    let p = self.epos(r);
                    let pd = self.ped(r);
                    let chasing = pd.ped_in_objective == player || pd.car_in_objective.is_some() && pd.car_in_objective == player_vehicle;
                    if Some(r) == player || pd.leader == player || !(p.x > x1 && p.x < x2 && p.y > y1 && p.y < y2) || !chasing {
                        continue;
                    }
                    if matches!(pd.ped_state, ped_state::DEAD | ped_state::DIE) || !matches!(pd.objective, objective::KILL_CHAR_ON_FOOT | objective::ENTER_CAR_AS_DRIVER | objective::KILL_CHAR_ANY_MEANS) {
                        continue;
                    }
                    if pd.is_ped_in_control() {
                        if pd.ped_type == ped_type::COP {
                            self.clear_pursuit(px, r);
                        } else {
                            self.set_idle(px, r);
                        }
                        self.set_objective(px, r, objective::NONE);
                    } else {
                        let pd = self.ped_mut(r);
                        pd.prev_objective = objective::NONE;
                        pd.last_ped_state = ped_state::IDLE;
                    }
                }
                _ => {}
            }
        }
    }

    // ---- doors ----

    fn is_door_entity(&self, r: EntityRef) -> bool {
        self.get(r).is_some_and(|e| self.ents.indices.is_garage_door(e.base.model_index))
    }

    /// `RefreshDoorPointers`: the doors are looked for again when they are gone or stopped being doors.
    fn refresh_door_pointers(&mut self, g: &mut Garage, create: bool) {
        let need = create || std::mem::take(&mut g.recreate_door_on_next_refresh) || [g.door1, g.door2].into_iter().flatten().any(|d| !self.is_door_entity(d));
        if need {
            self.find_doors_entities(g);
        }
    }

    /// `FindDoorsEntities`.
    fn find_doors_entities(&mut self, g: &mut Garage) {
        (g.door1, g.door2) = (None, None);
        let kinds = Kinds { objects: true, dummies: true, ..Kinds::NONE };
        let found = self.entities_in_sector_box(g.x1, g.y1, g.x2, g.y2, kinds);
        let (body, lid) = (self.ents.indices.crusher_body(), self.ents.indices.crusher_lid());
        for r in found {
            if !self.is_door_entity(r) {
                continue;
            }
            let (p, model) = (self.epos(r), self.ent(r).base.model_index);
            if (p.x - g.centre_x()).abs() >= DISTANCE_TO_CONSIDER_DOOR_FOR_GARAGE || (p.y - g.centre_y()).abs() >= DISTANCE_TO_CONSIDER_DOOR_FOR_GARAGE {
                continue;
            }
            if model == body {
                g.door1 = Some(r);
            } else if model == lid || g.door1.is_some() {
                g.door2 = Some(r);
            } else {
                g.door1 = Some(r);
            }
        }
        let (Some(d1), Some(d2)) = (g.door1, g.door2) else { return };
        let m1 = self.ent(d1).base.model_index;
        if m1 == body || m1 == lid {
            return;
        }
        let to_centre = |w: &World, d: EntityRef| (w.epos(d) - Vec3::new(g.centre_x(), g.centre_y(), 0.0)).truncate();
        let (v1, v2) = (to_centre(self, d1), to_centre(self, d2));
        if v1.dot(v2) > 0.0 {
            if v1.length_squared() >= v2.length_squared() {
                g.door1 = g.door2;
            }
            g.door2 = None;
        }
    }

    /// `BuildRotatedDoorMatrix`.
    fn build_rotated_door_matrix(&mut self, door: EntityRef, position: f32) {
        let angle = -position * FRAC_PI_2;
        if let Some(e) = self.get_mut(door) {
            let m = &mut e.base.matrix;
            let up = Vec3::new(-angle.sin() * m.forward.y, angle.sin() * m.forward.x, angle.cos());
            m.right = cross(up, m.forward);
            m.up = up;
        }
    }

    /// `UpdateDoorsHeight`.
    fn update_doors_height(&mut self, g: &mut Garage) {
        self.refresh_door_pointers(g, false);
        for (door, z) in [(g.door1, g.door1_z), (g.door2, g.door2_z)] {
            if let Some(d) = door {
                if let Some(e) = self.get_mut(d) {
                    e.base.matrix.pos.z = g.door_pos + z;
                }
                if g.rotated_door {
                    self.build_rotated_door_matrix(d, g.door_pos / g.door_height);
                }
            }
        }
    }

    /// `UpdateCrusherAngle`.
    fn update_crusher_angle(&mut self, g: &mut Garage) {
        self.refresh_door_pointers(g, false);
        if let Some(e) = g.door2.and_then(|d| self.get_mut(d)) {
            let pos = e.base.matrix.pos;
            e.base.matrix = Matrix::rotate_x_only(2.0 * PI - g.door_pos);
            e.base.matrix.pos = pos;
        }
    }

    /// `UpdateCrusherShake`: only the rendered frame moves, so there is nothing to keep.
    fn update_crusher_shake(&mut self, g: &mut Garage, _x: f32, _y: f32) {
        self.refresh_door_pointers(g, false);
    }

    /// `SetAllDoorsBackToOriginalHeight`.
    pub fn set_all_doors_back_to_original_height(&mut self) {
        for i in 0..NUM_GARAGES {
            if !self.ents.garages.garages[i].is_used() {
                continue;
            }
            let mut g = std::mem::take(&mut self.ents.garages.garages[i]);
            self.refresh_door_pointers(&mut g, true);
            for (door, z) in [(g.door1, g.door1_z), (g.door2, g.door2_z)] {
                if let Some(d) = door {
                    if let Some(e) = self.get_mut(d) {
                        e.base.matrix.pos.z = z;
                        if let Kind::Object(o) = &mut e.kind {
                            o.object_matrix.pos.z = z;
                        }
                    }
                    if g.rotated_door {
                        self.build_rotated_door_matrix(d, 0.0);
                    }
                }
            }
            self.ents.garages.garages[i] = g;
        }
    }

    // ---- adding ----

    /// `CGarages::Init`: the statics and the crusher.
    pub fn init_garages(&mut self) {
        self.ents.garages = Garages::default();
        self.add_garage(CRUSHER_GARAGE_MIN, CRUSHER_GARAGE_MAX, garage_type::CRUSHER, 0);
    }

    /// `CGarages::AddOne`: the index of the new garage.
    pub fn add_garage(&mut self, p1: Vec3, p2: Vec3, ty: u8, target_id: i32) -> usize {
        let i = self.ents.garages.add_one_geometry(p1, p2, ty, target_id);
        if i >= NUM_GARAGES {
            return i;
        }
        let mut g = std::mem::take(&mut self.ents.garages.garages[i]);
        self.refresh_door_pointers(&mut g, true);
        if let Some(d) = g.door1 {
            let p = self.epos(d);
            (g.door1_x, g.door1_y, g.door1_z) = (p.x, p.y, p.z);
        }
        if let Some(d) = g.door2 {
            let p = self.epos(d);
            (g.door2_x, g.door2_y, g.door2_z) = (p.x, p.y, p.z);
        }
        g.door_height = g.door1.map_or(4.0, |d| self.find_door_height_for_model(self.ent(d).base.model_index));
        g.door_pos = 0.0;
        g.state = garage_state::FULLYCLOSED;
        match ty {
            garage_type::BOMBSHOP1 | garage_type::BOMBSHOP2 | garage_type::BOMBSHOP3 | garage_type::RESPRAY => {
                g.state = garage_state::OPENED;
                g.door_pos = g.door_height;
            }
            garage_type::CRUSHER => {
                g.state = garage_state::OPENED;
                g.door_pos = FRAC_PI_2;
            }
            _ => {}
        }
        if ty == garage_type::CRUSHER {
            self.update_crusher_angle(&mut g);
        } else {
            self.update_doors_height(&mut g);
        }
        self.ents.garages.garages[i] = g;
        i
    }

    /// `FindDoorHeightForMI`.
    fn find_door_height_for_model(&self, model: i16) -> f32 {
        self.ents.col_model(model).map_or(4.0, |c| Garages::door_height_for_box_height(c.bounding_box.max.z - c.bounding_box.min.z))
    }

    // ---- queries ----

    /// `IsStaticPlayerCarEntirelyInside`.
    fn is_static_player_car_entirely_inside(&self, g: &Garage) -> bool {
        let (Some(v), Some(p)) = (self.player_vehicle_ref(), self.ents.player_ped) else { return false };
        if self.car_ref(v).is_none() || self.ped(p).ped_state != ped_state::DRIVING || self.ped(p).objective == objective::LEAVE_CAR {
            return false;
        }
        let pos = self.epos(v);
        if pos.x < g.x1 || pos.x > g.x2 || pos.y < g.y1 || pos.y > g.y2 {
            return false;
        }
        let s = self.ent(v).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
        if s.x.abs() > 0.01 || s.y.abs() > 0.01 || s.z.abs() > 0.01 || s.length_squared() > 0.01 * 0.01 {
            return false;
        }
        g.is_entity_entirely_inside_3d(&self.garage_footprint(v), 0.0)
    }

    fn is_player_outside_garage(&self, g: &Garage) -> bool {
        let who = self.player_vehicle_ref().or(self.ents.player_ped);
        who.is_none_or(|w| g.is_entity_entirely_outside(&self.garage_footprint(w), 0.0))
    }

    /// `IsPlayerEntirelyInsideGarage`.
    pub fn is_player_entirely_inside_garage(&self, garage: usize) -> bool {
        let g = &self.ents.garages.garages[garage];
        self.player_vehicle_ref().or(self.ents.player_ped).is_some_and(|w| g.is_entity_entirely_inside_3d(&self.garage_footprint(w), 0.0))
    }

    /// `IsThisCarWithinGarageArea`.
    pub fn is_this_car_within_garage_area(&self, garage: usize, car: EntityRef) -> bool {
        self.ents.garages.garages[garage].is_entity_entirely_inside_3d(&self.garage_footprint(car), 0.0)
    }

    fn is_player_ped_outside(&self, g: &Garage, margin: f32) -> bool {
        self.ents.player_ped.is_none_or(|p| g.is_entity_entirely_outside(&self.garage_footprint(p), margin))
    }

    fn is_garage_empty(&mut self, g: &Garage) -> bool {
        let kinds = Kinds { vehicles: true, peds: true, ..Kinds::NONE };
        self.find_objects_intersecting_cube(Vec3::new(g.x1, g.y1, g.z1), Vec3::new(g.x2, g.y2, g.z2), 2, kinds).is_empty()
    }

    fn is_any_other_car_touching_garage(&self, g: &Garage, exception: Option<EntityRef>) -> bool {
        self.vehicle_refs().into_iter().rev().any(|v| Some(v) != exception && g.car_touches(&self.garage_footprint(v)))
    }

    fn is_any_other_ped_touching_garage(&self, g: &Garage, exception: Option<EntityRef>) -> bool {
        let Some(ex) = exception else { return false };
        self.ped_refs().into_iter().rev().any(|p| {
            p != ex && {
                let f = self.footprint_with_spheres(p, ex);
                g.is_entity_touching_3d(&f) && f.spheres.iter().any(|&(c, r)| c.x + r > g.x1 && c.x - r < g.x2 && c.y + r > g.y1 && c.y - r < g.y2 && c.z + r > g.z1 && c.z - r < g.z2)
            }
        })
    }

    fn is_any_car_blocking_door(&self, g: &Garage) -> bool {
        self.vehicle_refs().into_iter().rev().any(|v| g.car_blocks_door(&self.garage_footprint(v)))
    }

    fn count_cars_with_centre_point_within_garage(&self, g: &Garage, exception: Option<EntityRef>) -> i32 {
        self.vehicle_refs().into_iter().filter(|&v| Some(v) != exception && g.contains(self.epos(v))).count() as i32
    }

    fn vehicle_can_be_deleted(&self, v: EntityRef) -> bool {
        self.try_veh(v).is_some_and(|vd| !vd.flags.is_locked && vd.can_be_deleted(|o| self.try_ped(o).is_none_or(|p| p.can_be_deleted())))
    }

    /// `RemoveCarsBlockingDoorNotInside`.
    fn remove_cars_blocking_door_not_inside(&mut self, g: &Garage) {
        for v in self.vehicle_refs().into_iter().rev() {
            if self.get(v).is_none() || !g.is_entity_touching_3d(&self.garage_footprint(v)) {
                continue;
            }
            let p = self.epos(v);
            let outside = p.x < g.x1 || p.x > g.x2 || p.y < g.y1 || p.y > g.y2 || p.z < g.z1 || p.z > g.z2;
            if outside && self.vehicle_can_be_deleted(v) {
                self.delete(v);
            }
        }
    }

    /// `CenterCarInGarage`: a nudge towards the middle of the garage.
    fn center_car_in_garage(&mut self, g: &Garage, v: EntityRef) {
        if self.is_any_other_car_touching_garage(g, self.player_vehicle_ref()) || self.is_any_other_ped_touching_garage(g, self.ents.player_ped) {
            return;
        }
        let pos = self.epos(v);
        let offset = Vec3::new(g.centre_x() - pos.x, g.centre_y() - pos.y, 0.0);
        let distance = magnitude(offset);
        if let Some(e) = self.get_mut(v) {
            if distance < RESPRAY_CENTERING_COEFFICIENT {
                e.base.matrix.pos.x = g.centre_x();
                e.base.matrix.pos.y = g.centre_y();
            } else {
                e.base.matrix.pos.x += offset.x * RESPRAY_CENTERING_COEFFICIENT / distance;
                e.base.matrix.pos.y += offset.y * RESPRAY_CENTERING_COEFFICIENT / distance;
            }
        }
        if !g.is_entity_entirely_inside_3d(&self.garage_footprint(v), 0.1)
            && let Some(e) = self.get_mut(v)
        {
            e.base.set_position(pos);
        }
    }

    /// `IsCarSprayable`.
    fn is_car_sprayable(&self, v: EntityRef) -> bool {
        use crate::modelinfo::indices_gta3::mi;
        !matches!(self.ent(v).base.model_index, mi::FIRETRUCK | mi::AMBULAN | mi::POLICE | mi::ENFORCER | mi::BUS | mi::RHINO | mi::BARRACKS | mi::DODO | mi::COACH)
    }

    /// `StopCarFromBlowingUp`.
    pub fn stop_car_from_blowing_up(&mut self, car: EntityRef) {
        if let Some(v) = self.try_veh_mut(car) {
            v.health = v.health.max(300.0);
        }
        if let Some(c) = self.car_mut(car) {
            c.fire_blow_up_timer = 0.0;
            let status = (c.damage.engine_status as u32).max(275);
            c.damage.set_engine_status(status);
        }
    }

    /// `GivePlayerDetonator`.
    pub fn give_player_detonator(&mut self) {
        let Some(p) = self.ents.player_ped else { return };
        self.give_weapon(p, wt::DETONATOR, 1);
        if let Some(s) = self.get_weapon_slot(p, wt::DETONATOR) {
            self.ped_mut(p).weapons[s].state = WeaponState::Ready;
        }
    }

    // ---- Craig's garages ----

    fn does_craig_need_this_car(&self, g: &Garage, model: i32) -> bool {
        g.craig_slot(model).is_some_and(|i| self.ents.garages.car_types_collected[g.craig_index()] & (1 << i) == 0)
    }

    fn has_craig_collected_this_car(&self, g: &Garage, model: i32) -> bool {
        g.craig_slot(model).is_some_and(|i| self.ents.garages.car_types_collected[g.craig_index()] & (1 << i) != 0)
    }

    /// `MarkThisCarAsCollectedForCraig`: true when that completed the list.
    fn mark_this_car_as_collected_for_craig(&mut self, g: &Garage, model: i32) -> bool {
        let Some(i) = g.craig_slot(model) else { return false };
        let ct = g.craig_index();
        self.ents.garages.car_types_collected[ct] |= 1 << i;
        *self.player_money() += IMPORT_REWARD;
        if self.ents.garages.car_types_collected[ct] & 0xFFFF != 0xFFFF {
            self.garage_message("GA_13", -1, 5000, -1);
            return false;
        }
        *self.player_money() += IMPORT_ALLCARS_REWARD;
        self.garage_message("GA_14", -1, 5000, -1);
        true
    }

    // ---- hideouts ----

    /// `StoreAndRemoveCarsForThisHideout`.
    fn store_and_remove_cars_for_this_hideout(&mut self, px: &mut Px, g: &Garage, house: usize) {
        self.ents.garages.safe_houses[house] = [StoredCar::default(); NUM_GARAGE_STORED_CARS];
        let mut index = 0;
        for v in self.vehicle_refs().into_iter().rev() {
            if self.get(v).is_none() || !g.contains(self.epos(v)) || self.try_veh(v).is_some_and(|vd| vd.created_by == MISSION_VEHICLE) {
                continue;
            }
            if index < NUM_GARAGE_STORED_CARS && !g.entity_has_a_sphere_way_outside_garage(&self.garage_footprint(v), 1.0) {
                self.ents.garages.safe_houses[house][index] = self.store_car(v);
                index += 1;
            }
            self.cancel_player_entering_cars(px, Some(v));
            self.delete(v);
        }
    }

    /// `CStoredCar::StoreCar`.
    fn store_car(&self, v: EntityRef) -> StoredCar {
        let e = self.ent(v);
        let vd = self.try_veh(v).expect("vehicle");
        StoredCar {
            model_index: e.base.model_index as i32,
            pos: e.base.position(),
            angle: e.base.matrix.forward,
            primary_colour: vd.current_colour1,
            secondary_colour: vd.current_colour2,
            radio_station: vd.radio_station,
            variation_a: vd.extras[0],
            variation_b: vd.extras[1],
            bullet_proof: e.base.flags.bullet_proof,
            fire_proof: e.base.flags.fire_proof,
            explosion_proof: e.base.flags.explosion_proof,
            collision_proof: e.base.flags.collision_proof,
            melee_proof: e.base.flags.melee_proof,
            bomb_type: self.car_ref(v).map_or(Default::default(), |c| c.bomb_type),
        }
    }

    /// `CStoredCar::RestoreCar`: None while the model is still loading.
    fn restore_car(&mut self, px: &mut Px, c: &StoredCar) -> Option<EntityRef> {
        let model = c.model_index as i16;
        px.hooks.request_model(model);
        if !px.hooks.model_loaded(model) {
            return None;
        }
        let boat = self.ents.indices.is_boat_model(model);
        let r = if boat { self.new_boat(model, RANDOM_VEHICLE, px.rng)? } else { self.new_car(model, RANDOM_VEHICLE, px.rng)? };
        let rigger = self.ents.player_ped;
        let e = self.get_mut(r)?;
        e.base.set_position(c.pos);
        e.base.status = crate::entity::status::ABANDONED;
        e.base.matrix.forward = c.angle;
        e.base.matrix.right = Vec3::new(c.angle.y, -c.angle.x, 0.0);
        e.base.matrix.up = Vec3::Z;
        let f = &mut e.base.flags;
        (f.bullet_proof, f.fire_proof, f.explosion_proof, f.collision_proof, f.melee_proof) = (c.bullet_proof, c.fire_proof, c.explosion_proof, c.collision_proof, c.melee_proof);
        if let Kind::Vehicle(v) = &mut e.kind {
            v.driver = None;
            (v.current_colour1, v.current_colour2, v.radio_station) = (c.primary_colour, c.secondary_colour, c.radio_station);
            v.extras = [c.variation_a, c.variation_b];
            v.flags.freebies = false;
            v.flags.has_been_owned_by_player = true;
            v.door_lock = CarLock::Unlocked;
            if let crate::vehicle::base::VehicleSub::Car(car) = &mut v.sub {
                car.bomb_type = c.bomb_type;
                if c.bomb_type != Default::default() {
                    car.bomb_rigger = rigger;
                }
            }
        }
        Some(r)
    }

    /// `RestoreCarsForThisHideout`: true when every stored car is back.
    fn restore_cars_for_this_hideout(&mut self, px: &mut Px, house: usize) -> bool {
        for i in 0..NUM_GARAGE_STORED_CARS {
            let c = self.ents.garages.safe_houses[house][i];
            if c.has_car()
                && let Some(v) = self.restore_car(px, &c)
            {
                let _ = self.add(v);
                self.ents.garages.safe_houses[house][i] = StoredCar::default();
            }
        }
        !self.ents.garages.safe_houses[house].iter().any(|c| c.has_car())
    }

    /// `CloseHideOutGaragesBeforeSave`.
    pub fn close_hide_out_garages_before_save(&mut self, px: &mut Px) {
        for i in 0..NUM_GARAGES {
            let mut g = std::mem::take(&mut self.ents.garages.garages[i]);
            if is_hideout(g.ty) {
                if g.state != garage_state::FULLYCLOSED && (g.ty != garage_type::HIDEOUT_ONE || !self.is_any_car_blocking_door(&g)) {
                    g.state = garage_state::FULLYCLOSED;
                    self.store_and_remove_cars_for_this_hideout(px, &g, (g.ty - garage_type::HIDEOUT_ONE) as usize);
                    self.remove_cars_blocking_door_not_inside(&g);
                }
                g.door_pos = 0.0;
                self.update_doors_height(&mut g);
            }
            self.ents.garages.garages[i] = g;
        }
    }

    // ---- tidying ----

    /// `TidyUpGarage`: wrecks and upside-down cars inside a far garage are removed.
    fn tidy_up_garage(&mut self, g: &Garage) {
        for v in self.vehicle_refs().into_iter().rev() {
            if self.car_ref(v).is_none() || !g.contains(self.epos(v)) {
                continue;
            }
            let e = self.ent(v);
            if e.base.status == crate::entity::status::WRECKED || e.base.matrix.up.z < 0.5 {
                self.delete(v);
            }
        }
    }

    /// `TidyUpGarageClose`: wrecks touching a near garage are removed unless the garage is open and they are inside.
    fn tidy_up_garage_close(&mut self, g: &Garage) {
        for v in self.vehicle_refs().into_iter().rev() {
            if self.car_ref(v).is_none() || self.ent(v).base.status != crate::entity::status::WRECKED {
                continue;
            }
            let f = self.garage_footprint(v);
            if !g.is_entity_touching_3d(&f) {
                continue;
            }
            let remove = g.state == garage_state::FULLYCLOSED || f.spheres.iter().any(|&(c, r)| c.x + r < g.x1 || c.x - r > g.x2 || c.y + r < g.y1 || c.y - r > g.y2 || c.z + r < g.z1 || c.z - r > g.z2);
            if remove {
                self.delete(v);
            }
        }
    }

    // ---- the update ----

    /// `CGarages::Update`.
    pub fn update_garages(&mut self, px: &mut Px) {
        if self.ents.timer.playing_back {
            return;
        }
        let gs = &mut self.ents.garages;
        (gs.cam_should_be_outside, gs.garage_we_are_in, gs.garage_we_are_in_for_hack_avoid_first_person) = (false, None, None);
        for i in 0..NUM_GARAGES {
            if self.ents.garages.garages[i].is_used() {
                self.update_garage(px, i);
            }
        }
        if self.ents.timer.frame_counter & 0xF != 0xC {
            return;
        }
        let gs = &mut self.ents.garages;
        gs.garage_to_be_tidied += 1;
        if gs.garage_to_be_tidied >= NUM_GARAGES {
            gs.garage_to_be_tidied = 0;
        }
        let i = gs.garage_to_be_tidied;
        let g = gs.garages[i].clone();
        if !g.is_used() {
            return;
        }
        if !g.is_far(px.hooks.camera_position()) {
            self.tidy_up_garage_close(&g);
        } else {
            self.tidy_up_garage(&g);
        }
    }

    /// `CGarage::Update`.
    fn update_garage(&mut self, px: &mut Px, i: usize) {
        let mut g = std::mem::take(&mut self.ents.garages.garages[i]);
        // the original's registered reference clears itself when the car goes
        if g.target.is_some_and(|t| self.get(t).is_none()) {
            g.target = None;
        }
        self.garage_camera_checks(&g, i);
        if !(g.deactivated && g.state == garage_state::FULLYCLOSED) {
            use garage_type::*;
            match g.ty {
                RESPRAY => self.update_respray(px, &mut g),
                BOMBSHOP1 | BOMBSHOP2 | BOMBSHOP3 => self.update_bomb_shop(px, &mut g),
                MISSION => self.update_mission_garage(px, &mut g),
                COLLECTSPECIFICCARS => self.update_collect_specific_cars(px, &mut g),
                COLLECTCARS_1 | COLLECTCARS_2 | COLLECTCARS_3 => self.update_collect_cars(px, &mut g),
                FORCARTOCOMEOUTOF => self.update_for_car_to_come_out_of(px, &mut g),
                CRUSHER => self.update_crusher(px, &mut g),
                MISSION_KEEPCAR | MISSION_KEEPCAR_REMAINCLOSED => self.update_keep_car(px, &mut g),
                FOR_SCRIPT_TO_OPEN => self.update_script_open(px, &mut g, false),
                FOR_SCRIPT_TO_OPEN_AND_CLOSE => self.update_script_open(px, &mut g, true),
                HIDEOUT_ONE | HIDEOUT_TWO | HIDEOUT_THREE => self.update_hideout(px, &mut g),
                KEEPS_OPENING_FOR_SPECIFIC_CAR => self.update_keeps_opening(px, &mut g),
                _ => {}
            }
        }
        self.ents.garages.garages[i] = g;
    }

    /// The camera stays outside while the player is in a garage.
    fn garage_camera_checks(&mut self, g: &Garage, i: usize) {
        if g.ty == garage_type::CRUSHER || g.state > garage_state::CLOSEDCONTAINSCAR || g.camera_follows_player {
            return;
        }
        let Some(p) = self.ents.player_ped else { return };
        let v = self.player_vehicle_ref();
        let player_inside = g.is_entity_entirely_inside_3d(&self.garage_footprint(p), 0.25);
        let (car_not_outside, mr_whoop_near) = v.map_or((false, false), |v| {
            let whoop = self.ent(v).base.model_index == crate::modelinfo::indices_gta3::mi::MRWHOOP
                && self.ent(v).base.is_within_area_2d(g.x1 - DISTANCE_FOR_MRWHOOP_HACK, g.y1 + DISTANCE_FOR_MRWHOOP_HACK, g.x2 - DISTANCE_FOR_MRWHOOP_HACK, g.y2 + DISTANCE_FOR_MRWHOOP_HACK);
            (!g.is_entity_entirely_outside(&self.garage_footprint(v), 0.0), whoop)
        });
        let gs = &mut self.ents.garages;
        if player_inside || mr_whoop_near {
            gs.garage_we_are_in = Some(i);
            gs.cam_should_be_outside = true;
        }
        if car_not_outside {
            gs.garage_we_are_in_for_hack_avoid_first_person = Some(i);
        }
    }

    fn call_off_chase_around(&mut self, px: &mut Px, g: &Garage) {
        self.call_off_chase_for_area(px, g.x1 - DISTANCE_TO_CALL_OFF_CHASE, g.y1 - DISTANCE_TO_CALL_OFF_CHASE, g.x2 + DISTANCE_TO_CALL_OFF_CHASE, g.y2 + DISTANCE_TO_CALL_OFF_CHASE);
    }

    /// The door finishes opening: the state it ends in, and the sound.
    fn finish_opening(&mut self, px: &mut Px, g: &mut Garage, step: f32, scale: f32, next: u8) {
        if open_step(g, step, scale) {
            g.state = next;
            Self::garage_door_sound(px, sound::GARAGE_DOOR_OPENED);
        }
        self.update_doors_height(g);
    }

    fn player_distance_to(&self, g: &Garage) -> f32 {
        let p = self.player_info_pos();
        (p.truncate() - glam::Vec2::new(g.centre_x(), g.centre_y())).length_squared()
    }

    fn vehicle_is_within_activation_distance(&self, g: &Garage, v: EntityRef, distance: f32) -> bool {
        let p = self.epos(v);
        g.calc_dist_to_garage_rectangle_squared(p.x, p.y) < distance * distance
    }

    fn update_respray(&mut self, px: &mut Px, g: &mut Garage) {
        use garage_state::*;
        let step = self.time_step();
        match g.state {
            OPENED => {
                let pv = self.player_vehicle_ref();
                if self.is_static_player_car_entirely_inside(g) && !self.is_any_other_car_touching_garage(g, pv) {
                    let v = pv.expect("static check");
                    if self.is_car_sprayable(v) {
                        if *self.player_money() >= RESPRAY_PRICE || self.ents.garages.resprays_are_free {
                            g.state = CLOSING;
                            Self::set_garage_controls(px, false);
                            self.set_player_ignored_by_cops(true);
                        } else {
                            self.garage_message("GA_3", -1, 4000, -1);
                            g.state = OPENEDCONTAINSCAR;
                            px.hooks.front_end_sound_with(sound::GARAGE_NO_MONEY, 1);
                        }
                    } else {
                        self.garage_message("GA_1", -1, 4000, -1);
                        g.state = OPENEDCONTAINSCAR;
                        px.hooks.front_end_sound_with(sound::GARAGE_BAD_VEHICLE, 1);
                    }
                }
                if let Some(v) = self.player_vehicle_ref()
                    && self.vehicle_is_within_activation_distance(g, v, DISTANCE_TO_ACTIVATE_GARAGE)
                {
                    self.call_off_chase_around(px, g);
                }
            }
            CLOSING => {
                if close_step(g, step, 1.0) {
                    g.state = FULLYCLOSED;
                    g.time_to_start_action = self.now() + TIME_TO_RESPRAY;
                    Self::garage_door_sound(px, sound::GARAGE_DOOR_CLOSED);
                    Self::stat(px, StatEvent::CheckPointReachedSuccessfully);
                }
                self.update_doors_height(g);
                if let Some(v) = self.player_vehicle_ref()
                    && let Some(c) = self.car_mut(v)
                {
                    c.fire_blow_up_timer = 0.0;
                }
                self.call_off_chase_around(px, g);
            }
            FULLYCLOSED => {
                if self.now() > g.time_to_start_action {
                    self.do_respray(px, g);
                }
                self.call_off_chase_around(px, g);
            }
            OPENING => self.finish_opening(px, g, step, 1.0, OPENEDCONTAINSCAR),
            OPENEDCONTAINSCAR if self.is_player_outside_garage(g) => g.state = OPENED,
            _ => {}
        }
    }

    /// The respray itself: the car is repaired and painted, the cops forget the player.
    fn do_respray(&mut self, px: &mut Px, g: &mut Garage) {
        use garage_state::*;
        g.state = OPENING;
        px.hooks.front_end_sound_with(sound::GARAGE_OPENING, 1);
        let mut take_money = self.player_wanted().is_some_and(|w| w.level != 0);
        self.reset_player_wanted(px);
        Self::set_garage_controls(px, true);
        self.set_player_ignored_by_cops(false);
        let mut changed_colour = false;
        if let Some(v) = self.player_vehicle_ref().filter(|&v| self.car_ref(v).is_some()) {
            let (model, handling) = (self.ent(v).base.model_index, self.try_veh(v).unwrap().handling);
            if self.try_veh(v).unwrap().health < FREE_RESPRAY_HEALTH_THRESHOLD {
                take_money = true;
            }
            self.try_veh_mut(v).unwrap().health = 1000.0;
            let h = *self.ents.handling.get(handling);
            if let Some(c) = self.car_mut(v) {
                c.fire_blow_up_timer = 0.0;
                c.fix(&h);
            }
            if let Some(e) = self.get_mut(v)
                && e.base.matrix.up.z < 0.0
            {
                e.base.matrix.up = -e.base.matrix.up;
                e.base.matrix.right = -e.base.matrix.right;
            }
            if !self.car_ref(v).unwrap().flags.fixed_colour {
                let (cur1, cur2) = (self.try_veh(v).unwrap().current_colour1, self.try_veh(v).unwrap().current_colour2);
                let crate::modelinfo::Kind::Vehicle(mi_data) = &mut self.ents.models.get_mut(model as i32).expect("model").kind else { unreachable!() };
                let (mut c1, mut c2) = crate::vehicle::automobile::choose_vehicle_colour(mi_data, None);
                let mut attempt = 0;
                while attempt < 10 {
                    if c1 != cur1 || c2 != cur2 {
                        break;
                    }
                    (c1, c2) = crate::vehicle::automobile::choose_vehicle_colour(mi_data, None);
                    attempt += 1;
                }
                changed_colour = attempt < 10;
                let vd = self.try_veh_mut(v).unwrap();
                (vd.current_colour1, vd.current_colour2) = (c1, c2);
                if changed_colour {
                    for _ in 0..NUM_PARTICLES_IN_RESPRAY {
                        let pos = Vec3::new(px.rng.range_f32(g.x1 + 0.5, g.x2 - 0.5), px.rng.range_f32(g.y1 + 0.5, g.y2 - 0.5), px.rng.range_f32(g.door1_z - 3.0, g.door1_z + 1.0));
                        px.hooks.garage_paint_spray(pos, c1);
                    }
                }
            }
            self.center_car_in_garage(g, v);
        }
        if take_money {
            if !self.ents.garages.resprays_are_free {
                let m = self.player_money();
                *m = (*m - RESPRAY_PRICE).max(0);
            }
            self.garage_message("GA_2", -1, 4000, -1);
        } else if changed_colour {
            let key = if px.rng.true_false() { "GA_15" } else { "GA_16" };
            self.garage_message(key, -1, 4000, -1);
        }
        g.respray_happened = true;
    }

    fn update_bomb_shop(&mut self, px: &mut Px, g: &mut Garage) {
        use garage_state::*;
        let step = self.time_step();
        match g.state {
            OPENED => {
                let pv = self.player_vehicle_ref();
                if self.is_static_player_car_entirely_inside(g) && !self.is_any_other_car_touching_garage(g, pv) {
                    let v = pv.expect("static check");
                    if self.car_ref(v).is_some_and(|c| c.bomb_type != Default::default()) {
                        self.garage_message("GA_5", -1, 4000, -1);
                        g.state = OPENEDCONTAINSCAR;
                        px.hooks.front_end_sound_with(sound::GARAGE_BOMB_ALREADY_SET, 1);
                    } else if !self.ents.garages.bombs_are_free && *self.player_money() < BOMB_PRICE {
                        self.garage_message("GA_4", -1, 4000, -1);
                        g.state = OPENEDCONTAINSCAR;
                        px.hooks.front_end_sound_with(sound::GARAGE_NO_MONEY, 1);
                    } else {
                        g.state = CLOSING;
                        Self::set_garage_controls(px, false);
                        self.set_player_ignored_by_cops(true);
                    }
                }
            }
            CLOSING => {
                if close_step(g, step, 1.0) {
                    g.state = FULLYCLOSED;
                    g.time_to_start_action = self.now() + TIME_TO_SETUP_BOMB;
                    Self::garage_door_sound(px, sound::GARAGE_DOOR_CLOSED);
                }
                self.update_doors_height(g);
            }
            FULLYCLOSED => {
                if self.now() > g.time_to_start_action {
                    self.do_set_bomb(px, g);
                }
            }
            OPENING => self.finish_opening(px, g, step, 1.0, OPENEDCONTAINSCAR),
            OPENEDCONTAINSCAR if self.is_player_outside_garage(g) => g.state = OPENED,
            _ => {}
        }
    }

    fn do_set_bomb(&mut self, px: &mut Px, g: &mut Garage) {
        use garage_type::*;
        px.hooks.front_end_sound_with(
            match g.ty {
                BOMBSHOP1 => sound::GARAGE_BOMB1_SET,
                BOMBSHOP2 => sound::GARAGE_BOMB2_SET,
                _ => sound::GARAGE_BOMB3_SET,
            },
            1,
        );
        g.state = garage_state::OPENING;
        if !self.ents.garages.bombs_are_free {
            let m = self.player_money();
            *m = (*m - BOMB_PRICE).max(0);
        }
        let rigger = self.ents.player_ped;
        if let Some(v) = self.player_vehicle_ref().filter(|&v| self.car_ref(v).is_some()) {
            let c = self.car_mut(v).unwrap();
            c.bomb_type = Garages::bomb_type_for_garage_type(g.ty);
            c.bomb_rigger = rigger;
            if g.ty == BOMBSHOP3 {
                self.give_player_detonator();
            }
            Self::stat(px, StatEvent::KgsOfExplosivesUsed(KGS_OF_EXPLOSIVES_IN_BOMB));
        }
        let mode = px.hooks.pad_mode();
        match g.ty {
            BOMBSHOP1 => px.hooks.help_message(if mode == 3 { "GA_6B" } else { "GA_6" }),
            BOMBSHOP2 => px.hooks.help_message(if mode == 3 { "GA_7B" } else { "GA_7" }),
            _ => px.hooks.help_message("GA_8"),
        }
        Self::set_garage_controls(px, true);
        self.set_player_ignored_by_cops(false);
    }

    fn update_mission_garage(&mut self, px: &mut Px, g: &mut Garage) {
        use garage_state::*;
        let step = self.time_step();
        match g.state {
            OPENED => {
                let pv = self.player_vehicle_ref();
                if self.player_distance_to(g) > DISTANCE_TO_CLOSE_MISSION_GARAGE.powi(2) {
                    if self.ents.timer.frame_counter & 0x1F == 0 && !self.is_any_other_car_touching_garage(g, None) {
                        g.state = CLOSING;
                        g.closing_without_target_car = true;
                    }
                } else if let Some(t) = g.target.filter(|_| pv.is_none())
                    && g.is_entity_entirely_inside_3d(&self.garage_footprint(t), 0.0)
                    && !self.is_any_other_car_touching_garage(g, Some(t))
                    && self.is_player_ped_outside(g, 2.0)
                {
                    Self::set_garage_controls(px, false);
                    self.set_player_ignored_by_cops(true);
                    g.state = CLOSING;
                    g.closing_without_target_car = false;
                }
            }
            CLOSING => {
                if close_step(g, step, 1.0) {
                    Self::garage_door_sound(px, sound::GARAGE_DOOR_CLOSED);
                    if g.closing_without_target_car {
                        g.state = FULLYCLOSED;
                    } else {
                        if let Some(t) = g.target.take() {
                            g.state = CLOSEDCONTAINSCAR;
                            self.destroy_vehicle_and_driver_and_passengers(px, t);
                        } else {
                            g.state = FULLYCLOSED;
                        }
                        Self::set_garage_controls(px, true);
                        self.set_player_ignored_by_cops(false);
                    }
                }
                self.update_doors_height(g);
            }
            FULLYCLOSED => {
                if let Some(t) = g.target
                    && self.player_vehicle_ref() == Some(t)
                    && self.vehicle_is_within_activation_distance(g, t, DISTANCE_TO_ACTIVATE_GARAGE)
                {
                    g.state = OPENING;
                }
            }
            OPENING => self.finish_opening(px, g, step, 1.0, OPENED),
            _ => {}
        }
    }

    fn update_collect_specific_cars(&mut self, px: &mut Px, g: &mut Garage) {
        use crate::modelinfo::indices_gta3::mi;
        use garage_state::*;
        let step = self.time_step();
        let player_in_target_model = |w: &World, g: &Garage| w.player_vehicle_ref().filter(|&v| w.ent(v).base.model_index as i32 == g.target_model_index);
        match g.state {
            OPENED => {
                if let Some(v) = player_in_target_model(self, g) {
                    g.target = Some(v);
                }
                if self.player_vehicle_ref().is_none() {
                    if let Some(t) = g.target.filter(|&t| g.is_entity_entirely_inside_3d(&self.garage_footprint(t), 0.0) && !self.is_any_other_car_touching_garage(g, Some(t))) {
                        let _ = t;
                        if self.is_player_ped_outside(g, 2.0) {
                            Self::set_garage_controls(px, false);
                            self.set_player_ignored_by_cops(true);
                            g.state = CLOSING;
                        }
                    } else {
                        let p = self.player_info_pos();
                        if (p.x - g.centre_x()).abs() > DISTANCE_TO_CLOSE_COLLECTSPECIFICCARS_GARAGE || (p.y - g.centre_y()).abs() > DISTANCE_TO_CLOSE_COLLECTSPECIFICCARS_GARAGE {
                            g.state = CLOSING;
                            g.target = None;
                        }
                    }
                }
            }
            CLOSING => {
                if close_step(g, step, 1.0) {
                    g.state = FULLYCLOSED;
                    Self::garage_door_sound(px, sound::GARAGE_DOOR_CLOSED);
                    if let Some(t) = g.target.take() {
                        self.destroy_vehicle_and_driver_and_passengers(px, t);
                        Self::set_garage_controls(px, true);
                        self.set_player_ignored_by_cops(false);
                        let gs = &mut self.ents.garages;
                        let reward = match g.target_model_index {
                            m if m == mi::POLICE as i32 => {
                                let r = REWARD_FOR_FIRST_POLICE_CAR * (MAX_POLICE_CARS_TO_COLLECT - gs.police_cars_collected) / MAX_POLICE_CARS_TO_COLLECT;
                                gs.police_cars_collected += 1;
                                r
                            }
                            m if m == mi::SECURICA as i32 => {
                                let r = REWARD_FOR_FIRST_BANK_VAN * (MAX_BANK_VANS_TO_COLLECT - gs.bank_vans_collected) / MAX_BANK_VANS_TO_COLLECT;
                                gs.bank_vans_collected += 1;
                                r
                            }
                            _ => 0,
                        };
                        if reward > 0 {
                            *self.player_money() += reward;
                            self.garage_message("GA_10", reward, 4000, -1);
                            px.hooks.front_end_sound_with(sound::GARAGE_VEHICLE_ACCEPTED, 1);
                        } else {
                            self.garage_message("GA_11", -1, 4000, -1);
                            px.hooks.front_end_sound_with(sound::GARAGE_VEHICLE_DECLINED, 1);
                        }
                    }
                }
                self.update_doors_height(g);
            }
            FULLYCLOSED => {
                if let Some(v) = player_in_target_model(self, g)
                    && self.vehicle_is_within_activation_distance(g, v, DISTANCE_TO_ACTIVATE_GARAGE)
                {
                    g.state = OPENING;
                }
            }
            OPENING => {
                if let Some(v) = player_in_target_model(self, g) {
                    g.target = Some(v);
                }
                self.finish_opening(px, g, step, 1.0, OPENED);
            }
            _ => {}
        }
    }

    fn update_collect_cars(&mut self, px: &mut Px, g: &mut Garage) {
        use garage_state::*;
        let step = self.time_step();
        let needed = |w: &World, g: &Garage| w.player_vehicle_ref().filter(|&v| w.does_craig_need_this_car(g, w.ent(v).base.model_index as i32));
        match g.state {
            OPENED => {
                if let Some(v) = needed(self, g) {
                    g.target = Some(v);
                }
                let p = self.player_info_pos();
                if (p.x - g.centre_x()).abs() > DISTANCE_TO_CLOSE_COLLECTCARS_GARAGE || (p.y - g.centre_y()).abs() > DISTANCE_TO_CLOSE_COLLECTCARS_GARAGE {
                    g.state = CLOSING;
                    g.target = None;
                    return;
                }
                if let Some(t) = g.target
                    && self.player_vehicle_ref().is_none()
                    && g.is_entity_entirely_inside_3d(&self.garage_footprint(t), 0.0)
                    && !self.is_any_other_car_touching_garage(g, Some(t))
                    && self.is_player_ped_outside(g, 2.0)
                {
                    let healthy = self.car_ref(t).is_none_or(|c| c.damage.engine_status <= engine::ON_FIRE && c.fire_blow_up_timer == 0.0);
                    if healthy && self.ent(t).base.status != crate::entity::status::WRECKED {
                        Self::set_garage_controls(px, false);
                        self.set_player_ignored_by_cops(true);
                        g.state = CLOSING;
                        px.hooks.camera_directly_behind_ped();
                    }
                }
            }
            CLOSING => {
                if close_step(g, step, 1.0) {
                    g.state = FULLYCLOSED;
                    Self::garage_door_sound(px, sound::GARAGE_DOOR_CLOSED);
                    if let Some(t) = g.target.take() {
                        let model = self.ent(t).base.model_index as i32;
                        self.mark_this_car_as_collected_for_craig(g, model);
                        self.destroy_vehicle_and_driver_and_passengers(px, t);
                        Self::set_garage_controls(px, true);
                        self.set_player_ignored_by_cops(false);
                    }
                }
                self.update_doors_height(g);
            }
            FULLYCLOSED => {
                if let Some(v) = self.player_vehicle_ref() {
                    let p = self.epos(v);
                    if g.calc_smallest_dist_to_garage_door_squared(p.x, p.y) < DISTANCE_TO_ACTIVATE_GARAGE * DISTANCE_TO_ACTIVATE_GARAGE {
                        let model = self.ent(v).base.model_index as i32;
                        if self.does_craig_need_this_car(g, model) {
                            if self.try_veh(v).is_some_and(|vd| vd.created_by == MISSION_VEHICLE) {
                                self.garage_message("GA_1A", -1, 5000, -1);
                            } else {
                                g.state = OPENING;
                            }
                        } else if self.has_craig_collected_this_car(g, model) {
                            self.garage_message("GA_20", -1, 5000, -1);
                        } else if magnitude(self.find_player_speed()) < MAX_SPEED_TO_SHOW_COLLECTED_MESSAGE {
                            self.garage_message("GA_19", -1, 5000, -1);
                        }
                    }
                }
                g.target = None;
            }
            OPENING => {
                if let Some(v) = needed(self, g) {
                    g.target = Some(v);
                }
                self.finish_opening(px, g, step, 1.0, OPENED);
            }
            _ => {}
        }
    }

    fn update_for_car_to_come_out_of(&mut self, px: &mut Px, g: &mut Garage) {
        use garage_state::*;
        let step = self.time_step();
        match g.state {
            OPENED => {
                if self.is_garage_empty(g) {
                    g.state = CLOSING;
                }
            }
            CLOSING => {
                if close_step(g, step, 1.0) {
                    g.state = FULLYCLOSED;
                    Self::garage_door_sound(px, sound::GARAGE_DOOR_CLOSED);
                }
                if !self.is_garage_empty(g) {
                    g.state = OPENING;
                }
            }
            OPENING => self.finish_opening(px, g, step, 1.0, OPENED),
            _ => {}
        }
    }

    fn update_crusher(&mut self, px: &mut Px, g: &mut Garage) {
        use garage_state::*;
        let step = self.time_step();
        match g.state {
            OPENED => {
                let pool = &self.ents.pools.vehicles;
                let (size, frame) = (pool.size(), self.ents.timer.frame_counter as usize);
                let span = size * (frame % CRUSHER_VEHICLE_TEST_SPAN) / CRUSHER_VEHICLE_TEST_SPAN..size * (frame % CRUSHER_VEHICLE_TEST_SPAN + 1) / CRUSHER_VEHICLE_TEST_SPAN;
                let candidates: Vec<EntityRef> = pool.iter().filter(|(i, _)| span.contains(i)).map(|(i, _)| crate::entity::EntityRef { pool: crate::entity::PoolKind::Vehicle, handle: pool.handle_of(i) }).collect();
                for v in candidates {
                    if self.car_ref(v).is_some() && g.is_entity_entirely_inside_3d(&self.garage_footprint(v), 0.0) {
                        g.state = CLOSING;
                        g.target = Some(v);
                    }
                }
            }
            CLOSING => {
                if let Some(t) = g.target {
                    g.door_pos = (g.door_pos - CRUSHER_CRANE_SPEED * step).max(0.0);
                    let down = g.door_pos < 2.0 * PI / 5.0;
                    if let Some(e) = self.get_mut(t) {
                        e.base.flags.uses_collision &= !down;
                        if let Some(ph) = e.phys.as_mut() {
                            if down {
                                ph.flags.affected_by_gravity = false;
                                ph.move_speed = Vec3::ZERO;
                            } else {
                                ph.move_speed *= 0.8f32.powf(step);
                            }
                        }
                    }
                    if g.door_pos == 0.0 {
                        self.ents.garages.crushed_car_id = t.handle;
                        let vd = self.try_veh(t).expect("vehicle");
                        let value = self.ents.handling.get(vd.handling).monetary_value as f32;
                        let reward = CRUSHER_MAX_REWARD.min(CRUSHER_MIN_REWARD + value * vd.health * CRUSHER_REWARD_COEFFICIENT);
                        let m = self.player_money();
                        *m = (*m as f32 + reward) as i32;
                        self.destroy_vehicle_and_driver_and_passengers(px, t);
                        Self::stat(px, StatEvent::CarsCrushed);
                        g.target = None;
                        g.state = AFTERDROPOFF;
                        g.time_to_start_action = self.now() + TIME_TO_CRUSH_CAR;
                        Self::garage_door_sound(px, sound::GARAGE_DOOR_CLOSED);
                    }
                } else {
                    g.state = OPENING;
                }
                self.update_crusher_angle(g);
            }
            AFTERDROPOFF => {
                if self.now() <= g.time_to_start_action {
                    // the original's precedence turns `(rand & 0xFF) - 128` into `rand & 127`
                    let (x, y) = ((px.rng.number() & 127) as f32 * 0.0002, (px.rng.number() & 127) as f32 * 0.0002);
                    self.update_crusher_shake(g, x, y);
                } else {
                    self.update_crusher_shake(g, 0.0, 0.0);
                    g.state = OPENING;
                }
            }
            OPENING => {
                g.door_pos = FRAC_PI_2.min(g.door_pos + step * CRUSHER_CRANE_SPEED);
                if g.door_pos == FRAC_PI_2 {
                    g.state = OPENED;
                    Self::garage_door_sound(px, sound::GARAGE_DOOR_OPENED);
                }
                self.update_crusher_angle(g);
            }
            _ => {}
        }
        if self.player_vehicle_ref().is_none()
            && self.ents.timer.frame_counter & 0x1F == 0x17
            && let Some(p) = self.ents.player_ped
            && g.is_entity_entirely_inside(&self.garage_footprint(p))
        {
            self.inflict_damage(px, p, Hit { by: None, weapon: wt::RAMMEDBYCAR, damage: 300.0, piece: crate::ped::data::piece::TORSO, direction: 0 });
        }
    }

    fn update_keep_car(&mut self, px: &mut Px, g: &mut Garage) {
        use garage_state::*;
        let step = self.time_step();
        match g.state {
            OPENED => {
                if self.player_distance_to(g) > DISTANCE_TO_CLOSE_MISSION_GARAGE.powi(2) && !self.is_any_other_car_touching_garage(g, None) {
                    g.state = CLOSING;
                    g.closing_without_target_car = true;
                } else if g.target.is_some() && g.target == self.player_vehicle_ref() && self.is_static_player_car_entirely_inside(g) && !self.is_any_car_blocking_door(g) {
                    Self::set_garage_controls(px, false);
                    self.set_player_ignored_by_cops(true);
                    g.state = CLOSING;
                    g.closing_without_target_car = false;
                }
            }
            CLOSING => {
                if close_step(g, step, 1.0) {
                    Self::garage_door_sound(px, sound::GARAGE_DOOR_CLOSED);
                    if g.closing_without_target_car {
                        g.state = FULLYCLOSED;
                    } else {
                        if g.target.take().is_some() {
                            g.state = CLOSEDCONTAINSCAR;
                            g.time_to_start_action = self.now() + TIME_TO_PROCESS_KEEPCAR_GARAGE;
                        } else {
                            g.state = FULLYCLOSED;
                        }
                        Self::set_garage_controls(px, true);
                        self.set_player_ignored_by_cops(false);
                    }
                }
                self.update_doors_height(g);
            }
            FULLYCLOSED => {
                if let Some(t) = g.target
                    && self.player_vehicle_ref() == Some(t)
                    && self.vehicle_is_within_activation_distance(g, t, DISTANCE_TO_ACTIVATE_KEEPCAR_GARAGE)
                {
                    g.state = OPENING;
                }
            }
            OPENING => self.finish_opening(px, g, step, 1.0, OPENED),
            CLOSEDCONTAINSCAR if g.ty == garage_type::MISSION_KEEPCAR && self.now() > g.time_to_start_action => g.state = OPENING,
            _ => {}
        }
    }

    fn update_script_open(&mut self, px: &mut Px, g: &mut Garage, can_close: bool) {
        use garage_state::*;
        let step = self.time_step();
        match g.state {
            CLOSING if can_close => {
                if close_step(g, step, 1.0) {
                    g.state = FULLYCLOSED;
                    Self::garage_door_sound(px, sound::GARAGE_DOOR_CLOSED);
                }
                self.update_doors_height(g);
            }
            OPENING => self.finish_opening(px, g, step, 1.0, OPENED),
            _ => {}
        }
    }

    fn update_hideout(&mut self, px: &mut Px, g: &mut Garage) {
        use garage_state::*;
        let step = self.time_step();
        let house = (g.ty - garage_type::HIDEOUT_ONE) as usize;
        let max_cars = Garages::find_max_num_stored_cars_for_garage(g.ty);
        let player = self.player_info_pos();
        let pv = self.player_vehicle_ref();
        match g.state {
            OPENED => {
                let distance = g.calc_dist_to_garage_rectangle_squared(player.x, player.y);
                let player_far = (distance > DISTANCE_TO_CLOSE_HIDEOUT_GARAGE_IN_CAR.powi(2) || pv.is_none() && distance > DISTANCE_TO_CLOSE_HIDEOUT_GARAGE_ON_FOOT.powi(2)) && !self.is_any_car_blocking_door(g);
                // full for the car the player is driving, or the player is out of sight
                if player_far || pv.is_some_and(|v| self.count_cars_with_centre_point_within_garage(g, Some(v)) >= max_cars) {
                    g.state = CLOSING;
                } else if distance > DISTANCE_TO_FORCE_CLOSE_HIDEOUT_GARAGE.powi(2) {
                    g.state = CLOSING;
                    self.remove_cars_blocking_door_not_inside(g);
                }
            }
            CLOSING => {
                close_step(g, step, HIDEOUT_DOOR_SPEED_COEFFICIENT);
                if !self.is_player_outside_garage(g) {
                    g.state = OPENING;
                } else if g.door_pos == 0.0 {
                    Self::garage_door_sound(px, sound::GARAGE_DOOR_CLOSED);
                    g.state = FULLYCLOSED;
                    self.store_and_remove_cars_for_this_hideout(px, g, house);
                }
                self.update_doors_height(g);
            }
            FULLYCLOSED => {
                let distance = g.calc_dist_to_garage_rectangle_squared(player.x, player.y);
                if distance < DISTANCE_TO_OPEN_HIDEOUT_GARAGE_ON_FOOT.powi(2) || distance < DISTANCE_TO_OPEN_HIDEOUT_GARAGE_IN_CAR.powi(2) && pv.is_some() {
                    if let Some(v) = pv.filter(|_| self.ents.garages.count_cars_in_hideout_garage(g.ty) >= max_cars) {
                        let now = self.now();
                        if let Some(d) = g.door1
                            && (self.epos(v) - self.epos(d)).truncate().length_squared() < DISTANCE_TO_SHOW_HIDEOUT_MESSAGE.powi(2)
                            && now.wrapping_sub(self.ents.garages.last_time_help_message) > TIME_BETWEEN_HIDEOUT_MESSAGES
                        {
                            px.hooks.help_message("GA_21");
                            self.ents.garages.last_time_help_message = now;
                        }
                    } else if self.restore_cars_for_this_hideout(px, house) {
                        g.state = OPENING;
                    }
                }
            }
            OPENING => self.finish_opening(px, g, step, HIDEOUT_DOOR_SPEED_COEFFICIENT, OPENED),
            _ => {}
        }
    }

    fn update_keeps_opening(&mut self, px: &mut Px, g: &mut Garage) {
        use garage_state::*;
        let step = self.time_step();
        match g.state {
            OPENED => {
                if self.player_distance_to(g) > DISTANCE_TO_CLOSE_MISSION_GARAGE.powi(2)
                    && let Some(t) = g.target
                    && g.is_entity_entirely_outside(&self.garage_footprint(t), 0.0)
                    && !self.is_any_other_car_touching_garage(g, None)
                {
                    g.state = CLOSING;
                    g.closing_without_target_car = true;
                }
            }
            CLOSING => {
                if close_step(g, step, 1.0) {
                    g.state = FULLYCLOSED;
                    Self::garage_door_sound(px, sound::GARAGE_DOOR_CLOSED);
                }
                self.update_doors_height(g);
            }
            FULLYCLOSED => {
                if let Some(t) = g.target
                    && self.player_vehicle_ref() == Some(t)
                    && self.vehicle_is_within_activation_distance(g, t, DISTANCE_TO_ACTIVATE_GARAGE)
                {
                    g.state = OPENING;
                }
            }
            OPENING => self.finish_opening(px, g, step, 1.0, OPENED),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::status;
    use crate::general::Rng;
    use crate::ped::ctx::PedHooks;
    use crate::vehicle::base::car_mission;
    use crate::world::tests::{world_with_cars, LANDSTAL_MODEL};

    #[derive(Default)]
    struct Spy {
        controls: Vec<bool>,
        stats: Vec<StatEvent>,
        help: Vec<String>,
        sprays: usize,
    }
    impl PedHooks for Spy {
        fn garage_controls(&mut self, enabled: bool) {
            self.controls.push(enabled);
        }
        fn stat(&mut self, stat: StatEvent) {
            self.stats.push(stat);
        }
        fn help_message(&mut self, key: &str) {
            self.help.push(key.to_owned());
        }
        fn garage_paint_spray(&mut self, _pos: Vec3, _colour: u8) {
            self.sprays += 1;
        }
        fn model_loaded(&mut self, _model: i16) -> bool {
            true
        }
    }

    const MIN: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    const MAX: Vec3 = Vec3::new(10.0, 10.0, 5.0);

    fn world_with_player() -> (World, EntityRef) {
        let mut w = world_with_cars();
        let mut hooks = Spy::default();
        let p = w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut hooks }, ped_type::PLAYER1, -1, None).unwrap();
        w.ped_mut(p).player = Some(Default::default());
        w.ents.player_ped = Some(p);
        w.get_mut(p).unwrap().base.set_position(Vec3::new(30.0, 5.0, 0.0));
        (w, p)
    }

    fn car_at(w: &mut World, pos: Vec3) -> EntityRef {
        let r = w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, &mut Rng::default()).unwrap();
        w.get_mut(r).unwrap().base.set_position(pos);
        w.get_mut(r).unwrap().base.status = status::PHYSICS;
        w.add(r).unwrap();
        r
    }

    /// The player drives `car`.
    fn put_player_in(w: &mut World, p: EntityRef, car: EntityRef) {
        let pos = w.epos(car);
        w.get_mut(p).unwrap().base.set_position(pos);
        let pd = w.ped_mut(p);
        (pd.in_vehicle, pd.my_vehicle, pd.ped_state) = (true, Some(car), ped_state::DRIVING);
        w.try_veh_mut(car).unwrap().driver = Some(p);
    }

    fn run(w: &mut World, spy: &mut Spy, n: usize) {
        for _ in 0..n {
            w.update_garages(&mut Px { rng: &mut Rng::default(), hooks: spy });
        }
    }

    fn run_until(w: &mut World, spy: &mut Spy, i: usize, state: u8) {
        for _ in 0..1000 {
            if w.ents.garages.garages[i].state == state {
                return;
            }
            run(w, spy, 1);
        }
        panic!("garage never reached state {state}, is in {}", w.ents.garages.garages[i].state);
    }

    #[test]
    fn a_new_respray_garage_is_open_and_a_driver_with_money_is_sprayed_when_the_door_has_closed() {
        let (mut w, p) = world_with_player();
        let mut spy = Spy::default();
        let i = w.add_garage(MIN, MAX, garage_type::RESPRAY, 0);
        let g = &w.ents.garages.garages[i];
        assert_eq!((g.state, g.door_pos, g.door_height), (garage_state::OPENED, 4.0, 4.0));
        let car = car_at(&mut w, Vec3::new(5.0, 5.0, 1.0));
        put_player_in(&mut w, p, car);
        w.ents.player_info.money = 1500;
        w.try_veh_mut(car).unwrap().health = 500.0;
        run(&mut w, &mut spy, 1);
        assert_eq!(w.ents.garages.garages[i].state, garage_state::CLOSING);
        assert_eq!(spy.controls, vec![false]);
        assert!(w.player_wanted().unwrap().ignored_by_cops);
        run_until(&mut w, &mut spy, i, garage_state::FULLYCLOSED);
        assert_eq!(spy.stats, vec![StatEvent::CheckPointReachedSuccessfully]);
        // nothing happens until the two seconds are up
        run(&mut w, &mut spy, 1);
        assert_eq!(w.ents.garages.garages[i].state, garage_state::FULLYCLOSED);
        w.ents.timer.time_in_ms = 10_000;
        run(&mut w, &mut spy, 1);
        assert_eq!(w.ents.garages.garages[i].state, garage_state::OPENING);
        assert_eq!((w.ents.player_info.money, w.try_veh(car).unwrap().health, w.ents.garages.message_id.as_str()), (500, 1000.0, "GA_2"));
        assert_eq!(spy.controls, vec![false, true]);
        assert!(!w.player_wanted().unwrap().ignored_by_cops);
        assert!(w.ents.garages.has_respray_happened(i) && !w.ents.garages.has_respray_happened(i));
        run_until(&mut w, &mut spy, i, garage_state::OPENEDCONTAINSCAR);
    }

    #[test]
    fn a_respray_without_the_money_or_for_a_police_car_is_refused() {
        let (mut w, p) = world_with_player();
        let mut spy = Spy::default();
        let i = w.add_garage(MIN, MAX, garage_type::RESPRAY, 0);
        let car = car_at(&mut w, Vec3::new(5.0, 5.0, 1.0));
        put_player_in(&mut w, p, car);
        w.ents.player_info.money = 999;
        run(&mut w, &mut spy, 1);
        assert_eq!((w.ents.garages.garages[i].state, w.ents.garages.message_id.as_str()), (garage_state::OPENEDCONTAINSCAR, "GA_3"));
        // once the player drives out the garage is ready for the next customer
        w.get_mut(p).unwrap().base.set_position(Vec3::new(50.0, 5.0, 0.0));
        w.get_mut(car).unwrap().base.set_position(Vec3::new(50.0, 5.0, 1.0));
        run(&mut w, &mut spy, 1);
        assert_eq!(w.ents.garages.garages[i].state, garage_state::OPENED);
        // police cars are too hot
        w.ents.player_info.money = 5000;
        w.get_mut(car).unwrap().base.model_index = crate::modelinfo::indices_gta3::mi::POLICE;
        w.get_mut(car).unwrap().base.set_position(Vec3::new(5.0, 5.0, 1.0));
        run(&mut w, &mut spy, 1);
        assert_eq!((w.ents.garages.garages[i].state, w.ents.garages.message_id.as_str()), (garage_state::OPENEDCONTAINSCAR, "GA_1"));
    }

    #[test]
    fn a_bomb_shop_fits_the_bomb_after_the_door_has_closed_and_takes_the_money() {
        let (mut w, p) = world_with_player();
        let mut spy = Spy::default();
        let i = w.add_garage(MIN, MAX, garage_type::BOMBSHOP2, 0);
        let car = car_at(&mut w, Vec3::new(5.0, 5.0, 1.0));
        put_player_in(&mut w, p, car);
        w.ents.player_info.money = 2000;
        run_until(&mut w, &mut spy, i, garage_state::FULLYCLOSED);
        w.ents.timer.time_in_ms = 10_000;
        run(&mut w, &mut spy, 1);
        assert_eq!(w.ents.garages.garages[i].state, garage_state::OPENING);
        let c = w.car_ref(car).unwrap();
        assert_eq!((c.bomb_type, c.bomb_rigger), (crate::vehicle::automobile::BombType::OnIgnition, Some(p)));
        assert_eq!((w.ents.player_info.money, spy.help.clone()), (1000, vec!["GA_7".to_owned()]));
        assert!(spy.stats.contains(&StatEvent::KgsOfExplosivesUsed(10)));
        // a car that has a bomb already is turned away
        run_until(&mut w, &mut spy, i, garage_state::OPENEDCONTAINSCAR);
        run(&mut w, &mut spy, 1);
        assert_eq!(w.ents.garages.garages[i].state, garage_state::OPENEDCONTAINSCAR);
        w.get_mut(p).unwrap().base.set_position(Vec3::new(50.0, 5.0, 0.0));
        w.get_mut(car).unwrap().base.set_position(Vec3::new(50.0, 5.0, 1.0));
        run(&mut w, &mut spy, 1);
        w.get_mut(car).unwrap().base.set_position(Vec3::new(5.0, 5.0, 1.0));
        run(&mut w, &mut spy, 1);
        assert_eq!(w.ents.garages.message_id, "GA_5");
    }

    #[test]
    fn a_mission_garage_swallows_the_target_car_once_the_player_has_stepped_out() {
        let (mut w, _) = world_with_player();
        let mut spy = Spy::default();
        let i = w.add_garage(MIN, MAX, garage_type::MISSION, 0);
        let car = car_at(&mut w, Vec3::new(5.0, 5.0, 1.0));
        let g = &mut w.ents.garages.garages[i];
        (g.state, g.door_pos, g.target) = (garage_state::OPENED, g.door_height, Some(car));
        run(&mut w, &mut spy, 1);
        assert_eq!(w.ents.garages.garages[i].state, garage_state::CLOSING);
        assert_eq!(spy.controls, vec![false]);
        run_until(&mut w, &mut spy, i, garage_state::CLOSEDCONTAINSCAR);
        assert_eq!((w.ents.pools.vehicles.used(), w.ents.garages.garages[i].target, spy.controls.clone()), (0, None, vec![false, true]));
        assert!(w.ents.garages.has_car_been_dropped_off_yet(i));
    }

    #[test]
    fn the_crusher_crushes_a_car_inside_it_and_pays_for_it() {
        let (mut w, _) = world_with_player();
        let mut spy = Spy::default();
        w.init_garages();
        assert_eq!((w.ents.garages.num_garages, w.ents.garages.garages[0].ty, w.ents.garages.garages[0].state), (1, garage_type::CRUSHER, garage_state::OPENED));
        let car = car_at(&mut w, Vec3::new(1142.0, 60.0, 0.5));
        w.ents.timer.frame_counter = 0;
        run(&mut w, &mut spy, 1);
        assert_eq!(w.ents.garages.garages[0].state, garage_state::CLOSING);
        run_until(&mut w, &mut spy, 0, garage_state::AFTERDROPOFF);
        assert!(w.ents.garages.has_car_been_crushed(car.handle));
        // 25 + 25000 * 1000 / 500000
        assert_eq!((w.ents.player_info.money, w.ents.pools.vehicles.used()), (75, 0));
        assert!(spy.stats.contains(&StatEvent::CarsCrushed));
        w.ents.timer.time_in_ms = 10_000;
        run(&mut w, &mut spy, 1);
        run_until(&mut w, &mut spy, 0, garage_state::OPENED);
    }

    #[test]
    fn a_hideout_keeps_the_cars_when_it_closes_and_gives_them_back_when_the_player_returns() {
        let (mut w, p) = world_with_player();
        let mut spy = Spy::default();
        let i = w.add_garage(MIN, MAX, garage_type::HIDEOUT_TWO, 0);
        let (a, b) = (car_at(&mut w, Vec3::new(3.0, 5.0, 1.0)), car_at(&mut w, Vec3::new(7.0, 5.0, 1.0)));
        w.try_veh_mut(a).unwrap().current_colour1 = 7;
        w.try_veh_mut(b).unwrap().flags.has_been_owned_by_player = false;
        let g = &mut w.ents.garages.garages[i];
        (g.state, g.door_pos) = (garage_state::OPENED, g.door_height);
        run_until(&mut w, &mut spy, i, garage_state::FULLYCLOSED);
        assert_eq!(w.ents.pools.vehicles.used(), 0);
        let stored = w.ents.garages.safe_houses[1];
        assert_eq!(stored.iter().filter(|c| c.has_car()).count(), 2);
        assert!(stored.iter().any(|c| c.primary_colour == 7 && c.pos.x == 3.0));
        assert_eq!(w.ents.garages.count_cars_in_hideout_garage(garage_type::HIDEOUT_TWO), 2);
        // the player walks up to the door
        w.get_mut(p).unwrap().base.set_position(Vec3::new(-1.0, 5.0, 0.0));
        run(&mut w, &mut spy, 1);
        assert_eq!((w.ents.garages.garages[i].state, w.ents.pools.vehicles.used(), w.ents.garages.count_cars_in_hideout_garage(garage_type::HIDEOUT_TWO)), (garage_state::OPENING, 2, 0));
        let back: Vec<_> = w.vehicle_refs();
        assert!(back.iter().all(|&v| w.try_veh(v).unwrap().flags.has_been_owned_by_player && w.ent(v).base.status == status::ABANDONED));
        assert!(back.iter().any(|&v| w.try_veh(v).unwrap().current_colour1 == 7));
    }

    #[test]
    fn a_hideout_will_not_take_a_car_in_that_cannot_be_stored() {
        let (mut w, p) = world_with_player();
        let mut spy = Spy::default();
        let i = w.add_garage(MIN, MAX, garage_type::HIDEOUT_ONE, 0);
        let g = &mut w.ents.garages.garages[i];
        (g.state, g.door_pos) = (garage_state::OPENED, g.door_height);
        // the one car of this garage is stored and the player arrives in another
        w.ents.garages.safe_houses[0][0] = StoredCar { model_index: LANDSTAL_MODEL as i32, ..Default::default() };
        w.ents.garages.garages[i].state = garage_state::FULLYCLOSED;
        w.ents.garages.garages[i].door_pos = 0.0;
        let car = car_at(&mut w, Vec3::new(-6.0, 5.0, 1.0));
        put_player_in(&mut w, p, car);
        w.ents.timer.time_in_ms = 100_000;
        run(&mut w, &mut spy, 1);
        assert_eq!((w.ents.garages.garages[i].state, spy.help.clone()), (garage_state::FULLYCLOSED, vec![]));
    }

    #[test]
    fn craigs_garage_pays_for_each_wanted_car_once_and_a_bonus_for_the_whole_list() {
        let (mut w, _) = world_with_player();
        let mut g = Garage { ty: garage_type::COLLECTCARS_1, ..Default::default() };
        let securica = crate::modelinfo::indices_gta3::mi::SECURICA as i32;
        assert!(w.does_craig_need_this_car(&g, securica) && !w.has_craig_collected_this_car(&g, securica));
        assert!(!w.mark_this_car_as_collected_for_craig(&g, securica));
        assert_eq!((w.ents.player_info.money, w.ents.garages.message_id.as_str()), (1000, "GA_13"));
        assert!(!w.does_craig_need_this_car(&g, securica) && w.has_craig_collected_this_car(&g, securica));
        // a model not on the list is not needed and not collected
        assert!(!w.does_craig_need_this_car(&g, 9999) && !w.has_craig_collected_this_car(&g, 9999));
        // the hearse stands in for a Manana
        g.ty = garage_type::COLLECTCARS_2;
        assert!(w.does_craig_need_this_car(&g, crate::modelinfo::indices_gta3::mi::CORPSE as i32));
        for m in CARS_TO_COLLECT_IN_CRAIGS_GARAGES[1] {
            let done = w.mark_this_car_as_collected_for_craig(&g, m as i32);
            assert_eq!(done, m == *CARS_TO_COLLECT_IN_CRAIGS_GARAGES[1].last().unwrap());
        }
        assert_eq!((w.ents.garages.message_id.as_str(), w.ents.player_info.money), ("GA_14", 1000 + 16 * 1000 + 200_000));
    }

    #[test]
    fn police_chasing_the_player_back_off_from_a_garage() {
        let (mut w, _) = world_with_player();
        let mut spy = Spy::default();
        let cop = car_at(&mut w, Vec3::new(-0.3, 5.0, 1.0));
        w.try_veh_mut(cop).unwrap().flags.is_law_enforcer = true;
        w.try_veh_mut(cop).unwrap().auto_pilot.car_mission = car_mission::RAMPLAYER_CLOSE;
        w.get_mut(cop).unwrap().phys.as_mut().unwrap().move_speed = Vec3::new(1.0, 0.5, 0.0);
        w.ents.timer.time_in_ms = 500;
        w.call_off_chase_for_area(&mut Px { rng: &mut Rng::default(), hooks: &mut spy }, 0.0, 0.0, 10.0, 10.0);
        // west and south of the middle: no more speed towards the garage
        assert_eq!(w.ent(cop).phys.as_ref().unwrap().move_speed, Vec3::ZERO);
        // a car that is not after the player is left alone
        let other = car_at(&mut w, Vec3::new(-0.3, 5.0, 1.0));
        w.get_mut(other).unwrap().phys.as_mut().unwrap().move_speed = Vec3::new(1.0, 0.0, 0.0);
        w.call_off_chase_for_area(&mut Px { rng: &mut Rng::default(), hooks: &mut spy }, 0.0, 0.0, 10.0, 10.0);
        assert_eq!(w.ent(other).phys.as_ref().unwrap().move_speed.x, 1.0);
        assert_eq!(w.try_veh(cop).unwrap().auto_pilot.time_temp_action, 2500);
    }

    fn door_at(w: &mut World, pos: Vec3) -> EntityRef {
        use crate::collision::{Box3, ColModel, Sphere};
        if w.ents.models.get(3000).is_none() {
            let b = Box3 { min: Vec3::new(-2.0, -0.1, -2.0), max: Vec3::new(2.0, 0.1, 2.1), surface: 0, piece: 0 };
            w.ents.models.add_simple_model(3000).unwrap().col_model = Some(std::sync::Arc::new(ColModel { bounding_sphere: Sphere::new(3.0, Vec3::ZERO, 0, 0), bounding_box: b, boxes: vec![b], ..Default::default() }));
        }
        let r = w.new_object(3000, Matrix::default(), &mut Rng::default()).unwrap();
        w.get_mut(r).unwrap().base.set_position(pos);
        w.add(r).unwrap();
        r
    }

    #[test]
    fn a_garage_finds_its_door_and_the_door_rises_and_falls_with_the_state() {
        let (mut w, _) = world_with_player();
        let mut spy = Spy::default();
        assert!(w.ents.indices.match_model_string("oddjgaragdoor", 3000));
        let far_door = door_at(&mut w, Vec3::new(200.0, 5.0, 0.0));
        let door = door_at(&mut w, Vec3::new(10.0, 5.0, 0.5));
        let i = w.add_garage(MIN, MAX, garage_type::FOR_SCRIPT_TO_OPEN_AND_CLOSE, 0);
        let g = w.ents.garages.garages[i].clone();
        assert_eq!((g.door1, g.door2, g.door1_z, g.door1_x), (Some(door), None, 0.5, 10.0));
        assert_ne!(g.door1, Some(far_door));
        // a script opens it: the door goes up by its height
        w.ents.garages.open_garage(i);
        assert_eq!(w.ents.garages.garages[i].state, garage_state::OPENING);
        run_until(&mut w, &mut spy, i, garage_state::OPENED);
        let h = w.ents.garages.garages[i].door_height;
        assert_eq!(w.epos(door).z, 0.5 + h);
        w.ents.garages.close_garage(i);
        run_until(&mut w, &mut spy, i, garage_state::FULLYCLOSED);
        assert_eq!(w.epos(door).z, 0.5);
        // the rotated kind turns instead of only rising
        w.ents.garages.set_garage_door_to_rotate(i);
        w.ents.garages.garages[i].door_pos = w.ents.garages.garages[i].door_height;
        w.update_doors_height(&mut w.ents.garages.garages[i].clone());
        let door_matrix = w.ent(door).base.matrix;
        assert!(door_matrix.up.z.abs() < 1e-5 && door_matrix.up.x > 0.99);
        // fully open it lies flat: up points along the door's right
        w.set_all_doors_back_to_original_height();
        assert_eq!(w.epos(door).z, 0.5);
        assert_eq!(w.ent(door).base.matrix.up, Vec3::Z);
    }

    #[test]
    fn a_door_that_is_gone_is_looked_for_again() {
        let (mut w, _) = world_with_player();
        w.ents.indices.match_model_string("oddjgaragdoor", 3000);
        let d1 = door_at(&mut w, Vec3::new(10.0, 5.0, 0.0));
        let i = w.add_garage(MIN, MAX, garage_type::FOR_SCRIPT_TO_OPEN, 0);
        assert_eq!(w.ents.garages.garages[i].door1, Some(d1));
        w.delete(d1);
        let d2 = door_at(&mut w, Vec3::new(10.0, 5.0, 0.0));
        let mut g = std::mem::take(&mut w.ents.garages.garages[i]);
        w.update_doors_height(&mut g);
        assert_eq!(g.door1, Some(d2));
    }

    #[test]
    fn wrecks_in_a_garage_are_cleared_away() {
        let (mut w, _) = world_with_player();
        let g = Garage { x1: 0.0, x2: 10.0, y1: 0.0, y2: 10.0, z1: 0.0, z2: 5.0, ..Default::default() };
        let (wreck, fine, flipped) = (car_at(&mut w, Vec3::new(5.0, 5.0, 1.0)), car_at(&mut w, Vec3::new(2.0, 2.0, 1.0)), car_at(&mut w, Vec3::new(8.0, 8.0, 1.0)));
        w.get_mut(wreck).unwrap().base.status = status::WRECKED;
        w.get_mut(flipped).unwrap().base.matrix.up = Vec3::new(0.0, 0.0, -1.0);
        w.tidy_up_garage(&g);
        assert!(w.get(wreck).is_none() && w.get(flipped).is_none() && w.get(fine).is_some());
    }
}
