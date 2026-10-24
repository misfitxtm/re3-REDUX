//! Cars in the world: construction and `CAutomobile::ProcessControl`.
// The per-wheel loops index several parallel arrays, as the original does.
#![allow(clippy::needless_range_loop)]
use super::{Ents, World};
use crate::collision::ColModel;
use crate::entity::{status, Ent, Entity, EntityRef, EntityType, Kind};
use crate::general::Rng;
use crate::math::{dot, magnitude, magnitude_squared};
use crate::modelinfo::indices_gta3::mi;
use crate::surface;
use crate::timer::Timer;
use crate::vehicle::automobile::control::{ControlCtx, HydraulicEnv};
use crate::vehicle::automobile::damage::{DamageCtx, DamageSource};
use crate::vehicle::automobile::{self, node, BombType, Parts, CARWHEEL_FRONT_LEFT, CARWHEEL_FRONT_RIGHT, CARWHEEL_REAR_LEFT, CARWHEEL_REAR_RIGHT};
use crate::vehicle::base::{flying_control, Flight, FlightModel, Wheel};
use crate::vehicle::damage::{door_status, engine};
use crate::vehicle::env::{CarEnv, CarEvent, CarHooks, CarSound, Particle, ParticleKind};
use crate::vehicle::handling::HandlingData;
use glam::Vec3;
use std::sync::Arc;

#[derive(Default)]
struct PhaseA {
    /// The frame ends here (collision for another level not loaded, postponed, simple car).
    done: bool,
    strong_grip1: bool,
    strong_grip2: bool,
    skip_physics: bool,
}

pub(super) fn model_is_police(model: i16) -> bool {
    matches!(model, mi::FBICAR | mi::POLICE | mi::ENFORCER | mi::SECURICA | mi::RHINO | mi::BARRACKS)
}

/// Blows up another car on behalf of `culprit` (the tank running over it).
fn blow_up_other(ents: &mut Ents, target: EntityRef, culprit: EntityRef, events: &mut Vec<CarEvent>) {
    let now = ents.timer.time_in_ms;
    let Some(mut e) = ents.pools.take(target) else { return };
    let model = e.base.model_index;
    if let Some(h) = match &e.kind {
        Kind::Vehicle(v) => Some(*ents.handling.get(v.handling)),
        _ => None,
    } && let Some(mut p) = Parts::of(target, &mut e)
    {
        p.blow_up_car(Some(culprit), &h, model, now, events);
        p.finish();
    }
    ents.pools.restore(target, e);
}

impl World {
    /// Builds a car of `model_index` (see [`automobile::new_car`]) and allocates it; place it with
    /// [`World::add`]. The model's collision model gets its suspension lines the first time.
    pub fn new_car(&mut self, model_index: i16, created_by: u8, rng: &mut Rng) -> Option<EntityRef> {
        let time_ms = self.ents.timer.time_in_ms;
        let player_colours = None;
        let all_taxi = self.ents.all_taxi_lights;
        let info = self.ents.models.get_mut(model_index as i32)?;
        let crate::modelinfo::Kind::Vehicle(mi_data) = &mut info.kind else { return None };
        let handling_id = mi_data.handling_id as usize;
        let handling = *self.ents.handling.get(handling_id);
        // lines are added once per model: the collision model is shared by all its cars
        let col = info.col_model.as_mut()?;
        let col_mut = Arc::make_mut(col);
        let suspension = automobile::setup_suspension_lines(col_mut, mi_data, &handling, model_index);
        let crate::modelinfo::Kind::Vehicle(mi_data) = &mut self.ents.models.get_mut(model_index as i32)?.kind else { return None };
        let car = automobile::new_car(model_index, created_by, time_ms, handling_id, &handling, mi_data, &suspension, all_taxi, player_colours, rng);
        let mut base = Entity::new(EntityType::Vehicle, rng);
        base.model_index = model_index;
        base.status = status::SIMPLE;
        base.flags.use_collision_records = true;
        if matches!(model_index, mi::RHINO) {
            base.flags.explosion_proof = true;
            base.flags.bullet_proof = true;
        }
        self.insert(Ent::new(base, Some(car.physical), Kind::Vehicle(Box::new(car.vehicle))))
    }

    /// `Teleport`: moves a car and resets its motion and suspension.
    pub fn teleport_car(&mut self, r: EntityRef, pos: Vec3) {
        self.remove(r);
        if let Some(e) = self.get_mut(r) {
            e.base.set_position(pos);
            e.base.set_orientation(0.0, 0.0, 0.0);
            if let Some(p) = e.phys.as_mut() {
                p.move_speed = Vec3::ZERO;
                p.turn_speed = Vec3::ZERO;
            }
            if let Kind::Vehicle(v) = &mut e.kind {
                let mut ratio = v.suspension_spring_ratio;
                if let crate::vehicle::base::VehicleSub::Car(c) = &mut v.sub {
                    c.reset_suspension(&mut ratio);
                }
                v.suspension_spring_ratio = ratio;
            }
        }
        let _ = self.add(r);
    }

    /// `RcbanditCheckHitWheels`: has the RC car touched the wheel of another car?
    pub fn rcbandit_check_hit_wheels(&mut self, r: EntityRef) -> bool {
        let Some(e) = self.get(r) else { return false };
        let pos = e.base.position();
        let me_matrix = e.base.matrix;
        let Some(col) = self.ents.col_model(e.base.model_index) else { return false };
        let g = self.grid;
        let (xmin, xmax) = (g.index_x(pos.x - 2.0).max(0), g.index_x(pos.x + 2.0).min(g.num - 1));
        let (ymin, ymax) = (g.index_y(pos.y - 2.0).max(0), g.index_y(pos.y + 2.0).min(g.num - 1));
        self.advance_scan_code();
        let code = self.ents.scan_code;
        let mut hit = false;
        'outer: for y in ymin..=ymax {
            for x in xmin..=xmax {
                let Some(s) = self.sector(x, y).cloned() else { continue };
                for list in [super::VEHICLES, super::VEHICLES + 1] {
                    for &rc in &s.lists[list] {
                        let Some(car) = self.ents.pools.get_mut(rc) else { continue };
                        let Kind::Vehicle(v) = &car.kind else { continue };
                        if rc == r || !v.is_car() || car.base.scan_code == code {
                            continue;
                        }
                        car.base.scan_code = code;
                        let (cp, cm) = (car.base.position(), car.base.matrix);
                        if (me_matrix.pos.x - cp.x).abs() >= 10.0 || (me_matrix.pos.y - cp.y).abs() >= 10.0 {
                            continue;
                        }
                        let Some(crate::modelinfo::Kind::Vehicle(mi_data)) = self.ents.models.get(car.base.model_index as i32).map(|m| &m.kind) else { continue };
                        let simple = car.base.status == status::SIMPLE;
                        let inv = me_matrix.inverted();
                        for i in 0..4 {
                            let prev = match &v.sub {
                                crate::vehicle::base::VehicleSub::Car(c) => c.suspension_spring_ratio_prev[i],
                                _ => 1.0,
                            };
                            if prev < 1.0 || simple {
                                let centre = inv.transform_point(cm.transform_point(mi_data.wheel_positions[i]));
                                let sph = crate::collision::Sphere::new(mi_data.wheel_scale * 0.25, centre, 0, 0);
                                if crate::collision::test_sphere_box(&sph, &col.bounding_box) {
                                    hit = true;
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
            }
        }
        hit
    }

    /// `IsRoomForPedToLeaveCar`: is there space beside the door for a ped to step out?
    /// `door_pos` is `CPed::GetPositionToOpenCarDoor(car, component)`.
    pub fn is_room_for_ped_to_leave_car(&mut self, r: EntityRef, component: usize, door_pos: Vec3, door_offset: Option<Vec3>) -> bool {
        use super::{Kinds, LineOpts};
        let Some(e) = self.get_mut(r) else { return false };
        let model = e.base.model_index;
        let (front, back) = match self.ents.models.get(model as i32).map(|m| &m.kind) {
            Some(crate::modelinfo::Kind::Vehicle(v)) => (v.positions[2], v.positions[3]),
            _ => return false,
        };
        let Some(mut e) = self.ents.pools.take(r) else { return false };
        let points = Parts::of(r, &mut e).map(|p| {
            let out = p.exit_points(component, front, back, door_pos, door_offset);
            p.finish();
            out
        });
        self.ents.pools.restore(r, e);
        let Some((seat, mut door)) = points else { return false };
        let dist = door - seat;
        // without this the check fails for van doors
        door.z += 0.5;
        let length = magnitude(dist);
        let ped_pos = seat + dist * ((length + 0.6) / length);
        let solid = Kinds { buildings: true, objects: true, ..Kinds::NONE };
        if !self.is_line_of_sight_clear(seat, ped_pos, solid, LineOpts::default()) {
            return false;
        }
        let around = Kinds { buildings: true, vehicles: true, objects: true, ..Kinds::NONE };
        if self.test_sphere_against_world(door, 0.6, Some(r), around, false).is_some() {
            return false;
        }
        let ground = Kinds { buildings: true, objects: true, ..Kinds::NONE };
        let mut upper_z = 0.0;
        if let Some(h) = self.process_vertical_line(door, 1000.0, ground, false, None) {
            let z = h.point.point.z;
            if z > door.z && z < door.z + 0.6 {
                return false;
            }
            upper_z = z;
        }
        let Some(h) = self.process_vertical_line(door, -1000.0, ground, false, None) else { return false };
        !(upper_z != 0.0 && upper_z < h.point.point.z)
    }

    /// `PlaceOnRoadProperly`: levels the car on the ground below its nose and tail.
    pub fn place_car_on_road_properly(&mut self, r: EntityRef) {
        use super::Kinds;
        let Some(e) = self.get(r) else { return };
        let (m, model) = (e.base.matrix, e.base.model_index);
        let Some(col) = self.ents.col_model(model) else { return };
        let (len_fwd, len_back) = (col.bounding_box.max.y, -col.bounding_box.min.y);
        let front = Vec3::new(m.pos.x + m.forward.x * len_fwd, m.pos.y + m.forward.y * len_fwd, m.pos.z + 5.0);
        let rear = Vec3::new(m.pos.x - m.forward.x * len_back, m.pos.y - m.forward.y * len_back, m.pos.z + 5.0);
        let b = Kinds { buildings: true, ..Kinds::NONE };
        let front_hit = self.process_vertical_line(front, m.pos.z - 5.0, b, false, None);
        let rear_hit = self.process_vertical_line(rear, m.pos.z - 5.0, b, false, None);
        let height = match self.get(r).map(|e| &e.kind) {
            Some(Kind::Vehicle(v)) => match &v.sub {
                crate::vehicle::base::VehicleSub::Car(c) => c.height_above_road,
                _ => 0.0,
            },
            _ => 0.0,
        };
        let Some(mut e) = self.ents.pools.take(r) else { return };
        if let Some(mut p) = Parts::of(r, &mut e) {
            // the ground entities found are remembered as the current ground
            p.veh.cur_ground_entity = front_hit.or(rear_hit).map(|h| h.entity);
            p.place_on_road_properly(&col, front_hit.map(|h| h.point.point.z), rear_hit.map(|h| h.point.point.z), height);
            p.finish();
        }
        self.ents.pools.restore(r, e);
    }

    /// `CAutomobile::ProcessControl`.
    pub fn process_control_car(&mut self, r: EntityRef, env: &mut CarEnv, hooks: &mut dyn CarHooks, rng: &mut Rng) {
        let Some(e0) = self.get(r) else { return };
        let Kind::Vehicle(v0) = &e0.kind else { return };
        if !matches!(v0.sub, crate::vehicle::base::VehicleSub::Car(_)) {
            return;
        }
        let (status0, model) = (e0.base.status, e0.base.model_index);
        let handling = *self.ents.handling.get(v0.handling);
        let rc_hit = status0 == status::PLAYER_REMOTE && model == mi::RCBANDIT && self.rcbandit_check_hit_wheels(r);

        let Some(mut e) = self.ents.pools.take(r) else { return };
        let a = car_phase_a(&mut self.ents, hooks, env, rng, r, &mut e, &handling, rc_hit);
        self.ents.pools.restore(r, e);
        if a.done {
            self.flush_after_car(env, rng);
            return;
        }

        // the collision the car has not had through ProcessEntityCollision yet
        let needs_col = !a.skip_physics && self.get(r).is_some_and(|e| matches!(&e.kind, Kind::Vehicle(v) if !v.flags.vehicle_col_processed));
        if needs_col {
            let step = self.ents.timer.time_step;
            let e = self.get_mut(r).unwrap();
            let mat = e.base.matrix;
            e.base.flags.is_stuck = false;
            e.base.flags.has_contacted = false;
            e.base.flags.is_in_safe_position = false;
            e.base.flags.was_postponed = false;
            e.base.flags.has_hit_wall = false;
            let p = e.phys.as_mut().unwrap();
            p.distance_travelled = 0.0;
            p.is_vehicle_being_shifted = false;
            p.flags.skip_line_col = false;
            p.apply_move_speed(&mut e.base.matrix, step);
            p.apply_turn_speed(&mut e.base.matrix, step);
            let mut i = 0;
            while self.check_collision(r, rng) && i < 5 {
                let e = self.get_mut(r).unwrap();
                e.base.matrix = mat;
                let p = e.phys.as_mut().unwrap();
                p.apply_move_speed(&mut e.base.matrix, step);
                p.apply_turn_speed(&mut e.base.matrix, step);
                i += 1;
            }
            let e = self.get_mut(r).unwrap();
            e.base.flags.is_in_safe_position = true;
            e.base.flags.is_stuck = false;
        }

        let Some(mut e) = self.ents.pools.take(r) else { return };
        car_phase_b(&mut self.ents, hooks, env, rng, r, &mut e, &handling, &a);
        self.ents.pools.restore(r, e);
        self.flush_after_car(env, rng);
    }

    /// `SpawnFlyingComponent`: a part of the car becomes a loose temp object. `group` is the
    /// `Group` index (bumper 0, wheel 1, door 2, bonnet 3, boot 4, panel 5). The part's node position
    /// comes from the model; its orientation is the car's.
    pub fn spawn_flying_component(&mut self, car: EntityRef, component: usize, group: u8, rng: &mut Rng) -> Option<EntityRef> {
        use crate::entity::object::CreatedBy;
        use crate::entity::physical::{Physical, GRAVITY};
        use crate::entity::ObjectData;
        let temp_count = self.ents.pools.objects.iter().filter(|(_, o)| matches!(&o.kind, Kind::Object(d) if d.created_by == CreatedBy::Temp)).count();
        if temp_count >= 30 {
            return None;
        }
        let ce = self.get(car)?;
        let Kind::Vehicle(v) = &ce.kind else { return None };
        let crate::vehicle::base::VehicleSub::Car(c) = &v.sub else { return None };
        let node_pos = c.node_positions.get(component).copied().flatten()?;
        let (cm, car_model, car_phys) = (ce.base.matrix, ce.base.model_index, ce.phys.as_ref()?.clone());
        let colours = (v.current_colour1 as i8, v.current_colour2 as i8);
        let scorched = ce.base.flags.render_scorched;
        let car_up_z = cm.up.z;

        let idx = &self.ents.indices;
        let _ = idx;
        use crate::modelinfo::indices_gta3::mi as gmi;
        let model = if component == node::WINDSCREEN {
            gmi::CAR_BONNET
        } else {
            match group {
                0 => gmi::CAR_BUMPER,
                1 => gmi::CAR_WHEEL,
                2 => gmi::CAR_DOOR,
                3 => gmi::CAR_BONNET,
                4 => gmi::CAR_BOOT,
                _ => gmi::CAR_PANEL,
            }
        };
        let mut base = Entity::new(EntityType::Object, rng);
        base.model_index = model;
        let mut matrix = cm;
        matrix.pos = cm.transform_point(node_pos);
        base.matrix = matrix;
        let mut phys = Physical { mass: 10.0, turn_mass: 25.0, air_resistance: 0.97, elasticity: 0.1, buoyancy: 10.0 * GRAVITY / 0.75, ..Default::default() };
        phys.centre_of_mass = match group {
            2 if component != node::WINDSCREEN => Vec3::new(0.0, -0.5, 0.0),
            3 if component != node::WINDSCREEN => Vec3::new(0.0, 0.4, 0.0),
            4 if component != node::WINDSCREEN => Vec3::new(0.0, -0.3, 0.0),
            _ => Vec3::ZERO,
        };
        let mut data = ObjectData::new();
        data.created_by = CreatedBy::Temp;
        data.use_vehicle_colours = true;
        data.colour1 = colours.0;
        data.colour2 = colours.1;
        data.ref_model_index = car_model;
        // the more temp objects there are, the shorter this one lives
        let n = temp_count + 1;
        let now = self.ents.timer.time_in_ms;
        data.end_of_life_time = if n > 20 { now + 4000 } else if n > 10 { now + 10000 } else { now + 20000 };

        phys.move_speed = car_phys.move_speed;
        let upish = component == node::WINDSCREEN || group == 3 || group == 4;
        if phys.move_speed.z > 0.0 {
            phys.move_speed.z *= 1.5;
        } else if car_up_z > 0.0 && upish {
            phys.move_speed.z *= -1.5;
            phys.move_speed.z += 0.04;
        } else {
            phys.move_speed.z *= 0.25;
        }
        phys.move_speed.x *= 0.75;
        phys.move_speed.y *= 0.75;
        phys.turn_speed = car_phys.turn_speed * 2.0;

        // push the part away from the car
        let mut dist = crate::math::normalise(base.matrix.pos - cm.pos);
        if upish {
            dist += cm.up;
            if cm.up.z > 0.0 {
                // fast cars throw it further up
                let speed = glam::Vec2::new(car_phys.move_speed.x, car_phys.move_speed.y).length();
                base.matrix.pos += cm.up * speed;
            }
        }
        phys.apply_move_force(dist);
        if group == 1 {
            phys.turn_mass = 5.0;
            phys.turn_speed.x = 0.5;
            phys.air_resistance = 0.99;
        }

        // is it still inside the car?
        let overlaps = match (self.ents.col_model(model), self.ents.col_model(car_model)) {
            (Some(oc), Some(cc)) => {
                let mut pts = vec![crate::collision::ColPoint::default(); crate::collision::MAX_SPHERES];
                crate::collision::process_col_models(&base.matrix, &oc, &cm, &cc, &mut pts, &mut [], &mut []) > 0
            }
            _ => false,
        };
        if overlaps {
            data.colliding_entity = Some(car);
        }
        base.flags.render_scorched = scorched;
        base.flags.is_static = false;
        self.ents.models.add_ref(car_model as i32);
        let r = self.insert(Ent::new(base, Some(phys), Kind::Object(data)))?;
        let _ = self.add(r);
        Some(r)
    }

    /// Hands the car's requests to the world where it can act on them itself.
    fn flush_after_car(&mut self, env: &mut CarEnv, rng: &mut Rng) {
        self.flush_moving();
        let (parts, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut env.events).into_iter().partition(|e| matches!(e, CarEvent::FlyingComponent { .. }));
        env.events = rest;
        for e in parts {
            if let CarEvent::FlyingComponent { car, component, group } = e {
                self.spawn_flying_component(car, component, group, rng);
            }
        }
    }
}

/// The part of `ProcessControl` before the car's own collision handling.
#[allow(clippy::too_many_arguments)]
fn car_phase_a(ents: &mut Ents, hooks: &mut dyn CarHooks, env: &mut CarEnv, rng: &mut Rng, r: EntityRef, e: &mut Ent, h: &HandlingData, rc_hit: bool) -> PhaseA {
    let mut out = PhaseA::default();
    let timer = ents.timer.clone();
    let model = e.base.model_index;
    let step = timer.time_step;
    let me_is_player_vehicle = env.player.vehicle == Some(r);

    let using_special = matches!(&e.kind, Kind::Vehicle(v) if v.flags.using_special_col_model);
    let col: Arc<ColModel> = if using_special { Arc::new(ents.player_col_model.clone()) } else { ents.col_model(model).unwrap_or_default() };
    if let Kind::Vehicle(v) = &mut e.kind {
        v.flags.warned_peds = false;
    }
    // skip if the collision is for another level
    if col.level > 0 && col.level != ents.collision_in_memory {
        out.done = true;
        return out;
    }

    let (mut strong1, mut strong2) = (false, false);
    {
        let Kind::Vehicle(v) = &e.kind else { return out };
        if env.player.vehicle.is_some()
            && !me_is_player_vehicle
            && env.player.wanted_level > 3
            && matches!(v.auto_pilot.car_mission, 2..=5)
            && magnitude(env.player.speed) > 0.3
        {
            strong1 = true;
            if magnitude(env.player.speed) > 0.4 && magnitude(e.phys.as_ref().unwrap().move_speed) < 0.3 || magnitude(e.base.position() - env.player.coors) > 50.0 {
                strong2 = true;
            }
        }
    }
    out.strong_grip1 = strong1;
    out.strong_grip2 = strong2;

    // the driver and passengers, before the parts borrow the entity
    let (driver, passengers, upside_down_enter) = {
        let Kind::Vehicle(v) = &e.kind else { return out };
        (v.driver, v.passengers, crate::vehicle::base::VehicleData::is_upside_down(&e.base.matrix) && crate::vehicle::base::VehicleData::can_ped_enter_car(&e.base.matrix, e.phys.as_ref().unwrap()))
    };
    let leave: Vec<EntityRef> = {
        let mut v = vec![];
        if let Some(d) = driver
            && upside_down_enter
            && hooks.occupant_can_be_ordered_out(d)
        {
            v.push(d);
        }
        let n = match &e.kind {
            Kind::Vehicle(v) => v.num_passengers,
            _ => 0,
        };
        if n != 0 && upside_down_enter {
            v.extend(passengers.iter().flatten().copied().filter(|&p| hooks.occupant_can_be_ordered_out(p)));
        }
        v
    };

    let mut p = Parts::of(r, e).expect("a car");
    if p.veh.flags.is_bus {
        p.process_auto_bus_doors(timer.time_in_ms, &mut env.events);
    }
    p.veh.process_car_alarm(&timer);

    // police cars watch the player
    if !matches!(p.base.status, status::ABANDONED | status::WRECKED | status::PLAYER | status::PLAYER_REMOTE | status::PLAYER_DISABLED) && model_is_police(model) {
        p.scan_for_crimes(&env.player, &mut env.events);
    }

    // driver
    if p.veh.driver.is_some() {
        if !p.car.flags.driver_last_frame && p.car.bomb_type == BombType::OnIgnitionActive {
            p.veh.bomb_timer = 1000;
            p.veh.blow_up_entity = p.car.bomb_rigger;
            env.events.push(CarEvent::Sound { sound: CarSound::BombTick, volume: 1.0 });
        }
        p.car.flags.driver_last_frame = true;
    } else {
        p.car.flags.driver_last_frame = false;
    }
    env.events.extend(leave.into_iter().map(|d| CarEvent::Vehicle(crate::vehicle::base::VehicleEvent::LeaveCar(d))));

    p.veh.auto_pilot.slowed_down_because_of_cars = false;
    p.veh.auto_pilot.slowed_down_because_of_peds = false;

    // centre of mass for stability
    if strong1 || env.cheats.cheat3 {
        p.phys.centre_of_mass.z = 0.3 * p.car.suspension_spring_length[0] - p.car.height_above_road;
    } else if h.flags & crate::vehicle::handling::flags::NONPLAYER_STABILISER != 0 && p.base.status == status::PHYSICS {
        p.phys.centre_of_mass.z = h.centre_of_mass.z - 0.2 * h.dimension.z;
    } else {
        p.phys.centre_of_mass.z = h.centre_of_mass.z;
    }

    // by status
    let mut player_remote = false;
    let st = p.base.status;
    let mut cx_events = std::mem::take(&mut env.events);
    {
        let mut cx = ControlCtx { pad: &env.pad, player: &env.player, timer: &timer, rng, events: &mut cx_events, handling: h, model_index: model };
        match st {
            status::PLAYER_REMOTE | status::PLAYER => {
                if st == status::PLAYER_REMOTE {
                    if env.pad.car_gun_just_down {
                        p.blow_up_car(env.player.ped, h, model, timer.time_in_ms, cx.events);
                        cx.events.push(CarEvent::TakeRemoteControl);
                    }
                    if model == mi::RCBANDIT {
                        let pos = p.base.position();
                        if rc_hit || p.phys.flags.is_in_water || hooks.is_point_in_safe_zone(pos) {
                            if hooks.is_point_in_safe_zone(pos) {
                                cx.events.push(CarEvent::TriggerMessage("HM2_5"));
                            }
                            cx.events.push(CarEvent::TakeRemoteControl);
                            p.blow_up_car(env.player.ped, h, model, timer.time_in_ms, cx.events);
                        }
                    }
                    if env.player.remote_vehicle == Some(r) {
                        player_remote = true;
                    }
                }
                let driver_ok = p.veh.driver.is_some_and(|d| hooks.driver_controlled_by_player(d));
                if player_remote || p.veh.driver.is_some() {
                    if player_remote || driver_ok {
                        p.process_control_inputs(&mut cx);
                    }
                    if p.base.status == status::PLAYER && !hooks.recording_chase() && p.veh.driver.is_some() {
                        cx.events.push(CarEvent::DriveBy { looking_left: env.pad.looking_left, looking_right: env.pad.looking_right });
                        // the weapon doors close again once the driver stops looking sideways
                        if !env.pad.looking_left && p.car.weapon_door_timer_left > 0.0 {
                            p.car.weapon_door_timer_left = (p.car.weapon_door_timer_left - step * 0.1).max(0.0);
                            let t = p.car.weapon_door_timer_left;
                            p.open_door(automobile::door_id::FRONT_LEFT, t, cx.events);
                        }
                        if !env.pad.looking_right && p.car.weapon_door_timer_right > 0.0 {
                            p.car.weapon_door_timer_right = (p.car.weapon_door_timer_right - step * 0.1).max(0.0);
                            let t = p.car.weapon_door_timer_right;
                            p.open_door(automobile::door_id::FRONT_RIGHT, t, cx.events);
                        }
                    }
                }
            }
            status::SIMPLE => {
                env.events = cx_events;
                // hooks need the whole entity, so give the parts back for them
                p.finish();
                hooks.update_car_ai(e);
                crate::entity::physical::process_control(&mut e.base, e.phys.as_mut().unwrap(), step);
                hooks.update_car_on_rails(e);
                let stopped = hooks.car_stopped_because_of_light(e);
                let mut p = Parts::of(r, e).expect("a car");
                p.car.wheels_on_ground = 4;
                p.car.drive_wheels_on_ground_prev = p.car.drive_wheels_on_ground;
                p.car.drive_wheels_on_ground = 4;
                let max_speed = p.veh.auto_pilot.max_traffic_speed / 50.0;
                let mut gear = p.veh.current_gear;
                let mut tr = h.transmission;
                tr.gear_for_simple_car(max_speed, &mut gear);
                p.veh.current_gear = gear;
                p.simple_car_wheels(&timer);
                p.play_horn_if_necessary(stopped, rng, &mut env.events);
                p.reduce_horn_counter();
                p.veh.flags.vehicle_col_processed = false;
                p.finish();
                out.done = true;
                return out;
            }
            status::PHYSICS => {
                env.events = cx_events;
                p.finish();
                hooks.update_car_ai(e);
                hooks.steer_ai_car_with_physics(e);
                let stopped = hooks.car_stopped_because_of_light(e);
                let mut p = Parts::of(r, e).expect("a car");
                p.play_horn_if_necessary(stopped, rng, &mut env.events);
                // continue below with a fresh set of parts
                return car_a_after_status(ents, hooks, env, rng, r, h, out, &timer, &col, p);
            }
            status::ABANDONED => {
                p.veh.brake_pedal = 0.2;
                p.veh.flags.is_handbrake_on = false;
                p.veh.steer_angle = 0.0;
                p.veh.gas_pedal = 0.0;
                p.veh.car_horn_timer = 0;
            }
            status::WRECKED => {
                p.veh.brake_pedal = 0.05;
                p.veh.flags.is_handbrake_on = true;
                p.veh.steer_angle = 0.0;
                p.veh.gas_pedal = 0.0;
                p.veh.car_horn_timer = 0;
            }
            status::PLAYER_DISABLED => {
                p.veh.brake_pedal = 1.0;
                p.veh.flags.is_handbrake_on = true;
                p.veh.steer_angle = 0.0;
                p.veh.gas_pedal = 0.0;
                p.veh.car_horn_timer = 0;
            }
            _ => {}
        }
    }
    env.events = cx_events;
    car_a_after_status(ents, hooks, env, rng, r, h, out, &timer, &col, p)
}

/// The rest of the first half, after the status switch.
#[allow(clippy::too_many_arguments)]
fn car_a_after_status(ents: &mut Ents, hooks: &mut dyn CarHooks, env: &mut CarEnv, rng: &mut Rng, r: EntityRef, h: &HandlingData, mut out: PhaseA, timer: &Timer, _col: &ColModel, mut p: Parts) -> PhaseA {
    let _ = hooks;
    let step = timer.time_step;
    let model = p.base.model_index;
    let me_is_player_vehicle = env.player.vehicle == Some(r);

    // what's going on here? flattened cars lose spin
    if p.base.position().z < -0.6 && p.phys.move_speed.x.abs() < 0.05 && p.phys.move_speed.y.abs() < 0.05 {
        p.phys.turn_speed *= 0.95f32.powf(step);
    }

    // skip the physics once the car has been static for a while
    let mut skip_physics = false;
    if !p.base.flags.is_stuck && matches!(p.base.status, status::ABANDONED | status::WRECKED) {
        let mut make_static = false;
        if !p.veh.flags.vehicle_col_processed && p.phys.move_speed == Vec3::ZERO && p.car.suspension_spring_ratio_prev[3] != 1.0 {
            make_static = true;
        }
        let (move_limit, turn_limit, distance_limit) = if p.base.status == status::WRECKED { (0.006, 0.0015, 0.015) } else { (0.003, 0.0009, 0.005) };
        p.phys.move_speed_avg = (p.phys.move_speed_avg + p.phys.move_speed) / 2.0;
        p.phys.turn_speed_avg = (p.phys.turn_speed_avg + p.phys.turn_speed) / 2.0;
        let sq = |x: f32| x * x;
        if magnitude_squared(p.phys.move_speed_avg) <= sq(move_limit * step) && magnitude_squared(p.phys.turn_speed_avg) <= sq(turn_limit * step) && p.phys.distance_travelled < distance_limit || make_static {
            p.phys.static_frames += 1;
            if (p.phys.static_frames > 10 || make_static) && !hooks.map_could_move_in_this_area(p.base.position().x, p.base.position().y) {
                if !make_static || p.phys.static_frames > 10 {
                    p.phys.static_frames = 10;
                }
                skip_physics = true;
                p.phys.move_speed = Vec3::ZERO;
                p.phys.turn_speed = Vec3::ZERO;
            }
        } else {
            p.phys.static_frames = 0;
        }
    }
    out.skip_physics = skip_physics;

    // postpone while something we stand on has not been placed yet
    if !ents.force_process_control {
        for g in p.car.ground_physical.iter().flatten() {
            if ents.pools.get(*g).is_some_and(|g| g.base.flags.is_in_safe_position) {
                p.base.flags.was_postponed = true;
                p.finish();
                out.done = true;
                return out;
            }
        }
    }

    let mut cx_events = std::mem::take(&mut env.events);
    {
        // collision damage of the last frame
        let src = p.phys.damage_entity.and_then(|d| ents.pools.get(d)).map(|d| DamageSource { is_building: d.base.ty == EntityType::Building, status: d.base.status });
        let colour = [0u8; 4];
        let mut dctx = DamageCtx {
            player: &env.player,
            source: src,
            on_screen: false,
            model_index: model,
            player_vehicle_move_speed: env.player.vehicle_move_speed,
            timer,
            rng,
            events: &mut cx_events,
        };
        p.vehicle_damage(h, 0.0, 0, colour, &mut dctx);
    }
    {
        let mut cx = ControlCtx { pad: &env.pad, player: &env.player, timer, rng, events: &mut cx_events, handling: h, model_index: model };
        match model {
            mi::FIRETRUCK => {
                let fire = hooks.find_furthest_fire(p.base.position());
                p.fire_truck_control(&mut cx, fire, me_is_player_vehicle);
            }
            mi::RHINO => {
                p.tank_control(&mut cx, me_is_player_vehicle);
                // run over what is in the way
                if magnitude(p.phys.move_speed) > 0.1 {
                    let hits: Vec<EntityRef> = p.phys.collision_records.clone();
                    for t in hits {
                        let victim = ents.pools.get(t).is_some_and(|t| t.base.ty == EntityType::Vehicle && t.base.model_index != mi::RHINO && !t.base.flags.render_scorched);
                        if victim {
                            blow_up_other(ents, t, r, cx.events);
                        }
                    }
                }
            }
            mi::YARDIE => {
                let normal = ents.col_model(model).unwrap_or_default();
                let wheel_positions = match ents.models.get(model as i32).map(|m| &m.kind) {
                    Some(crate::modelinfo::Kind::Vehicle(v)) => (v.wheel_positions, v.wheel_scale),
                    _ => ([Vec3::ZERO; 4], 0.0),
                };
                if normal.lines.len() == 4 {
                    let mut he = HydraulicEnv { normal: &normal, special: &mut ents.player_col_model, vehicle_ex: &mut ents.player_vehicle_ex, wheel_positions: wheel_positions.0, wheel_scale: wheel_positions.1 };
                    p.hydraulic_control(&mut cx, &mut he);
                }
            }
            _ => {
                if env.cheats.cheat3 && p.base.status == status::PLAYER && magnitude_squared(p.phys.move_speed) > 0.2 * 0.2 && p.veh.suspension_spring_ratio[0] < 1.0 && env.pad.horn_just_down {
                    // the horn makes the car jump
                    cx.events.push(CarEvent::Sound { sound: CarSound::Hydraulic1, volume: 0.0 });
                    cx.events.push(CarEvent::Sound { sound: CarSound::CarJump, volume: 1.0 });
                    let up = p.base.matrix.up;
                    let fwd = p.base.matrix.forward;
                    let sp = p.phys.move_speed;
                    for (i, back) in [(0, false), (2, false), (0, true), (2, true)] {
                        let mut pos = p.car.wheel_col_points[i].point + 0.5 * up;
                        if back {
                            pos -= fwd;
                        }
                        cx.events.push(CarEvent::Particle(Particle::new(ParticleKind::EngineSteam, pos, 1.3 * sp, 2.5)));
                        cx.events.push(CarEvent::Particle(Particle::new(ParticleKind::EngineSmoke, pos, 1.2 * sp, 2.0)));
                    }
                    let mass = p.phys.mass;
                    p.phys.apply_move_force(Vec3::Z * mass * 0.4);
                    let m = p.base.matrix;
                    let tm = p.phys.mass;
                    p.phys.apply_turn_force(&m, up * tm * 0.035, fwd);
                }
            }
        }
    }
    env.events = cx_events;
    p.finish();
    out
}

/// The part of `ProcessControl` after the car's own collision handling.
#[allow(clippy::too_many_arguments)]
fn car_phase_b(ents: &mut Ents, hooks: &mut dyn CarHooks, env: &mut CarEnv, rng: &mut Rng, r: EntityRef, e: &mut Ent, h: &HandlingData, a: &PhaseA) {
    let timer = ents.timer.clone();
    let step = timer.time_step;
    let model = e.base.model_index;
    let me_is_player_vehicle = env.player.vehicle == Some(r);
    let using_special = matches!(&e.kind, Kind::Vehicle(v) if v.flags.using_special_col_model);
    let col: Arc<ColModel> = if using_special { Arc::new(ents.player_col_model.clone()) } else { ents.col_model(model).unwrap_or_default() };
    let wheel_info = match ents.models.get(model as i32).map(|m| &m.kind) {
        Some(crate::modelinfo::Kind::Vehicle(v)) => (v.positions[0], v.wheel_scale),
        _ => (Vec3::ZERO, 0.0),
    };

    // the ground physicals' motion, gathered before the car is borrowed
    let ground: [Option<(Vec3, Vec3)>; 4] = {
        let Kind::Vehicle(v) = &e.kind else { return };
        let crate::vehicle::base::VehicleSub::Car(c) = &v.sub else { return };
        std::array::from_fn(|i| c.ground_physical[i].and_then(|g| ents.pools.get(g)).and_then(|g| g.phys.as_ref().map(|gp| (gp.speed_at(c.ground_offset[i]), Vec3::ZERO))))
    };

    let mut p = Parts::of(r, e).expect("a car");
    let mut events = std::mem::take(&mut env.events);
    let mut brake = 0.0f32;

    if a.skip_physics {
        p.base.flags.has_contacted = false;
        p.base.flags.is_in_safe_position = false;
        p.base.flags.was_postponed = false;
        p.base.flags.has_hit_wall = false;
        p.phys.collision_records.clear();
        p.base.flags.has_collided = false;
        p.veh.flags.vehicle_col_processed = false;
        p.phys.damage_piece_type = 0;
        p.phys.damage_impulse = 0.0;
        p.phys.damage_entity = None;
        p.phys.turn_friction = Vec3::ZERO;
        p.phys.move_friction = Vec3::ZERO;
    } else {
        crate::entity::physical::process_control(p.base, p.phys, step);

        // water
        {
            let mut cx = ControlCtx { pad: &env.pad, player: &env.player, timer: &timer, rng, events: &mut events, handling: h, model_index: model };
            let Ents { buoyancy, water, splash_state, .. } = &mut *ents;
            p.process_buoyancy(&mut cx, buoyancy, water.as_ref(), &col, 0.0, splash_state);
        }

        // rescale the spring ratios: 0 is fully compressed, 1 fully extended
        for i in 0..4 {
            let wheel_radius = 1.0 - p.car.suspension_spring_length[i] / p.car.suspension_line_length[i];
            p.veh.suspension_spring_ratio[i] = (p.veh.suspension_spring_ratio[i] - wheel_radius) / (1.0 - wheel_radius);
        }

        let mut fwd_speed = dot(p.phys.move_speed, p.base.matrix.forward).abs();
        let mut contact_points = [Vec3::ZERO; 4];
        let mut contact_speeds = [Vec3::ZERO; 4];
        let mut spring_dirs = [Vec3::ZERO; 4];
        for i in 0..4 {
            #[allow(clippy::collapsible_match)]
            match p.car.damage.wheel_status(i) {
                s if s == crate::vehicle::damage::wheel_status::MISSING => p.veh.suspension_spring_ratio[i] = 1.0,
                s if s == crate::vehicle::damage::wheel_status::BURST => {
                    // a burst wheel is bumpier the faster we go
                    if rng.range_i32(0, (40.0 * fwd_speed) as u16 as i32 + 98) < 100 {
                        p.veh.suspension_spring_ratio[i] += 0.3 * (p.car.suspension_line_length[i] - p.car.suspension_spring_length[i]) / p.car.suspension_spring_length[i];
                        if p.veh.suspension_spring_ratio[i] > 1.0 {
                            p.veh.suspension_spring_ratio[i] = 1.0;
                        }
                    }
                }
                _ => {}
            }
            if p.veh.suspension_spring_ratio[i] < 1.0 {
                contact_points[i] = p.car.wheel_col_points[i].point - p.base.position();
                spring_dirs[i] = crate::math::normalise(p.base.matrix.rotate_vector(col.lines[i].p1 - col.lines[i].p0));
            }
        }

        // the springs push the car up
        for i in 0..4 {
            if p.veh.suspension_spring_ratio[i] < 1.0 {
                let mut bias = h.suspension_bias;
                if i == CARWHEEL_REAR_LEFT || i == CARWHEEL_REAR_RIGHT {
                    bias = 1.0 - bias;
                }
                let m = p.base.matrix;
                p.phys.apply_spring_collision(&m, step, h.suspension_force_level, spring_dirs[i], contact_points[i], p.veh.suspension_spring_ratio[i], bias);
                let s = p.car.wheel_col_points[i].surface_b;
                p.car.wheel_skidmark_muddy[i] = s == surface::GRASS || s == surface::MUD_DRY || s == surface::SAND;
            } else {
                contact_points[i] = p.base.matrix.rotate_vector(col.lines[i].p1);
            }
        }

        // speeds at the contacts, minus what we stand on
        for i in 0..4 {
            contact_speeds[i] = p.phys.speed_at(contact_points[i]);
            if let Some((gs, _)) = ground[i] {
                contact_speeds[i] -= gs;
            }
        }
        // dampen the springs
        for i in 0..4 {
            if p.veh.suspension_spring_ratio[i] < 1.0 {
                let m = p.base.matrix;
                p.phys.apply_spring_dampening(&m, step, h.suspension_damping_level, spring_dirs[i], contact_points[i], contact_speeds[i]);
            }
        }
        // again, now with the dampened motion; the ground references are cleared here
        for i in 0..4 {
            contact_speeds[i] = p.phys.speed_at(contact_points[i]);
            if let Some((gs, _)) = ground[i] {
                contact_speeds[i] -= gs;
            }
            p.car.ground_physical[i] = None;
        }

        let grip_cheat = a.strong_grip1 || env.cheats.cheat3;
        fwd_speed = dot(p.phys.move_speed, p.base.matrix.forward);
        let mut gear = p.veh.current_gear;
        let mut change = p.veh.change_gear_time;
        let mut tr = h.transmission;
        let mut acceleration = tr.drive_acceleration(p.veh.gas_pedal, &mut gear, &mut change, fwd_speed, grip_cheat, &timer);
        p.veh.current_gear = gear;
        p.veh.change_gear_time = change;
        acceleration /= p.phys.force_multiplier;
        // these models never accelerate (unused in GTA III)
        brake = p.veh.brake_pedal * h.brake_deceleration * step;
        let neutral = h.flags & crate::vehicle::handling::flags::NEUTRALHANDLING != 0;
        let brake_bias_front = if neutral { 1.0 } else { 2.0 * h.brake_bias };
        let brake_bias_rear = if neutral { 1.0 } else { 2.0 * (1.0 - h.brake_bias) };
        let traction_bias_front = if neutral { 1.0 } else { 2.0 * h.traction_bias };
        let traction_bias_rear = if neutral { 1.0 } else { 2.0 - traction_bias_front };

        // how many wheels touch the ground
        p.car.wheels_on_ground = 0;
        p.car.drive_wheels_on_ground_prev = p.car.drive_wheels_on_ground;
        p.car.drive_wheels_on_ground = 0;
        for i in 0..4 {
            if p.veh.suspension_spring_ratio[i] < 1.0 {
                p.car.wheel_timer[i] = 4.0;
            } else {
                p.car.wheel_timer[i] = (p.car.wheel_timer[i] - step).max(0.0);
            }
            if p.car.wheel_timer[i] > 0.0 {
                p.car.wheels_on_ground += 1;
                match h.transmission.drive_type {
                    b'4' => p.car.drive_wheels_on_ground += 1,
                    b'F' if i == CARWHEEL_FRONT_LEFT || i == CARWHEEL_FRONT_RIGHT => p.car.drive_wheels_on_ground += 1,
                    b'R' if i == CARWHEEL_REAR_LEFT || i == CARWHEEL_REAR_RIGHT => p.car.drive_wheels_on_ground += 1,
                    _ => {}
                }
            }
        }

        let mut traction = if p.base.status == status::PHYSICS { 0.004 * p.car.traction } else { 0.004 };
        traction *= h.traction_multiplier / 4.0;
        traction /= p.phys.force_multiplier;
        if env.cheats.cheat3 {
            traction *= 4.0;
        }

        if me_is_player_vehicle {
            if env.pad.car_gun_just_down {
                let ped = env.player.ped;
                p.arm_bomb(ped, &mut events);
            }
        } else if a.strong_grip1 || env.cheats.cheat3 {
            traction *= 1.2;
            acceleration *= 1.4;
            if a.strong_grip2 || env.cheats.cheat3 {
                traction *= 1.3;
                acceleration *= 1.4;
            }
        }

        // the wheels
        let front_drive = ents.handling.has_front_wheel_drive(p.veh.handling);
        let rear_drive = ents.handling.has_rear_wheel_drive(p.veh.handling);
        let wet = ents.wet_roads;
        let game = ents.models.game;
        let m = p.base.matrix;
        let mass = p.phys.mass;
        let _ = mass;
        let n_on_ground = p.car.wheels_on_ground as i32;
        let wheel_damage_effect = p.car.damage.wheel_damage_effect;
        let handbrake = p.veh.flags.is_handbrake_on;
        let mut run_wheel = |p: &mut Parts, ents_statics: &mut crate::vehicle::base::WheelStatics, i: usize, fwd: Vec3, right: Vec3, drive: bool, brake_in: f32, brake_bias: f32, traction_bias: f32| {
            let thrust = if drive { acceleration } else { 0.0 };
            p.car.wheel_col_points[i].surface_a = surface::WHEELBASE;
            let mut adhesion = {
                let (ga, gb) = (surface::adhesion_group(game, p.car.wheel_col_points[i].surface_b), surface::adhesion_group(game, p.car.wheel_col_points[i].surface_a));
                ents_adhesive(&ents.adhesive_limits, ga, gb) * traction
            };
            if p.base.status == status::PLAYER {
                adhesion *= surface::wet_multiplier(game, p.car.wheel_col_points[i].surface_b, wet);
            }
            ents_statics.scratch_states[i] = p.car.wheel_state[i];
            let burst = p.car.damage.wheel_status(i) == crate::vehicle::damage::wheel_status::BURST;
            let (status_flag, damage_mul) = if burst { (crate::vehicle::damage::wheel_status::BURST, wheel_damage_effect) } else { (crate::vehicle::damage::wheel_status::OK, 1.0) };
            let w = Wheel {
                fwd,
                right,
                contact_speed: contact_speeds[i],
                contact_point: contact_points[i],
                wheels_on_ground: n_on_ground,
                thrust,
                brake: brake_in * brake_bias,
                adhesion: adhesion * traction_bias * damage_mul,
                status: status_flag,
            };
            let mut state = ents_statics.scratch_states[i];
            // the vehicle data and physics are separate borrows of the same entity
            p.veh.process_wheel(p.phys, &m, h, &ents.handling, model, &w, &mut state, ents_statics, &timer, rng);
            ents_statics.scratch_states[i] = state;
        };

        let wheel_off = |p: &mut Parts, i: usize, drive: bool, acceleration: f32| {
            if drive && acceleration != 0.0 {
                if acceleration > 0.0 {
                    if p.car.wheel_speed[i] < 2.0 {
                        p.car.wheel_speed[i] -= 0.2;
                    }
                } else if p.car.wheel_speed[i] > -2.0 {
                    p.car.wheel_speed[i] += 0.1;
                }
            } else {
                p.car.wheel_speed[i] *= 0.95;
            }
            p.car.wheel_rotation[i] += p.car.wheel_speed[i];
        };

        let mut statics = std::mem::take(&mut ents.wheel_statics);

        // front wheels on the ground
        if p.car.wheel_timer[CARWHEEL_FRONT_LEFT] > 0.0 || p.car.wheel_timer[CARWHEEL_FRONT_RIGHT] > 0.0 {
            let (s, c) = (p.veh.steer_angle.sin(), p.veh.steer_angle.cos());
            let wheel_fwd = m.rotate_vector(Vec3::new(-s, c, 0.0));
            let wheel_right = m.rotate_vector(Vec3::new(c, s, 0.0));
            for i in [CARWHEEL_FRONT_LEFT, CARWHEEL_FRONT_RIGHT] {
                if p.car.wheel_timer[i] > 0.0 {
                    run_wheel(&mut p, &mut statics, i, wheel_fwd, wheel_right, front_drive, brake, brake_bias_front, traction_bias_front);
                }
            }
        }
        // front wheels off the ground
        for i in [CARWHEEL_FRONT_LEFT, CARWHEEL_FRONT_RIGHT] {
            if p.car.wheel_timer[i] <= 0.0 {
                wheel_off(&mut p, i, front_drive, acceleration);
            }
        }
        // rear wheels on the ground
        if p.car.wheel_timer[CARWHEEL_REAR_LEFT] > 0.0 || p.car.wheel_timer[CARWHEEL_REAR_RIGHT] > 0.0 {
            if handbrake {
                brake = 20000.0 * timer.time_step_fix();
            }
            for i in [CARWHEEL_REAR_LEFT, CARWHEEL_REAR_RIGHT] {
                if p.car.wheel_timer[i] > 0.0 {
                    run_wheel(&mut p, &mut statics, i, m.forward, m.right, rear_drive, brake, brake_bias_rear, traction_bias_rear);
                }
            }
        }
        for i in [CARWHEEL_REAR_LEFT, CARWHEEL_REAR_RIGHT] {
            if p.car.wheel_timer[i] <= 0.0 {
                wheel_off(&mut p, i, rear_drive, acceleration);
            }
        }

        for i in 0..4 {
            let mut wheel_pos = col.lines[i].p0.z;
            if p.veh.suspension_spring_ratio[i] > 0.0 {
                wheel_pos -= p.veh.suspension_spring_ratio[i] * p.car.suspension_spring_length[i];
            }
            p.car.wheel_position[i] += (wheel_pos - p.car.wheel_position[i]) * 0.75;
        }
        for i in 0..4 {
            p.car.wheel_state[i] = statics.scratch_states[i];
        }
        ents.wheel_statics = statics;

        // horn
        if p.base.status != status::PLAYER {
            p.reduce_horn_counter();
        } else {
            let hist = &env.pad.horn_history;
            let n = hist.len();
            let cur = n - 1;
            let prev = |k: usize| hist[(cur + n - k) % n];
            if model == mi::MRWHOOP {
                if hist[cur] && !prev(1) {
                    p.veh.siren_or_alarm = !p.veh.siren_or_alarm;
                }
            } else if crate::vehicle::base::VehicleData::uses_siren(model) {
                if hist[cur] {
                    p.veh.car_horn_timer = u8::from(prev(1) && prev(2));
                } else if prev(1) && !hist[(cur + 1) % n] {
                    p.veh.car_horn_timer = 0;
                    p.veh.siren_or_alarm = !p.veh.siren_or_alarm;
                } else {
                    p.veh.car_horn_timer = 0;
                }
            } else if model != mi::YARDIE && !env.cheats.cheat3 {
                p.veh.car_horn_timer = u8::from(hist[cur]);
            }
        }

        // flying
        if p.base.status != status::PLAYER && p.base.status != status::PLAYER_REMOTE && p.base.status != status::PHYSICS {
            // (the Vice City RC helicopters reduce their rotor speed here)
        } else if (model == mi::DODO || env.cheats.all_dodos) && magnitude(p.phys.move_speed) > 0.0 && timer.time_step > 0.0 {
            let flight = Flight { pad: &env.pad.state, col: &col, entity_status: p.base.status, is_sparrow: false, cheat5: env.cheats.cheat5, steer_angle: p.veh.steer_angle, timer: &timer };
            let m = p.base.matrix;
            flying_control(FlightModel::Dodo, p.phys, &m, &flight);
        }
    }

    // fire
    let engine_status = p.car.damage.engine_status;
    let _ = engine_status;
    let mut damage_pos = wheel_info.0 + p.engine_offset();
    if me_is_player_vehicle && env.player.looking_forward_first_person && p.veh.health < 250.0 && p.base.status != status::WRECKED {
        damage_pos += if model == mi::FIRETRUCK { Vec3::new(0.0, 3.0, -0.2) } else { Vec3::new(0.0, 1.2, -0.8) };
    }
    damage_pos = p.base.matrix.transform_point(damage_pos);
    damage_pos.z += 0.15;
    if p.veh.health < 250.0 && p.base.status != status::WRECKED {
        // the car is on fire
        events.push(CarEvent::Particle(Particle::new(ParticleKind::CarFlame, damage_pos, Vec3::new(0.0, 0.0, rng.range_f32(0.01125, 0.09)), 0.9)));
        let mut coors = damage_pos;
        coors.x += rng.range_f32(-0.5625, 0.5625);
        coors.y += rng.range_f32(-0.5625, 0.5625);
        coors.z += rng.range_f32(0.5625, 2.25);
        events.push(CarEvent::Particle(Particle::new(ParticleKind::CarFlameSmoke, coors, Vec3::ZERO, 0.0)));
        events.push(CarEvent::Particle(Particle::new(ParticleKind::EngineSmoke2, damage_pos, Vec3::ZERO, 0.5)));
        // it blows up after five seconds
        p.car.fire_blow_up_timer += timer.time_step_in_milliseconds() as f32;
        if p.car.fire_blow_up_timer > 5000.0 {
            events.push(CarEvent::AwardMoneyForExplosion);
            let culprit = p.car.set_on_fire_entity;
            p.blow_up_car(culprit, h, model, timer.time_in_ms, &mut events);
        }
    } else {
        p.car.fire_blow_up_timer = 0.0;
    }

    // a badly damaged engine drains health
    if p.car.damage.engine_status > engine::ON_FIRE && p.veh.health > 250.0 {
        p.veh.health -= 2.0;
    }
    let bomb_active = p.car.bomb_type == BombType::TimedActive;
    events.extend(p.veh.process_delayed_explosion(&timer, bomb_active).into_iter().map(CarEvent::Vehicle));

    if p.veh.siren_or_alarm && timer.frame_counter & 7 == 5 && crate::vehicle::base::VehicleData::uses_siren(model) && model != mi::MRWHOOP {
        events.push(CarEvent::MakeWayForSiren);
    }

    // how much the pad shakes, from the suspension and the ground
    let (mut susp_shake, mut surf_shake) = (0.0f32, 0.0f32);
    for i in 0..4 {
        let change = p.car.suspension_spring_ratio_prev[i] - p.veh.suspension_spring_ratio[i];
        if change > 0.3 {
            events.push(CarEvent::Sound { sound: CarSound::CarJump, volume: change });
            if change > susp_shake {
                susp_shake = change;
            }
        }
        let surf = p.car.wheel_col_points[i].surface_b;
        if surf == surface::GRAVEL || surf == surface::WATER || surf == surface::HEDGE {
            if surf_shake < 0.2 {
                surf_shake = 0.3;
            }
        } else if surf == surface::MUD_DRY || surf == surface::SAND {
            if surf_shake < 0.1 {
                surf_shake = 0.2;
            }
        } else if surf == surface::GRASS && surf_shake < 0.05 {
            surf_shake = 0.1;
        }
        p.car.suspension_spring_ratio_prev[i] = p.veh.suspension_spring_ratio[i];
        p.veh.suspension_spring_ratio[i] = 1.0;
    }
    if (susp_shake > 0.0 || surf_shake > 0.0) && p.base.status == status::PLAYER {
        let speed = magnitude_squared(p.phys.move_speed);
        if speed > 0.1 * 0.1 {
            let speed = speed.sqrt();
            if susp_shake > 0.0 {
                let freq = (200.0 * susp_shake * speed * 2000.0 / p.phys.mass + 100.0).min(250.0) as u8;
                events.push(CarEvent::PadShake { duration: 20000.0 * step / freq as f32, freq });
            } else {
                let freq = (200.0 * surf_shake * speed * 2000.0 / p.phys.mass + 40.0).min(150.0) as u8;
                events.push(CarEvent::PadShake { duration: 5000.0 * step / freq as f32, freq });
            }
        }
    }

    p.veh.flags.vehicle_col_processed = false;
    let warned = p.veh.flags.warned_peds;
    p.finish();
    if !warned {
        hooks.scan_for_ped_danger(e);
    }
    let p = Parts::of(r, e).expect("a car");

    // turn around at the edge of the world
    use std::f32::consts::{FRAC_PI_2 as HALFPI, PI};
    let heading_of = |m: &crate::math::Matrix| (-m.forward.x).atan2(m.forward.y);
    let pos = p.base.position();
    if pos.x > 1900.0 {
        if p.phys.move_speed.x > 0.0 {
            p.phys.move_speed.x *= -1.0;
        }
        let heading = heading_of(&p.base.matrix);
        if heading > 0.0 {
            p.base.set_heading(-heading);
        }
    } else if pos.x < -1900.0 {
        if p.phys.move_speed.x < 0.0 {
            p.phys.move_speed.x *= -1.0;
        }
        let heading = heading_of(&p.base.matrix);
        if heading < 0.0 {
            p.base.set_heading(-heading);
        }
    }
    if pos.y > 1900.0 {
        if p.phys.move_speed.y > 0.0 {
            p.phys.move_speed.y *= -1.0;
        }
        let heading = heading_of(&p.base.matrix);
        if heading < HALFPI && heading > 0.0 {
            p.base.set_heading(PI - heading);
        } else if heading > -HALFPI && heading < 0.0 {
            p.base.set_heading(-PI - heading);
        }
    } else if pos.y < -1900.0 {
        if p.phys.move_speed.y < 0.0 {
            p.phys.move_speed.y *= -1.0;
        }
        let heading = heading_of(&p.base.matrix);
        if heading > HALFPI {
            p.base.set_heading(PI - heading);
        } else if heading < -HALFPI {
            p.base.set_heading(-PI - heading);
        }
    }

    if p.phys.flags.infinite_mass {
        p.phys.move_speed = Vec3::ZERO;
        p.phys.turn_speed = Vec3::ZERO;
        p.phys.move_friction = Vec3::ZERO;
        p.phys.turn_friction = Vec3::ZERO;
    } else if !a.skip_physics && ((p.veh.gas_pedal == 0.0 && brake == 0.0) || p.base.status == status::WRECKED) {
        let s = p.phys.move_speed;
        if s.x.abs() < 0.005 && s.y.abs() < 0.005 && s.z.abs() < 0.005 {
            p.phys.move_speed = Vec3::ZERO;
            p.phys.turn_speed.z = 0.0;
        }
    }
    let _ = (door_status::OK, node::BONNET, CarEvent::MakeWayForSiren, Arc::<ColModel>::default());
    p.finish();
    env.events = events;
}

/// `GetAdhesiveLimit` for an adhesion pair.
fn ents_adhesive(table: &[Vec<f32>], a: usize, b: usize) -> f32 {
    table.get(a).and_then(|r| r.get(b)).copied().unwrap_or(0.0)
}

#[cfg(test)]
impl World {
    pub(super) fn flush_after_car_for_tests(&mut self, env: &mut CarEnv) {
        self.flush_after_car(env, &mut Rng::default());
    }
}
