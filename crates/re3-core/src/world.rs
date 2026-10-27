//! The world: entity pools, the sector grid and the spatial queries (`World.cpp`).
//!
//! Sector lists insert at the front and the query order decides ties, so they are `VecDeque`s
//! with `push_front`. Globals of the original (`pIgnoreEntity`, `bIncludeDeadPeds`, the scan
//! code) are fields of [`Ents`].
use crate::collision::{self, Box3, ColModel, ColPoint, Line, LineFilter, Sphere, StoredCollPoly};
use crate::entity::object::{CreatedBy, ObjectTable};
use crate::entity::physical::Physical;
use crate::entity::{Ent, Entity, EntityRef, EntityType, EntryInfo, Kind, ObjectData, PoolKind, TreadableData};
use crate::general::Rng;
use crate::math::{magnitude, magnitude_squared, Matrix, Rect};
use crate::modelinfo::{indices_gta3, indices_vc, vehicle_type, Indices, ModelInfoStore};
use crate::pool::Pool;
use crate::temp_col::TempColModels;
use crate::timer::Timer;
use crate::vehicle::automobile::control::SplashState;
use crate::vehicle::base::WheelStatics;
use crate::vehicle::floater::{Buoyancy, NoWater, WaterLevel};
use crate::vehicle::handling::HandlingMgr;
use glam::Vec3;
use re3_formats::datafile::Game;
use std::collections::VecDeque;
use std::sync::Arc;

mod boats;
mod cars;
mod helis;
mod planes;
mod trains;
mod cranes;
mod cargens;
mod ped_collision;
mod physics;
mod process;
#[cfg(test)]
pub(crate) mod tests;

pub use physics::WorldEvent;

pub const MAP_Z_LOW_LIMIT: f32 = -100.0;

// Indices into a sector's lists; every kind has a main list and an overlap list after it.
pub const BUILDINGS: usize = 0;
pub const OBJECTS: usize = 2;
pub const VEHICLES: usize = 4;
pub const PEDS: usize = 6;
pub const DUMMIES: usize = 8;
const NUM_LISTS: usize = 10;

/// Sector grid geometry (`World.h`): 100x100 of 40 in GTA III, 80x80 of 50 in Vice City.
#[derive(Debug, Clone, Copy)]
pub struct Grid {
    pub size: f32,
    pub num: i32,
    pub min_x: f32,
    pub min_y: f32,
}

impl Grid {
    pub fn for_game(game: Game) -> Self {
        match game {
            Game::Gta3 => Self { size: 40.0, num: 100, min_x: -2000.0, min_y: -2000.0 },
            Game::Vc => Self { size: 50.0, num: 80, min_x: -2400.0, min_y: -2000.0 },
        }
    }

    pub fn sector_x(&self, f: f32) -> f32 {
        (f - self.min_x) / self.size
    }

    pub fn sector_y(&self, f: f32) -> f32 {
        (f - self.min_y) / self.size
    }

    /// `GetSectorIndexX`: truncates toward zero like the C cast, so values just below the map edge land in sector 0.
    pub fn index_x(&self, f: f32) -> i32 {
        self.sector_x(f) as i32
    }

    pub fn index_y(&self, f: f32) -> i32 {
        self.sector_y(f) as i32
    }

    pub fn world_x(&self, x: i32) -> f32 {
        x as f32 * self.size + self.min_x
    }

    pub fn world_y(&self, y: i32) -> f32 {
        y as f32 * self.size + self.min_y
    }

    fn contains(&self, x: i32, y: i32) -> bool {
        (0..self.num).contains(&x) && (0..self.num).contains(&y)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Sector {
    pub lists: [VecDeque<EntityRef>; NUM_LISTS],
}

/// Which entity kinds a query looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kinds {
    pub buildings: bool,
    pub vehicles: bool,
    pub peds: bool,
    pub objects: bool,
    pub dummies: bool,
}

impl Kinds {
    pub const ALL: Self = Self { buildings: true, vehicles: true, peds: true, objects: true, dummies: true };
    pub const NONE: Self = Self { buildings: false, vehicles: false, peds: false, objects: false, dummies: false };

    /// Lists in the order the original visits them: buildings, vehicles, peds, objects, dummies.
    fn bases(self) -> impl Iterator<Item = usize> {
        [(self.buildings, BUILDINGS), (self.vehicles, VEHICLES), (self.peds, PEDS), (self.objects, OBJECTS), (self.dummies, DUMMIES)]
            .into_iter()
            .filter_map(|(on, base)| on.then_some(base))
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LineOpts {
    pub ignore_see_through: bool,
    pub ignore_some_objects: bool,
    /// Vice City only.
    pub ignore_shoot_through: bool,
}

impl LineOpts {
    fn filter(self) -> LineFilter {
        LineFilter { ignore_see_through: self.ignore_see_through, ignore_shoot_through: self.ignore_shoot_through }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LineHit {
    pub point: ColPoint,
    pub entity: EntityRef,
}

#[derive(Debug, PartialEq, Eq)]
pub enum WorldError {
    /// The entity's bounds leave the map (the original asserts).
    OutOfWorld,
    /// The entity's model has no collision model, so it has no bounds.
    NoCollision,
    BadEntity,
}

#[derive(Debug)]
pub struct Pools {
    pub buildings: Pool<Ent>,
    pub treadables: Pool<Ent>,
    pub dummies: Pool<Ent>,
    pub objects: Pool<Ent>,
    pub vehicles: Pool<Ent>,
    pub peds: Pool<Ent>,
}

impl Pools {
    /// Pool sizes of the PC builds.
    pub fn new(game: Game) -> Self {
        let (b, t, d, o) = match game {
            Game::Gta3 => (5500, 1214, 2802, 450),
            Game::Vc => (7000, 1, 2340, 460),
        };
        Self { buildings: Pool::new(b), treadables: Pool::new(t), dummies: Pool::new(d), objects: Pool::new(o), vehicles: Pool::new(110), peds: Pool::new(140) }
    }

    fn pool(&self, k: PoolKind) -> &Pool<Ent> {
        match k {
            PoolKind::Building => &self.buildings,
            PoolKind::Treadable => &self.treadables,
            PoolKind::Dummy => &self.dummies,
            PoolKind::Object => &self.objects,
            PoolKind::Vehicle => &self.vehicles,
            PoolKind::Ped => &self.peds,
        }
    }

    fn pool_mut(&mut self, k: PoolKind) -> &mut Pool<Ent> {
        match k {
            PoolKind::Building => &mut self.buildings,
            PoolKind::Treadable => &mut self.treadables,
            PoolKind::Dummy => &mut self.dummies,
            PoolKind::Object => &mut self.objects,
            PoolKind::Vehicle => &mut self.vehicles,
            PoolKind::Ped => &mut self.peds,
        }
    }

    pub fn get(&self, r: EntityRef) -> Option<&Ent> {
        self.pool(r.pool).get(r.handle)
    }

    /// Lends an entity out so two can be borrowed at once; give it back with [`Pools::restore`].
    pub fn take(&mut self, r: EntityRef) -> Option<Ent> {
        self.pool_mut(r.pool).take(r.handle)
    }

    pub fn restore(&mut self, r: EntityRef, e: Ent) {
        self.pool_mut(r.pool).restore(r.handle, e);
    }

    pub fn get_mut(&mut self, r: EntityRef) -> Option<&mut Ent> {
        self.pool_mut(r.pool).get_mut(r.handle)
    }
}

fn pool_kind(kind: &Kind) -> PoolKind {
    match kind {
        Kind::Building => PoolKind::Building,
        Kind::Treadable(_) => PoolKind::Treadable,
        Kind::Dummy => PoolKind::Dummy,
        Kind::Object(_) => PoolKind::Object,
        Kind::Vehicle(_) => PoolKind::Vehicle,
        Kind::Ped(_) => PoolKind::Ped,
    }
}

/// Everything the queries read and write apart from the sector lists.
#[derive(Debug)]
pub struct Ents {
    pub pools: Pools,
    pub models: ModelInfoStore,
    pub temp_col: TempColModels,
    scan_code: u16,
    /// `pIgnoreEntity`.
    pub ignore_entity: Option<EntityRef>,
    /// `bIncludeDeadPeds`.
    pub include_dead_peds: bool,
    /// Model ids `CGarages::IsModelIndexADoor` accepts; filled by the garages.
    pub garage_doors: Vec<i16>,
    pub objects: ObjectTable,
    pub indices: Indices,
    /// `CTimer`: the step the physics runs with (`ProcessCollision` changes it temporarily).
    pub timer: Timer,
    /// `CWorld::bNoMoreCollisionTorque`.
    pub no_more_collision_torque: bool,
    /// `CWorld::bSecondShift`.
    pub second_shift: bool,
    /// `CSurfaceTable::ms_aAdhesiveLimitTable` (adhesion group by group), from surface.dat.
    pub adhesive_limits: Vec<Vec<f32>>,
    /// What the physics asks the rest of the game to do; drained by the caller.
    pub events: Vec<WorldEvent>,
    pub handling: HandlingMgr,
    /// `CPlayerInfo::m_ColModel`: the collision model of a car with hydraulics.
    pub player_col_model: ColModel,
    /// `m_pVehicleEx`.
    pub player_vehicle_ex: Option<EntityRef>,
    pub water: Box<dyn WaterLevel>,
    pub buoyancy: Buoyancy,
    pub splash_state: SplashState,
    /// `CCollision::ms_collisionInMemory`.
    pub collision_in_memory: i32,
    pub all_taxi_lights: bool,
    /// `CWeather::WetRoads`.
    pub wet_roads: f32,
    /// `CWorld::bForceProcessControl`.
    pub force_process_control: bool,
    pub wheel_statics: WheelStatics,
    pub helis: crate::vehicle::heli::HeliManager,
    pub planes: crate::vehicle::plane::PlaneManager,
    pub trains: crate::vehicle::train::TrainManager,
    pub cranes: crate::vehicle::cranes::CraneManager,
    pub cargens: crate::vehicle::cargen::CarGenManager,
    /// Entities a collision made non-static; they join the moving list after the pass.
    pending_moving: Vec<EntityRef>,
    pub explosions: crate::weapons::explosion::Explosions,
    /// Explosions set off while another explosion is being handled; added afterwards.
    pub pending_explosions: Vec<process::PendingExplosion>,
    /// `FindPlayerPed()`.
    pub player_ped: Option<EntityRef>,
    pub ped_types: crate::ped::types::PedTypes,
    pub ped_stats: crate::ped::types::PedStatTable,
    pub fight_moves: Vec<crate::ped::data::FightMove>,
    pub weapon_infos: crate::weapons::info::WeaponInfos,
    pub gangs: crate::ped::types::Gangs,
    pub population: crate::ped::population::Population,
    pub car_ctrl: crate::control::car_ctrl::CarCtrl,
    pub bridge: crate::control::bridge::Bridge,
    pub road_blocks: crate::control::road_blocks::RoadBlocks,
    pub restart: crate::control::restart::Restart,
    pub zones: crate::zones::Zones,
    pub pickups: crate::control::pickups::Pickups,
    pub garages: crate::control::garages::Garages,
    pub scripts: crate::script::Scripts,
    pub script_checks: crate::script::checks::ScriptChecks,
    pub clock: crate::clock::Clock,
    pub darkel: crate::control::darkel::Darkel,
    pub phones: crate::control::phones::PhoneInfo,
    pub onscreen_timer: crate::control::onscreen_timer::OnscreenTimer,
    /// The function-static counters of the AK47 and M16 muzzle effects (every second shot).
    pub gun_flash_counters: [u8; 2],
    pub projectiles: crate::weapons::projectile::Projectiles,
    pub bullets: crate::weapons::bullet::Bullets,
    pub shots: crate::weapons::shot::Shots,
    /// `AddGunshell`'s function-static previous shooter position.
    pub gunshell_prev_pos: Vec3,
    pub anim: crate::anim::AnimManager,
    pub paths: crate::control::pathfind::PathFind,
    /// `CPed::ms_vec2DFleePosition`.
    pub flee_position: glam::Vec2,
    /// `gaRoutes`.
    pub ped_routes: crate::ped::routes::PedRoutes,
    /// The enter and exit animation offsets (`vecPedCarDoorAnimOffset` and friends).
    pub ped_anim_offsets: crate::ped::doors::AnimOffsets,
    /// `gAccidentManager`.
    pub accidents: crate::control::accident::AccidentManager,
    /// `gFireManager`.
    pub fires: crate::control::fire::FireManager,
    /// `CWorld::Players[PlayerInFocus]`.
    pub player_info: crate::player_info::PlayerInfo,
    /// The function statics of `CPed::ProcessBuoyancy`: when the next raindrops and water circles are due.
    pub ped_splash_timers: [u32; 2],
    /// The function static `cancelJack` of `CPed::ProcessControl`.
    pub cancel_jack: bool,
}

impl Ents {
    fn game(&self) -> Game {
        self.models.game
    }

    pub(crate) fn col_model(&self, model_index: i16) -> Option<Arc<ColModel>> {
        self.models.get(model_index as i32)?.col_model.clone()
    }

    /// `CameraToIgnoreThisObject`.
    fn camera_ignores(&self, e: &Ent) -> bool {
        if self.garage_doors.contains(&e.base.model_index) {
            return false;
        }
        !matches!(&e.kind, Kind::Object(o) if o.camera_to_avoid)
    }

    /// Marks `r` as seen by the current query; false if an earlier list already saw it.
    fn first_visit(&mut self, r: EntityRef) -> bool {
        let code = self.scan_code;
        match self.pools.get_mut(r) {
            Some(e) if e.base.scan_code != code => {
                e.base.scan_code = code;
                true
            }
            _ => false,
        }
    }

    /// `CEntity::GetBoundRect`, or the bounding-circle square for physicals (`CPhysical::GetBoundRect`).
    fn bound_rect(&self, e: &Ent) -> Result<Rect, WorldError> {
        let col = self.col_model(e.base.model_index).ok_or(WorldError::NoCollision)?;
        if e.phys.is_some() {
            let (c, r) = (e.base.bound_centre(&col), e.base.bound_radius(&col));
            Ok(Rect::new(c.x - r, c.y - r, c.x + r, c.y + r))
        } else {
            Ok(e.base.bound_rect(&col))
        }
    }

    fn bound_radius(&self, e: &Ent) -> f32 {
        self.col_model(e.base.model_index).map_or(0.0, |c| e.base.bound_radius(&c))
    }

    /// Collision model a line of sight tests an entity with (peds use their hit model).
    fn los_col_model(&self, e: &Ent, dead_peds: bool) -> Option<Arc<ColModel>> {
        match &e.kind {
            Kind::Ped(p) => {
                if !(e.base.flags.uses_collision || dead_peds && p.dead()) {
                    return None;
                }
                // shortcut: Vice City animates a skinned col model here, the renderer is not ported
                if p.use_ground_col_model() && self.game() == Game::Gta3 {
                    return Some(Arc::new(self.temp_col.ped_ground_hit.clone()));
                }
                match &self.models.get(e.base.model_index as i32)?.kind {
                    crate::modelinfo::Kind::Ped(pm) => pm.hit_col_model.clone(),
                    _ => None,
                }
            }
            _ => e.base.flags.uses_collision.then(|| self.col_model(e.base.model_index)).flatten(),
        }
    }
}

#[derive(Debug)]
pub struct World {
    pub ents: Ents,
    pub grid: Grid,
    sectors: Vec<Sector>,
    big_buildings: Vec<VecDeque<EntityRef>>,
    moving: VecDeque<EntityRef>,
}

struct Cover {
    x: (i32, i32, i32),
    y: (i32, i32, i32),
}

impl World {
    pub fn new(models: ModelInfoStore, temp_col: TempColModels) -> Self {
        let game = models.game;
        let grid = Grid::for_game(game);
        let levels = match game {
            Game::Gta3 => 4,
            Game::Vc => 3,
        };
        Self {
            ents: Ents {
                pools: Pools::new(game),
                models,
                temp_col,
                scan_code: 0,
                ignore_entity: None,
                include_dead_peds: false,
                garage_doors: vec![],
                objects: ObjectTable::new(game),
                indices: Indices::new(game),
                timer: Timer::default(),
                no_more_collision_torque: false,
                second_shift: false,
                adhesive_limits: vec![vec![0.0; 6]; 6],
                events: vec![],
                pending_moving: vec![],
                handling: HandlingMgr::new(&re3_formats::handling::Handling::default()),
                player_col_model: ColModel::default(),
                player_vehicle_ex: None,
                water: Box::new(NoWater),
                buoyancy: Buoyancy::default(),
                splash_state: SplashState::default(),
                collision_in_memory: 0,
                all_taxi_lights: false,
                wet_roads: 0.0,
                force_process_control: false,
                wheel_statics: WheelStatics::default(),
                helis: Default::default(),
                planes: Default::default(),
                trains: Default::default(),
                cranes: Default::default(),
                cargens: Default::default(),
                explosions: Default::default(),
                pending_explosions: vec![],
                player_ped: None,
                ped_types: Default::default(),
                ped_stats: Default::default(),
                fight_moves: vec![Default::default(); crate::ped::data::fight_move::NUM],
                weapon_infos: Default::default(),
                gangs: crate::ped::types::Gangs::new(-1, [-1; 7]),
                population: Default::default(),
                car_ctrl: Default::default(),
                bridge: Default::default(),
                road_blocks: Default::default(),
                restart: Default::default(),
                zones: Default::default(),
                pickups: Default::default(),
                garages: Default::default(),
                scripts: Default::default(),
                script_checks: Default::default(),
                clock: Default::default(),
                darkel: Default::default(),
                phones: Default::default(),
                onscreen_timer: Default::default(),
                gun_flash_counters: [0; 2],
                projectiles: Default::default(),
                bullets: Default::default(),
                shots: Default::default(),
                gunshell_prev_pos: Vec3::ZERO,
                anim: crate::anim::AnimManager::new(game),
                paths: Default::default(),
                flee_position: glam::Vec2::ZERO,
                ped_routes: Default::default(),
                ped_anim_offsets: Default::default(),
                accidents: Default::default(),
                fires: Default::default(),
                player_info: Default::default(),
                ped_splash_timers: [0; 2],
                cancel_jack: false,
            },
            grid,
            sectors: vec![Sector::default(); (grid.num * grid.num) as usize],
            big_buildings: vec![VecDeque::new(); levels],
            moving: VecDeque::new(),
        }
    }

    pub fn sector(&self, x: i32, y: i32) -> Option<&Sector> {
        self.grid.contains(x, y).then(|| &self.sectors[(y * self.grid.num + x) as usize])
    }

    fn sector_mut(&mut self, x: i32, y: i32) -> &mut Sector {
        &mut self.sectors[(y * self.grid.num + x) as usize]
    }

    pub fn big_buildings(&self, level: usize) -> &VecDeque<EntityRef> {
        &self.big_buildings[level]
    }

    pub fn moving_entities(&self) -> &VecDeque<EntityRef> {
        &self.moving
    }

    pub fn get(&self, r: EntityRef) -> Option<&Ent> {
        self.ents.pools.get(r)
    }

    pub fn get_mut(&mut self, r: EntityRef) -> Option<&mut Ent> {
        self.ents.pools.get_mut(r)
    }

    /// Allocates the entity in its pool (the `new` of the original); it is not in the world until [`World::add`].
    pub fn insert(&mut self, ent: Ent) -> Option<EntityRef> {
        let pool = pool_kind(&ent.kind);
        // `CVehicle::CVehicle` counts the car
        let counted = match &ent.kind {
            Kind::Vehicle(v) => Some((v.created_by, v.flags.is_law_enforcer)),
            _ => None,
        };
        let handle = self.ents.pools.pool_mut(pool).insert(ent)?;
        if let Some((by, law)) = counted {
            self.ents.car_ctrl.update_car_count(by, law, false);
        }
        Some(EntityRef { pool, handle })
    }

    /// The `delete`: takes the entity out of the world if needed, then frees its slot.
    pub fn delete(&mut self, r: EntityRef) -> Option<Ent> {
        self.get(r)?;
        self.remove(r);
        // `CVehicle::~CVehicle` counts the car out
        if let Some(Kind::Vehicle(v)) = self.get(r).map(|e| &e.kind) {
            let (by, law, amb, fire) = (v.created_by, v.flags.is_law_enforcer, v.flags.is_ambulance_on_duty, v.flags.is_fire_truck_on_duty);
            let cc = &mut self.ents.car_ctrl;
            cc.update_car_count(by, law, true);
            cc.num_ambulances_on_duty -= amb as i32;
            cc.num_firetrucks_on_duty -= fire as i32;
        }
        // the original would leave a stale node behind for an entity that turned static
        self.moving.retain(|&m| m != r);
        self.ents.pools.pool_mut(r.pool).remove_handle(r.handle)
    }

    // ---- creating ----

    pub fn new_building(&mut self, model_index: i16, rng: &mut Rng) -> Option<EntityRef> {
        let mut base = Entity::new(EntityType::Building, rng);
        base.model_index = model_index;
        // `CBuilding::CBuilding`
        base.flags.uses_collision = true;
        self.insert(Ent::new(base, None, Kind::Building))
    }

    pub fn new_treadable(&mut self, model_index: i16, rng: &mut Rng) -> Option<EntityRef> {
        let mut base = Entity::new(EntityType::Building, rng);
        base.model_index = model_index;
        base.flags.uses_collision = true;
        self.insert(Ent::new(base, None, Kind::Treadable(TreadableData::default())))
    }

    pub fn new_dummy(&mut self, model_index: i16, rng: &mut Rng) -> Option<EntityRef> {
        let mut base = Entity::new(EntityType::Dummy, rng);
        base.model_index = model_index;
        self.insert(Ent::new(base, None, Kind::Dummy))
    }

    /// `CObject::CObject(mi, createRW)` including `Init`; place it with [`World::add`].
    /// When the pool is full a temp object is deleted to make room (`FIX_BUGS`).
    pub fn new_object(&mut self, model_index: i16, matrix: Matrix, rng: &mut Rng) -> Option<EntityRef> {
        let mut base = Entity::new(EntityType::Object, rng);
        base.model_index = model_index;
        base.matrix = matrix;
        let mut phys = Physical::default();
        let mut data = ObjectData::new();
        // CObject::Init
        if let Some(row) = self.ents.models.get(model_index as i32).and_then(|m| self.ents.objects.row(m.object_id)).copied() {
            phys.mass = row.mass;
            phys.turn_mass = row.turn_mass;
            phys.air_resistance = row.air_resistance;
            phys.elasticity = row.elasticity;
            phys.buoyancy = row.buoyancy;
            data.apply_row(&row);
            if row.mass >= 99998.0 {
                phys.flags.infinite_mass = true;
                phys.flags.affected_by_gravity = false;
                base.flags.explosion_proof = true;
            }
        }
        data.created_by = CreatedBy::Game;
        base.flags.is_static = true;
        let pos = base.position();
        let mut e = Ent::new(base, Some(phys), Kind::Object(data));
        let mut surface = None;
        if let Some(h) = self.process_vertical_line(pos, pos.z - 10.0, Kinds { buildings: true, ..Kinds::NONE }, false, None) {
            surface = Some(h.entity);
        }
        if let Kind::Object(o) = &mut e.kind {
            o.cur_surface = surface;
        }
        if self.ents.models.get(model_index as i32).is_some() && model_index == self.ents.indices.buoy() {
            e.base.flags.touching_water = true;
        }
        if self.ents.models.game == Game::Gta3 && model_index == self.ents.indices.body_cast() {
            self.ents.objects.body_cast_health = 1000;
        }
        if let Some(r) = self.insert(e.clone()) {
            return Some(r);
        }
        // pool full: free the first temp object and take its slot
        let victim = self.ents.pools.objects.iter().find(|(_, o)| matches!(&o.kind, Kind::Object(d) if d.created_by == CreatedBy::Temp)).map(|(i, _)| self.ents.pools.objects.handle_of(i))?;
        self.delete(EntityRef { pool: PoolKind::Object, handle: victim })?;
        self.insert(e)
    }

    /// `CObject::DeleteAllMissionObjects` / `DeleteAllTempObjects`.
    pub fn delete_objects_created_by(&mut self, by: CreatedBy) {
        self.delete_objects_where(|d, _| d.created_by == by);
    }

    /// `CObject::DeleteAllTempObjectsInArea`.
    pub fn delete_temp_objects_in_area(&mut self, point: Vec3, radius: f32) {
        self.delete_objects_where(|d, e| d.created_by == CreatedBy::Temp && magnitude_squared(point - e.base.position()) < radius * radius);
    }

    fn delete_objects_where(&mut self, f: impl Fn(&ObjectData, &Ent) -> bool) {
        let doomed: Vec<i32> = self.ents.pools.objects.iter().filter(|(_, e)| matches!(&e.kind, Kind::Object(d) if f(d, e))).map(|(i, _)| self.ents.pools.objects.handle_of(i)).collect();
        for handle in doomed {
            self.delete(EntityRef { pool: PoolKind::Object, handle });
        }
    }

    /// `CObject::Teleport`.
    pub fn teleport_object(&mut self, r: EntityRef, pos: Vec3) {
        self.remove(r);
        if let Some(e) = self.get_mut(r) {
            e.base.matrix.pos = pos;
        }
        let _ = self.add(r);
    }

    // ---- adding and removing ----

    fn cover(&self, b: Rect) -> Result<Cover, WorldError> {
        let g = &self.grid;
        let x = (g.index_x(b.left), g.index_x(b.right), g.index_x((b.left + b.right) / 2.0));
        let y = (g.index_y(b.top), g.index_y(b.bottom), g.index_y((b.top + b.bottom) / 2.0));
        if x.0 < 0 || x.1 >= g.num || y.0 < 0 || y.1 >= g.num {
            return Err(WorldError::OutOfWorld);
        }
        Ok(Cover { x, y })
    }

    /// Main-list index for the entity's type; the overlap list is one after.
    fn list_base(ty: EntityType) -> Result<usize, WorldError> {
        Ok(match ty {
            EntityType::Building => BUILDINGS,
            EntityType::Vehicle => VEHICLES,
            EntityType::Ped => PEDS,
            EntityType::Object => OBJECTS,
            EntityType::Dummy => DUMMIES,
            EntityType::Nothing => return Err(WorldError::BadEntity),
        })
    }

    /// The (sector, list) slots an entity occupies, in the order the original walks them.
    fn slots(&self, r: EntityRef) -> Result<Vec<EntryInfo>, WorldError> {
        let e = self.get(r).ok_or(WorldError::BadEntity)?;
        let base = Self::list_base(e.base.ty)?;
        let c = self.cover(self.ents.bound_rect(e)?)?;
        let mut out = vec![];
        for y in c.y.0..=c.y.1 {
            for x in c.x.0..=c.x.1 {
                out.push(EntryInfo { sector: (x, y), list: base + usize::from(!(x == c.x.2 && y == c.y.2)) });
            }
        }
        Ok(out)
    }

    /// `CEntity::Add` / `CPhysical::Add` / `CDummy::Add`: file the entity in the sectors its bounds touch.
    fn add_to_sectors(&mut self, r: EntityRef) -> Result<(), WorldError> {
        let slots = self.slots(r)?;
        for s in &slots {
            self.sector_mut(s.sector.0, s.sector.1).lists[s.list].push_front(r);
        }
        let e = self.get_mut(r).ok_or(WorldError::BadEntity)?;
        if !matches!(e.kind, Kind::Building | Kind::Treadable(_)) {
            // the entry list also inserts at the front
            e.entry_info = slots.into_iter().rev().collect();
        }
        Ok(())
    }

    fn remove_from_list(&mut self, s: EntryInfo, r: EntityRef) {
        let list = &mut self.sector_mut(s.sector.0, s.sector.1).lists[s.list];
        if let Some(i) = list.iter().position(|&m| m == r) {
            list.remove(i);
        }
    }

    fn remove_from_sectors(&mut self, r: EntityRef) {
        let Some(e) = self.get(r) else { return };
        let slots = if matches!(e.kind, Kind::Building | Kind::Treadable(_)) {
            // buildings keep no entry info and are found again by their (unchanging) bounds
            self.slots(r).unwrap_or_default()
        } else {
            e.entry_info.clone()
        };
        for s in slots {
            self.remove_from_list(s, r);
        }
        if let Some(e) = self.get_mut(r) {
            e.entry_info.clear();
        }
    }

    /// `CWorld::Add`.
    pub fn add(&mut self, r: EntityRef) -> Result<(), WorldError> {
        let e = self.get(r).ok_or(WorldError::BadEntity)?;
        let (big, level, kind_static) = (e.base.flags.is_big_building, e.base.level as usize, e.base.flags.is_static);
        let simple = matches!(e.kind, Kind::Building | Kind::Treadable(_) | Kind::Dummy);
        if big {
            self.big_buildings.get_mut(level).ok_or(WorldError::BadEntity)?.push_front(r);
        } else {
            self.add_to_sectors(r)?;
        }
        if !simple && !kind_static {
            self.moving.push_front(r);
            if let Some(p) = self.get_mut(r).and_then(|e| e.phys.as_mut()) {
                p.in_moving_list = true;
            }
        }
        Ok(())
    }

    /// `CWorld::Remove`.
    pub fn remove(&mut self, r: EntityRef) {
        let Some(e) = self.get(r) else { return };
        let (big, level, is_static) = (e.base.flags.is_big_building, e.base.level as usize, e.base.flags.is_static);
        let simple = matches!(e.kind, Kind::Building | Kind::Treadable(_) | Kind::Dummy);
        if big {
            if let Some(list) = self.big_buildings.get_mut(level)
                && let Some(i) = list.iter().position(|&m| m == r)
            {
                list.remove(i);
            }
        } else {
            self.remove_from_sectors(r);
        }
        if !simple && !is_static {
            self.remove_from_moving_list(r);
        }
    }

    fn remove_from_moving_list(&mut self, r: EntityRef) {
        let Some(p) = self.get_mut(r).and_then(|e| e.phys.as_mut()) else { return };
        if std::mem::take(&mut p.in_moving_list)
            && let Some(i) = self.moving.iter().position(|&m| m == r)
        {
            self.moving.remove(i);
        }
    }

    /// `CPhysical::RemoveAndAdd`: refile after moving, reusing the old nodes in order.
    pub fn remove_and_add(&mut self, r: EntityRef) -> Result<(), WorldError> {
        let new = self.slots(r)?;
        let old = std::mem::take(&mut self.get_mut(r).ok_or(WorldError::BadEntity)?.entry_info);
        let (mut kept, mut extra) = (vec![], vec![]);
        for (i, s) in new.into_iter().enumerate() {
            if let Some(&o) = old.get(i) {
                self.remove_from_list(o, r);
                kept.push(s);
            } else {
                extra.push(s);
            }
            self.sector_mut(s.sector.0, s.sector.1).lists[s.list].push_front(r);
        }
        // nodes beyond the new count are deleted
        for &o in old.iter().skip(kept.len()) {
            self.remove_from_list(o, r);
        }
        extra.reverse();
        extra.extend(kept);
        self.get_mut(r).ok_or(WorldError::BadEntity)?.entry_info = extra;
        Ok(())
    }

    // ---- scan codes ----

    /// `AdvanceCurrentScanCode`: on wrap the codes are cleared from the main lists.
    pub fn advance_scan_code(&mut self) {
        self.ents.scan_code = self.ents.scan_code.wrapping_add(1);
        if self.ents.scan_code == 0 {
            self.clear_scan_codes();
            self.ents.scan_code = 1;
        }
    }

    pub fn scan_code(&self) -> u16 {
        self.ents.scan_code
    }

    /// `ClearScanCodes`: only the main lists, as in the original.
    pub fn clear_scan_codes(&mut self) {
        for s in &self.sectors {
            for base in [BUILDINGS, VEHICLES, PEDS, OBJECTS, DUMMIES] {
                for &r in &s.lists[base] {
                    if let Some(e) = self.ents.pools.get_mut(r) {
                        e.base.scan_code = 0;
                    }
                }
            }
        }
    }

    // ---- sector walks ----

    /// Sectors a line crosses, in the order `ProcessLineOfSight` visits them.
    pub fn sectors_on_line(&self, p1: Vec3, p2: Vec3) -> Vec<(i32, i32)> {
        let g = &self.grid;
        let (xs, ys) = (g.index_x(p1.x), g.index_y(p1.y));
        let (xe, ye) = (g.index_x(p2.x), g.index_y(p2.y));
        let mut out = vec![];
        let column = |out: &mut Vec<(i32, i32)>, x: i32, y1: i32, y2: i32| {
            if y1 < y2 {
                out.extend((y1..=y2).map(|y| (x, y)));
            } else {
                out.extend((y2..=y1).rev().map(|y| (x, y)));
            }
        };
        if xs == xe {
            column(&mut out, xs, ys, ye);
        } else if ys == ye {
            if xs < xe {
                out.extend((xs..=xe).map(|x| (x, ys)));
            } else {
                out.extend((xe..=xs).rev().map(|x| (x, ys)));
            }
        } else {
            let m = (p2.y - p1.y) / (p2.x - p1.x);
            let y_at = |wx: f32| g.index_y((wx - p1.x) * m + p1.y);
            let mut y2;
            if p1.x < p2.x {
                y2 = y_at(g.world_x(xs + 1));
                column(&mut out, xs, ys, y2);
                for x in xs + 1..xe {
                    let y1 = y2;
                    y2 = y_at(g.world_x(x + 1));
                    column(&mut out, x, y1, y2);
                }
            } else {
                y2 = y_at(g.world_x(xs));
                column(&mut out, xs, ys, y2);
                for x in (xe + 1..xs).rev() {
                    let y1 = y2;
                    y2 = y_at(g.world_x(x));
                    column(&mut out, x, y1, y2);
                }
            }
            column(&mut out, xe, y2, ye);
        }
        out
    }

    /// The sector box a radius query covers; the upper end is clamped (`FIX_BUGS`).
    fn range_box(&self, centre: Vec3, radius: f32) -> ((i32, i32), (i32, i32)) {
        let g = &self.grid;
        (
            (g.index_x(centre.x - radius).max(0), g.index_y(centre.y - radius).max(0)),
            (g.index_x(centre.x + radius).min(g.num - 1), g.index_y(centre.y + radius).min(g.num - 1)),
        )
    }

    /// Calls `f(ents, entity, list base)` on the entities of the chosen kinds in each sector, in the
    /// original's order (main list then overlap list per kind), until it returns true.
    fn scan_sectors(&mut self, coords: impl IntoIterator<Item = (i32, i32)>, kinds: Kinds, mut f: impl FnMut(&mut Ents, EntityRef, usize) -> bool) {
        let Self { ents, grid, sectors, .. } = self;
        for (x, y) in coords {
            if !grid.contains(x, y) {
                continue;
            }
            let s = &sectors[(y * grid.num + x) as usize];
            for base in kinds.bases() {
                for list in [base, base + 1] {
                    if s.lists[list].iter().any(|&r| f(ents, r, base)) {
                        return;
                    }
                }
            }
        }
    }

    fn scan_box(&mut self, min: (i32, i32), max: (i32, i32), kinds: Kinds, f: impl FnMut(&mut Ents, EntityRef, usize) -> bool) {
        self.scan_sectors((min.1..=max.1).flat_map(move |y| (min.0..=max.0).map(move |x| (x, y))), kinds, f);
    }

    // ---- lines ----

    /// `ProcessLineOfSight`: the nearest hit along the line.
    pub fn process_line_of_sight(&mut self, p1: Vec3, p2: Vec3, kinds: Kinds, opts: LineOpts) -> Option<LineHit> {
        self.advance_scan_code();
        let line = Line { p0: p1, p1: p2 };
        let (mut dist, mut point, mut hit) = (1.0f32, ColPoint::default(), None);
        let path = self.sectors_on_line(p1, p2);
        self.scan_sectors(path, kinds, |ents, r, base| {
            let dead_peds = base == PEDS && ents.include_dead_peds;
            let Some(e) = ents.pools.get(r) else { return false };
            let skip = e.base.scan_code == ents.scan_code
                || Some(r) == ents.ignore_entity
                || !(e.base.flags.uses_collision || dead_peds)
                || (opts.ignore_some_objects && base == OBJECTS && ents.camera_ignores(e));
            if skip {
                return false;
            }
            let (col, matrix) = (ents.los_col_model(e, dead_peds), e.base.matrix);
            ents.first_visit(r);
            if let Some(col) = col
                && collision::process_line_of_sight(ents.game(), &line, &matrix, &col, &mut point, &mut dist, opts.filter())
            {
                hit = Some(r);
            }
            false
        });
        hit.map(|entity| LineHit { point, entity })
    }

    /// `ProcessVerticalLine`: straight down (or up) from `p` to height `z2`, in `p`'s sector only.
    pub fn process_vertical_line(&mut self, p: Vec3, z2: f32, kinds: Kinds, ignore_see_through: bool, mut poly: Option<&mut StoredCollPoly>) -> Option<LineHit> {
        self.advance_scan_code();
        let line = Line { p0: p, p1: Vec3::new(p.x, p.y, z2) };
        let sector = (self.grid.index_x(p.x).clamp(0, self.grid.num - 1), self.grid.index_y(p.y).clamp(0, self.grid.num - 1));
        let filter = LineFilter { ignore_see_through, ignore_shoot_through: false };
        let (mut dist, mut point, mut hit) = (1.0f32, ColPoint::default(), None);
        self.scan_sectors([sector], kinds, |ents, r, _| {
            let Some(e) = ents.pools.get(r) else { return false };
            if e.base.scan_code == ents.scan_code || !e.base.flags.uses_collision {
                return false;
            }
            let (col, matrix) = (ents.col_model(e.base.model_index), e.base.matrix);
            ents.first_visit(r);
            if let Some(col) = col
                && collision::process_vertical_line(ents.game(), &line, &matrix, &col, &mut point, &mut dist, filter, poly.as_deref_mut())
            {
                hit = Some(r);
            }
            false
        });
        hit.map(|entity| LineHit { point, entity })
    }

    /// `GetIsLineOfSightClear`.
    pub fn is_line_of_sight_clear(&mut self, p1: Vec3, p2: Vec3, kinds: Kinds, opts: LineOpts) -> bool {
        self.advance_scan_code();
        let line = Line { p0: p1, p1: p2 };
        let path = self.sectors_on_line(p1, p2);
        let mut blocked = false;
        self.scan_sectors(path, kinds, |ents, r, base| {
            let Some(e) = ents.pools.get(r) else { return false };
            if e.base.scan_code == ents.scan_code || !e.base.flags.uses_collision {
                return false;
            }
            let skip = Some(r) == ents.ignore_entity || (opts.ignore_some_objects && base == OBJECTS && ents.camera_ignores(e));
            let (col, matrix) = (ents.col_model(e.base.model_index), e.base.matrix);
            ents.first_visit(r);
            if !skip && let Some(col) = col {
                blocked = collision::test_line_of_sight(ents.game(), &line, &matrix, &col, opts.filter());
            }
            blocked
        });
        !blocked
    }

    /// The vehicles and objects a ped route walks around, in the sector box `CPedPath::CalcPedRoute` scans.
    pub fn route_blockades(&mut self, start: Vec3, end_x: f32, end_y: f32) -> Vec<crate::control::pathfind::Blockade> {
        let g = self.grid;
        let min = (g.index_x(start.x).max(0), g.index_y(start.y).max(0));
        let max = (g.index_x(end_x).min(g.num - 1), g.index_y(end_y).min(g.num - 1));
        self.advance_scan_code();
        let mut out = vec![];
        let kinds = Kinds { vehicles: true, objects: true, ..Kinds::NONE };
        self.scan_box(min, max, kinds, |ents, r, _| {
            let Some(e) = ents.pools.get(r) else { return false };
            if e.base.scan_code == ents.scan_code || !e.base.flags.uses_collision {
                return false;
            }
            let (m, model) = (e.base.matrix, e.base.model_index);
            ents.first_visit(r);
            if let Some(col) = ents.col_model(model) {
                out.push(crate::control::pathfind::Blockade {
                    pos: m.pos,
                    right: m.right,
                    forward: m.forward,
                    bb_min: col.bounding_box.min,
                    bb_max: col.bounding_box.max,
                    bound_centre: m.transform_point(col.bounding_sphere.center),
                    bound_radius: col.bounding_sphere.radius,
                });
            }
            false
        });
        out
    }

    /// `CPedPath::CalcPedRoute` with the blockades gathered from the world.
    pub fn calc_ped_route(&mut self, path_type: i8, position: Vec3, destination: Vec3, max_points: usize) -> Option<Vec<Vec3>> {
        let mid = (position + destination) * 0.5;
        let blockades = if path_type != crate::control::pathfind::ROUTE_NO_BLOCKADE {
            // the original's end y is mid.x + 28, kept
            self.route_blockades(Vec3::new(mid.x - 14.0, mid.y - 14.0, mid.z), mid.x + 28.0, mid.x + 28.0)
        } else {
            vec![]
        };
        crate::control::pathfind::calc_ped_route(path_type, position, destination, max_points, &blockades)
    }

    /// `GetBoundCentre`: the centre of the collision model's bounding sphere in the world.
    pub fn bound_centre(&self, r: EntityRef) -> Vec3 {
        let Some(e) = self.get(r) else { return Vec3::ZERO };
        self.ents.col_model(e.base.model_index).map_or(e.base.position(), |c| e.base.bound_centre(&c))
    }

    /// The hit collision model a ped's attacks are tested with (`GetHitColModel`, or the ground-hit model).
    pub fn ped_hit_col_model(&self, r: EntityRef, ground: bool) -> Option<Arc<ColModel>> {
        if ground {
            return Some(Arc::new(self.ents.temp_col.ped_ground_hit.clone()));
        }
        match &self.ents.models.get(self.get(r)?.base.model_index as i32)?.kind {
            crate::modelinfo::Kind::Ped(pm) => pm.hit_col_model.clone(),
            _ => None,
        }
    }

    /// The entity's collision model (`GetColModel`).
    pub fn col_model_of(&self, r: EntityRef) -> Option<Arc<ColModel>> {
        self.ents.col_model(self.get(r)?.base.model_index)
    }

    /// `GetBoundRadius`.
    pub fn bound_radius_of(&self, r: EntityRef) -> f32 {
        self.get(r).map_or(0.0, |e| self.ents.bound_radius(e))
    }

    /// `FindGroundZForCoord`.
    pub fn find_ground_z_for_coord(&mut self, x: f32, y: f32) -> f32 {
        let kinds = Kinds { buildings: true, dummies: true, ..Kinds::NONE };
        self.process_vertical_line(Vec3::new(x, y, 1000.0), -1000.0, kinds, false, None).map_or(20.0, |h| h.point.point.z)
    }

    /// `FindGroundZFor3DCoord`: the height and whether any ground was found.
    pub fn find_ground_z_for_3d_coord(&mut self, p: Vec3) -> (f32, bool) {
        let kinds = Kinds { buildings: true, ..Kinds::NONE };
        self.process_vertical_line(p, -1000.0, kinds, false, None).map_or((0.0, false), |h| (h.point.point.z, true))
    }

    /// `FindRoofZFor3DCoord`.
    pub fn find_roof_z_for_3d_coord(&mut self, p: Vec3) -> (f32, bool) {
        let kinds = Kinds { buildings: true, dummies: true, ..Kinds::NONE };
        self.process_vertical_line(p, 1000.0, kinds, false, None).map_or((20.0, false), |h| (h.point.point.z, true))
    }

    // ---- areas ----

    /// `TestSphereAgainstWorld`: the first entity a sphere touches.
    pub fn test_sphere_against_world(&mut self, centre: Vec3, radius: f32, ignore: Option<EntityRef>, kinds: Kinds, ignore_some_objects: bool) -> Option<EntityRef> {
        self.test_sphere_against_world_point(centre, radius, ignore, kinds, ignore_some_objects).map(|(r, _)| r)
    }

    /// `TestSphereAgainstWorld` plus the first contact point of the sphere (`gaTempSphereColPoints[0]`).
    pub fn test_sphere_against_world_point(&mut self, centre: Vec3, radius: f32, ignore: Option<EntityRef>, kinds: Kinds, ignore_some_objects: bool) -> Option<(EntityRef, Vec3)> {
        let (min, max) = self.range_box(centre, radius);
        self.advance_scan_code();
        let sphere = Sphere::new(radius, Vec3::ZERO, 0, 0);
        let our_col = ColModel {
            bounding_sphere: sphere,
            bounding_box: Box3 { min: Vec3::splat(-radius), max: Vec3::splat(radius), surface: 0, piece: 0 },
            spheres: vec![sphere],
            ..Default::default()
        };
        let sphere_mat = Matrix::from_translation(centre);
        let mut points = vec![ColPoint::default(); collision::MAX_SPHERES];
        let mut found = None;
        self.scan_box(min, max, kinds, |ents, r, base| {
            if !ents.first_visit(r) {
                return false;
            }
            let Some(e) = ents.pools.get(r) else { return false };
            if Some(r) == ignore || !e.base.flags.uses_collision || (ignore_some_objects && base == OBJECTS && ents.camera_ignores(e)) {
                return false;
            }
            let Some(col) = ents.col_model(e.base.model_index) else { return false };
            let distance = magnitude(centre - e.base.bound_centre(&col));
            if e.base.bound_radius(&col) + radius > distance {
                let hit = collision::process_col_models(&sphere_mat, &our_col, &e.base.matrix, &col, &mut points, &mut [], &mut []) != 0;
                // shortcut: the model's vehicle type stands in for the instance's `m_vehType`
                let dodo = if ents.game() == Game::Vc { indices_vc::mi::DODO } else { indices_gta3::mi::DODO };
                let car = matches!(&e.kind, Kind::Vehicle(_))
                    && ents.models.get(e.base.model_index as i32).and_then(|m| m.vehicle()).is_some_and(|v| v.vehicle_type == vehicle_type::CAR)
                    && e.base.model_index != dodo;
                if hit || (car && radius + col.bounding_box.max.x > distance) {
                    found = Some((r, points[0].point));
                    return true;
                }
            }
            false
        });
        found
    }

    /// `FindObjectsInRange`: entities whose position is within `radius`, at most `max`.
    pub fn find_objects_in_range(&mut self, centre: Vec3, radius: f32, ignore_z: bool, max: usize, kinds: Kinds) -> Vec<EntityRef> {
        let (min_s, max_s) = self.range_box(centre, radius);
        self.advance_scan_code();
        let mut out = vec![];
        self.scan_box(min_s, max_s, kinds, |ents, r, _| {
            if ents.first_visit(r) {
                let d = centre - ents.pools.get(r).map_or(Vec3::ZERO, |e| e.base.position());
                let sq = if ignore_z { d.x * d.x + d.y * d.y } else { magnitude_squared(d) };
                if sq < radius * radius && out.len() < max {
                    out.push(r);
                }
            }
            false
        });
        out
    }

    /// `FindObjectsOfTypeInRange`: like the above, for one model id.
    pub fn find_objects_of_type_in_range(&mut self, model: i16, pos: Vec3, radius: f32, check_2d_only: bool, max: usize, kinds: Kinds) -> Vec<EntityRef> {
        let (min_s, max_s) = self.range_box(pos, radius);
        self.advance_scan_code();
        let mut out = vec![];
        self.scan_box(min_s, max_s, kinds, |ents, r, _| {
            if ents.first_visit(r)
                && let Some(e) = ents.pools.get(r)
                && e.base.model_index == model
            {
                let d = pos - e.base.position();
                let sq = if check_2d_only { d.x * d.x + d.y * d.y } else { magnitude_squared(d) };
                if sq < radius * radius && out.len() < max {
                    out.push(r);
                }
            }
            false
        });
        out
    }

    /// `FindObjectsKindaColliding`: entities whose bounding sphere reaches `pos`.
    pub fn find_objects_kinda_colliding(&mut self, pos: Vec3, radius: f32, check_2d_only: bool, max: usize, kinds: Kinds) -> Vec<EntityRef> {
        let (min_s, max_s) = self.range_box(pos, radius);
        self.advance_scan_code();
        let mut out = vec![];
        self.scan_box(min_s, max_s, kinds, |ents, r, _| {
            if ents.first_visit(r)
                && let Some(e) = ents.pools.get(r)
            {
                let d = pos - e.base.position();
                let dist = if check_2d_only { (d.x * d.x + d.y * d.y).sqrt() } else { magnitude(d) };
                if ents.bound_radius(e) + radius > dist && out.len() < max {
                    out.push(r);
                }
            }
            false
        });
        out
    }

    /// The vehicles in the sectors covering a box, each once, in the order the original scans them.
    pub fn vehicles_in_sector_box(&mut self, min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Vec<EntityRef> {
        self.entities_in_sector_box(min_x, min_y, max_x, max_y, Kinds { vehicles: true, ..Kinds::NONE })
    }

    /// The entities of the chosen kinds in the sectors covering a box, each once, in the order the original scans them.
    pub fn entities_in_sector_box(&mut self, min_x: f32, min_y: f32, max_x: f32, max_y: f32, kinds: Kinds) -> Vec<EntityRef> {
        let g = self.grid;
        let min_s = (g.index_x(min_x).max(0), g.index_y(min_y).max(0));
        let max_s = (g.index_x(max_x).min(g.num - 1), g.index_y(max_y).min(g.num - 1));
        self.advance_scan_code();
        let mut out = vec![];
        self.scan_box(min_s, max_s, kinds, |ents, r, _| {
            if ents.first_visit(r) {
                out.push(r);
            }
            false
        });
        out
    }

    /// `FindObjectsIntersectingCube`.
    pub fn find_objects_intersecting_cube(&mut self, start: Vec3, end: Vec3, max: usize, kinds: Kinds) -> Vec<EntityRef> {
        let g = self.grid;
        let min_s = (g.index_x(start.x).max(0), g.index_y(start.y).max(0));
        let max_s = (g.index_x(end.x).min(g.num - 1), g.index_y(end.y).min(g.num - 1));
        self.advance_scan_code();
        let mut out = vec![];
        self.scan_box(min_s, max_s, kinds, |ents, r, _| {
            if ents.first_visit(r)
                && let Some(e) = ents.pools.get(r)
            {
                let (rad, p) = (ents.bound_radius(e), e.base.position());
                if rad + p.x >= start.x && p.x - rad <= end.x && rad + p.y >= start.y && p.y - rad <= end.y && rad + p.z >= start.z && p.z - rad <= end.z && out.len() < max {
                    out.push(r);
                }
            }
            false
        });
        out
    }
}
