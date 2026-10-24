//! `cHandlingDataMgr` (GTA III): the per-model handling table, converted to game units.
use super::transmission::Transmission;
use crate::entity::physical::GRAVITY;
use glam::Vec3;
use re3_formats::handling::Handling;

/// `HANDLING_*` flag bits.
pub mod flags {
    pub const BOOST_1G: u32 = 1;
    pub const BOOST_2G: u32 = 2;
    pub const REV_BONNET: u32 = 4;
    pub const HANGING_BOOT: u32 = 8;
    pub const NO_DOORS: u32 = 0x10;
    pub const IS_VAN: u32 = 0x20;
    pub const IS_BUS: u32 = 0x40;
    pub const IS_LOW: u32 = 0x80;
    pub const DBL_EXHAUST: u32 = 0x100;
    pub const TAILGATE_BOOT: u32 = 0x200;
    pub const NOSWING_BOOT: u32 = 0x400;
    pub const NONPLAYER_STABILISER: u32 = 0x800;
    pub const NEUTRALHANDLING: u32 = 0x1000;
    pub const HAS_NO_ROOF: u32 = 0x2000;
    pub const IS_BIG: u32 = 0x4000;
    pub const HALOGEN_LIGHTS: u32 = 0x8000;
}

/// The handling ids, in the order of `tVehicleType` (also the row order of the name table).
pub const VEHICLE_NAMES: &[&str] = &[
    "LANDSTAL", "IDAHO", "STINGER", "LINERUN", "PEREN", "SENTINEL", "PATRIOT", "FIRETRUK", "TRASH", "STRETCH", "MANANA", "INFERNUS", "BLISTA", "PONY",
    "MULE", "CHEETAH", "AMBULAN", "FBICAR", "MOONBEAM", "ESPERANT", "TAXI", "KURUMA", "BOBCAT", "MRWHOOP", "BFINJECT", "POLICE", "ENFORCER", "SECURICA",
    "BANSHEE", "PREDATOR", "BUS", "RHINO", "BARRACKS", "TRAIN", "HELI", "DODO", "COACH", "CABBIE", "STALLION", "RUMPO", "RCBANDIT", "BELLYUP", "MRWONGS",
    "MAFIA", "YARDIE", "YAKUZA", "DIABLOS", "COLUMB", "HOODS", "AIRTRAIN", "DEADDODO", "SPEEDER", "REEFER", "PANLANT", "FLATBED", "YANKEE", "BORGNINE",
];

pub const RCBANDIT: usize = 40;

#[derive(Debug, Clone, Copy, Default)]
pub struct HandlingData {
    pub identifier: usize,
    pub mass: f32,
    pub inv_mass: f32,
    pub turn_mass: f32,
    pub dimension: Vec3,
    pub centre_of_mass: Vec3,
    pub percent_submerged: i8,
    pub buoyancy: f32,
    pub traction_multiplier: f32,
    pub transmission: Transmission,
    pub brake_deceleration: f32,
    pub brake_bias: f32,
    pub abs: bool,
    pub steering_lock: f32,
    pub traction_loss: f32,
    pub traction_bias: f32,
    pub suspension_force_level: f32,
    pub suspension_damping_level: f32,
    pub suspension_upper_limit: f32,
    pub suspension_lower_limit: f32,
    pub suspension_bias: f32,
    pub collision_damage_multiplier: f32,
    pub flags: u32,
    pub seat_offset_distance: f32,
    pub monetary_value: i32,
    pub front_lights: i8,
    pub rear_lights: i8,
}

impl HandlingData {
    /// `ConvertDataToGameUnits`: per-second units to per-frame (50 fps) and the derived values.
    pub fn convert_to_game_units(&mut self) {
        let t = &mut self.transmission;
        t.engine_acceleration *= 1.0 / (50.0 * 50.0);
        t.max_velocity *= 1000.0 / (60.0 * 60.0 * 50.0);
        self.brake_deceleration *= 1.0 / (50.0 * 50.0);
        self.turn_mass = (self.dimension.x * self.dimension.x + self.dimension.y * self.dimension.y) * self.mass / 12.0;
        if self.turn_mass < 10.0 {
            self.turn_mass *= 5.0;
        }
        self.inv_mass = 1.0 / self.mass;
        self.buoyancy = 100.0 / self.percent_submerged as f32 * GRAVITY * self.mass;

        // The top speed is where the engine's push meets the air drag.
        let (mut a, mut b) = (0.0f32, 100.0f32);
        let mut velocity = t.max_velocity;
        while a < b && velocity > 0.0 {
            velocity -= 0.01;
            a = t.engine_acceleration / 6.0;
            let drag = 0.5 * (velocity * velocity) * self.dimension.x * self.dimension.z / self.mass;
            b = -velocity * (1.0 / (drag + 1.0) - 1.0);
        }
        if self.identifier == RCBANDIT {
            t.max_cruise_velocity = t.max_velocity;
        } else {
            t.max_cruise_velocity = velocity;
            t.max_velocity = velocity * 1.2;
        }
        t.max_reverse_velocity = -0.2;
        if t.drive_type == b'4' {
            t.engine_acceleration /= 4.0;
        } else {
            t.engine_acceleration /= 2.0;
        }
        t.init_gear_ratios();
    }
}

#[derive(Debug, Clone)]
pub struct HandlingMgr {
    pub wheel_friction: f32,
    data: Vec<HandlingData>,
}

impl HandlingMgr {
    /// `Initialise`/`LoadHandlingData` from the parsed `handling.cfg`. Rows naming no known
    /// vehicle are ignored (the original asserts).
    pub fn new(cfg: &Handling) -> Self {
        let mut data = vec![HandlingData::default(); VEHICLE_NAMES.len()];
        for c in &cfg.cars {
            let Some(id) = VEHICLE_NAMES.iter().position(|n| c.name == *n) else { continue };
            let v = |a: [f32; 3]| Vec3::from(a);
            let mut h = HandlingData {
                identifier: id,
                mass: c.mass,
                dimension: v(c.dimension),
                centre_of_mass: v(c.centre_of_mass),
                percent_submerged: c.percent_submerged as i8,
                traction_multiplier: c.traction_multiplier,
                traction_loss: c.traction_loss,
                traction_bias: c.traction_bias,
                brake_deceleration: c.brake_deceleration,
                brake_bias: c.brake_bias,
                abs: c.abs,
                steering_lock: c.steering_lock,
                suspension_force_level: c.suspension_force,
                suspension_damping_level: c.suspension_damping,
                seat_offset_distance: c.seat_offset,
                collision_damage_multiplier: c.collision_damage_multiplier,
                monetary_value: c.monetary_value,
                suspension_upper_limit: c.suspension_upper,
                suspension_lower_limit: c.suspension_lower,
                suspension_bias: c.suspension_bias,
                flags: c.flags,
                front_lights: c.front_lights as i8,
                rear_lights: c.rear_lights as i8,
                ..Default::default()
            };
            h.transmission = Transmission {
                num_gears: c.gears as i8,
                max_velocity: c.max_velocity,
                // the double product narrows once, like the original
                engine_acceleration: (c.engine_acceleration * 0.4) as f32,
                drive_type: c.drive_type as u8,
                engine_type: c.engine_type as u8,
                flags: c.flags as u8,
                ..Default::default()
            };
            h.convert_to_game_units();
            data[id] = h;
        }
        Self { wheel_friction: 0.9, data }
    }

    pub fn get(&self, id: usize) -> &HandlingData {
        &self.data[id]
    }

    /// `GetHandlingId`: the table size if the name is unknown.
    pub fn handling_id(name: &str) -> usize {
        VEHICLE_NAMES.iter().position(|n| name == *n).unwrap_or(VEHICLE_NAMES.len())
    }

    pub fn has_rear_wheel_drive(&self, id: usize) -> bool {
        self.data[id].transmission.drive_type != b'F'
    }

    pub fn has_front_wheel_drive(&self, id: usize) -> bool {
        self.data[id].transmission.drive_type != b'R'
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use re3_formats::datafile::Game;
    use re3_formats::handling::parse_handling;

    const CFG: &str = "LANDSTAL 1700.0 2.0 5.0 2.0 0.0 0.0 0.25 85 0.85 0.85 0.51 5 160.0 20.0 4 D 6.2 0.60 0 35.0 2.0 0.1 0.27 0.35 25000 0.25 -0.1 0.5 C00B 0 1\n;the end\n";

    #[test]
    fn rows_are_converted_to_frame_units() {
        let m = HandlingMgr::new(&parse_handling(CFG, Game::Gta3));
        let h = m.get(0);
        assert_eq!(h.identifier, 0);
        assert!((h.turn_mass - (4.0 + 25.0) * 1700.0 / 12.0).abs() < 1e-2);
        assert!((h.inv_mass - 1.0 / 1700.0).abs() < 1e-9);
        // acceleration: 20 * 0.4 per s^2 -> per frame^2, halved or quartered by drive type (4WD here)
        assert!((h.transmission.engine_acceleration - 20.0 * 0.4 / 2500.0 / 4.0).abs() < 1e-7);
        let t = &h.transmission;
        assert!(t.max_cruise_velocity > 0.0 && (t.max_velocity - t.max_cruise_velocity * 1.2).abs() < 1e-6);
        // drag-limited, so below the nominal 160 km/h
        assert!(t.max_cruise_velocity < 160.0 * 1000.0 / 3600.0 / 50.0);
        assert_eq!(t.gears[5].shift_up_velocity, t.max_velocity);
        assert!(m.has_front_wheel_drive(0) && m.has_rear_wheel_drive(0));
        assert_eq!(HandlingMgr::handling_id("STINGER"), 2);
        assert_eq!(HandlingMgr::handling_id("nothing"), VEHICLE_NAMES.len());
    }
}
