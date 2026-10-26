//! Ped functions that are called but not ported yet. Each panics when reached; this file must be empty before phase 5 ends.
use super::ctx::Px;
use crate::entity::EntityRef;
use crate::world::World;

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

impl World {
    /// Weapon.cpp `CWeapon::Fire` for the ped's current weapon; true if it fired.
    pub fn fire_weapon(&mut self, _px: &mut Px, _r: EntityRef, _fire_pos: glam::Vec3) -> bool {
        unimplemented!("CWeapon::Fire")
    }

    /// PedFight.cpp `CPed::IsPedDoingDriveByShooting`.
    pub fn is_ped_doing_drive_by_shooting(&self, _r: EntityRef) -> bool {
        unimplemented!("IsPedDoingDriveByShooting")
    }

}
