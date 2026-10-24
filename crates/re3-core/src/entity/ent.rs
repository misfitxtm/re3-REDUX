//! The record every pool stores: entity base, optional physics state and the kind-specific data.
use super::physical::Physical;
use super::{Entity, EntityRef};
use crate::vehicle::base::VehicleData;
pub use super::object::ObjectData;

/// Where an entity sits in a sector list (`CEntryInfoNode`): sector (x, y) and list index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryInfo {
    pub sector: (i32, i32),
    pub list: usize,
}

/// Placeholder until phase 5: the `CPed` state the world queries read.
#[derive(Debug, Clone, Copy, Default)]
pub struct PedState {
    pub dead: bool,
    pub is_player: bool,
    /// `m_pCurrentPhysSurface`: the physical the ped stands on.
    pub current_phys_surface: Option<EntityRef>,
    /// `m_pCollidingEntity`: something the ped is passing through without colliding.
    pub colliding_entity: Option<EntityRef>,
    pub knocked_up_into_air: bool,
    pub was_standing: bool,
    pub is_standing: bool,
    /// `UseGroundColModel()`: fall, dive-away, die or dead.
    pub use_ground_col_model: bool,
    /// `bInVehicle`.
    pub in_vehicle: bool,
}

/// Path nodes of a treadable (road) building: `[car, ped][12]`, -1 for none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreadableData {
    pub node_indices: [[i16; 12]; 2],
}

impl Default for TreadableData {
    fn default() -> Self {
        Self { node_indices: [[-1; 12]; 2] }
    }
}

#[derive(Debug, Clone)]
pub enum Kind {
    Building,
    Treadable(TreadableData),
    Dummy,
    Object(ObjectData),
    Vehicle(Box<VehicleData>),
    Ped(PedState),
}

#[derive(Debug, Clone)]
pub struct Ent {
    pub base: Entity,
    /// Present for objects, vehicles and peds.
    pub phys: Option<Physical>,
    pub kind: Kind,
    /// Sector lists holding this entity; kept for physicals and dummies (buildings are found by bounds).
    pub entry_info: Vec<EntryInfo>,
}

impl Ent {
    pub fn new(base: Entity, phys: Option<Physical>, kind: Kind) -> Self {
        Self { base, phys, kind, entry_info: vec![] }
    }
}
