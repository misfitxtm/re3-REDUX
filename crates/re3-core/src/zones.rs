//! `Zones.cpp`: the named areas of the map, nested in each other, with the traffic and pedestrian mix of each.
use crate::clock::Clock;
use crate::ped::population::{level, ZoneInfo};
use crate::ped::types::NUM_GANGS;
use glam::Vec3;

pub const NUMAUDIOZONES: usize = 36;
pub const NUMZONES: usize = 50;
pub const NUMMAPZONES: usize = 25;

/// `eZoneType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZoneType {
    #[default]
    Default,
    Navig,
    Info,
    MapZone,
}

/// `CZone`. Links are indices into the zone table.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Zone {
    pub name: String,
    pub min: Vec3,
    pub max: Vec3,
    pub ty: ZoneType,
    pub level: i32,
    pub zoneinfo_day: i16,
    pub zoneinfo_night: i16,
    pub child: Option<usize>,
    pub parent: Option<usize>,
    pub next: Option<usize>,
}

fn is_normal_zone(ty: ZoneType) -> bool {
    matches!(ty, ZoneType::Default | ZoneType::Navig | ZoneType::Info)
}

/// What `SetZoneCarInfo` changes; -1 leaves a number as it was.
#[derive(Debug, Clone, Copy)]
pub struct CarInfoChange {
    pub car_density: i16,
    pub gangs: [i16; NUM_GANGS],
    pub cops: i16,
    pub cars: [i16; 6],
}

/// `CheckZoneInfo`: the thresholds only ever grow.
fn check_zone_info(i: &ZoneInfo) {
    debug_assert!(i.car_threshold[0] >= 0);
    debug_assert!(i.car_threshold.windows(2).all(|w| w[0] <= w[1]));
    debug_assert!(i.car_threshold[5] <= i.cop_threshold && i.cop_threshold <= i.gang_threshold[0]);
    debug_assert!(i.gang_threshold.windows(2).all(|w| w[0] <= w[1]));
}

/// `CTheZones`' statics.
#[derive(Debug, Clone)]
pub struct Zones {
    pub players_zone: Option<usize>,
    pub find_index: i16,
    pub audio_zone_array: Vec<Option<usize>>,
    pub total_number_of_map_zones: usize,
    pub total_number_of_zones: usize,
    pub zone_array: Vec<Zone>,
    pub map_zone_array: Vec<Zone>,
    pub total_number_of_zone_infos: usize,
    pub zone_info_array: Vec<ZoneInfo>,
    pub curr_level: i32,
}

impl Default for Zones {
    /// `CTheZones::Init`: the whole map is one zone with ordinary traffic.
    fn default() -> Self {
        let x = 1000 / 6;
        let mut info = ZoneInfo { car_density: 10, ..Default::default() };
        info.car_threshold[0] = x;
        for i in 1..4 {
            info.car_threshold[i] = info.car_threshold[i - 1] + x;
        }
        info.car_threshold[4] = info.car_threshold[3];
        info.car_threshold[5] = info.car_threshold[4];
        info.cop_threshold = info.car_threshold[5] + x;
        info.gang_threshold = [info.cop_threshold; NUM_GANGS];
        check_zone_info(&info);
        let whole_map = |name: &str, ty| Zone { name: name.to_owned(), min: Vec3::new(-4000.0, -4000.0, -500.0), max: Vec3::new(4000.0, 4000.0, 500.0), ty, level: level::GENERIC, ..Default::default() };
        let mut zone_array = vec![Zone::default(); NUMZONES];
        zone_array[0] = whole_map("CITYZON", ZoneType::Default);
        let mut map_zone_array = vec![Zone { ty: ZoneType::MapZone, ..Default::default() }; NUMMAPZONES];
        map_zone_array[0] = whole_map("THEMAP", ZoneType::MapZone);
        Self {
            players_zone: Some(0),
            find_index: 0,
            audio_zone_array: vec![None; NUMAUDIOZONES],
            total_number_of_map_zones: 1,
            total_number_of_zones: 1,
            zone_array,
            map_zone_array,
            // the original starts at 1 so that info 0 is the default
            total_number_of_zone_infos: 1,
            zone_info_array: vec![info; 2 * NUMZONES],
            curr_level: level::GENERIC,
        }
    }
}

fn normalised(name: &str) -> String {
    name.to_ascii_uppercase().chars().take(7).collect()
}

fn ordered(a: Vec3, b: Vec3) -> (Vec3, Vec3) {
    (a.min(b), a.max(b))
}

pub fn point_lies_within_zone(v: Vec3, z: &Zone) -> bool {
    z.min.x <= v.x && v.x <= z.max.x && z.min.y <= v.y && v.y <= z.max.y && z.min.z <= v.z && v.z <= z.max.z
}

impl Zones {
    /// `Update`: where the player is.
    pub fn update(&mut self, player_pos: Vec3) {
        self.players_zone = self.find_smallest_zone_position(player_pos);
        self.curr_level = self.get_level_from_position(player_pos);
    }

    /// `CreateZone`.
    pub fn create_zone(&mut self, name: &str, ty: ZoneType, a: Vec3, b: Vec3, level: i32) {
        let (min, max) = ordered(a, b);
        let i = self.total_number_of_zones;
        let (mut day, mut night) = (0, 0);
        if is_normal_zone(ty) {
            day = self.total_number_of_zone_infos as i16;
            night = day + 1;
            self.total_number_of_zone_infos += 2;
        }
        self.zone_array[i] = Zone { name: normalised(name), min, max, ty, level, zoneinfo_day: day, zoneinfo_night: night, ..Default::default() };
        self.total_number_of_zones += 1;
    }

    /// `CreateMapZone`.
    pub fn create_map_zone(&mut self, name: &str, ty: ZoneType, a: Vec3, b: Vec3, level: i32) {
        let (min, max) = ordered(a, b);
        let i = self.total_number_of_map_zones;
        self.map_zone_array[i] = Zone { name: normalised(name), min, max, ty, level, ..Default::default() };
        self.total_number_of_map_zones += 1;
    }

    /// `PostZoneCreation`: nests the zones and lists the audio zones.
    pub fn post_zone_creation(&mut self) {
        for i in 1..self.total_number_of_zones {
            self.insert_zone_into_zone_hierarchy(i);
        }
        self.initialise_audio_zone_array();
    }

    /// `InsertZoneIntoZoneHierarchy`.
    pub fn insert_zone_into_zone_hierarchy(&mut self, zone: usize) {
        let z = &mut self.zone_array[zone];
        (z.child, z.parent, z.next) = (None, None, None);
        self.insert_zone_into_zone_hier_recursive(zone, 0);
    }

    /// `InsertZoneIntoZoneHierRecursive`: false if `inner` is not inside `outer`.
    pub fn insert_zone_into_zone_hier_recursive(&mut self, inner: usize, outer: usize) -> bool {
        if !self.zone_is_entirely_contained_within_other_zone(inner, outer) {
            return false;
        }
        // into one of the children if it fits
        let mut child = self.zone_array[outer].child;
        while let Some(c) = child {
            if self.insert_zone_into_zone_hier_recursive(inner, c) {
                return true;
            }
            child = self.zone_array[c].next;
        }
        // otherwise as a child of outer, taking the children of outer it contains
        let mut n = 0;
        let mut child = self.zone_array[outer].child;
        while let Some(c) = child {
            if self.zone_is_entirely_contained_within_other_zone(c, inner) {
                n += 1;
            }
            child = self.zone_array[c].next;
        }
        self.zone_array[inner].next = self.zone_array[outer].child;
        self.zone_array[inner].parent = Some(outer);
        self.zone_array[outer].child = Some(inner);
        if n > 0 {
            let mut insert = inner;
            let mut child = self.zone_array[inner].next;
            while let Some(c) = child {
                let next = self.zone_array[c].next;
                if self.zone_is_entirely_contained_within_other_zone(c, inner) {
                    self.zone_array[insert].next = self.zone_array[c].next;
                    self.zone_array[c].parent = Some(inner);
                    self.zone_array[c].next = self.zone_array[inner].child;
                    self.zone_array[inner].child = Some(c);
                } else {
                    insert = c;
                }
                child = next;
            }
        }
        true
    }

    /// `ZoneIsEntirelyContainedWithinOtherZone`.
    pub fn zone_is_entirely_contained_within_other_zone(&self, inner: usize, outer: usize) -> bool {
        let (i, o) = (&self.zone_array[inner], &self.zone_array[outer]);
        i.min.cmpge(o.min).all() && i.max.cmple(o.max).all()
    }

    /// `GetLevelFromPosition`: the level of the first map zone the point is in, else the whole map's.
    pub fn get_level_from_position(&self, v: Vec3) -> i32 {
        self.map_zone_array[1..self.total_number_of_map_zones].iter().find(|z| point_lies_within_zone(v, z)).map_or(self.map_zone_array[0].level, |z| z.level)
    }

    fn find_smallest(&self, v: Vec3, keep: impl Fn(&Zone) -> bool, start: Option<usize>) -> Option<usize> {
        let mut best = start;
        let mut zone = self.zone_array[0].child;
        // inside a zone, go down into its children; outside, on to the next one
        while let Some(z) = zone {
            if point_lies_within_zone(v, &self.zone_array[z]) {
                if keep(&self.zone_array[z]) {
                    best = Some(z);
                }
                zone = self.zone_array[z].child;
            } else {
                zone = self.zone_array[z].next;
            }
        }
        best
    }

    /// `FindSmallestZonePosition`.
    pub fn find_smallest_zone_position(&self, v: Vec3) -> Option<usize> {
        self.find_smallest(v, |_| true, Some(0))
    }

    /// `FindSmallestZonePositionType`.
    pub fn find_smallest_zone_position_type(&self, v: Vec3, ty: ZoneType) -> Option<usize> {
        self.find_smallest(v, |z| z.ty == ty, (self.zone_array[0].ty == ty).then_some(0))
    }

    /// `FindSmallestZonePositionILN`: of the zones with traffic information.
    pub fn find_smallest_zone_position_iln(&self, v: Vec3) -> Option<usize> {
        self.find_smallest(v, |z| is_normal_zone(z.ty), is_normal_zone(self.zone_array[0].ty).then_some(0))
    }

    /// `FindZoneByLabelAndReturnIndex`.
    pub fn find_zone_by_label_and_return_index(&mut self, name: &str) -> Option<usize> {
        let found = (0..self.total_number_of_zones).find(|&i| self.zone_array[i].name == name);
        self.find_index = found.map_or(self.total_number_of_zones as i16, |i| i as i16);
        found
    }

    /// `GetZoneInfo`: the zone's day or night mix, or the default one outside any.
    pub fn get_zone_info(&self, v: Vec3, day: bool) -> &ZoneInfo {
        match self.find_smallest_zone_position_iln(v) {
            None => &self.zone_info_array[0],
            Some(z) => {
                let z = &self.zone_array[z];
                &self.zone_info_array[if day { z.zoneinfo_day } else { z.zoneinfo_night } as usize]
            }
        }
    }

    /// `GetZoneInfoForTimeOfDay`: the day mix by day, the night mix by night, a blend at dusk and dawn.
    pub fn get_zone_info_for_time_of_day(&self, pos: Vec3, clock: &Clock) -> ZoneInfo {
        let (day, night) = (self.get_zone_info(pos, true), self.get_zone_info(pos, false));
        let mut info;
        if clock.is_time_in_range(8, 19) {
            info = *day;
        } else if clock.is_time_in_range(22, 5) {
            info = *night;
        } else {
            let (d, n);
            if clock.is_time_in_range(19, 22) {
                n = (clock.hours as f32 - 19.0) / 3.0;
                d = 1.0 - n;
            } else {
                d = (clock.hours as f32 - 5.0) / 3.0;
                n = 1.0 - d;
            }
            debug_assert!((0.0..=1.0).contains(&d) && (0.0..=1.0).contains(&n));
            let mix = |a: i16, b: i16| (a as f32 * d + b as f32 * n) as i16;
            let mixu = |a: u16, b: u16| (a as f32 * d + b as f32 * n) as u16;
            info = ZoneInfo { car_density: mix(day.car_density, night.car_density), ..Default::default() };
            for i in 0..6 {
                info.car_threshold[i] = mix(day.car_threshold[i], night.car_threshold[i]);
            }
            info.cop_threshold = mix(day.cop_threshold, night.cop_threshold);
            for i in 0..NUM_GANGS {
                info.gang_threshold[i] = mix(day.gang_threshold[i], night.gang_threshold[i]);
                info.gang_density[i] = mixu(day.gang_density[i], night.gang_density[i]);
            }
            info.ped_density = mixu(day.ped_density, night.ped_density);
            info.cop_density = mixu(day.cop_density, night.cop_density);
        }
        info.ped_group = if clock.is_time_in_range(5, 19) { day.ped_group } else { night.ped_group };
        check_zone_info(&info);
        info
    }

    fn info_of(&mut self, zone: usize, day: bool) -> &mut ZoneInfo {
        let z = &self.zone_array[zone];
        let i = if day { z.zoneinfo_day } else { z.zoneinfo_night } as usize;
        &mut self.zone_info_array[i]
    }

    /// `SetZoneCarInfo`.
    pub fn set_zone_car_info(&mut self, zone: usize, day: bool, c: CarInfoChange) {
        let info = self.info_of(zone, day);
        check_zone_info(info);
        if c.car_density != -1 {
            info.car_density = c.car_density;
        }
        let t = info.car_threshold;
        let old_cars = [t[1] - t[0], t[2] - t[1], t[3] - t[2], t[4] - t[3], t[5] - t[4]];
        let old_cop = info.cop_threshold - t[5];
        let g = info.gang_threshold;
        let mut old_gangs = [g[0] - info.cop_threshold; NUM_GANGS];
        for i in 1..NUM_GANGS {
            old_gangs[i] = g[i] - g[i - 1];
        }
        if c.cars[0] != -1 {
            info.car_threshold[0] = c.cars[0];
        }
        for i in 1..6 {
            info.car_threshold[i] = info.car_threshold[i - 1] + if c.cars[i] != -1 { c.cars[i] } else { old_cars[i - 1] };
        }
        info.cop_threshold = info.car_threshold[5] + if c.cops != -1 { c.cops } else { old_cop };
        let mut prev = info.cop_threshold;
        for (i, gap) in old_gangs.into_iter().enumerate() {
            prev += if c.gangs[i] != -1 { c.gangs[i] } else { gap };
            info.gang_threshold[i] = prev;
        }
        check_zone_info(info);
    }

    /// `SetZonePedInfo`.
    pub fn set_zone_ped_info(&mut self, zone: usize, day: bool, ped_density: i16, gang_density: [i16; NUM_GANGS], cop_density: i16) {
        let info = self.info_of(zone, day);
        if ped_density != -1 {
            info.ped_density = ped_density as u16;
        }
        if cop_density != -1 {
            info.cop_density = cop_density as u16;
        }
        for (i, d) in gang_density.into_iter().enumerate() {
            if d != -1 {
                info.gang_density[i] = d as u16;
            }
        }
    }

    /// `SetCarDensity`.
    pub fn set_car_density(&mut self, zone: usize, day: bool, density: u16) {
        if is_normal_zone(self.zone_array[zone].ty) {
            self.info_of(zone, day).car_density = density as i16;
        }
    }

    /// `SetPedDensity`.
    pub fn set_ped_density(&mut self, zone: usize, day: bool, density: u16) {
        if is_normal_zone(self.zone_array[zone].ty) {
            self.info_of(zone, day).ped_density = density;
        }
    }

    /// `SetPedGroup`.
    pub fn set_ped_group(&mut self, zone: usize, day: bool, group: u16) {
        if is_normal_zone(self.zone_array[zone].ty) {
            self.info_of(zone, day).ped_group = group;
        }
    }

    /// `FindAudioZone`.
    pub fn find_audio_zone(&self, pos: Vec3) -> Option<usize> {
        self.audio_zone_array.iter().flatten().position(|&z| point_lies_within_zone(pos, &self.zone_array[z]))
    }

    /// `FindZoneForPoint`: which of the three islands a point is on, by the labelled zones.
    pub fn find_zone_for_point(&mut self, pos: Vec3) -> i32 {
        for (label, lvl) in [("IND_ZON", level::INDUSTRIAL), ("COM_ZON", level::COMMERCIAL), ("SUB_ZON", level::SUBURBAN)] {
            // the original indexes with -1 when the zone is missing; a missing zone contains nothing here
            if let Some(i) = self.find_zone_by_label_and_return_index(label)
                && point_lies_within_zone(pos, &self.zone_array[i])
            {
                return lvl;
            }
        }
        level::GENERIC
    }

    fn add_zone_to_audio_zone_array(&mut self, zone: usize, count: &mut usize) {
        if self.zone_array[zone].ty != ZoneType::Default {
            return;
        }
        self.audio_zone_array[*count] = Some(zone);
        *count += 1;
    }

    /// `InitialiseAudioZoneArray`: the default zones in depth-first order, children before their next sibling's turn.
    pub fn initialise_audio_zone_array(&mut self) {
        let mut count = 0;
        let mut go_next = false;
        let mut zone = Some(0usize);
        while let Some(z) = zone {
            if go_next {
                self.add_zone_to_audio_zone_array(z, &mut count);
                if let Some(n) = self.zone_array[z].next {
                    go_next = false;
                    zone = Some(n);
                } else {
                    zone = self.zone_array[z].parent;
                }
            } else if let Some(c) = self.zone_array[z].child {
                zone = Some(c);
            } else {
                self.add_zone_to_audio_zone_array(z, &mut count);
                if let Some(n) = self.zone_array[z].next {
                    zone = Some(n);
                } else {
                    go_next = true;
                    zone = self.zone_array[z].parent;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32) -> Vec3 {
        Vec3::new(x, y, 0.0)
    }

    /// The city, a downtown zone in it, a block in downtown and a harbour next to downtown.
    fn city() -> Zones {
        let mut z = Zones::default();
        z.create_zone("downtown", ZoneType::Default, v(0.0, 0.0), v(1000.0, 1000.0), level::COMMERCIAL);
        z.create_zone("block", ZoneType::Navig, v(100.0, 100.0), v(200.0, 200.0), level::COMMERCIAL);
        z.create_zone("harbour", ZoneType::Default, v(2000.0, 0.0), v(3000.0, 1000.0), level::INDUSTRIAL);
        z.post_zone_creation();
        z
    }

    #[test]
    fn zones_nest_by_containment_and_the_smallest_one_at_a_point_is_found() {
        let z = city();
        let (down, block, harbour) = (z.find_smallest_zone_position(v(500.0, 500.0)), z.find_smallest_zone_position(v(150.0, 150.0)), z.find_smallest_zone_position(v(2500.0, 500.0)));
        assert_eq!(z.zone_array[down.unwrap()].name, "DOWNTOW");
        assert_eq!(z.zone_array[block.unwrap()].name, "BLOCK");
        assert_eq!(z.zone_array[harbour.unwrap()].name, "HARBOUR");
        assert_eq!(z.find_smallest_zone_position(v(-3000.0, 0.0)), Some(0));
        // the block sits inside downtown, which sits inside the city
        let b = block.unwrap();
        assert_eq!(z.zone_array[z.zone_array[b].parent.unwrap()].name, "DOWNTOW");
    }

    #[test]
    fn a_zone_made_after_its_inner_zone_adopts_it() {
        let mut z = Zones::default();
        z.create_zone("small", ZoneType::Default, v(10.0, 10.0), v(20.0, 20.0), 0);
        z.create_zone("big", ZoneType::Default, v(0.0, 0.0), v(100.0, 100.0), 0);
        z.post_zone_creation();
        let small = z.find_zone_by_label_and_return_index("SMALL").unwrap();
        let big = z.find_zone_by_label_and_return_index("BIG").unwrap();
        assert_eq!((z.zone_array[small].parent, z.zone_array[big].parent), (Some(big), Some(0)));
    }

    #[test]
    fn type_filters_pick_the_smallest_zone_of_that_kind() {
        let z = city();
        let nav = z.find_smallest_zone_position_type(v(150.0, 150.0), ZoneType::Navig).unwrap();
        assert_eq!(z.zone_array[nav].name, "BLOCK");
        assert_eq!(z.find_smallest_zone_position_type(v(500.0, 500.0), ZoneType::Navig), None);
        assert_eq!(z.find_smallest_zone_position_type(v(500.0, 500.0), ZoneType::Default).map(|i| z.zone_array[i].name.clone()).as_deref(), Some("DOWNTOW"));
    }

    #[test]
    fn map_zones_give_the_level_and_the_whole_map_is_the_fallback() {
        let mut z = Zones::default();
        z.create_map_zone("ind", ZoneType::MapZone, v(0.0, 0.0), v(100.0, 100.0), level::INDUSTRIAL);
        assert_eq!(z.get_level_from_position(v(50.0, 50.0)), level::INDUSTRIAL);
        assert_eq!(z.get_level_from_position(v(500.0, 500.0)), level::GENERIC);
    }

    #[test]
    fn the_labelled_zones_say_which_island_a_point_is_on() {
        let mut z = Zones::default();
        z.create_zone("ind_zon", ZoneType::Default, v(0.0, 0.0), v(100.0, 100.0), 0);
        z.create_zone("com_zon", ZoneType::Default, v(200.0, 0.0), v(300.0, 100.0), 0);
        z.create_zone("sub_zon", ZoneType::Default, v(400.0, 0.0), v(500.0, 100.0), 0);
        assert_eq!((z.find_zone_for_point(v(50.0, 50.0)), z.find_zone_for_point(v(250.0, 50.0)), z.find_zone_for_point(v(450.0, 50.0)), z.find_zone_for_point(v(900.0, 50.0))), (level::INDUSTRIAL, level::COMMERCIAL, level::SUBURBAN, level::GENERIC));
        // a missing label contains nothing
        assert_eq!(Zones::default().find_zone_for_point(v(0.0, 0.0)), level::GENERIC);
    }

    #[test]
    fn traffic_info_is_the_day_mix_by_day_the_night_mix_by_night_and_a_blend_between() {
        let mut z = city();
        let down = z.find_zone_by_label_and_return_index("DOWNTOW").unwrap();
        z.set_car_density(down, true, 40);
        z.set_car_density(down, false, 10);
        z.set_ped_group(down, true, 3);
        z.set_ped_group(down, false, 7);
        let mut clock = Clock::new(1000, 0);
        let p = v(500.0, 500.0);
        clock.hours = 12;
        let day = z.get_zone_info_for_time_of_day(p, &clock);
        assert_eq!((day.car_density, day.ped_group), (40, 3));
        clock.hours = 0;
        let night = z.get_zone_info_for_time_of_day(p, &clock);
        assert_eq!((night.car_density, night.ped_group), (10, 7));
        // 20:30 is half way through dusk: a third of the way from day to night is hour 20
        clock.hours = 20;
        let dusk = z.get_zone_info_for_time_of_day(p, &clock);
        assert_eq!(dusk.car_density, (40.0 * (2.0 / 3.0) + 10.0 * (1.0 / 3.0)) as i16);
        assert_eq!(dusk.ped_group, 7);
    }

    #[test]
    fn changing_one_threshold_shifts_the_ones_after_it_and_minus_one_keeps_the_rest() {
        let mut z = city();
        let down = z.find_zone_by_label_and_return_index("DOWNTOW").unwrap();
        let before = *z.get_zone_info(v(500.0, 500.0), true);
        let mut change = CarInfoChange { car_density: -1, gangs: [-1; NUM_GANGS], cops: -1, cars: [-1; 6] };
        change.cars[2] = 300;
        z.set_zone_car_info(down, true, change);
        let after = *z.get_zone_info(v(500.0, 500.0), true);
        // the gap before car 2 became 300; everything after keeps its own gap
        assert_eq!(after.car_threshold[2] - after.car_threshold[1], 300);
        assert_eq!(after.car_threshold[3] - after.car_threshold[2], before.car_threshold[3] - before.car_threshold[2]);
        assert_eq!(after.gang_threshold[8] - after.gang_threshold[0], before.gang_threshold[8] - before.gang_threshold[0]);
    }

    #[test]
    fn the_audio_zones_are_the_default_zones_in_depth_first_order() {
        let z = city();
        let names: Vec<_> = z.audio_zone_array.iter().flatten().map(|&i| z.zone_array[i].name.as_str()).collect();
        assert!(names.contains(&"DOWNTOW") && names.contains(&"HARBOUR") && names.contains(&"CITYZON"));
        assert!(!names.contains(&"BLOCK"));
        assert!(z.find_audio_zone(v(2500.0, 500.0)).is_some());
        // the smallest zones come first, the city last
        assert_eq!(*names.last().unwrap(), "CITYZON");
    }
}
