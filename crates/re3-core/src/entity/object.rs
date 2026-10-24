//! Objects (`CObject`, `CObjectData`): per-model physics data and collision damage.
//! Particles and sounds come back as events; the RNG is passed in.
use super::{Ent, EntityRef, Kind};
use crate::general::Rng;
use crate::math::Matrix;
use crate::modelinfo::ModelInfoStore;
use crate::entity::physical::GRAVITY;
use glam::Vec3;
use re3_formats::datafile::Game;
use re3_formats::world::ObjectInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CreatedBy {
    #[default]
    Unknown = 0,
    Game = 1,
    Mission = 2,
    Temp = 3,
    Cutscene = 4,
}

/// `CollisionDamageEffect`.
pub mod damage_effect {
    pub const NONE: u8 = 0;
    pub const CHANGE_MODEL: u8 = 1;
    pub const SPLIT_MODEL: u8 = 2;
    pub const SMASH_COMPLETELY: u8 = 3;
    pub const CHANGE_THEN_SMASH: u8 = 4;
    pub const SMASH_CARDBOARD_COMPLETELY: u8 = 50;
    pub const SMASH_WOODENBOX_COMPLETELY: u8 = 60;
    pub const SMASH_TRAFFICCONE_COMPLETELY: u8 = 70;
    pub const SMASH_BARPOST_COMPLETELY: u8 = 80;
}

/// The `CObject` fields on top of the physical state.
#[derive(Debug, Clone, Default)]
pub struct ObjectData {
    pub object_matrix: Matrix,
    pub uproot_limit: f32,
    pub created_by: CreatedBy,
    pub is_pickup: bool,
    pub pickup_with_message: bool,
    pub out_of_stock: bool,
    pub glass_cracked: bool,
    pub glass_broken: bool,
    pub has_been_damaged: bool,
    pub use_vehicle_colours: bool,
    pub bonus_value: i8,
    pub collision_damage_multiplier: f32,
    pub collision_damage_effect: u8,
    pub special_collision_response: u8,
    pub camera_to_avoid: bool,
    pub end_of_life_time: u32,
    /// Vehicle model whose colours a temp object borrows (`RefModelInfo`).
    pub ref_model_index: i16,
    pub cur_surface: Option<EntityRef>,
    pub colliding_entity: Option<EntityRef>,
    pub colour1: i8,
    pub colour2: i8,
}

impl ObjectData {
    /// `CObject::CObject()`.
    pub fn new() -> Self {
        Self { collision_damage_multiplier: 0.0, ref_model_index: -1, ..Default::default() }
    }
}

/// One `object.dat` row.
#[derive(Debug, Clone, Copy, Default)]
pub struct ObjectRow {
    pub mass: f32,
    pub turn_mass: f32,
    pub air_resistance: f32,
    pub elasticity: f32,
    pub buoyancy: f32,
    pub uproot_limit: f32,
    pub collision_damage_multiplier: f32,
    pub collision_damage_effect: u8,
    pub special_collision_response: u8,
    pub camera_to_avoid: bool,
}

/// `CObjectData::ms_aObjectInfo` plus the statics of `CObject`.
#[derive(Debug, Clone)]
pub struct ObjectTable {
    rows: Vec<ObjectRow>,
    /// `CObject::nBodyCastHealth`.
    pub body_cast_health: i16,
    /// The function-local `nFrameGen` of `ObjectDamage`.
    frame_gen: i8,
}

impl ObjectTable {
    /// Vice City starts with four built-in rows (static fences and the like) before `object.dat`.
    pub fn new(game: Game) -> Self {
        let mut rows = vec![];
        if game == Game::Vc {
            let base = ObjectRow { mass: 99999.0, turn_mass: 99999.0, air_resistance: 0.99, elasticity: 0.1, buoyancy: GRAVITY * 99999.0 * 2.0, collision_damage_multiplier: 1.0, ..Default::default() };
            rows.push(base);
            rows.push(ObjectRow { camera_to_avoid: true, ..base });
            rows.push(ObjectRow { special_collision_response: 4, ..base });
            rows.push(ObjectRow { special_collision_response: 4, camera_to_avoid: true, ..base });
        }
        Self { rows, body_cast_health: 1000, frame_gen: 0 }
    }

    /// `CObjectData::Initialise`: appends the rows and links each model to its row by name.
    pub fn load(&mut self, infos: &[ObjectInfo], models: &mut ModelInfoStore) {
        for i in infos {
            let id = self.rows.len();
            self.rows.push(ObjectRow {
                mass: i.mass,
                turn_mass: i.turn_mass,
                air_resistance: i.air_resistance,
                elasticity: i.elasticity,
                buoyancy: 100.0 / i.percent_submerged * GRAVITY * i.mass,
                uproot_limit: i.uproot_limit,
                collision_damage_multiplier: i.collision_damage_multiplier,
                collision_damage_effect: i.damage_effect as u8,
                special_collision_response: i.response_case as u8,
                camera_to_avoid: i.camera_avoid != 0,
            });
            // a name no model has is skipped and its row id is reused, like the original
            let found = models.find_by_name(&i.name).map(|(m, _)| m);
            match found {
                Some(m) => models.get_mut(m).unwrap().object_id = id as i16,
                None => {
                    self.rows.pop();
                }
            }
        }
    }

    pub fn row(&self, id: i16) -> Option<&ObjectRow> {
        self.rows.get(usize::try_from(id).ok()?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptSound {
    BoxDestroyed1,
    BoxDestroyed2,
    MetalCollision,
    TireCollision,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ObjectEvent {
    /// `PARTICLE_CAR_DEBRIS`.
    Debris { pos: Vec3, dir: Vec3, size: f32, colour: [u8; 4], rotation_speed: i32, frame: i32 },
    Sound { sound: ScriptSound, pos: Vec3 },
}

impl ObjectData {
    /// `CObjectData::SetObjectData`: copies the model's row into the object; returns the physics values.
    pub fn apply_row(&mut self, row: &ObjectRow) {
        self.uproot_limit = row.uproot_limit;
        self.collision_damage_multiplier = row.collision_damage_multiplier;
        self.collision_damage_effect = row.collision_damage_effect;
        self.special_collision_response = row.special_collision_response;
        self.camera_to_avoid = row.camera_to_avoid;
    }

    /// `CObject::CanBeDeleted`.
    pub fn can_be_deleted(&self) -> bool {
        !matches!(self.created_by, CreatedBy::Mission | CreatedBy::Cutscene)
    }
}

impl ObjectTable {
    /// `CObject::ObjectDamage` (GTA III behaviour). `amount` is the impulse of the last hit.
    /// Vice City's version has more effects (traffic lights, the beach ball) and is not ported.
    pub fn object_damage(&mut self, e: &mut Ent, amount: f32, body_cast_model: i16, rng: &mut Rng) -> Vec<ObjectEvent> {
        let Kind::Object(o) = &e.kind else { return vec![] };
        let (effect, multiplier) = (o.collision_damage_effect, o.collision_damage_multiplier);
        if effect == 0 || !e.base.flags.uses_collision {
            return vec![];
        }
        let mut amount = amount;
        let mut body_cast_effect = false;
        if e.base.model_index == body_cast_model {
            if amount > 50.0 {
                self.body_cast_health = (self.body_cast_health as f32 - 0.5 * amount) as i16;
            }
            self.body_cast_health = self.body_cast_health.max(0);
            body_cast_effect = self.body_cast_health < 200;
            amount = 0.0;
        }
        if !(amount * multiplier > 150.0 || body_cast_effect) {
            return vec![];
        }
        let pos = e.base.position();
        let dir_z = 0.0002 * amount;
        let mut out = vec![];
        let smash = |e: &mut Ent| {
            e.base.flags.is_visible = false;
            e.base.flags.uses_collision = false;
            e.base.flags.is_static = true;
            e.base.flags.explosion_proof = true;
            if let Some(p) = &mut e.phys {
                p.move_speed = Vec3::ZERO;
                p.turn_speed = Vec3::ZERO;
            }
        };
        // (count, sound, colour of piece n given a random factor) per smash effect
        let debris = |out: &mut Vec<ObjectEvent>, count: i32, pick: &dyn Fn(&mut Rng, i8) -> [u8; 4], frame_gen: &mut i8, rng: &mut Rng| {
            for _ in 0..count {
                // shortcut: the original builds this vector in one call whose argument order is unspecified
                let x = rng.range_f32(-0.35, 0.35);
                let y = rng.range_f32(-0.35, 0.35);
                let z = rng.range_f32(0.1, 0.25) + dir_z;
                *frame_gen = frame_gen.wrapping_add(1);
                let frame = (*frame_gen & 3) as i32;
                let colour = pick(rng, *frame_gen);
                let size = rng.range_f32(0.02, 0.20);
                let rotation_speed = rng.range_i32(-40, 40);
                out.push(ObjectEvent::Debris { pos, dir: Vec3::new(x, y, z), size, colour, rotation_speed, frame });
            }
        };
        use damage_effect::*;
        match effect {
            CHANGE_MODEL => e.base.flags.render_damaged = true,
            SMASH_COMPLETELY => smash(e),
            CHANGE_THEN_SMASH => {
                if !e.base.flags.render_damaged {
                    e.base.flags.render_damaged = true;
                } else {
                    smash(e);
                }
            }
            SMASH_CARDBOARD_COMPLETELY => {
                smash(e);
                let pick = |rng: &mut Rng, _| {
                    let f = rng.range_f32(0.01, 1.0);
                    [(96.0 * f) as u8, (48.0 * f) as u8, 0, 255]
                };
                debris(&mut out, 25, &pick, &mut self.frame_gen, rng);
                out.push(ObjectEvent::Sound { sound: ScriptSound::BoxDestroyed2, pos });
            }
            SMASH_WOODENBOX_COMPLETELY => {
                smash(e);
                let pick = |rng: &mut Rng, _| {
                    let f = rng.range_f32(0.5, 1.0);
                    let c = (128.0 * f) as u8;
                    [c, c, c, 255]
                };
                debris(&mut out, 45, &pick, &mut self.frame_gen, rng);
                out.push(ObjectEvent::Sound { sound: ScriptSound::BoxDestroyed1, pos });
            }
            SMASH_TRAFFICCONE_COMPLETELY | SMASH_BARPOST_COMPLETELY => {
                smash(e);
                let (count, sound) = if effect == SMASH_TRAFFICCONE_COMPLETELY { (10, ScriptSound::TireCollision) } else { (32, ScriptSound::MetalCollision) };
                // red on odd frames, white on even
                let pick = |_: &mut Rng, tick: i8| if tick & 1 != 0 { [200, 0, 0, 255] } else { [200, 200, 200, 255] };
                debris(&mut out, count, &pick, &mut self.frame_gen, rng);
                out.push(ObjectEvent::Sound { sound, pos });
            }
            _ => {}
        }
        out
    }
}
