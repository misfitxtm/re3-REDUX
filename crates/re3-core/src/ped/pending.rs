//! Argument records shared by the ped routines.
use crate::entity::EntityRef;

/// The arguments of `SetInvestigateEvent`.
#[derive(Debug, Clone, Copy)]
pub struct Investigate {
    pub event: u32,
    pub pos: glam::Vec2,
    pub distance_to_count_done: f32,
    pub time: i32,
    pub angle: f32,
}

/// The arguments of `InflictDamage` after the victim.
#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub by: Option<EntityRef>,
    pub weapon: usize,
    pub damage: f32,
    pub piece: u8,
    pub direction: u8,
}
