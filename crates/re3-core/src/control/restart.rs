//! `Restart.cpp`: where the player wakes up after being wasted or busted.
use super::pathfind::PATH_PED;
use crate::ped::population::level;
use crate::world::World;
use glam::Vec3;

pub const NUM_RESTART_POINTS: usize = 8;

/// `CRestart`'s statics.
#[derive(Debug, Clone, PartialEq)]
pub struct Restart {
    pub override_hospital_level: i32,
    pub override_police_station_level: i32,
    pub fade_in_after_next_arrest: bool,
    pub fade_in_after_next_death: bool,
    pub override_restart: bool,
    pub override_position: Vec3,
    pub override_heading: f32,
    pub hospital_points: Vec<(Vec3, f32)>,
    pub police_points: Vec<(Vec3, f32)>,
}

impl Default for Restart {
    /// `CRestart::Initialise`.
    fn default() -> Self {
        Self {
            override_hospital_level: level::GENERIC,
            override_police_station_level: level::GENERIC,
            fade_in_after_next_arrest: true,
            fade_in_after_next_death: true,
            override_restart: false,
            override_position: Vec3::ZERO,
            override_heading: 0.0,
            hospital_points: vec![],
            police_points: vec![],
        }
    }
}

impl Restart {
    /// `AddHospitalRestartPoint`; points beyond the table's size are dropped (the original writes past it).
    pub fn add_hospital_restart_point(&mut self, pos: Vec3, heading: f32) {
        if self.hospital_points.len() < NUM_RESTART_POINTS {
            self.hospital_points.push((pos, heading));
        }
    }

    pub fn add_police_restart_point(&mut self, pos: Vec3, heading: f32) {
        if self.police_points.len() < NUM_RESTART_POINTS {
            self.police_points.push((pos, heading));
        }
    }

    /// `OverrideNextRestart`.
    pub fn override_next_restart(&mut self, pos: Vec3, heading: f32) {
        self.override_restart = true;
        self.override_position = pos;
        self.override_heading = heading;
    }

    /// `CancelOverrideRestart`.
    pub fn cancel_override_restart(&mut self) {
        self.override_restart = false;
    }
}

/// The closest of `points` to `pos` (squared distance at most 4000 m), only those on `level` when one is given.
fn closest(points: &[(Vec3, f32)], pos: Vec3, on_level: Option<&mut dyn FnMut(Vec3) -> bool>) -> Option<usize> {
    let mut min = 4000.0f32 * 4000.0;
    let mut found = None;
    let mut on_level = on_level;
    for (i, (p, _)) in points.iter().enumerate() {
        if let Some(f) = on_level.as_mut()
            && !f(*p)
        {
            continue;
        }
        let d = (pos - *p).length_squared();
        if min >= d {
            min = d;
            found = Some(i);
        }
    }
    found
}

impl World {
    /// `FindClosestHospitalRestartPoint` and its police twin: the override, a point on the level, any point, or a path node.
    fn find_restart_point(&mut self, pos: Vec3, police: bool) -> (Vec3, f32) {
        let r = &mut self.ents.restart;
        if r.override_restart {
            let out = (r.override_position, r.override_heading);
            r.cancel_override_restart();
            return out;
        }
        let (points, override_level) = if police { (r.police_points.clone(), r.override_police_station_level) } else { (r.hospital_points.clone(), r.override_hospital_level) };
        let cur_level = self.ents.zones.find_zone_for_point(pos);
        let wanted_level = if override_level != level::GENERIC { override_level } else { cur_level };
        let zones = &mut self.ents.zones;
        let mut on_level = |p: Vec3| zones.find_zone_for_point(p) == wanted_level;
        let pick = closest(&points, pos, Some(&mut on_level)).or_else(|| closest(&points, pos, None));
        match pick {
            Some(i) => points[i],
            None => {
                // no restart point at all: the nearest place a ped can stand
                let node = self.ents.paths.find_node_closest_to_coors(pos, PATH_PED, 999999.9, false, false);
                (if node >= 0 { self.ents.paths.nodes[node as usize].pos } else { pos }, 0.0)
            }
        }
    }

    pub fn find_closest_hospital_restart_point(&mut self, pos: Vec3) -> (Vec3, f32) {
        self.find_restart_point(pos, false)
    }

    pub fn find_closest_police_restart_point(&mut self, pos: Vec3) -> (Vec3, f32) {
        self.find_restart_point(pos, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zones::ZoneType;

    /// Everything west of x = 0 is level 1, the rest level 2.
    fn world() -> World {
        let mut w = World::new(crate::modelinfo::ModelInfoStore::new(re3_formats::datafile::Game::Gta3), crate::temp_col::TempColModels::new(re3_formats::datafile::Game::Gta3));
        let z = &mut w.ents.zones;
        z.create_zone("IND_ZON", ZoneType::Default, Vec3::new(-1000.0, -1000.0, -100.0), Vec3::new(-0.001, 1000.0, 100.0), 0);
        z.create_zone("COM_ZON", ZoneType::Default, Vec3::new(0.0, -1000.0, -100.0), Vec3::new(1000.0, 1000.0, 100.0), 0);
        w
    }

    #[test]
    fn the_closest_point_on_the_players_level_wins_over_a_nearer_one_elsewhere() {
        let mut w = world();
        w.ents.restart.add_hospital_restart_point(Vec3::new(-10.0, 0.0, 0.0), 1.0);
        w.ents.restart.add_hospital_restart_point(Vec3::new(500.0, 0.0, 0.0), 2.0);
        // the player is on level 2 at x = 5: the level 2 point is chosen even though the other is nearer
        assert_eq!(w.find_closest_hospital_restart_point(Vec3::new(5.0, 0.0, 0.0)), (Vec3::new(500.0, 0.0, 0.0), 2.0));
        // with the hospital level overridden to 1
        w.ents.restart.override_hospital_level = 1;
        assert_eq!(w.find_closest_hospital_restart_point(Vec3::new(5.0, 0.0, 0.0)), (Vec3::new(-10.0, 0.0, 0.0), 1.0));
    }

    #[test]
    fn with_no_point_on_the_level_any_point_will_do_and_an_override_is_used_once() {
        let mut w = world();
        w.ents.restart.add_police_restart_point(Vec3::new(-50.0, 0.0, 0.0), 3.0);
        assert_eq!(w.find_closest_police_restart_point(Vec3::new(20.0, 0.0, 0.0)), (Vec3::new(-50.0, 0.0, 0.0), 3.0));
        w.ents.restart.override_next_restart(Vec3::new(1.0, 2.0, 3.0), 0.5);
        assert_eq!(w.find_closest_police_restart_point(Vec3::ZERO), (Vec3::new(1.0, 2.0, 3.0), 0.5));
        assert!(!w.ents.restart.override_restart);
        assert_eq!(w.find_closest_police_restart_point(Vec3::ZERO).1, 3.0);
    }

    #[test]
    fn points_beyond_the_table_are_dropped() {
        let mut r = Restart::default();
        for i in 0..12 {
            r.add_hospital_restart_point(Vec3::splat(i as f32), 0.0);
        }
        assert_eq!(r.hospital_points.len(), NUM_RESTART_POINTS);
    }
}
