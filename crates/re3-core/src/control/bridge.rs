//! `Bridge.cpp`: the lift bridge of Callahan, which goes up and down once the commercial island is open.
use crate::entity::{EntityRef, PoolKind};
use crate::world::World;

/// `bridgeStates`.
pub mod bridge_state {
    pub const LOCKED: i32 = 0;
    pub const LIFT_PART_IS_UP: i32 = 1;
    pub const LIFT_PART_MOVING_DOWN: i32 = 2;
    pub const LIFT_PART_IS_DOWN: i32 = 3;
    pub const LIFT_PART_ABOUT_TO_MOVE_UP: i32 = 4;
    pub const LIFT_PART_MOVING_UP: i32 = 5;
}

/// The area the bridge lights cover on the road links.
const LIGHTS_AREA: (f32, f32, f32, f32) = (-330.0, -230.0, -700.0, -588.0);

/// `CBridge`'s statics.
#[derive(Debug, Clone, Copy)]
pub struct Bridge {
    pub lift_road: Option<EntityRef>,
    pub lift_part: Option<EntityRef>,
    pub weight: Option<EntityRef>,
    pub state: i32,
    pub old_state: i32,
    pub default_z_lift_part: f32,
    pub default_z_lift_road: f32,
    pub default_z_lift_weight: f32,
    pub old_lift: f32,
    pub time_of_bridge_becoming_operational: u32,
}

impl Default for Bridge {
    fn default() -> Self {
        Self {
            lift_road: None,
            lift_part: None,
            weight: None,
            state: bridge_state::LOCKED,
            old_state: bridge_state::LOCKED,
            default_z_lift_part: 0.0,
            default_z_lift_road: 0.0,
            default_z_lift_weight: 0.0,
            old_lift: -1.0,
            time_of_bridge_becoming_operational: 0,
        }
    }
}

/// The lift height and bridge state `elapsed` ms (modulo 65536, as the original's 16 bit counter) after the bridge opened.
pub fn lift_at(elapsed: u16) -> (f32, i32) {
    use bridge_state::*;
    let t = elapsed as f32;
    match elapsed {
        0..=9999 => (25.0 - t / 10000.0 * 25.0, LIFT_PART_MOVING_DOWN),
        10000..=39999 => (0.0, LIFT_PART_IS_DOWN),
        40000..=49999 => (0.0, LIFT_PART_ABOUT_TO_MOVE_UP),
        50000..=59999 => ((t - 50000.0) / 10000.0 * 25.0, LIFT_PART_MOVING_UP),
        _ => (25.0, LIFT_PART_IS_UP),
    }
}

impl World {
    /// `CBridge::FindBridgeEntities`.
    pub fn find_bridge_entities(&mut self) {
        let (lift, segment, weight) = match &self.ents.indices {
            crate::modelinfo::Indices::Gta3(i) => {
                use crate::modelinfo::indices_gta3::slot as s;
                (i.get(s::BRIDGELIFT), i.get(s::BRIDGEROADSEGMENT), i.get(s::BRIDGEWEIGHT))
            }
            _ => (-1, -1, -1),
        };
        let b = &mut self.ents.bridge;
        (b.weight, b.lift_road, b.lift_part) = (None, None, None);
        let pool = &self.ents.pools.buildings;
        // the original scans from the top slot down, so of several buildings of a model the lowest slot is kept
        let all: Vec<_> = pool.iter().collect();
        for (i, e) in all.into_iter().rev() {
            let r = EntityRef { pool: PoolKind::Building, handle: pool.handle_of(i) };
            let m = e.base.model_index;
            if m == lift {
                b.lift_part = Some(r);
            } else if m == segment {
                b.lift_road = Some(r);
            } else if m == weight {
                b.weight = Some(r);
            }
        }
    }

    /// `CBridge::Init`.
    pub fn init_bridge(&mut self) {
        self.find_bridge_entities();
        self.ents.bridge.old_lift = -1.0;
        let b = self.ents.bridge;
        if let (Some(part), Some(weight)) = (b.lift_part, b.weight) {
            let (zp, zw) = (self.epos(part).z, self.epos(weight).z);
            let zr = b.lift_road.map(|r| self.epos(r).z);
            let b = &mut self.ents.bridge;
            b.default_z_lift_part = zp;
            b.default_z_lift_weight = zw;
            if let Some(z) = zr {
                b.default_z_lift_road = z;
            }
            let (x1, x2, y1, y2) = LIGHTS_AREA;
            self.ents.paths.set_links_bridge_lights(x1, x2, y1, y2, true);
        }
    }

    /// `CBridge::Update`: `commercial_passed` is the story flag that opens the bridge.
    pub fn update_bridge(&mut self, commercial_passed: bool) {
        let now = self.now();
        let b = self.ents.bridge;
        let (Some(part), Some(weight)) = (b.lift_part, b.weight) else { return };
        self.ents.bridge.old_state = b.state;
        let lift_height;
        if commercial_passed {
            if b.time_of_bridge_becoming_operational == 0 {
                self.ents.bridge.time_of_bridge_becoming_operational = now;
            }
            // 16 bits, so after a little over a minute the cycle starts again
            let elapsed = now.wrapping_sub(self.ents.bridge.time_of_bridge_becoming_operational) as u16;
            let (h, s) = lift_at(elapsed);
            lift_height = h;
            self.ents.bridge.state = s;
        } else {
            lift_height = 25.0;
            self.ents.bridge.time_of_bridge_becoming_operational = 0;
            self.ents.bridge.state = bridge_state::LOCKED;
        }
        if lift_height != b.old_lift {
            let b = self.ents.bridge;
            self.get_mut(part).expect("lift").base.matrix.pos.z = b.default_z_lift_part + lift_height;
            if let Some(road) = b.lift_road {
                self.get_mut(road).expect("lift road").base.matrix.pos.z = b.default_z_lift_road + lift_height;
            }
            self.get_mut(weight).expect("weight").base.matrix.pos.z = b.default_z_lift_weight - lift_height;
            self.ents.bridge.old_lift = lift_height;
        }
        let (x1, x2, y1, y2) = LIGHTS_AREA;
        let b = self.ents.bridge;
        if b.state == bridge_state::LIFT_PART_ABOUT_TO_MOVE_UP && b.old_state == bridge_state::LIFT_PART_IS_DOWN {
            self.ents.paths.set_links_bridge_lights(x1, x2, y1, y2, true);
        } else if b.state == bridge_state::LIFT_PART_IS_DOWN && b.old_state == bridge_state::LIFT_PART_MOVING_DOWN {
            self.ents.paths.set_links_bridge_lights(x1, x2, y1, y2, false);
        }
    }

    /// `CBridge::ShouldLightsBeFlashing`.
    pub fn should_bridge_lights_be_flashing(&self) -> bool {
        self.ents.bridge.state != bridge_state::LIFT_PART_IS_DOWN
    }

    /// `CBridge::ThisIsABridgeObjectMovingUp`.
    pub fn this_is_a_bridge_object_moving_up(&self, model: i16) -> bool {
        if model != self.ents.indices.bridge_road_segment() && model != self.ents.indices.bridge_lift() {
            return false;
        }
        matches!(self.ents.bridge.state, bridge_state::LIFT_PART_ABOUT_TO_MOVE_UP | bridge_state::LIFT_PART_MOVING_UP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lift_goes_down_waits_and_comes_back_up() {
        assert_eq!(lift_at(0), (25.0, bridge_state::LIFT_PART_MOVING_DOWN));
        assert_eq!(lift_at(5000).0, 12.5);
        assert_eq!(lift_at(10000), (0.0, bridge_state::LIFT_PART_IS_DOWN));
        assert_eq!(lift_at(45000), (0.0, bridge_state::LIFT_PART_ABOUT_TO_MOVE_UP));
        assert_eq!(lift_at(55000), (12.5, bridge_state::LIFT_PART_MOVING_UP));
        assert_eq!(lift_at(62000), (25.0, bridge_state::LIFT_PART_IS_UP));
    }

    #[test]
    fn the_sixteen_bit_counter_restarts_the_cycle() {
        let start = 1_000u32;
        let elapsed = (start + 65_536 + 5_000).wrapping_sub(start) as u16;
        assert_eq!(lift_at(elapsed).1, bridge_state::LIFT_PART_MOVING_DOWN);
    }

    #[test]
    fn the_bridge_parts_move_with_the_cycle() {
        use crate::entity::physical::Physical;
        use crate::entity::{Ent, Entity, EntityType, Kind};
        let mut w = World::new(crate::modelinfo::ModelInfoStore::new(re3_formats::datafile::Game::Gta3), crate::temp_col::TempColModels::new(re3_formats::datafile::Game::Gta3));
        let mut rng = crate::general::Rng::default();
        let mk = |w: &mut World, rng: &mut crate::general::Rng, z: f32| {
            let mut e = Entity::new(EntityType::Building, rng);
            e.set_position(glam::Vec3::new(0.0, 0.0, z));
            w.insert(Ent::new(e, None::<Physical>, Kind::Building)).unwrap()
        };
        let (part, weight) = (mk(&mut w, &mut rng, 10.0), mk(&mut w, &mut rng, 50.0));
        w.ents.bridge.lift_part = Some(part);
        w.ents.bridge.weight = Some(weight);
        (w.ents.bridge.default_z_lift_part, w.ents.bridge.default_z_lift_weight) = (10.0, 50.0);
        w.ents.timer.time_in_ms = 100;
        w.update_bridge(false);
        // locked: the lift part stays up
        assert_eq!((w.epos(part).z, w.epos(weight).z, w.ents.bridge.state), (35.0, 25.0, bridge_state::LOCKED));
        w.update_bridge(true);
        assert_eq!(w.ents.bridge.state, bridge_state::LIFT_PART_MOVING_DOWN);
        w.ents.timer.time_in_ms = 100 + 20_000;
        w.update_bridge(true);
        assert_eq!((w.epos(part).z, w.epos(weight).z, w.ents.bridge.state), (10.0, 50.0, bridge_state::LIFT_PART_IS_DOWN));
        assert!(!w.should_bridge_lights_be_flashing());
        w.ents.timer.time_in_ms = 100 + 55_000;
        w.update_bridge(true);
        assert!(w.should_bridge_lights_be_flashing());
        assert!(!w.this_is_a_bridge_object_moving_up(5));
    }
}
