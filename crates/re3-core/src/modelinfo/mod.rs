//! The model registry: every object, vehicle, ped and weapon the game knows by id
//! (`ModelInfo.cpp`, `BaseModelInfo`, `SimpleModelInfo`, `TimeModelInfo`, `TxdStore`).
//! Rendering resources (atomics, clumps, materials) are not stored here; they arrive with the renderer.
use crate::collision::ColModel;
use glam::Vec3;
use re3_formats::datafile::{Effect, Game, Ide, Obj};
use std::sync::Arc;

pub mod indices_gta3;
pub mod indices_vc;
pub mod names_gta3;
pub mod names_vc;

/// Vehicle kinds as stored in vehicle model info (`eVehicleType`).
pub mod vehicle_type {
    pub const CAR: i32 = 0;
    pub const BOAT: i32 = 1;
    pub const TRAIN: i32 = 2;
    pub const HELI: i32 = 3;
    pub const PLANE: i32 = 4;
    pub const BIKE: i32 = 5;
}

pub const NUM_VEHICLE_POSITIONS: usize = 10;
pub const NUM_VEHICLE_COLOURS: usize = 8;

/// Table sizes of the original (PC build).
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub models: usize,
    pub txd_slots: usize,
    pub simple: usize,
    pub time: usize,
    pub clump: usize,
    pub ped: usize,
    pub vehicle: usize,
    pub weapon: usize,
}

impl Limits {
    pub fn for_game(game: Game) -> Self {
        match game {
            Game::Gta3 => Self { models: 5500, txd_slots: 1024, simple: 5000, time: 30, clump: 5, ped: 90, vehicle: 120, weapon: 0 },
            Game::Vc => Self { models: 6500, txd_slots: 1385, simple: 3885, time: 385, clump: 5, ped: 130, vehicle: 110, weapon: 37 },
        }
    }
}

/// Behaviour flags of simple (map) models, set from the IDE flags column.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SimpleFlags {
    /// GTA III: `normalCull`; Vice City: `wetRoadReflection` (same bit).
    pub normal_cull_or_wet_reflection: bool,
    pub no_fade: bool,
    pub draw_last: bool,
    pub additive: bool,
    pub is_subway: bool,
    pub ignore_light: bool,
    pub no_zwrite: bool,
    /// Vice City only.
    pub no_shadows: bool,
    pub ignore_draw_dist: bool,
    pub is_code_glass: bool,
    pub is_artist_glass: bool,
    /// GTA III extras (`EXTRA_MODEL_FLAGS`).
    pub is_tree: bool,
    pub is_double_sided: bool,
    pub can_be_ignored: bool,
}

impl SimpleFlags {
    /// `SetModelInfoFlags`.
    pub fn from_ide(game: Game, f: u32) -> Self {
        let b = |m: u32| f & m != 0;
        let mut s = Self {
            normal_cull_or_wet_reflection: b(1),
            no_fade: b(2),
            draw_last: b(4 | 8),
            additive: b(8),
            is_subway: b(0x10),
            ignore_light: b(0x20),
            no_zwrite: b(0x40),
            ..Default::default()
        };
        if game == Game::Vc {
            s.no_shadows = b(0x80);
            s.ignore_draw_dist = b(0x100);
            s.is_code_glass = b(0x200);
            s.is_artist_glass = b(0x400);
        } else {
            s.is_tree = b(0x2000);
            s.is_double_sided = b(0x200000);
            s.can_be_ignored = b(0x10000);
        }
        s
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SimpleData {
    /// Render objects per LOD level, assigned by the renderer.
    pub atomics: [Option<u32>; 3],
    /// The related (LOD or non-LOD) model; the C++ overloads `atomics[2]` for this.
    pub related_model: Option<i16>,
    /// `lod_distances[2]` holds the near distance for LODs.
    pub lod_distances: [f32; 3],
    pub num_atomics: u8,
    pub alpha: u8,
    /// 0: no damage model; 1: atomics 1 and 2 are damage models; 2: atomic 2 is.
    pub first_damaged: u8,
    pub is_damaged: bool,
    pub is_big_building: bool,
    pub flags: SimpleFlags,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimeData {
    pub simple: SimpleData,
    pub time_on: i32,
    pub time_off: i32,
    pub other_time_model: i32,
}

/// Vice City weapon models.
#[derive(Debug, Clone, PartialEq)]
pub struct WeaponData {
    pub simple: SimpleData,
    pub anim_file: Option<String>,
    pub weapon_id: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleData {
    pub game_name: String,
    pub vehicle_type: i32,
    /// Wheel model (cars) or plane LOD id (planes); the C++ uses a union.
    pub wheel_id: i32,
    pub wheel_scale: f32,
    pub num_doors: i32,
    pub handling_id: i32,
    pub vehicle_class: i32,
    pub frequency: u32,
    pub level: i32,
    pub positions: [Vec3; NUM_VEHICLE_POSITIONS],
    pub comp_rules: u32,
    pub bike_steer_angle: f32,
    pub colours1: [u8; NUM_VEHICLE_COLOURS],
    pub colours2: [u8; NUM_VEHICLE_COLOURS],
    pub num_colours: u8,
    pub anim_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PedData {
    pub anim_group: u32,
    pub ped_type: i32,
    pub ped_stat_type: i32,
    pub cars_can_drive: u32,
    pub hit_col_model: Option<Arc<ColModel>>,
    /// Vice City only.
    pub anim_file: Option<String>,
    pub radio: (i32, i32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Simple(SimpleData),
    Time(TimeData),
    Weapon(WeaponData),
    Mlo { draw_dist: f32, first_instance: i32, last_instance: i32 },
    Clump,
    Vehicle(Box<VehicleData>),
    Ped(Box<PedData>),
    XtraComps,
}

impl Kind {
    /// `GetModelType()`; the numbering differs between the games.
    pub fn type_id(&self, game: Game) -> u8 {
        let vc = game == Game::Vc;
        match self {
            Kind::Simple(_) => 1,
            Kind::Mlo { .. } => 2,
            Kind::Time(_) => 3,
            Kind::Weapon(_) => 4,
            Kind::Clump => if vc { 5 } else { 4 },
            Kind::Vehicle(_) => if vc { 6 } else { 5 },
            Kind::Ped(_) => if vc { 7 } else { 6 },
            Kind::XtraComps => if vc { 8 } else { 7 },
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub col_model: Option<Arc<ColModel>>,
    pub owns_col_model: bool,
    pub effects: Vec<Effect>,
    pub object_id: i16,
    pub ref_count: u16,
    pub txd_slot: i16,
    pub kind: Kind,
}

impl ModelInfo {
    fn new(kind: Kind) -> Self {
        Self { name: String::new(), col_model: None, owns_col_model: false, effects: vec![], object_id: -1, ref_count: 0, txd_slot: -1, kind }
    }

    pub fn simple(&self) -> Option<&SimpleData> {
        match &self.kind {
            Kind::Simple(s) => Some(s),
            Kind::Time(t) => Some(&t.simple),
            Kind::Weapon(w) => Some(&w.simple),
            _ => None,
        }
    }

    pub fn simple_mut(&mut self) -> Option<&mut SimpleData> {
        match &mut self.kind {
            Kind::Simple(s) => Some(s),
            Kind::Time(t) => Some(&mut t.simple),
            Kind::Weapon(w) => Some(&mut w.simple),
            _ => None,
        }
    }

    /// Simple or time model: the map buildings (`IsSimple` in GTA III, `IsBuilding` in Vice City).
    pub fn is_building(&self) -> bool {
        matches!(self.kind, Kind::Simple(_) | Kind::Time(_))
    }

    pub fn is_simple(&self, game: Game) -> bool {
        self.is_building() || (game == Game::Vc && matches!(self.kind, Kind::Weapon(_)))
    }

    pub fn is_clump(&self, game: Game) -> bool {
        match &self.kind {
            Kind::Clump | Kind::Ped(_) | Kind::Vehicle(_) => true,
            Kind::Mlo { .. } | Kind::XtraComps => game == Game::Gta3,
            _ => false,
        }
    }

    pub fn vehicle(&self) -> Option<&VehicleData> {
        match &self.kind {
            Kind::Vehicle(v) => Some(v),
            _ => None,
        }
    }

    /// `GetLodDistance(i)`.
    pub fn lod_distance(&self, i: usize, lod_mult: f32) -> f32 {
        self.simple().map_or(0.0, |s| s.lod_distances[i] * lod_mult)
    }

    /// `GetNearDistance()`.
    pub fn near_distance(&self, lod_mult: f32) -> f32 {
        self.lod_distance(2, lod_mult)
    }

    /// `GetLargestLodDistance()`.
    pub fn largest_lod_distance(&self, lod_mult: f32) -> f32 {
        let Some(s) = self.simple() else { return 0.0 };
        let d = if s.first_damaged == 0 || s.is_damaged { s.lod_distances[s.num_atomics as usize - 1] } else { s.lod_distances[s.first_damaged as usize - 1] };
        d * lod_mult
    }

    /// `GetAtomicFromDistance(dist)`: the LOD level to draw at `dist`, if any.
    pub fn atomic_from_distance(&self, dist: f32, lod_mult: f32) -> Option<usize> {
        let s = self.simple()?;
        let start = if s.is_damaged { s.first_damaged as usize } else { 0 };
        (start..s.num_atomics as usize).find(|&i| dist < s.lod_distances[i] * lod_mult)
    }

    /// Vice City's `GetFirstAtomicFromDistance`.
    pub fn first_atomic_from_distance(&self, dist: f32, lod_mult: f32) -> Option<usize> {
        let s = self.simple()?;
        (dist < s.lod_distances[0] * lod_mult).then_some(0)
    }

    /// `IncreaseAlpha()`: fade in by 16 steps.
    pub fn increase_alpha(&mut self) {
        if let Some(s) = self.simple_mut() {
            s.alpha = if s.alpha >= 0xEF { 0xFF } else { s.alpha + 0x10 };
        }
    }
}

#[derive(Debug, Clone)]
struct TxdDef {
    name: String,
    ref_count: i32,
}

/// Texture dictionary slots shared by models (`CTxdStore`).
#[derive(Debug, Clone, Default)]
pub struct TxdStore {
    slots: Vec<Option<TxdDef>>,
}

impl TxdStore {
    pub fn add_slot(&mut self, name: &str) -> usize {
        let def = Some(TxdDef { name: name.to_owned(), ref_count: 0 });
        match self.slots.iter().position(Option::is_none) {
            Some(i) => {
                self.slots[i] = def;
                i
            }
            None => {
                self.slots.push(def);
                self.slots.len() - 1
            }
        }
    }

    pub fn find_slot(&self, name: &str) -> Option<usize> {
        self.slots.iter().position(|s| s.as_ref().is_some_and(|d| d.name.eq_ignore_ascii_case(name)))
    }

    pub fn remove_slot(&mut self, slot: usize) {
        self.slots[slot] = None;
    }

    pub fn name(&self, slot: usize) -> Option<&str> {
        self.slots.get(slot)?.as_ref().map(|d| d.name.as_str())
    }

    pub fn num_refs(&self, slot: usize) -> i32 {
        self.slots[slot].as_ref().map_or(0, |d| d.ref_count)
    }

    pub fn add_ref(&mut self, slot: usize) {
        if let Some(d) = self.slots[slot].as_mut() {
            d.ref_count += 1;
        }
    }

    /// Drops a reference; true when nothing uses the dictionary any more (the caller unloads it).
    pub fn remove_ref(&mut self, slot: usize) -> bool {
        match self.slots[slot].as_mut() {
            Some(d) => {
                d.ref_count -= 1;
                d.ref_count <= 0
            }
            None => false,
        }
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Name lookups the IDE loader needs from systems that live elsewhere (handling, peds, animations).
pub trait IdeLookups {
    fn handling_id(&self, name: &str) -> i32;
    fn ped_type(&self, name: &str) -> i32;
    fn ped_stat_type(&self, name: &str) -> i32;
    /// Animation group id for a ped's animation group name.
    fn anim_group(&self, name: &str) -> u32;
}

/// Lookups from the generated tables plus the loaded ped stats and animation groups.
pub struct TableLookups<'a> {
    pub game: Game,
    pub ped_stat_names: &'a [String],
}

impl IdeLookups for TableLookups<'_> {
    fn handling_id(&self, name: &str) -> i32 {
        let names: &[&str] = if self.game == Game::Vc { &names_vc::HANDLING_NAMES } else { &names_gta3::HANDLING_NAMES };
        // the original compares only the first 14 characters
        names.iter().position(|n| n.eq_ignore_ascii_case(&name[..name.len().min(14)]) && n.len() == name.len().min(14)).map_or(names.len() as i32, |i| i as i32)
    }

    fn ped_type(&self, name: &str) -> i32 {
        let (table, n): (&[(&str, i32)], i32) =
            if self.game == Game::Vc { (&names_vc::PED_TYPES, names_vc::NUM_PED_TYPES) } else { (&names_gta3::PED_TYPES, names_gta3::NUM_PED_TYPES) };
        table.iter().find(|(t, _)| *t == name).map_or(n, |&(_, i)| i)
    }

    fn ped_stat_type(&self, name: &str) -> i32 {
        self.ped_stat_names.iter().position(|n| n == name).map_or(self.ped_stat_names.len() as i32, |i| i as i32)
    }

    fn anim_group(&self, name: &str) -> u32 {
        let names: Vec<&str> = if self.game == Game::Vc {
            crate::anim::tables_vc::GROUPS.iter().map(|g| g.name).collect()
        } else {
            crate::anim::tables_gta3::GROUPS.iter().map(|g| g.name).collect()
        };
        names.iter().position(|n| *n == name).unwrap_or(names.len()) as u32
    }
}

/// Distance beyond which a model is a "big building" that needs a related LOD model.
pub const LOD_DISTANCE: f32 = 300.0;

#[derive(Debug)]
pub enum Error {
    /// A table of the original game is full (it would crash).
    Full(&'static str),
    BadId(i32),
}

#[derive(Debug)]
pub struct ModelInfoStore {
    pub game: Game,
    pub limits: Limits,
    models: Vec<Option<ModelInfo>>,
    pub txd: TxdStore,
    counts: [usize; 6],
}

const SIMPLE: usize = 0;
const TIME: usize = 1;
const CLUMP: usize = 2;
const PED: usize = 3;
const VEHICLE: usize = 4;
const WEAPON: usize = 5;

impl ModelInfoStore {
    pub fn new(game: Game) -> Self {
        let limits = Limits::for_game(game);
        Self { game, limits, models: vec![None; limits.models], txd: TxdStore::default(), counts: [0; 6] }
    }

    pub fn get(&self, id: i32) -> Option<&ModelInfo> {
        self.models.get(usize::try_from(id).ok()?)?.as_ref()
    }

    pub fn get_mut(&mut self, id: i32) -> Option<&mut ModelInfo> {
        self.models.get_mut(usize::try_from(id).ok()?)?.as_mut()
    }

    /// `GetModelInfo(name, &id)`: first model with that name (ASCII case-insensitive).
    pub fn find_by_name(&self, name: &str) -> Option<(i32, &ModelInfo)> {
        self.models.iter().enumerate().find_map(|(i, m)| m.as_ref().filter(|m| m.name.eq_ignore_ascii_case(name)).map(|m| (i as i32, m)))
    }

    /// Vice City's `GetModelInfo(name, minIndex, maxIndex)`.
    pub fn find_by_name_in(&self, name: &str, min: i32, max: i32) -> Option<&ModelInfo> {
        (min..=max).filter_map(|i| self.get(i)).find(|m| m.name.eq_ignore_ascii_case(name))
    }

    fn slot(&mut self, id: i32, kind: usize, cap: usize, what: &'static str) -> Result<usize, Error> {
        let idx = usize::try_from(id).ok().filter(|&i| i < self.limits.models).ok_or(Error::BadId(id))?;
        if self.counts[kind] >= cap {
            return Err(Error::Full(what));
        }
        self.counts[kind] += 1;
        Ok(idx)
    }

    fn add(&mut self, id: i32, kind: usize, cap: usize, what: &'static str, k: Kind) -> Result<&mut ModelInfo, Error> {
        let idx = self.slot(id, kind, cap, what)?;
        self.models[idx] = Some(ModelInfo::new(k));
        Ok(self.models[idx].as_mut().unwrap())
    }

    pub fn add_simple_model(&mut self, id: i32) -> Result<&mut ModelInfo, Error> {
        let cap = self.limits.simple;
        self.add(id, SIMPLE, cap, "simple models", Kind::Simple(SimpleData::default()))
    }

    pub fn add_time_model(&mut self, id: i32) -> Result<&mut ModelInfo, Error> {
        let cap = self.limits.time;
        self.add(id, TIME, cap, "time models", Kind::Time(TimeData { simple: SimpleData::default(), time_on: 0, time_off: 0, other_time_model: -1 }))
    }

    /// Vice City only.
    pub fn add_weapon_model(&mut self, id: i32) -> Result<&mut ModelInfo, Error> {
        let cap = self.limits.weapon;
        self.add(id, WEAPON, cap, "weapon models", Kind::Weapon(WeaponData { simple: SimpleData::default(), anim_file: None, weapon_id: -1 }))
    }

    pub fn add_clump_model(&mut self, id: i32) -> Result<&mut ModelInfo, Error> {
        let cap = self.limits.clump;
        self.add(id, CLUMP, cap, "clump models", Kind::Clump)
    }

    pub fn add_ped_model(&mut self, id: i32) -> Result<&mut ModelInfo, Error> {
        let cap = self.limits.ped;
        self.add(id, PED, cap, "ped models", Kind::Ped(Box::default()))
    }

    pub fn add_vehicle_model(&mut self, id: i32) -> Result<&mut ModelInfo, Error> {
        let cap = self.limits.vehicle;
        let v = VehicleData {
            game_name: String::new(),
            vehicle_type: -1,
            wheel_id: -1,
            wheel_scale: 0.0,
            num_doors: 0,
            handling_id: 0,
            vehicle_class: 0,
            frequency: 0,
            level: 0,
            positions: [Vec3::ZERO; NUM_VEHICLE_POSITIONS],
            comp_rules: 0,
            bike_steer_angle: 999.99,
            colours1: [0; NUM_VEHICLE_COLOURS],
            colours2: [0; NUM_VEHICLE_COLOURS],
            num_colours: 0,
            anim_file: None,
        };
        self.add(id, VEHICLE, cap, "vehicle models", Kind::Vehicle(Box::new(v)))
    }

    /// `SetTexDictionary(name)`: attach the model to a (possibly new) texture slot.
    pub fn set_tex_dictionary(&mut self, id: i32, txd: &str) {
        let slot = self.txd.find_slot(txd).unwrap_or_else(|| self.txd.add_slot(txd));
        if let Some(m) = self.get_mut(id) {
            m.txd_slot = slot as i16;
        }
    }

    /// `SetModelName` (names are cut to the original's fixed buffer).
    pub fn set_name(&mut self, id: i32, name: &str) {
        let max = if self.game == Game::Vc { 21 } else { 24 };
        if let Some(m) = self.get_mut(id) {
            m.name = name.chars().take(max).collect();
        }
    }

    pub fn add_ref(&mut self, id: i32) {
        if let Some(m) = self.get_mut(id) {
            m.ref_count += 1;
            let slot = m.txd_slot;
            if slot >= 0 {
                self.txd.add_ref(slot as usize);
            }
        }
    }

    /// Returns true when the model's texture dictionary is no longer used.
    pub fn remove_ref(&mut self, id: i32) -> bool {
        let Some(m) = self.get_mut(id) else { return false };
        m.ref_count = m.ref_count.saturating_sub(1);
        let slot = m.txd_slot;
        slot >= 0 && self.txd.remove_ref(slot as usize)
    }

    pub fn is_boat_model(&self, id: i32) -> bool {
        self.get(id).and_then(ModelInfo::vehicle).is_some_and(|v| v.vehicle_type == vehicle_type::BOAT)
    }

    pub fn is_bike_model(&self, id: i32) -> bool {
        self.get(id).and_then(ModelInfo::vehicle).is_some_and(|v| v.vehicle_type == vehicle_type::BIKE)
    }

    /// `CModelInfo::Initialise`: the default car-part and body-part models.
    pub fn initialise(&mut self, temp: &crate::temp_col::TempColModels) {
        use indices_gta3::mi as m3;
        use indices_vc::mi as mv;
        let vc = self.game == Game::Vc;
        let parts: [(i16, &ColModel); 8] = if vc {
            [(mv::CAR_DOOR, &temp.door1), (mv::CAR_BUMPER, &temp.bumper1), (mv::CAR_PANEL, &temp.panel1), (mv::CAR_BONNET, &temp.bonnet1), (mv::CAR_BOOT, &temp.boot1), (mv::CAR_WHEEL, &temp.wheel1), (mv::BODYPARTA, &temp.body_part1), (mv::BODYPARTB, &temp.body_part2)]
        } else {
            [(m3::CAR_DOOR, &temp.door1), (m3::CAR_BUMPER, &temp.bumper1), (m3::CAR_PANEL, &temp.panel1), (m3::CAR_BONNET, &temp.bonnet1), (m3::CAR_BOOT, &temp.boot1), (m3::CAR_WHEEL, &temp.wheel1), (m3::BODYPARTA, &temp.body_part1), (m3::BODYPARTB, &temp.body_part2)]
        };
        for (id, col) in parts {
            let id = id as i32;
            let mi = self.add_simple_model(id).expect("default models fit in the tables");
            mi.col_model = Some(Arc::new(col.clone()));
            if let Some(s) = mi.simple_mut() {
                s.num_atomics = 1;
                s.lod_distances[0] = 80.0;
            }
            self.set_tex_dictionary(id, "generic");
        }
    }

    /// `SetupBigBuilding`: models drawn from afar get a related LOD model and a near distance.
    /// `range` is Vice City's id range of the IDE file just loaded; GTA III scans everything.
    pub fn setup_big_building(&mut self, id: i32, lod_mult: f32, range: Option<(i32, i32)>) {
        let Some(m) = self.get(id) else { return };
        let Some(s) = m.simple() else { return };
        if !(s.lod_distances[0] > LOD_DISTANCE * lod_mult && s.related_model.is_none()) {
            return;
        }
        let tail = |n: &str| n.chars().skip(3).collect::<String>();
        let key = tail(&m.name);
        let (lo, hi) = range.unwrap_or((0, self.limits.models as i32 - 1));
        let related = (lo..=hi).find(|&i| i != id && self.get(i).is_some_and(|o| tail(&o.name) == key));
        let near = match related {
            Some(r) => self.get(r).unwrap().largest_lod_distance(lod_mult) / lod_mult,
            None if self.game == Game::Vc => 0.0,
            None if m.name.len() >= 3 && m.name[..3].eq_ignore_ascii_case("lod") => 100.0,
            None => 0.0,
        };
        let vc = self.game == Game::Vc;
        let m = self.get_mut(id).unwrap();
        let s = m.simple_mut().unwrap();
        s.is_big_building = true;
        s.related_model = related.map(|r| r as i16);
        s.lod_distances[2] = near;
        if vc && related.is_some() {
            s.flags.draw_last = false;
        }
    }

    /// `FindOtherTimeModel`: pair a `_nt` model with its `_dy` twin (and back).
    pub fn find_other_time_model(&mut self, id: i32) -> Option<i32> {
        let name = self.get(id)?.name.clone();
        let other = if let Some(p) = name.find("_nt") {
            format!("{}_dy{}", &name[..p], &name[p + 3..])
        } else {
            let p = name.find("_dy")?;
            format!("{}_nt{}", &name[..p], &name[p + 3..])
        };
        let other_id = self.models.iter().position(|m| m.as_ref().is_some_and(|m| matches!(m.kind, Kind::Time(_)) && m.name == other))? as i32;
        if let Some(Kind::Time(t)) = self.get_mut(id).map(|m| &mut m.kind) {
            t.other_time_model = other_id;
        }
        Some(other_id)
    }

    /// Loads the model definitions of one IDE file (`CFileLoader::LoadObjectTypes`).
    /// Returns the ids of the objects it defined, so the caller can match model names.
    pub fn load_ide(&mut self, ide: &Ide, lookups: &dyn IdeLookups, lod_mult: f32) -> Result<Vec<i32>, Error> {
        let mut ids = vec![];
        let game = self.game;
        let define = |this: &mut Self, o: &Obj, timed: bool| -> Result<i32, Error> {
            let id = o.id;
            let mi = if timed { this.add_time_model(id)? } else { this.add_simple_model(id)? };
            let (on, off) = o.time.unwrap_or((0, 0));
            match &mut mi.kind {
                Kind::Time(t) => {
                    t.time_on = on;
                    t.time_off = off;
                    fill_simple(&mut t.simple, o, game);
                }
                Kind::Simple(s) => fill_simple(s, o, game),
                _ => unreachable!(),
            }
            this.set_name(id, &o.model);
            this.set_tex_dictionary(id, &o.txd);
            if timed
                && let Some(other) = this.find_other_time_model(id)
                    && let Some(Kind::Time(t)) = this.get_mut(other).map(|m| &mut m.kind) {
                        t.other_time_model = id;
                    }
            Ok(id)
        };
        for o in &ide.objs {
            ids.push(define(self, o, false)?);
        }
        for o in &ide.tobj {
            ids.push(define(self, o, true)?);
        }
        for w in &ide.weap {
            let mi = self.add_weapon_model(w.id)?;
            if let Kind::Weapon(wd) = &mut mi.kind {
                wd.simple.num_atomics = 1;
                wd.simple.lod_distances[0] = w.dist;
                wd.anim_file = Some(w.anim.clone());
            }
            self.set_name(w.id, &w.model);
            self.set_tex_dictionary(w.id, &w.txd);
        }
        for h in &ide.hier {
            self.add_clump_model(h.id)?;
            self.set_name(h.id, &h.model);
            self.set_tex_dictionary(h.id, &h.txd);
        }
        for c in &ide.cars {
            let handling = lookups.handling_id(&c.handling);
            let mi = self.add_vehicle_model(c.id)?;
            let Kind::Vehicle(v) = &mut mi.kind else { unreachable!() };
            v.game_name = c.game_name.clone();
            v.level = c.level;
            v.comp_rules = c.comp_rules;
            v.frequency = c.frequency;
            v.vehicle_class = vehicle_class(game, &c.class);
            v.anim_file = c.anim.clone();
            v.handling_id = handling;
            let misc = c.misc.unwrap_or(0);
            match c.kind.as_str() {
                "car" => {
                    v.wheel_id = misc;
                    v.wheel_scale = c.wheel_scale.unwrap_or(0.0);
                    v.vehicle_type = vehicle_type::CAR;
                }
                "boat" => v.vehicle_type = vehicle_type::BOAT,
                "train" => v.vehicle_type = vehicle_type::TRAIN,
                "heli" => v.vehicle_type = vehicle_type::HELI,
                "plane" => {
                    v.wheel_id = misc; // plane LOD id
                    v.wheel_scale = 1.0;
                    v.vehicle_type = vehicle_type::PLANE;
                }
                "bike" => {
                    v.bike_steer_angle = misc as f32;
                    v.wheel_scale = c.wheel_scale.unwrap_or(0.0);
                    v.vehicle_type = vehicle_type::BIKE;
                }
                other => panic!("unknown vehicle type {other:?}"), // the original asserts
            }
            self.set_name(c.id, &c.model);
            self.set_tex_dictionary(c.id, &c.txd);
        }
        for p in &ide.peds {
            let (ped_type, stat, group) = (lookups.ped_type(&p.ped_type), lookups.ped_stat_type(&p.stats), lookups.anim_group(&p.anim_group));
            let mi = self.add_ped_model(p.id)?;
            let Kind::Ped(d) = &mut mi.kind else { unreachable!() };
            d.ped_type = ped_type;
            d.ped_stat_type = stat;
            d.anim_group = group;
            d.cars_can_drive = p.cars_can_drive;
            if let Some((anim, r1, r2)) = &p.vc {
                d.anim_file = Some(anim.clone());
                d.radio = (*r1, *r2);
            }
            self.set_name(p.id, &p.model);
            self.set_tex_dictionary(p.id, &p.txd);
        }
        for e in &ide.fx2d {
            if let Some(m) = self.get_mut(e.model_id) {
                m.effects.push(e.clone());
            }
        }
        // after each file, buildings that are drawn from afar get their LOD partners
        let range = (self.game == Game::Vc).then(|| (ids.iter().copied().min().unwrap_or(i32::MAX), ids.iter().copied().max().unwrap_or(-1)));
        let candidates: Vec<i32> = if let Some((lo, hi)) = range { (lo..=hi).collect() } else { (0..self.limits.models as i32).collect() };
        for id in candidates {
            if self.get(id).is_some_and(|m| m.is_building() && (game == Game::Vc || m.is_simple(game))) {
                self.setup_big_building(id, lod_mult, range);
            }
        }
        Ok(ids)
    }
}

/// Vehicle class ids as `CCarCtrl` numbers them; unknown names keep the default 0 as in the original.
pub fn vehicle_class(game: Game, name: &str) -> i32 {
    let classes: &[&str] = if game == Game::Vc {
        &["normal", "poorfamily", "richfamily", "executive", "worker", "big", "taxi", "moped", "motorbike", "leisureboat", "workerboat"]
    } else {
        &["poorfamily", "richfamily", "executive", "worker", "special", "big", "taxi"]
    };
    classes.iter().position(|c| *c == name).map_or(-1, |i| i as i32)
}

fn fill_simple(s: &mut SimpleData, o: &Obj, game: Game) {
    s.num_atomics = o.dists.len() as u8;
    for (i, d) in o.dists.iter().enumerate() {
        s.lod_distances[i] = *d;
    }
    s.flags = SimpleFlags::from_ide(game, o.flags);
    s.first_damaged = o.first_damaged;
}

#[cfg(test)]
mod tests;
