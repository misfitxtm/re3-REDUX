//! `CPedType`, `CPedStats` and `CGangs`: the ped.dat, pedstats.dat and gang tables.
use re3_formats::ped::{parse_ped_dat, parse_ped_stats};

pub mod ped_type {
    pub const PLAYER1: usize = 0;
    pub const PLAYER2: usize = 1;
    pub const PLAYER3: usize = 2;
    pub const PLAYER4: usize = 3;
    pub const CIVMALE: usize = 4;
    pub const CIVFEMALE: usize = 5;
    pub const COP: usize = 6;
    pub const GANG1: usize = 7;
    pub const GANG2: usize = 8;
    pub const GANG3: usize = 9;
    pub const GANG4: usize = 10;
    pub const GANG5: usize = 11;
    pub const GANG6: usize = 12;
    pub const GANG7: usize = 13;
    pub const GANG8: usize = 14;
    pub const GANG9: usize = 15;
    pub const EMERGENCY: usize = 16;
    pub const FIREMAN: usize = 17;
    pub const CRIMINAL: usize = 18;
    pub const UNUSED1: usize = 19;
    pub const PROSTITUTE: usize = 20;
    pub const SPECIAL: usize = 21;
    pub const UNUSED2: usize = 22;
    pub const NUM: usize = 23;
}

/// `PED_FLAG_*`: one bit per ped type, plus a few threat kinds.
pub mod ped_flag {
    pub const PLAYER1: u32 = 1 << 0;
    pub const PLAYER2: u32 = 1 << 1;
    pub const PLAYER3: u32 = 1 << 2;
    pub const PLAYER4: u32 = 1 << 3;
    pub const CIVMALE: u32 = 1 << 4;
    pub const CIVFEMALE: u32 = 1 << 5;
    pub const COP: u32 = 1 << 6;
    pub const GANG1: u32 = 1 << 7;
    pub const GANG2: u32 = 1 << 8;
    pub const GANG3: u32 = 1 << 9;
    pub const GANG4: u32 = 1 << 10;
    pub const GANG5: u32 = 1 << 11;
    pub const GANG6: u32 = 1 << 12;
    pub const GANG7: u32 = 1 << 13;
    pub const GANG8: u32 = 1 << 14;
    pub const GANG9: u32 = 1 << 15;
    pub const EMERGENCY: u32 = 1 << 16;
    pub const PROSTITUTE: u32 = 1 << 17;
    pub const CRIMINAL: u32 = 1 << 18;
    pub const SPECIAL: u32 = 1 << 19;
    pub const GUN: u32 = 1 << 20;
    pub const COP_CAR: u32 = 1 << 21;
    pub const FAST_CAR: u32 = 1 << 22;
    pub const EXPLOSION: u32 = 1 << 23;
    pub const FIREMAN: u32 = 1 << 24;
    pub const DEADPEDS: u32 = 1 << 25;
}

const TYPE_NAMES: [&str; ped_type::NUM] = [
    "PLAYER1", "PLAYER2", "PLAYER3", "PLAYER4", "CIVMALE", "CIVFEMALE", "COP", "GANG1", "GANG2", "GANG3", "GANG4", "GANG5", "GANG6", "GANG7", "GANG8", "GANG9",
    "EMERGENCY", "FIREMAN", "CRIMINAL", "", "PROSTITUTE", "SPECIAL", "",
];

/// `CPedType::FindPedType`.
pub fn find_ped_type(name: &str) -> Option<usize> {
    if name.is_empty() {
        return None;
    }
    TYPE_NAMES.iter().position(|n| *n == name)
}

/// `CPedType::FindPedFlag`: unknown names give 0.
pub fn find_ped_flag(name: &str) -> u32 {
    use ped_flag::*;
    match name {
        "PLAYER1" => PLAYER1,
        "PLAYER2" => PLAYER2,
        "PLAYER3" => PLAYER3,
        "PLAYER4" => PLAYER4,
        "CIVMALE" => CIVMALE,
        "CIVFEMALE" => CIVFEMALE,
        "COP" => COP,
        "GANG1" => GANG1,
        "GANG2" => GANG2,
        "GANG3" => GANG3,
        "GANG4" => GANG4,
        "GANG5" => GANG5,
        "GANG6" => GANG6,
        "GANG7" => GANG7,
        "GANG8" => GANG8,
        "GANG9" => GANG9,
        "EMERGENCY" => EMERGENCY,
        "FIREMAN" => FIREMAN,
        "CRIMINAL" => CRIMINAL,
        "SPECIAL" => SPECIAL,
        "GUN" => GUN,
        "COP_CAR" => COP_CAR,
        "FAST_CAR" => FAST_CAR,
        "EXPLOSION" => EXPLOSION,
        "PROSTITUTE" => PROSTITUTE,
        "DEADPEDS" => DEADPEDS,
        _ => 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PedType {
    pub flag: u32,
    pub unknown: [f32; 5],
    pub threats: u32,
    pub avoid: u32,
}

impl Default for PedType {
    fn default() -> Self {
        Self { flag: ped_flag::PLAYER1, unknown: [0.0; 5], threats: 0, avoid: 0 }
    }
}

#[derive(Debug, Clone)]
pub struct PedTypes(pub [PedType; ped_type::NUM]);

impl Default for PedTypes {
    fn default() -> Self {
        Self([PedType::default(); ped_type::NUM])
    }
}

impl PedTypes {
    /// `CPedType::LoadPedData`. Lines of unknown types are skipped.
    pub fn from_dat(text: &str) -> Self {
        let mut out = Self::default();
        for p in parse_ped_dat(text) {
            let Some(t) = find_ped_type(&p.name) else { continue };
            let e = &mut out.0[t];
            e.flag = find_ped_flag(&p.name);
            e.unknown = [p.values[0] / 50.0, p.values[1] / 50.0, p.values[2] / 50.0, p.values[3], p.values[4]];
            e.threats = p.threats.iter().fold(0, |a, w| a | find_ped_flag(w));
            e.avoid = p.avoid.iter().fold(0, |a, w| a | find_ped_flag(w));
        }
        out
    }

    pub fn flag(&self, t: usize) -> u32 {
        self.0[t].flag
    }

    pub fn avoid(&self, t: usize) -> u32 {
        self.0[t].avoid
    }

    pub fn threats(&self, t: usize) -> u32 {
        self.0[t].threats
    }

    pub fn set_threats(&mut self, t: usize, threats: u32) {
        self.0[t].threats = threats;
    }

    pub fn add_threat(&mut self, t: usize, threat: u32) {
        self.0[t].threats |= threat;
    }

    pub fn remove_threat(&mut self, t: usize, threat: u32) {
        self.0[t].threats &= !threat;
    }

    pub fn is_threat(&self, t: usize, threat: u32) -> bool {
        self.0[t].threats & threat != 0
    }
}

pub const NUM_PEDSTATS: usize = 35;

/// `PEDSTAT_*` indices that code refers to by name.
pub mod ped_stat {
    pub const PLAYER: usize = 0;
    pub const COP: usize = 1;
    pub const MEDIC: usize = 2;
    pub const FIREMAN: usize = 3;
    pub const GANG1: usize = 4;
    pub const STREET_GUY: usize = 11;
    pub const SUIT_GUY: usize = 12;
    pub const SENSIBLE_GUY: usize = 13;
    pub const GEEK_GUY: usize = 14;
    pub const OLD_GUY: usize = 15;
    pub const TOUGH_GUY: usize = 16;
    pub const STREET_GIRL: usize = 17;
    pub const TRAMP_MALE: usize = 23;
    pub const TOURIST: usize = 25;
    pub const PROSTITUTE: usize = 26;
    pub const CRIMINAL: usize = 27;
    pub const PSYCHO: usize = 30;
}

/// `STAT_*` flags of a ped's fighting style.
pub mod stat_flag {
    pub const PUNCH_ONLY: i16 = 1;
    pub const CAN_KNEE_HEAD: i16 = 2;
    pub const CAN_KICK: i16 = 4;
    pub const CAN_ROUNDHOUSE: i16 = 8;
    pub const NO_DIVE: i16 = 0x10;
    pub const ONE_HIT_KNOCKDOWN: i16 = 0x20;
    pub const SHOPPING_BAGS: i16 = 0x40;
    pub const GUN_PANIC: i16 = 0x80;
}

#[derive(Debug, Clone, PartialEq)]
pub struct PedStats {
    pub ty: usize,
    pub name: String,
    pub flee_distance: f32,
    pub heading_change_rate: f32,
    pub fear: i8,
    pub temper: i8,
    pub lawfulness: i8,
    pub sexiness: i8,
    pub attack_strength: f32,
    pub defend_weakness: f32,
    pub flags: i16,
}

impl Default for PedStats {
    fn default() -> Self {
        Self {
            ty: ped_stat::PLAYER,
            name: String::new(),
            flee_distance: 20.0,
            heading_change_rate: 15.0,
            fear: 50,
            temper: 50,
            lawfulness: 50,
            sexiness: 50,
            attack_strength: 1.0,
            defend_weakness: 1.0,
            flags: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PedStatTable(pub Vec<PedStats>);

impl Default for PedStatTable {
    fn default() -> Self {
        Self(vec![PedStats::default(); NUM_PEDSTATS])
    }
}

impl PedStatTable {
    /// `CPedStats::LoadPedStats`: the n-th line fills the n-th entry.
    pub fn from_dat(text: &str) -> Self {
        let mut out = Self::default();
        for (i, s) in parse_ped_stats(text).into_iter().take(NUM_PEDSTATS).enumerate() {
            out.0[i] = PedStats {
                ty: i,
                name: s.name.chars().take(23).collect(),
                flee_distance: s.flee_distance,
                heading_change_rate: s.heading_change_rate,
                fear: s.fear as i8,
                temper: s.temper as i8,
                lawfulness: s.lawfulness as i8,
                sexiness: s.sexiness as i8,
                attack_strength: s.attack_strength,
                defend_weakness: s.defend_weakness,
                flags: s.flags as i16,
            };
        }
        out
    }

    /// `CPedStats::GetPedStatType`: `NUM_PEDSTATS` if there's no such name.
    pub fn find(&self, name: &str) -> usize {
        self.0.iter().position(|s| s.name.eq_ignore_ascii_case(name)).unwrap_or(NUM_PEDSTATS)
    }
}

pub const NUM_GANGS: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GangInfo {
    pub vehicle_mi: i32,
    pub ped_model_override: i8,
    pub weapon1: i32,
    pub weapon2: i32,
}

/// `CGangs`. The vehicle models are model indices, filled in once the IDE is loaded.
#[derive(Debug, Clone)]
pub struct Gangs(pub [GangInfo; NUM_GANGS]);

impl Gangs {
    /// `CGangs::Initialise`, given the gang cars' model ids (mafia, triad, diablos, yakuza, yardie, columbian, hoods) and `MI_BUS`.
    pub fn new(bus: i32, cars: [i32; 7]) -> Self {
        let base = GangInfo { vehicle_mi: bus, ped_model_override: -1, weapon1: 0, weapon2: 0 };
        let mut g = [base; NUM_GANGS];
        for (i, c) in cars.into_iter().enumerate() {
            g[i].vehicle_mi = c;
        }
        g[7].vehicle_mi = -1;
        g[8].vehicle_mi = -1;
        Self(g)
    }

    pub fn set_vehicle_model(&mut self, gang: usize, model: i32) {
        self.0[gang].vehicle_mi = model;
    }

    pub fn set_weapons(&mut self, gang: usize, w1: i32, w2: i32) {
        self.0[gang].weapon1 = w1;
        self.0[gang].weapon2 = w2;
    }

    pub fn set_ped_model_override(&mut self, gang: usize, o: i8) {
        self.0[gang].ped_model_override = o;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ped_dat_and_stats_load() {
        let t = PedTypes::from_dat("COP 50 100 150 4 5\nThreat GANG1 EXPLOSION\nAvoid PLAYER1\nCIVMALE 0 0 0 0 0\nThreat GUN\n");
        assert_eq!(t.flag(ped_type::COP), ped_flag::COP);
        assert_eq!(t.0[ped_type::COP].unknown, [1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(t.is_threat(ped_type::COP, ped_flag::GANG1 | ped_flag::EXPLOSION));
        assert_eq!(t.avoid(ped_type::COP), ped_flag::PLAYER1);
        assert_eq!(t.threats(ped_type::CIVMALE), ped_flag::GUN);
        let s = PedStatTable::from_dat("PLAYER 20 15 50 50 50 50 1 1 0\nCOP 25 7.5 10 40 100 50 2 3 6\n");
        assert_eq!((s.0[1].fear, s.0[1].flags, s.find("cop")), (10, 6, 1));
        assert_eq!(s.find("nope"), NUM_PEDSTATS);
    }

    #[test]
    fn real_files() {
        let Ok(dir) = std::env::var("GTA3_DIR") else { return };
        let read = |n: &str| std::fs::read_to_string(format!("{dir}/data/{n}")).or_else(|_| std::fs::read_to_string(format!("{dir}/data/{}", n.to_lowercase()))).unwrap();
        let t = PedTypes::from_dat(&read("ped.dat"));
        assert_eq!(t.flag(ped_type::GANG3), ped_flag::GANG3);
        assert!(t.threats(ped_type::COP) != 0);
        let s = PedStatTable::from_dat(&read("pedstats.dat"));
        assert_eq!(s.0[ped_stat::COP].ty, ped_stat::COP);
        assert!(s.find("STAT_COP") < NUM_PEDSTATS || s.find("COP") < NUM_PEDSTATS);
    }

    #[test]
    fn gangs_default_to_the_bus() {
        let g = Gangs::new(5, [1, 2, 3, 4, 5, 6, 7]);
        assert_eq!((g.0[0].vehicle_mi, g.0[7].vehicle_mi, g.0[3].ped_model_override), (1, -1, -1));
    }
}
