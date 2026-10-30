//! `CWorld::Process`, explosions and the area helpers (clear, extinguish, set on fire).
use super::cars::model_is_police;
use super::physics::WorldEvent;
use super::{Kinds, World, OBJECTS, PEDS, VEHICLES};
use crate::entity::collide::CollisionEvent;
use crate::entity::physical;
use crate::entity::{status, Ent, EntityRef, EntityType, Kind, PoolKind};
use crate::general::Rng;
use crate::vehicle::automobile::BombType;
use crate::vehicle::base::{weapon, VehicleSub, MISSION_VEHICLE, PARKED_VEHICLE, RANDOM_VEHICLE};
use crate::vehicle::env::{CarEnv, CarEvent, CarHooks};
use crate::weapons::explosion::{ExplosionEnv, ExplosionType, Victim};
use glam::Vec3;

const MAP_Z_LOW_LIMIT: f32 = -100.0;

#[derive(Debug, Clone, Copy)]
pub struct PendingExplosion {
    pub exploding: Option<EntityRef>,
    pub culprit: Option<EntityRef>,
    pub ty: ExplosionType,
    pub pos: Vec3,
    pub lifetime: u32,
}

/// The ped, path and population side of the world loop. Defaults do nothing.
pub trait WorldHooks: CarHooks {
    /// `RpAnimBlendClumpUpdateAnimations` for an entity with a playing animation.
    fn update_animations(&mut self, _w: &mut World, _r: EntityRef, _non_clipped: bool) {}
    /// A ped's `ProcessControl`.
    fn process_ped(&mut self, _w: &mut World, _r: EntityRef, _rng: &mut Rng) {}
    /// Keeps a ped that is entering, leaving or sitting in a vehicle attached to it.
    fn ped_vehicle_fixup(&mut self, _w: &mut World, _r: EntityRef) {}
    /// `CWorld::RemoveFallenPeds`.
    fn remove_fallen_peds(&mut self, _w: &mut World) {}
    /// `CPopulation::RemovePed`.
    fn remove_ped(&mut self, w: &mut World, r: EntityRef) {
        w.delete(r);
    }
    /// `CVehicle::CanBeDeleted` for one occupant: not a mission ped, and driving or dead.
    fn occupant_can_be_deleted(&mut self, _w: &World, _ped: EntityRef) -> bool {
        true
    }
    /// Position of the path node closest to `pos` (`FindNodeClosestToCoors`).
    fn closest_path_node(&mut self, _car: bool, pos: Vec3) -> Vec3 {
        pos
    }
}

/// The ground, water and fire queries an explosion makes, answered by the world.
struct ExplEnv<'a> {
    w: &'a mut World,
    camera: Vec3,
}

impl ExplosionEnv for ExplEnv<'_> {
    fn ground_z(&mut self, p: Vec3) -> (f32, bool) {
        self.w.find_ground_z_for_3d_coord(p)
    }

    fn water_level_no_waves(&mut self, p: Vec3) -> Option<f32> {
        self.w.ents.water.height(p.x, p.y, p.z, false)
    }

    fn start_fire(&mut self, pos: Vec3, size: f32) {
        self.w.ents.events.push(WorldEvent::StartFireAt { pos, size });
    }

    fn trigger_explosion(&mut self, rng: &mut Rng, pos: Vec3, radius: f32, power: f32, culprit: Option<EntityRef>, fiery: bool) {
        self.w.trigger_explosion(rng, pos, radius, power, culprit, fiery);
    }

    fn set_peds_on_fire(&mut self, pos: Vec3, radius: f32, culprit: Option<EntityRef>) {
        self.w.set_peds_on_fire(pos, radius, culprit);
    }

    fn set_cars_on_fire(&mut self, pos: Vec3, radius: f32, culprit: Option<EntityRef>) {
        self.w.set_cars_on_fire(pos, radius, culprit);
    }

    fn vertical_line_z(&mut self, from: Vec3) -> f32 {
        let kinds = Kinds { buildings: true, dummies: true, ..Kinds::NONE };
        self.w.process_vertical_line(from, -1000.0, kinds, false, None).map_or(0.0, |h| h.point.point.z)
    }

    fn victim(&self, e: EntityRef) -> Option<Victim> {
        let ent = self.w.get(e)?;
        let Kind::Vehicle(v) = &ent.kind else { return None };
        let wing_pos = match &v.sub {
            VehicleSub::Car(c) => c.node_positions.get(crate::vehicle::automobile::node::WING_LR).copied().flatten().map(|p| ent.base.matrix.transform_point(p)),
            _ => None,
        };
        Some(Victim { pos: ent.base.position(), is_boat: v.is_boat(), wing_pos })
    }

    fn camera_pos(&self) -> Vec3 {
        self.camera
    }
}

impl World {
    /// `CExplosion::AddExplosion`.
    #[allow(clippy::too_many_arguments)] // mirrors the original's signature
    pub fn add_explosion(&mut self, rng: &mut Rng, camera: Vec3, exploding: Option<EntityRef>, culprit: Option<EntityRef>, ty: ExplosionType, pos: Vec3, lifetime: u32) -> bool {
        let mut ex = std::mem::take(&mut self.ents.explosions);
        let timer = self.ents.timer.clone();
        let ok = ex.add(&mut ExplEnv { w: self, camera }, rng, &timer, exploding, culprit, ty, pos, lifetime);
        self.ents.explosions = ex;
        self.flush_pending_explosions(rng, camera);
        ok
    }

    /// `CExplosion::Update`.
    pub fn update_explosions(&mut self, rng: &mut Rng, camera: Vec3) {
        let mut ex = std::mem::take(&mut self.ents.explosions);
        let timer = self.ents.timer.clone();
        ex.update(&mut ExplEnv { w: self, camera }, rng, &timer);
        self.ents.explosions = ex;
        self.flush_pending_explosions(rng, camera);
    }

    fn flush_pending_explosions(&mut self, rng: &mut Rng, camera: Vec3) {
        while !self.ents.pending_explosions.is_empty() {
            for p in std::mem::take(&mut self.ents.pending_explosions) {
                self.add_explosion(rng, camera, p.exploding, p.culprit, p.ty, p.pos, p.lifetime);
            }
        }
    }

    /// `CWeapon::BlowUpExplosiveThings`.
    pub fn blow_up_explosive_thing(&mut self, rng: &mut Rng, r: EntityRef) {
        let Some(mut e) = self.ents.pools.take(r) else { return };
        self.blow_up_explosive_ent(rng, r, &mut e);
        self.ents.pools.restore(r, e);
        self.flush_moving();
    }

    fn blow_up_explosive_ent(&mut self, rng: &mut Rng, r: EntityRef, e: &mut Ent) {
        let model = e.base.model_index;
        let Kind::Object(o) = &mut e.kind else { return };
        if !self.ents.indices.is_explosive_thing(model) || o.has_been_damaged {
            return;
        }
        o.has_been_damaged = true;
        let pos = e.base.position() + Vec3::Z * 0.5;
        self.ents.pending_explosions.push(PendingExplosion { exploding: Some(r), culprit: self.ents.player_ped, ty: ExplosionType::Barrel, pos, lifetime: 100 });
        let p = e.phys.as_mut().expect("physical");
        p.move_speed.z += if model == self.ents.indices.exploding_barrel() { 0.75 } else { 0.45 };
        p.move_speed.x += ((rng.number() & 255) as f32 - 128.0) * 0.0002;
        p.move_speed.y += ((rng.number() & 255) as f32 - 128.0) * 0.0002;
        if e.base.flags.is_static {
            e.base.flags.is_static = false;
            self.ents.pending_moving.push(r);
        }
    }

    // ---- explosions ----

    /// `CWorld::TriggerExplosion`.
    pub fn trigger_explosion(&mut self, rng: &mut Rng, pos: Vec3, radius: f32, power: f32, creator: Option<EntityRef>, process_bomb_timer: bool) {
        let g = self.grid;
        let (x0, y0) = (g.index_x(pos.x - radius).max(0), g.index_y(pos.y - radius).max(0));
        let (x1, y1) = (g.index_x(pos.x + radius).min(g.num - 1), g.index_y(pos.y + radius).min(g.num - 1));
        for y in y0..=y1 {
            for x in x0..=x1 {
                for list in [VEHICLES, PEDS, OBJECTS] {
                    let members: Vec<EntityRef> = self.sector(x, y).map(|s| s.lists[list].iter().copied().collect()).unwrap_or_default();
                    for r in members {
                        self.explosion_on(rng, r, pos, radius, power, creator, process_bomb_timer);
                    }
                }
            }
        }
        self.flush_moving();
    }

    #[allow(clippy::too_many_arguments)] // the original's list loop body
    fn explosion_on(&mut self, rng: &mut Rng, r: EntityRef, pos: Vec3, radius: f32, power: f32, creator: Option<EntityRef>, process_bomb_timer: bool) {
        let Some(mut e) = self.ents.pools.take(r) else { return };
        let dist = e.base.position() - pos;
        let magnitude = dist.length();
        if radius > magnitude {
            self.blow_up_explosive_ent(rng, r, &mut e);
            self.explosion_hits(rng, r, &mut e, dist, magnitude, pos, radius, power, creator, process_bomb_timer);
        }
        self.ents.pools.restore(r, e);
    }

    #[allow(clippy::too_many_arguments)] // the original's list loop body
    fn explosion_hits(&mut self, rng: &mut Rng, r: EntityRef, e: &mut Ent, dist: Vec3, magnitude: f32, pos: Vec3, radius: f32, power: f32, creator: Option<EntityRef>, process_bomb_timer: bool) {
        let ped_in_vehicle = matches!(&e.kind, Kind::Ped(p) if p.in_vehicle);
        if e.base.flags.explosion_proof || ped_in_vehicle {
            return;
        }
        let model = e.base.model_index;
        let body_cast = self.ents.indices.body_cast();
        if e.base.flags.is_static {
            if let Kind::Object(o) = &e.kind {
                let (uproot, damaged) = (o.uproot_limit, o.has_been_damaged);
                if power > uproot || self.ents.indices.is_fence(model) {
                    if self.ents.indices.is_glass(&self.ents.models, model) {
                        self.ents.events.push(WorldEvent::WindowExplosion { entity: r, pos });
                    } else {
                        e.base.flags.is_static = false;
                        self.ents.pending_moving.push(r);
                        if model != self.ents.indices.fire_hydrant() || damaged {
                            if model != self.ents.indices.exploding_barrel() && !self.ents.indices.is_explosive_thing(model)
                                && let Kind::Object(o) = &mut e.kind {
                                    o.has_been_damaged = true;
                                }
                        } else {
                            let p = e.base.position() - Vec3::Z * 0.5;
                            self.ents.events.push(WorldEvent::Collision(CollisionEvent::FireHydrant { pos: p }));
                            if let Kind::Object(o) = &mut e.kind {
                                o.has_been_damaged = true;
                            }
                        }
                    }
                }
                if e.base.flags.is_static {
                    let multiplier = (radius - magnitude) * 2.0 / radius;
                    let damage = 300.0 * multiplier.min(1.0);
                    let evs = self.ents.objects.object_damage(e, damage, body_cast, rng);
                    self.ents.events.extend(evs.into_iter().map(|o| WorldEvent::Collision(CollisionEvent::Object(o))));
                }
            } else {
                e.base.flags.is_static = false;
                self.ents.pending_moving.push(r);
            }
        }
        if e.base.flags.is_static {
            return;
        }
        let multiplier = ((radius - magnitude) * 2.0 / radius).min(1.0);
        let p = e.phys.as_mut().expect("physical");
        let mut force = dist * (power * p.mass / 1400.0 * multiplier / magnitude.max(0.01));
        force.z = force.z.max(0.0);
        if self.ents.player_ped == Some(r) {
            force.z = force.z.min(1.0);
        }
        p.apply_move_force(force);
        if !e.base.flags.ped_physics {
            let bound = self.ents.bound_radius(e);
            let p = e.phys.as_mut().expect("physical");
            let dz = pos.z - e.base.position().z;
            let point_z = if dz.max(-bound) < bound {
                if dz <= -bound { -bound } else { dz }
            } else {
                bound
            };
            p.apply_turn_force(&e.base.matrix, force, Vec3::new(0.0, 0.0, point_z));
        }
        match e.base.ty {
            EntityType::Vehicle => {
                if e.base.status == status::SIMPLE {
                    e.base.status = status::PHYSICS;
                    self.ents.events.push(WorldEvent::SwitchToRealPhysics(r));
                }
                self.inflict_vehicle_damage(rng, e, creator, weapon::EXPLOSION, 1100.0 * multiplier);
                if process_bomb_timer && let Kind::Vehicle(v) = &mut e.kind && v.bomb_timer != 0 {
                    v.bomb_timer /= 10;
                }
            }
            EntityType::Ped => {
                if let Kind::Ped(ped) = &mut e.kind {
                    ped.flags.is_standing = false;
                }
                let p = e.phys.as_mut().expect("physical");
                p.apply_move_force(Vec3::new(0.0, 0.0, 2.0));
                self.ents.events.push(WorldEvent::ExplosionHitPed { ped: r, culprit: creator, damage: 250.0 * multiplier, force_dir: force });
            }
            EntityType::Object => {
                let evs = self.ents.objects.object_damage(e, 300.0 * multiplier, body_cast, rng);
                self.ents.events.extend(evs.into_iter().map(|o| WorldEvent::Collision(CollisionEvent::Object(o))));
            }
            _ => {}
        }
    }

    /// `CVehicle::InflictDamage` on a lent-out vehicle; what happens next goes out as events.
    pub fn inflict_vehicle_damage(&mut self, rng: &mut Rng, e: &mut Ent, culprit: Option<EntityRef>, weapon_type: i8, damage: f32) {
        let model = e.base.model_index;
        let Kind::Vehicle(v) = &mut e.kind else { return };
        let max_cruise = self.ents.handling.get(v.handling).transmission.max_cruise_velocity;
        let player_is_attacker = culprit.is_some() && (culprit == self.ents.player_ped || culprit == self.ents.player_vehicle_ex);
        let evs = v.inflict_damage(&mut e.base, model_is_police(model), culprit, player_is_attacker, weapon_type, damage, max_cruise, true, rng);
        self.ents.events.extend(evs.into_iter().map(|ev| WorldEvent::Car(CarEvent::Vehicle(ev))));
    }

    /// `SetIsStatic(false)` and `AddToMovingList()` for a shot or pushed entity.
    pub fn make_moving(&mut self, r: EntityRef) {
        if let Some(e) = self.get_mut(r) {
            e.base.flags.is_static = false;
            self.ents.pending_moving.push(r);
            self.flush_moving();
        }
    }

    /// `CWorld::UseDetonator`.
    pub fn use_detonator(&mut self, rigger: EntityRef) {
        let timer_ms = 500;
        for (_, e) in self.ents.pools.vehicles.iter_mut().collect::<Vec<_>>().into_iter().rev() {
            let Kind::Vehicle(v) = &mut e.kind else { continue };
            if let VehicleSub::Car(c) = &mut v.sub
                && c.bomb_type == BombType::Remote
                && c.bomb_rigger == Some(rigger)
            {
                c.bomb_type = BombType::None;
                v.bomb_timer = timer_ms;
                v.blow_up_entity = c.bomb_rigger;
            }
        }
    }

    // ---- area helpers ----

    pub(crate) fn vehicle_refs(&self) -> Vec<EntityRef> {
        let p = &self.ents.pools.vehicles;
        p.iter().map(|(i, _)| EntityRef { pool: PoolKind::Vehicle, handle: p.handle_of(i) }).collect()
    }

    pub(crate) fn object_refs(&self) -> Vec<EntityRef> {
        let p = &self.ents.pools.objects;
        p.iter().map(|(i, _)| EntityRef { pool: PoolKind::Object, handle: p.handle_of(i) }).collect()
    }

    pub(crate) fn ped_refs(&self) -> Vec<EntityRef> {
        let p = &self.ents.pools.peds;
        p.iter().map(|(i, _)| EntityRef { pool: PoolKind::Ped, handle: p.handle_of(i) }).collect()
    }

    /// `CWorld::SetCarsOnFire`.
    pub fn set_cars_on_fire(&mut self, pos: Vec3, radius: f32, reason: Option<EntityRef>) {
        // shortcut: the fire manager isn't ported, so the "already burning" check is left to the consumer
        for r in self.vehicle_refs().into_iter().rev() {
            let Some(e) = self.get(r) else { continue };
            let p = e.base.position();
            if e.base.status != status::WRECKED && !e.base.flags.fire_proof && (p.z - pos.z).abs() < 5.0 && (p.x - pos.x).abs() < radius && (p.y - pos.y).abs() < radius {
                self.ents.events.push(WorldEvent::StartFire { target: r, culprit: reason });
            }
        }
    }

    /// `CWorld::SetPedsOnFire`.
    pub fn set_peds_on_fire(&mut self, pos: Vec3, radius: f32, reason: Option<EntityRef>) {
        for r in self.ped_refs().into_iter().rev() {
            let Some(e) = self.get(r) else { continue };
            let Kind::Ped(ped) = &e.kind else { continue };
            let p = e.base.position();
            if !ped.dead() && !ped.in_vehicle && !e.base.flags.fire_proof && (p.z - pos.z).abs() < 5.0 && (p.x - pos.x).abs() < radius && (p.y - pos.y).abs() < radius {
                self.ents.events.push(WorldEvent::StartFire { target: r, culprit: reason });
            }
        }
    }

    /// `CWorld::ExtinguishAllCarFiresInArea`.
    pub fn extinguish_all_car_fires_in_area(&mut self, point: Vec3, range: f32) {
        for r in self.vehicle_refs() {
            let Some(e) = self.get_mut(r) else { continue };
            if (point - e.base.position()).length_squared() < range * range {
                if let Kind::Vehicle(v) = &mut e.kind {
                    v.extinguish_health();
                }
                self.ents.events.push(WorldEvent::ExtinguishCarFire(r));
            }
        }
    }

    /// `CWorld::SetAllCarsCanBeDamaged`.
    pub fn set_all_cars_can_be_damaged(&mut self, toggle: bool) {
        for (_, e) in self.ents.pools.vehicles.iter_mut() {
            if let Kind::Vehicle(v) = &mut e.kind {
                v.flags.can_be_damaged = toggle;
            }
        }
    }

    /// `CWorld::StopAllLawEnforcersInTheirTracks`.
    pub fn stop_all_law_enforcers_in_their_tracks(&mut self) {
        for (_, e) in self.ents.pools.vehicles.iter_mut() {
            let (Kind::Vehicle(v), Some(p)) = (&e.kind, e.phys.as_mut()) else { continue };
            if v.flags.is_law_enforcer {
                p.move_speed = Vec3::ZERO;
            }
        }
    }

    /// `CWorld::ClearCarsFromArea`.
    pub fn clear_cars_from_area(&mut self, min: Vec3, max: Vec3, hooks: &mut dyn WorldHooks) {
        for r in self.vehicle_refs() {
            let Some(e) = self.get(r) else { continue };
            let p = e.base.position();
            let Kind::Vehicle(v) = &e.kind else { continue };
            let inside = p.cmpge(min).all() && p.cmple(max).all();
            let occupants: Vec<EntityRef> = v.driver.into_iter().chain(v.passengers[..v.num_max_passengers as usize].iter().flatten().copied()).collect();
            let ok: Vec<(EntityRef, bool)> = v.driver.into_iter().chain(v.passengers.iter().flatten().copied()).map(|o| (o, hooks.occupant_can_be_deleted(self, o))).collect();
            let Some(Kind::Vehicle(v)) = self.get(r).map(|e| &e.kind) else { continue };
            let deletable = !v.flags.is_locked && v.can_be_deleted(|o| ok.iter().find(|(p, _)| *p == o).is_none_or(|(_, b)| *b));
            if !(inside && deletable) {
                continue;
            }
            for o in occupants {
                hooks.remove_ped(self, o);
            }
            self.delete(r);
        }
    }

    /// `CWorld::ClearPedsFromArea`.
    pub fn clear_peds_from_area(&mut self, min: Vec3, max: Vec3, hooks: &mut dyn WorldHooks, can_be_deleted: &dyn Fn(&World, EntityRef) -> bool) {
        for r in self.ped_refs() {
            let Some(e) = self.get(r) else { continue };
            let Kind::Ped(ped) = &e.kind else { continue };
            let p = e.base.position();
            if !ped.is_player() && can_be_deleted(self, r) && p.cmpge(min).all() && p.cmple(max).all() {
                hooks.remove_ped(self, r);
            }
        }
    }

    /// `CWorld::RemoveFallenCars`.
    pub fn remove_fallen_cars(&mut self, hooks: &mut dyn WorldHooks, player_vehicle: Option<EntityRef>, driver_is_player: &dyn Fn(EntityRef) -> bool) {
        for r in self.vehicle_refs().into_iter().rev() {
            let Some(e) = self.get(r) else { continue };
            let Kind::Vehicle(v) = &e.kind else { continue };
            if e.base.position().z >= MAP_Z_LOW_LIMIT {
                continue;
            }
            let (created_by, pos) = (v.created_by, e.base.position());
            let player_driven = v.driver.is_some_and(driver_is_player);
            if created_by == MISSION_VEHICLE || Some(r) == player_vehicle || player_driven {
                let mut node = hooks.closest_path_node(true, pos);
                node.z += 3.0;
                self.teleport_car(r, node);
                if let Some(p) = self.get_mut(r).and_then(|e| e.phys.as_mut()) {
                    p.move_speed = Vec3::ZERO;
                }
            } else if created_by == RANDOM_VEHICLE || created_by == PARKED_VEHICLE {
                self.delete(r);
            }
        }
    }

    /// `CWorld::RemoveReferencesToDeletedObject`: forgets `r` everywhere the world keeps it.
    pub fn remove_references_to(&mut self, r: EntityRef) {
        self.ents.explosions.remove_references(r);
        for pool in [PoolKind::Object, PoolKind::Vehicle, PoolKind::Ped] {
            let handles: Vec<i32> = {
                let p = self.ents.pools.pool_mut(pool);
                p.iter().map(|(i, _)| p.handle_of(i)).collect()
            };
            for handle in handles {
                if let Some(p) = self.get_mut(EntityRef { pool, handle }).and_then(|e| e.phys.as_mut()) {
                    p.remove_refs_to(r);
                }
            }
        }
    }

    /// `CWorld::RepositionCertainDynamicObjects`.
    pub fn reposition_certain_dynamic_objects(&mut self) {
        let dummies = &self.ents.pools.dummies;
        let refs: Vec<EntityRef> = dummies.iter().map(|(i, _)| EntityRef { pool: PoolKind::Dummy, handle: dummies.handle_of(i) }).collect();
        for r in refs.into_iter().rev() {
            self.reposition_one_object(r);
        }
    }

    /// `CWorld::RepositionOneObject`.
    pub fn reposition_one_object(&mut self, r: EntityRef) {
        const OFFSET_Z: f32 = 2.0;
        let Some(e) = self.get(r) else { return };
        let (model, pos) = (e.base.model_index, e.base.position());
        let Some(col) = self.ents.col_model(model) else { return };
        if self.ents.indices.is_ground_repositioned(model) {
            let (z, _) = self.find_ground_z_for_3d_coord(Vec3::new(pos.x, pos.y, pos.z + OFFSET_Z));
            self.get_mut(r).unwrap().base.matrix.pos.z = z - col.bounding_box.min.z;
        } else if model == self.ents.indices.buoy() {
            let (ground, found) = self.find_ground_z_for_3d_coord(Vec3::new(pos.x, pos.y, pos.z + OFFSET_Z));
            if let Some(water) = self.ents.water.height(pos.x, pos.y, pos.z + OFFSET_Z, false)
                && (!found || water > ground)
            {
                let height = col.bounding_box.max.z - col.bounding_box.min.z;
                self.get_mut(r).unwrap().base.matrix.pos.z = 0.2 * height + water - 0.5 * height;
            }
        }
    }

    /// `CWorld::ClearForRestart`: temp and mission objects, peds, planes and vehicles go. Projectiles and
    /// the objects-to-dummies conversion belong to the callers.
    pub fn clear_for_restart(&mut self) {
        self.delete_objects_created_by(crate::entity::object::CreatedBy::Temp);
        self.delete_objects_created_by(crate::entity::object::CreatedBy::Mission);
        for r in self.ped_refs() {
            self.delete(r);
        }
        let planes: Vec<EntityRef> = self.big_buildings(0).iter().copied().filter(|&r| matches!(self.get(r).map(|e| &e.kind), Some(Kind::Vehicle(v)) if matches!(v.sub, VehicleSub::Plane(_)))).collect();
        for r in planes {
            self.delete(r);
        }
        for r in self.vehicle_refs() {
            self.delete(r);
        }
    }

    // ---- the frame ----

    /// `ProcessControl` for one moving entity.
    fn process_control_entity(&mut self, r: EntityRef, env: &mut CarEnv, hooks: &mut dyn WorldHooks, rng: &mut Rng) {
        let Some(e) = self.get(r) else { return };
        match &e.kind {
            Kind::Object(_) => self.process_control_object(r, rng),
            Kind::Ped(_) => hooks.process_ped(self, r, rng),
            Kind::Vehicle(v) => match &v.sub {
                VehicleSub::Car(_) => self.process_control_car(r, env, hooks, rng),
                VehicleSub::Boat(_) => self.process_control_boat(r, env, hooks, rng),
                VehicleSub::Heli(_) => self.process_control_heli(r, env, rng),
                VehicleSub::Plane(_) => self.process_control_plane(r, env, rng),
                VehicleSub::Train(_) => self.process_control_train(r, env),
                VehicleSub::None => {}
            },
            _ => {}
        }
    }

    /// `CObject::ProcessControl`.
    pub fn process_control_object(&mut self, r: EntityRef, rng: &mut Rng) {
        let Some(mut e) = self.ents.pools.take(r) else { return };
        let step = self.ents.timer.time_step;
        let body_cast = self.ents.indices.body_cast();
        let Some(mut phys) = e.phys.take() else {
            self.ents.pools.restore(r, e);
            return;
        };
        if matches!(&e.kind, Kind::Object(o) if o.collision_damage_effect != 0) {
            let saved = e.phys.replace(phys);
            let amount = e.phys.as_ref().map_or(0.0, |p| p.damage_impulse);
            let evs = self.ents.objects.object_damage(&mut e, amount, body_cast, rng);
            self.ents.events.extend(evs.into_iter().map(|o| WorldEvent::Collision(CollisionEvent::Object(o))));
            phys = e.phys.take().or(saved).expect("physical");
        }
        physical::process_control(&mut e.base, &mut phys, step);
        if let Some(col) = self.ents.col_model(e.base.model_index) {
            let m = e.base.matrix;
            if let Some((point, impulse)) = self.ents.buoyancy.process(self.ents.water.as_ref(), &m, &col, None, e.base.flags.touching_water, phys.buoyancy, step) {
                phys.flags.is_in_water = true;
                if e.base.flags.is_static {
                    e.base.flags.is_static = false;
                    self.ents.pending_moving.push(r);
                }
                phys.apply_move_force(impulse);
                phys.apply_turn_force(&m, impulse, point);
                let damp = 0.97_f32.powf(step);
                phys.move_speed *= damp;
                phys.turn_speed *= damp;
            }
        }
        if self.ents.indices.is_explosive_thing(e.base.model_index) && matches!(&e.kind, Kind::Object(o) if o.has_been_damaged) && e.base.flags.is_visible && rng.number() & 0x1F == 10 {
            e.base.flags.explosion_proof = true;
            e.base.flags.is_visible = false;
            e.base.flags.uses_collision = false;
            phys.flags.affected_by_gravity = false;
            phys.move_speed = Vec3::ZERO;
        }
        e.phys = Some(phys);
        self.ents.pools.restore(r, e);
        self.flush_moving();
    }

    fn remove_instead_of_processing(&mut self, r: EntityRef, hooks: &mut dyn WorldHooks) {
        if r.pool == PoolKind::Ped && self.ents.player_ped != Some(r) {
            hooks.remove_ped(self, r);
        } else {
            self.delete(r);
        }
    }

    fn process_one(&mut self, r: EntityRef, env: &mut CarEnv, hooks: &mut dyn WorldHooks, rng: &mut Rng) {
        if self.get(r).is_some_and(|e| e.base.flags.remove_from_world) {
            self.remove_instead_of_processing(r, hooks);
            return;
        }
        self.process_control_entity(r, env, hooks, rng);
        if self.get(r).is_some_and(|e| e.base.flags.is_static) {
            self.remove_from_moving_list(r);
        }
    }

    /// `CWorld::Process`. Skipped: the cutscene-only branch, chase recording, messages and the player info update.
    pub fn process(&mut self, env: &mut CarEnv, hooks: &mut dyn WorldHooks, rng: &mut Rng) {
        let moving = |w: &World| -> Vec<EntityRef> { w.moving.iter().copied().collect() };
        for r in moving(self) {
            let non_clipped = r.pool == PoolKind::Object;
            hooks.update_animations(self, r, non_clipped);
        }
        for r in moving(self) {
            self.process_one(r, env, hooks, rng);
        }
        self.ents.force_process_control = true;
        for r in moving(self) {
            if self.get(r).is_some_and(|e| e.base.flags.was_postponed) {
                self.process_one(r, env, hooks, rng);
            }
        }
        self.ents.force_process_control = false;

        let unsafe_ents = |w: &World| -> Vec<EntityRef> { w.moving.iter().copied().filter(|&r| w.get(r).is_some_and(|e| !e.base.flags.is_in_safe_position)).collect() };
        self.ents.no_more_collision_torque = false;
        for r in moving(self) {
            if self.get(r).is_some_and(|e| !e.base.flags.is_in_safe_position) {
                self.process_collision(r, rng);
            }
        }
        self.ents.no_more_collision_torque = true;
        for _ in 0..4 {
            for r in moving(self) {
                if self.get(r).is_some_and(|e| !e.base.flags.is_in_safe_position) {
                    self.process_collision(r, rng);
                }
            }
        }
        for r in moving(self) {
            if self.get(r).is_some_and(|e| !e.base.flags.is_in_safe_position) {
                self.get_mut(r).unwrap().base.flags.is_stuck = true;
                self.process_collision(r, rng);
                if let Some(e) = self.get_mut(r)
                    && !e.base.flags.is_in_safe_position
                {
                    e.base.flags.is_stuck = true;
                }
            }
        }
        self.ents.second_shift = false;
        for r in moving(self) {
            if self.get(r).is_some_and(|e| !e.base.flags.is_in_safe_position) {
                self.process_shift(r, rng);
                if let Some(e) = self.get_mut(r)
                    && !e.base.flags.is_in_safe_position
                {
                    e.base.flags.is_stuck = true;
                }
            }
        }
        self.ents.second_shift = true;
        let step = self.ents.timer.time_step;
        for r in unsafe_ents(self) {
            self.process_shift(r, rng);
            let Some(e) = self.get_mut(r) else { continue };
            if e.base.flags.is_in_safe_position {
                continue;
            }
            e.base.flags.is_stuck = true;
            if e.base.status == status::PLAYER
                && let Some(p) = e.phys.as_mut()
            {
                p.move_speed *= 0.3;
                p.apply_move_speed(&mut e.base.matrix, step);
                p.apply_turn_speed(&mut e.base.matrix, step);
            }
        }
        for r in moving(self) {
            if r.pool == PoolKind::Ped {
                hooks.ped_vehicle_fixup(self, r);
            }
        }
        match self.ents.timer.frame_counter & 7 {
            1 => hooks.remove_fallen_peds(self),
            5 => {
                let (player_vehicle, ents_player) = (env.player.vehicle, env.player.ped);
                self.remove_fallen_cars(hooks, player_vehicle, &|d| Some(d) == ents_player);
            }
            _ => {}
        }
        self.flush_moving();
    }
}


impl WorldHooks for crate::vehicle::env::NoHooks {}
