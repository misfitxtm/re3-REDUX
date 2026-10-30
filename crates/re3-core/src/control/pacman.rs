//! `Pickups.cpp`, `CPacManPickups`: the gold bars of the Bling-Bling Scramble and the pills of the Diablo race.
use crate::entity::object::CreatedBy;
use crate::entity::{EntityRef, Kind, PoolKind};
use crate::math::Matrix;
use crate::modelinfo::indices_gta3::slot;
use crate::ped::ctx::Px;
use crate::world::{Kinds, World};
use glam::Vec3;
use std::f32::consts::FRAC_PI_2;

/// `NUMPACMANPICKUPS`.
pub const NUM_PACMAN_PICKUPS: usize = 256;
/// How many frames the pickups are spread over when they are updated.
const FRAME_SPAN: u32 = 4;
/// Where the scramble's gold is handed in.
const HAND_IN_POINT: Vec3 = Vec3::new(1072.0, -948.0, 14.5);
/// How many tries one gold bar gets to find a road; the original has no limit.
const MAX_TRIES: u32 = 1000;

/// `ePacManPickupType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PacManType {
    #[default]
    None,
    Scramble,
    Race,
}

/// `CPacManPickup`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PacManPickup {
    pub ty: PacManType,
    pub pos: Vec3,
    pub object: Option<EntityRef>,
}

/// `CPacManPickups`' statics.
#[derive(Debug, Clone, PartialEq)]
pub struct PacManPickups {
    pub pickups: Vec<PacManPickup>,
    pub last_pick_up_coors: Vec3,
    pub pills_eaten_in_race: i32,
    pub active: bool,
    pub collect_game_state: i32,
    pub things_to_collect: i16,
}

impl Default for PacManPickups {
    /// `CPacManPickups::Init`.
    fn default() -> Self {
        Self { pickups: vec![PacManPickup::default(); NUM_PACMAN_PICKUPS], last_pick_up_coors: Vec3::ZERO, pills_eaten_in_race: 0, active: false, collect_game_state: 0, things_to_collect: 0 }
    }
}

/// The pills of the only race (`aRacePoints1`).
#[allow(clippy::excessive_precision)] // the digits of the original
const RACE_POINTS: [Vec3; 106] = [
    Vec3::new(913.62219, -155.13692, 4.9699469),
    Vec3::new(913.92401, -124.12943, 4.9692569),
    Vec3::new(913.27899, -93.524231, 7.4325991),
    Vec3::new(912.60852, -63.15905, 7.4533591),
    Vec3::new(934.22144, -42.049122, 7.4511471),
    Vec3::new(958.88092, -23.863735, 7.4652338),
    Vec3::new(978.50812, -0.78458798, 5.13515),
    Vec3::new(1009.4175, -2.1041219, 2.4461579),
    Vec3::new(1040.6313, -2.0793829, 2.293175),
    Vec3::new(1070.7863, -2.084095, 2.2789791),
    Vec3::new(1100.5773, -8.468729, 5.3248072),
    Vec3::new(1119.9341, -31.738031, 7.1913071),
    Vec3::new(1122.1664, -62.762737, 7.4703908),
    Vec3::new(1122.814, -93.650566, 8.5577497),
    Vec3::new(1125.8253, -124.26616, 9.9803305),
    Vec3::new(1153.8727, -135.47169, 14.150617),
    Vec3::new(1184.0831, -135.82845, 14.973998),
    Vec3::new(1192.0432, -164.57816, 19.18627),
    Vec3::new(1192.7761, -194.28871, 24.799675),
    Vec3::new(1215.1527, -215.0714, 25.74975),
    Vec3::new(1245.79, -215.39304, 28.70726),
    Vec3::new(1276.2477, -216.39485, 33.71236),
    Vec3::new(1306.5535, -216.71007, 39.711472),
    Vec3::new(1335.0244, -224.59329, 46.474979),
    Vec3::new(1355.4879, -246.27664, 49.934841),
    Vec3::new(1362.6003, -276.47064, 49.96265),
    Vec3::new(1363.027, -307.30847, 49.969173),
    Vec3::new(1365.343, -338.08609, 49.967789),
    Vec3::new(1367.5957, -368.01105, 50.092304),
    Vec3::new(1368.2749, -398.38049, 50.061268),
    Vec3::new(1366.9034, -429.98483, 50.057545),
    Vec3::new(1356.8534, -459.09259, 50.035545),
    Vec3::new(1335.5819, -481.13544, 47.217903),
    Vec3::new(1306.7552, -491.07443, 40.202629),
    Vec3::new(1275.5978, -491.33194, 33.969223),
    Vec3::new(1244.702, -491.46451, 29.111021),
    Vec3::new(1213.2222, -491.8754, 25.771168),
    Vec3::new(1182.7729, -492.19995, 24.749964),
    Vec3::new(1152.6874, -491.42221, 21.70038),
    Vec3::new(1121.5352, -491.94604, 20.075182),
    Vec3::new(1090.7056, -492.63751, 17.585758),
    Vec3::new(1059.6008, -491.65762, 14.848632),
    Vec3::new(1029.113, -489.66031, 14.918498),
    Vec3::new(998.20679, -486.78107, 14.945688),
    Vec3::new(968.00555, -484.91266, 15.001229),
    Vec3::new(937.74939, -492.09015, 14.958629),
    Vec3::new(927.17352, -520.97736, 14.972308),
    Vec3::new(929.29749, -552.08643, 14.978855),
    Vec3::new(950.69525, -574.47778, 14.972788),
    Vec3::new(974.02826, -593.56024, 14.966445),
    Vec3::new(989.04779, -620.12854, 14.951016),
    Vec3::new(1014.1639, -637.3905, 14.966736),
    Vec3::new(1017.5961, -667.3736, 14.956415),
    Vec3::new(1041.9735, -685.94391, 15.003841),
    Vec3::new(1043.3064, -716.11298, 14.974236),
    Vec3::new(1043.5337, -746.63855, 14.96919),
    Vec3::new(1044.142, -776.93823, 14.965424),
    Vec3::new(1044.2657, -807.29395, 14.97171),
    Vec3::new(1017.0797, -820.1076, 14.975431),
    Vec3::new(986.23865, -820.37103, 14.972883),
    Vec3::new(956.10065, -820.23291, 14.981133),
    Vec3::new(925.86914, -820.19049, 14.976553),
    Vec3::new(897.69702, -831.08734, 14.962709),
    Vec3::new(868.06586, -835.99237, 14.970685),
    Vec3::new(836.93054, -836.84387, 14.965049),
    Vec3::new(811.63586, -853.7915, 15.067576),
    Vec3::new(811.46344, -884.27368, 12.247812),
    Vec3::new(811.60651, -914.70959, 9.2393751),
    Vec3::new(811.10425, -945.16272, 5.817255),
    Vec3::new(816.54584, -975.64587, 4.998558),
    Vec3::new(828.2951, -1003.3685, 5.0471172),
    Vec3::new(852.28839, -1021.5963, 4.9371028),
    Vec3::new(882.50067, -1025.4459, 5.14077),
    Vec3::new(912.84821, -1026.7874, 8.3415451),
    Vec3::new(943.68274, -1026.6914, 11.341879),
    Vec3::new(974.4129, -1027.3682, 14.410345),
    Vec3::new(1004.1079, -1036.0778, 14.92961),
    Vec3::new(1030.1144, -1051.1224, 14.850387),
    Vec3::new(1058.7585, -1060.342, 14.821624),
    Vec3::new(1087.7797, -1068.3263, 14.800561),
    Vec3::new(1099.8807, -1095.656, 11.877907),
    Vec3::new(1130.0005, -1101.994, 11.853914),
    Vec3::new(1160.3809, -1101.6355, 11.854824),
    Vec3::new(1191.8524, -1102.1577, 11.853843),
    Vec3::new(1223.3307, -1102.7448, 11.852233),
    Vec3::new(1253.564, -1098.1045, 11.853944),
    Vec3::new(1262.0203, -1069.1785, 14.8147),
    Vec3::new(1290.9998, -1059.1882, 14.816016),
    Vec3::new(1316.246, -1041.0635, 14.81109),
    Vec3::new(1331.7539, -1013.835, 14.81207),
    Vec3::new(1334.0579, -983.55402, 14.827253),
    Vec3::new(1323.2429, -954.23083, 14.954678),
    Vec3::new(1302.7495, -932.21216, 14.962917),
    Vec3::new(1317.418, -905.89325, 14.967506),
    Vec3::new(1337.9503, -883.5025, 14.969675),
    Vec3::new(1352.6929, -855.96954, 14.967854),
    Vec3::new(1357.2388, -826.26971, 14.97295),
    Vec3::new(1384.8668, -812.47693, 12.907736),
    Vec3::new(1410.8983, -795.39056, 12.052228),
    Vec3::new(1433.901, -775.55811, 11.96265),
    Vec3::new(1443.8615, -746.92511, 11.976114),
    Vec3::new(1457.7015, -720.00903, 11.971177),
    Vec3::new(1481.5685, -701.30237, 11.977908),
    Vec3::new(1511.4004, -696.83295, 11.972709),
    Vec3::new(1542.1796, -695.61676, 11.970441),
    Vec3::new(1570.3301, -684.6239, 11.969202),
];

impl World {
    /// The weight the carried gold added is taken off the car again.
    fn reset_pacman_weight(&mut self, veh: EntityRef) {
        let p = self.ent_mut(veh).phys.as_mut().expect("a car is physical");
        let f = p.force_multiplier;
        p.mass /= f;
        p.turn_mass /= f;
        p.force_multiplier = 1.0;
    }

    /// `CPacManPickup::Update`: the player's car touching one picks it up.
    fn update_pacman_pickup(&mut self, px: &mut Px, i: usize) {
        let Some(veh) = self.find_player_vehicle() else { return };
        let pickup = self.ents.pacman.pickups[i];
        let (at, vp) = (pickup.pos, self.epos(veh));
        if (vp.x - at.x).powi(2) + (vp.y - at.y).powi(2) >= 100.0 || !self.is_sphere_touching_vehicle(veh, at, 1.5) {
            return;
        }
        match pickup.ty {
            PacManType::Scramble => {
                if let Some(v) = self.try_veh_mut(veh) {
                    v.pacman_pickups_carried += 1;
                }
                let p = self.ent_mut(veh).phys.as_mut().expect("a car is physical");
                p.move_speed *= 0.65;
                let mult = (p.mass + 250.0) / p.mass;
                p.mass *= mult;
                p.turn_mass *= mult;
                p.force_multiplier *= mult;
                let now = self.now();
                if let Some(w) = self.player_wanted_mut() {
                    w.chaos += 10;
                    w.update_wanted_level(now);
                }
                px.hooks.play_front_end_sound(crate::audio::sound::PICKUP_PACMAN_PACKAGE);
            }
            PacManType::Race => {
                self.ents.pacman.pills_eaten_in_race += 1;
                px.hooks.play_front_end_sound(crate::audio::sound::PICKUP_PACMAN_PILL);
            }
            PacManType::None => {}
        }
        self.ents.pacman.pickups[i].ty = PacManType::None;
        if let Some(o) = self.ents.pacman.pickups[i].object.take() {
            self.remove(o);
            self.delete(o);
        }
    }

    /// `CPacManPickups::Update`.
    pub fn update_pacman_pickups(&mut self, px: &mut Px) {
        if let Some(veh) = self.find_player_vehicle() {
            let here = self.player_info_pos();
            let dist = (here - HAND_IN_POINT).length();
            match self.ents.pacman.collect_game_state {
                1 => {
                    if dist < 10.0 {
                        let carried = std::mem::take(&mut self.try_veh_mut(veh).expect("a car").pacman_pickups_carried);
                        self.ents.pacman.things_to_collect -= carried as i16;
                        self.reset_pacman_weight(veh);
                    }
                    if self.ents.pacman.things_to_collect <= 0 {
                        self.ents.pacman.collect_game_state = 2;
                        self.clear_pm_pickups();
                    }
                }
                2 => {
                    if dist > 11.0 {
                        self.ents.pacman.collect_game_state = 0;
                    }
                }
                20 if (here - self.ents.pacman.last_pick_up_coors).length() > 30.0 => self.ents.pacman.last_pick_up_coors = here,
                _ => {}
            }
        }
        if self.ents.pacman.active {
            let per_frame = NUM_PACMAN_PICKUPS / FRAME_SPAN as usize;
            let first = (self.ents.timer.frame_counter % FRAME_SPAN) as usize * per_frame;
            for i in first..first + per_frame {
                if self.ents.pacman.pickups[i].ty != PacManType::None {
                    self.update_pacman_pickup(px, i);
                }
            }
        }
    }

    /// The gold bar or pill object that turns on its spot.
    fn new_pacman_object(&mut self, px: &mut Px, model: i16, pos: Vec3) -> Option<EntityRef> {
        let obj = self.new_object(model, Matrix::from_translation(pos), px.rng)?;
        if let Kind::Object(o) = &mut self.ent_mut(obj).kind {
            o.created_by = CreatedBy::Mission;
            o.is_pickup = false;
        }
        let e = self.ent_mut(obj);
        e.base.set_orientation(0.0, 0.0, -FRAC_PI_2);
        e.base.flags.explosion_proof = true;
        e.base.flags.uses_collision = false;
        if let Some(p) = e.phys.as_mut() {
            p.flags.affected_by_gravity = false;
        }
        let _ = self.add(obj);
        Some(obj)
    }

    fn free_pacman_slot(&self) -> Option<usize> {
        self.ents.pacman.pickups.iter().position(|p| p.ty == PacManType::None)
    }

    /// `CPacManPickups::GeneratePMPickUps`: `count` gold bars on roads within `scramble_mult` of `pos`.
    pub fn generate_pm_pickups(&mut self, px: &mut Px, pos: Vec3, scramble_mult: f32, count: i16, ty: PacManType) {
        let model = self.ents.indices.gta3_slot(slot::BULLION);
        for _ in 0..count.max(0) {
            let Some(i) = self.free_pacman_slot() else { break };
            for _ in 0..MAX_TRIES {
                let r = px.rng.number();
                let mut at = pos;
                at.x += ((r & 0xFF) as f32 - 128.0) * scramble_mult / 128.0;
                at.y += (((r >> 8) & 0xFF) as f32 - 128.0) * scramble_mult / 128.0;
                at.z = 1000.0;
                let kinds = Kinds { buildings: true, dummies: true, ..Kinds::NONE };
                let Some(hit) = self.process_vertical_line(at, -1000.0, kinds, false, None) else { continue };
                if hit.entity.pool != PoolKind::Treadable {
                    continue;
                }
                at.z = 0.7 + hit.point.point.z;
                let object = self.new_pacman_object(px, model, at);
                self.ents.pacman.pickups[i] = PacManPickup { ty, pos: at, object };
                break;
            }
        }
        self.ents.pacman.active = true;
    }

    /// `CPacManPickups::GeneratePMPickUpsForRace`.
    pub fn generate_pm_pickups_for_race(&mut self, px: &mut Px, race: i32) {
        assert_eq!(race, 0, "there is only one race");
        let model = self.ents.indices.gta3_slot(slot::DONKEYMAG);
        for &pos in &RACE_POINTS {
            let Some(i) = self.free_pacman_slot() else { break };
            let object = self.new_pacman_object(px, model, pos);
            self.ents.pacman.pickups[i] = PacManPickup { ty: PacManType::Race, pos, object };
        }
        self.ents.pacman.active = true;
    }

    /// `CPacManPickups::GenerateOnePMPickUp`: a pill with nothing to draw.
    pub fn generate_one_pm_pickup(&mut self, pos: Vec3) {
        self.ents.pacman.active = true;
        let first = &mut self.ents.pacman.pickups[0];
        (first.ty, first.pos) = (PacManType::Race, pos);
    }

    /// `CPacManPickups::ClearPMPickUps`.
    pub fn clear_pm_pickups(&mut self) {
        self.ents.pacman.active = false;
        for i in 0..NUM_PACMAN_PICKUPS {
            if let Some(o) = self.ents.pacman.pickups[i].object.take() {
                self.remove(o);
                self.delete(o);
            }
            self.ents.pacman.pickups[i].ty = PacManType::None;
        }
    }

    /// `CPacManPickups::StartPacManRace`.
    pub fn start_pacman_race(&mut self, px: &mut Px, race: i32) {
        self.generate_pm_pickups_for_race(px, race);
        self.ents.pacman.pills_eaten_in_race = 0;
    }

    /// `CPacManPickups::StartPacManRecord`.
    pub fn start_pacman_record(&mut self) {
        self.ents.pacman.collect_game_state = 20;
        self.ents.pacman.last_pick_up_coors = self.player_info_pos();
    }

    /// `CPacManPickups::QueryPowerPillsCarriedByPlayer`.
    pub fn power_pills_carried_by_player(&self) -> i32 {
        self.find_player_vehicle().and_then(|v| self.try_veh(v)).map_or(0, |v| i32::from(v.pacman_pickups_carried))
    }

    /// `CPacManPickups::ResetPowerPillsCarriedByPlayer`.
    pub fn reset_power_pills_carried_by_player(&mut self) {
        let Some(veh) = self.find_player_vehicle() else { return };
        self.try_veh_mut(veh).expect("a car").pacman_pickups_carried = 0;
        self.reset_pacman_weight(veh);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::general::Rng;
    use crate::ped::ctx::NoPedHooks;
    use crate::ped::types::ped_type;
    use crate::vehicle::base::RANDOM_VEHICLE;
    use crate::world::tests::{world_with_cars, LANDSTAL_MODEL};

    fn player_in_car() -> (World, EntityRef) {
        let mut w = world_with_cars();
        let mut rng = Rng::default();
        let car = w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, &mut rng).unwrap();
        w.ent_mut(car).base.set_position(Vec3::new(10.0, 10.0, 0.0));
        let p = w.create_ped(&mut Px { rng: &mut rng, hooks: &mut NoPedHooks }, ped_type::PLAYER1, -1, None).unwrap();
        w.ped_mut(p).player = Some(Default::default());
        w.ents.player_ped = Some(p);
        let pd = w.ped_mut(p);
        (pd.in_vehicle, pd.my_vehicle) = (true, Some(car));
        (w, car)
    }

    #[test]
    fn a_pill_under_the_players_car_is_eaten() {
        let (mut w, _car) = player_in_car();
        w.generate_one_pm_pickup(Vec3::new(10.0, 10.0, 0.0));
        w.ents.timer.frame_counter = 0;
        w.update_pacman_pickups(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks });
        assert_eq!((w.ents.pacman.pills_eaten_in_race, w.ents.pacman.pickups[0].ty), (1, PacManType::None));
    }

    #[test]
    fn gold_makes_the_car_heavier_until_the_player_lets_go_of_it() {
        let (mut w, car) = player_in_car();
        let before = w.ent(car).phys.as_ref().unwrap().mass;
        w.ents.pacman.pickups[0] = PacManPickup { ty: PacManType::Scramble, pos: Vec3::new(10.0, 10.0, 0.0), object: None };
        w.ents.pacman.active = true;
        w.ents.timer.frame_counter = 0;
        w.update_pacman_pickups(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks });
        assert_eq!(w.power_pills_carried_by_player(), 1);
        let heavy = w.ent(car).phys.as_ref().unwrap().mass;
        assert!((heavy - (before + 250.0)).abs() < 1e-2, "{heavy}");
        w.reset_power_pills_carried_by_player();
        let p = w.ent(car).phys.as_ref().unwrap();
        assert!((p.mass - before).abs() < 1e-2 && p.force_multiplier == 1.0);
        assert_eq!(w.power_pills_carried_by_player(), 0);
    }

    #[test]
    fn the_race_has_its_pills_and_clearing_removes_them() {
        let mut w = world_with_cars();
        w.ents.pacman.pickups[0].ty = PacManType::None;
        w.generate_pm_pickups_for_race(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, 0);
        let pills = w.ents.pacman.pickups.iter().filter(|p| p.ty == PacManType::Race).count();
        assert_eq!((pills, w.ents.pacman.active), (RACE_POINTS.len(), true));
        w.clear_pm_pickups();
        assert!(w.ents.pacman.pickups.iter().all(|p| p.ty == PacManType::None) && !w.ents.pacman.active);
    }
}
