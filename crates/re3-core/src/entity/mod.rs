//! The entity base (`CPlaceable` + `CEntity`) and the unified entity record the world stores.
use crate::collision::ColModel;
use crate::general::Rng;
use crate::math::{Matrix, Rect};
use glam::{Vec2, Vec3};

pub mod collide;
pub mod ent;
pub mod object;
pub mod physical;

pub use ent::{Ent, EntryInfo, Kind, ObjectData, PedState, TreadableData};

/// Which pool a handle belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PoolKind {
    Building,
    Treadable,
    Dummy,
    Object,
    Vehicle,
    Ped,
}

/// A handle to an entity in one of the world's pools (what the C++ holds as a `CEntity*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityRef {
    pub pool: PoolKind,
    pub handle: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum EntityType {
    #[default]
    Nothing = 0,
    Building = 1,
    Vehicle = 2,
    Ped = 3,
    Object = 4,
    Dummy = 5,
}

impl EntityType {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Self::Nothing,
            1 => Self::Building,
            2 => Self::Vehicle,
            3 => Self::Ped,
            4 => Self::Object,
            5 => Self::Dummy,
            _ => return None,
        })
    }
}

/// `eEntityStatus` values.
pub mod status {
    pub const PLAYER: u8 = 0;
    pub const PLAYER_PLAYBACKFROMBUFFER: u8 = 1;
    pub const SIMPLE: u8 = 2;
    pub const PHYSICS: u8 = 3;
    pub const ABANDONED: u8 = 4;
    pub const WRECKED: u8 = 5;
    pub const TRAIN_MOVING: u8 = 6;
    pub const TRAIN_NOT_MOVING: u8 = 7;
    pub const HELI: u8 = 8;
    pub const PLANE: u8 = 9;
    pub const PLAYER_REMOTE: u8 = 10;
    pub const PLAYER_DISABLED: u8 = 11;
}

/// All the `b…` booleans of `CEntity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityFlags {
    pub uses_collision: bool,
    pub collision_processed: bool,
    pub is_static: bool,
    pub has_contacted: bool,
    pub ped_physics: bool,
    pub is_stuck: bool,
    pub is_in_safe_position: bool,
    pub use_collision_records: bool,
    pub was_postponed: bool,
    pub explosion_proof: bool,
    pub is_visible: bool,
    pub has_collided: bool,
    pub render_scorched: bool,
    pub has_blip: bool,
    pub is_big_building: bool,
    pub render_damaged: bool,
    pub bullet_proof: bool,
    pub fire_proof: bool,
    pub collision_proof: bool,
    pub melee_proof: bool,
    pub only_damaged_by_player: bool,
    pub streaming_dont_delete: bool,
    pub zone_culled: bool,
    pub zone_culled2: bool,
    pub remove_from_world: bool,
    pub has_hit_wall: bool,
    pub im_being_rendered: bool,
    pub touching_water: bool,
    pub is_subway: bool,
    pub draw_last: bool,
    pub no_bright_headlights: bool,
    pub do_not_render: bool,
    pub distance_fade: bool,
    pub flag_e2: bool,
}

impl Default for EntityFlags {
    /// `CEntity::CEntity`: everything off except visibility.
    fn default() -> Self {
        let mut f = Self::unpack(0, 0);
        f.is_visible = true;
        f
    }
}

impl EntityFlags {
    /// The two words `SaveEntityFlags` writes (with type and status packed into the first by the caller).
    pub fn pack(&self) -> (u32, u32) {
        let a = [
            self.uses_collision, self.collision_processed, self.is_static, self.has_contacted, self.ped_physics, self.is_stuck,
            self.is_in_safe_position, self.use_collision_records, self.was_postponed, self.explosion_proof, self.is_visible, self.has_collided,
            self.render_scorched, self.has_blip, self.is_big_building, self.render_damaged, self.bullet_proof, self.fire_proof,
            self.collision_proof, self.melee_proof, self.only_damaged_by_player, self.streaming_dont_delete, self.zone_culled, self.zone_culled2,
        ];
        let b = [
            self.remove_from_world, self.has_hit_wall, self.im_being_rendered, self.touching_water, self.is_subway, self.draw_last,
            self.no_bright_headlights, self.do_not_render, self.distance_fade, self.flag_e2,
        ];
        let word = |bits: &[bool], shift: u32| bits.iter().enumerate().fold(0u32, |w, (i, &on)| w | ((on as u32) << (i as u32 + shift)));
        (word(&a, 8), word(&b, 0))
    }

    pub fn unpack(a: u32, b: u32) -> Self {
        let ba = |i: u32| a & (1 << (i + 8)) != 0;
        let bb = |i: u32| b & (1 << i) != 0;
        Self {
            uses_collision: ba(0), collision_processed: ba(1), is_static: ba(2), has_contacted: ba(3), ped_physics: ba(4), is_stuck: ba(5),
            is_in_safe_position: ba(6), use_collision_records: ba(7), was_postponed: ba(8), explosion_proof: ba(9), is_visible: ba(10),
            has_collided: ba(11), render_scorched: ba(12), has_blip: ba(13), is_big_building: ba(14), render_damaged: ba(15), bullet_proof: ba(16),
            fire_proof: ba(17), collision_proof: ba(18), melee_proof: ba(19), only_damaged_by_player: ba(20), streaming_dont_delete: ba(21),
            zone_culled: ba(22), zone_culled2: ba(23), remove_from_world: bb(0), has_hit_wall: bb(1), im_being_rendered: bb(2), touching_water: bb(3),
            is_subway: bb(4), draw_last: bb(5), no_bright_headlights: bb(6), do_not_render: bb(7), distance_fade: bb(8), flag_e2: bb(9),
        }
    }
}

/// `CEntity` (which includes the `CPlaceable` matrix).
#[derive(Debug, Clone)]
pub struct Entity {
    pub matrix: Matrix,
    pub ty: EntityType,
    pub status: u8,
    pub flags: EntityFlags,
    pub scan_code: u16,
    pub random_seed: u16,
    pub model_index: i16,
    /// Map region (`eLevelName`).
    pub level: u16,
    /// Render-engine object, assigned by the renderer.
    pub render_object: Option<u32>,
}

impl Entity {
    pub fn new(ty: EntityType, rng: &mut Rng) -> Self {
        Self {
            matrix: Matrix::from_scale(1.0),
            ty,
            status: status::ABANDONED,
            flags: EntityFlags::default(),
            scan_code: 0,
            random_seed: rng.number(),
            model_index: -1,
            level: 0,
            render_object: None,
        }
    }

    pub fn position(&self) -> Vec3 {
        self.matrix.pos
    }

    pub fn set_position(&mut self, p: Vec3) {
        self.matrix.pos = p;
    }

    /// `CPlaceable::SetHeading`: face `angle` radians about z, keeping the position.
    pub fn set_heading(&mut self, angle: f32) {
        let pos = self.matrix.pos;
        self.matrix = Matrix::rotate_z_only(angle);
        self.matrix.pos = pos;
    }

    /// `CPlaceable::SetOrientation`.
    pub fn set_orientation(&mut self, x: f32, y: f32, z: f32) {
        let pos = self.matrix.pos;
        self.matrix = Matrix::from_euler(x, y, z);
        self.matrix.pos = pos;
    }

    /// `IsWithinArea` (2D); the corners may be given in any order.
    pub fn is_within_area_2d(&self, x1: f32, y1: f32, x2: f32, y2: f32) -> bool {
        let (xa, xb, ya, yb) = (x1.min(x2), x1.max(x2), y1.min(y2), y1.max(y2));
        let p = self.matrix.pos;
        xa <= p.x && p.x <= xb && ya <= p.y && p.y <= yb
    }

    pub fn is_within_area(&self, a: Vec3, b: Vec3) -> bool {
        let (lo, hi) = (a.min(b), a.max(b));
        let p = self.matrix.pos;
        lo.x <= p.x && p.x <= hi.x && lo.y <= p.y && p.y <= hi.y && lo.z <= p.z && p.z <= hi.z
    }

    /// `CEntity::GetBoundRect`: the model's collision box corners projected to the ground plane.
    pub fn bound_rect(&self, col: &ColModel) -> Rect {
        let (min, max) = (col.bounding_box.min, col.bounding_box.max);
        let mut r = Rect::EMPTY;
        r.contain_point(self.matrix.transform_point(min));
        r.contain_point(self.matrix.transform_point(max));
        r.contain_point(self.matrix.transform_point(Vec3::new(max.x, min.y, min.z)));
        r.contain_point(self.matrix.transform_point(Vec3::new(min.x, max.y, max.z)));
        r
    }

    pub fn bound_centre(&self, col: &ColModel) -> Vec3 {
        self.matrix.transform_point(col.bounding_sphere.center)
    }

    pub fn bound_radius(&self, col: &ColModel) -> f32 {
        col.bounding_sphere.radius
    }

    pub fn distance_from_centre_of_mass_to_base_of_model(&self, col: &ColModel) -> f32 {
        -col.bounding_box.min.z
    }

    /// `GetIsTouching(centre, radius)`.
    pub fn is_touching(&self, col: &ColModel, centre: Vec3, radius: f32) -> bool {
        let r = self.bound_radius(col) + radius;
        r * r > crate::math::magnitude_squared(self.bound_centre(col) - centre)
    }

    /// 2D distance helper used all over the AI code.
    pub fn distance_2d(&self, other: Vec3) -> f32 {
        Vec2::new(self.matrix.pos.x - other.x, self.matrix.pos.y - other.y).length()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::{Box3, Sphere};

    #[test]
    fn flag_packing_matches_the_save_format() {
        // fresh entity: only the "visible" bit (bit 18 of the first word) is set
        let (a, b) = EntityFlags::default().pack();
        assert_eq!((a, b), (1 << 18, 0));
        let f = EntityFlags { uses_collision: true, zone_culled2: true, flag_e2: true, draw_last: true, ..Default::default() };
        let (a, b) = f.pack();
        assert_eq!(a, (1 << 8) | (1 << 18) | (1 << 31));
        assert_eq!(b, (1 << 5) | (1 << 9));
        assert_eq!(EntityFlags::unpack(a, b), f);
    }

    #[test]
    fn every_flag_round_trips() {
        for bit in 0..34u32 {
            let (a, b) = if bit < 24 { (1 << (bit + 8), 0) } else { (0, 1 << (bit - 24)) };
            let f = EntityFlags::unpack(a, b);
            assert_eq!(f.pack(), (a, b), "bit {bit}");
        }
    }

    fn col() -> ColModel {
        ColModel {
            bounding_sphere: Sphere::new(3.0, Vec3::new(0.0, 0.0, 1.0), 0, 0),
            bounding_box: Box3 { min: Vec3::new(-1.0, -2.0, -0.5), max: Vec3::new(1.0, 2.0, 1.5), surface: 0, piece: 0 },
            ..Default::default()
        }
    }

    #[test]
    fn bounds_follow_the_matrix() {
        let mut rng = Rng::default();
        let mut e = Entity::new(EntityType::Vehicle, &mut rng);
        assert_eq!(e.status, status::ABANDONED);
        assert!(e.flags.is_visible && !e.flags.uses_collision);
        e.set_position(Vec3::new(10.0, 20.0, 5.0));
        let r = e.bound_rect(&col());
        assert_eq!((r.left, r.right, r.top, r.bottom), (9.0, 11.0, 18.0, 22.0));
        // turned a quarter turn the box swaps its extents
        e.set_heading(std::f32::consts::FRAC_PI_2);
        let r = e.bound_rect(&col());
        assert!((r.left - 8.0).abs() < 1e-4 && (r.right - 12.0).abs() < 1e-4 && (r.top - 19.0).abs() < 1e-4 && (r.bottom - 21.0).abs() < 1e-4);
        assert_eq!(e.position(), Vec3::new(10.0, 20.0, 5.0));
        assert_eq!(e.bound_centre(&col()), Vec3::new(10.0, 20.0, 6.0));
        assert!(e.is_touching(&col(), Vec3::new(10.0, 20.0, 9.5), 1.0));
        assert!(!e.is_touching(&col(), Vec3::new(10.0, 20.0, 12.0), 1.0));
        assert_eq!(e.distance_from_centre_of_mass_to_base_of_model(&col()), 0.5);
    }

    #[test]
    fn areas_accept_corners_in_any_order() {
        let mut rng = Rng::default();
        let mut e = Entity::new(EntityType::Ped, &mut rng);
        e.set_position(Vec3::new(5.0, 5.0, 5.0));
        assert!(e.is_within_area_2d(10.0, 10.0, 0.0, 0.0) && e.is_within_area_2d(0.0, 0.0, 10.0, 10.0));
        assert!(!e.is_within_area_2d(6.0, 6.0, 9.0, 9.0));
        assert!(e.is_within_area(Vec3::splat(9.0), Vec3::splat(1.0)));
        assert!(!e.is_within_area(Vec3::new(0.0, 0.0, 6.0), Vec3::new(9.0, 9.0, 9.0)));
    }
}
