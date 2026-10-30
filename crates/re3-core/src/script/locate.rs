//! `LocatePlayerCommand` and its relatives: is the player, a char or a car in the box around a point, or around another entity.
use super::locate_table::{AREA_SPECS, SPECS};
use super::{RunningScript, ScriptHooks};
use crate::anim::tables_gta3::anim;
use crate::entity::{EntityRef, PoolKind};
use crate::ped::data::move_state;
use crate::world::World;
use glam::Vec3;

const MAP_Z_LOW_LIMIT: f32 = -100.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    Player,
    Char,
    Car,
}

/// What the subject has to be doing as well as being in the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Any,
    OnFoot,
    InCar,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Coords,
    Char,
    Car,
    Object,
    /// A box given by two corners, in any order.
    Area,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub stopped: bool,
    pub subject: Subject,
    pub mode: Mode,
    pub target: Target,
    pub three_d: bool,
}

pub fn spec_of(command: u16) -> Option<Spec> {
    SPECS.iter().chain(AREA_SPECS.iter()).find(|(id, _)| *id == command).map(|(_, s)| *s)
}

impl World {
    /// `IsVehicleStopped`.
    pub fn is_vehicle_stopped(&self, v: EntityRef) -> bool {
        self.ent(v).phys.as_ref().is_some_and(|p| 0.01 * self.ents.timer.time_step >= p.distance_travelled)
    }

    /// `IsPedStopped`.
    pub fn is_ped_stopped(&self, ped: EntityRef) -> bool {
        let pd = self.ped(ped);
        match pd.my_vehicle.filter(|_| pd.in_vehicle) {
            Some(v) => self.is_vehicle_stopped(v),
            None => matches!(pd.move_state, move_state::NONE | move_state::STILL),
        }
    }

    /// `IsPlayerStopped`: the player also counts as moving while stopping from a run or in a jump.
    pub fn is_player_stopped(&self) -> bool {
        let Some(p) = self.ents.player_ped else { return true };
        if self.ped(p).in_vehicle {
            return self.ped(p).my_vehicle.is_none_or(|v| self.is_vehicle_stopped(v));
        }
        if [anim::STD_RUNSTOP1, anim::STD_RUNSTOP2, anim::STD_JUMP_LAUNCH, anim::STD_JUMP_GLIDE].into_iter().any(|a| self.ped_assoc(p, a).is_some()) {
            return false;
        }
        matches!(self.ped(p).move_state, move_state::NONE | move_state::STILL)
    }

    /// The car of a ped in one, else the ped: where it is for the purposes of a script.
    fn locate_position_of_ped(&self, ped: EntityRef) -> Vec3 {
        let pd = self.ped(ped);
        self.epos(pd.my_vehicle.filter(|_| pd.in_vehicle).unwrap_or(ped))
    }

    /// One of the `LOCATE_*` commands.
    pub(crate) fn process_locate_command(&mut self, sh: &mut dyn ScriptHooks, s: &mut RunningScript, spec: Spec) {
        if spec.target == Target::Area {
            return self.process_area_command(sh, s, spec);
        }
        let entity_target = spec.target != Target::Coords;
        let n = match (entity_target, spec.three_d) {
            (false, false) => 6,
            (false, true) => 8,
            (true, false) => 5,
            (true, true) => 6,
        };
        self.collect(s, n);
        // who is being asked about, and where they are
        let (subject_ped, pos, stopped) = self.locate_subject(spec.subject, self.sp(0));
        // the middle of the box and its half sizes
        let centre = if entity_target {
            match spec.target {
                Target::Char => self.locate_position_of_ped(self.script_ped(self.sp(1)).expect("LOCATE: no such target ped")),
                Target::Car => self.epos(self.script_vehicle(self.sp(1)).expect("LOCATE: no such target car")),
                _ => self.epos(EntityRef { pool: PoolKind::Object, handle: self.sp(1) }),
            }
        } else {
            self.spv(1)
        };
        let (d, debug) = if entity_target {
            if spec.three_d { (Vec3::new(self.spf(2), self.spf(3), self.spf(4)), self.sp(5)) } else { (Vec3::new(self.spf(2), self.spf(3), 0.0), self.sp(4)) }
        } else if spec.three_d {
            (Vec3::new(self.spf(4), self.spf(5), self.spf(6)), self.sp(7))
        } else {
            (Vec3::new(self.spf(3), self.spf(4), 0.0), self.sp(5))
        };
        let result = if spec.stopped && !entity_target && !stopped {
            false
        } else {
            let in_area = centre.x - d.x <= pos.x && centre.x + d.x >= pos.x && centre.y - d.y <= pos.y && centre.y + d.y >= pos.y && (!spec.three_d || centre.z - d.z <= pos.z && centre.z + d.z >= pos.z);
            let in_vehicle = subject_ped.is_some_and(|p| self.ped(p).in_vehicle);
            in_area
                && match spec.mode {
                    Mode::Any | Mode::None => true,
                    Mode::OnFoot => !in_vehicle,
                    Mode::InCar => in_vehicle,
                }
        };
        s.update_compare_flag(result);
        if debug != 0 {
            sh.highlight_important_area(s.ip, centre.x - d.x, centre.y - d.y, centre.x + d.x, centre.y + d.y, if spec.three_d { centre.z } else { MAP_Z_LOW_LIMIT });
        }
    }

    /// The subject of a locate command: its ped (None for a car), where it is and whether it has stopped.
    fn locate_subject(&self, subject: Subject, handle: i32) -> (Option<EntityRef>, Vec3, bool) {
        match subject {
            Subject::Player => {
                let p = self.ents.player_ped.expect("LOCATE: no player");
                (Some(p), self.player_info_pos(), self.is_player_stopped())
            }
            Subject::Char => {
                let p = self.script_ped(handle).expect("LOCATE: no such ped");
                (Some(p), self.locate_position_of_ped(p), self.is_ped_stopped(p))
            }
            Subject::Car => {
                let v = self.script_vehicle(handle).expect("LOCATE: no such car");
                (None, self.epos(v), self.is_vehicle_stopped(v))
            }
        }
    }

    /// `PlayerInAreaCheckCommand` and its char and car versions.
    fn process_area_command(&mut self, sh: &mut dyn ScriptHooks, s: &mut RunningScript, spec: Spec) {
        self.collect(s, if spec.three_d { 8 } else { 6 });
        let (ped, pos, stopped) = self.locate_subject(spec.subject, self.sp(0));
        let (a, b, debug) = if spec.three_d { (self.spv(1), self.spv(4), self.sp(7)) } else { (Vec3::new(self.spf(1), self.spf(2), 0.0), Vec3::new(self.spf(3), self.spf(4), 0.0), self.sp(5)) };
        let (lo, hi) = (a.min(b), a.max(b));
        let result = if spec.stopped && !stopped {
            false
        } else {
            let inside = lo.x <= pos.x && hi.x >= pos.x && lo.y <= pos.y && hi.y >= pos.y && (!spec.three_d || lo.z <= pos.z && hi.z >= pos.z);
            let in_vehicle = ped.is_some_and(|p| self.ped(p).in_vehicle);
            inside
                && match spec.mode {
                    Mode::Any | Mode::None => true,
                    Mode::OnFoot => !in_vehicle,
                    Mode::InCar => in_vehicle,
                }
        };
        s.update_compare_flag(result);
        if debug != 0 {
            sh.highlight_important_area(s.ip, lo.x, lo.y, hi.x, hi.y, if spec.three_d { (lo.z + hi.z) / 2.0 } else { MAP_Z_LOW_LIMIT });
        }
    }

    /// `LOCATE_SNIPER_BULLET_2D` and `_3D`.
    pub(crate) fn process_locate_sniper_bullet(&mut self, sh: &mut dyn ScriptHooks, s: &mut RunningScript, three_d: bool) {
        self.collect(s, if three_d { 7 } else { 5 });
        let (x, y) = (self.spf(0), self.spf(1));
        let (z, dx, dy, dz, debug) = if three_d { (self.spf(2), self.spf(3), self.spf(4), self.spf(5), self.sp(6)) } else { (0.0, self.spf(2), self.spf(3), 0.0, self.sp(4)) };
        let (zmin, zmax) = if three_d { (z - dz, z + dz) } else { (-1000.0, 1000.0) };
        let hit = sh.test_for_sniper_bullet(x - dx, x + dx, y - dy, y + dy, zmin, zmax);
        s.update_compare_flag(hit);
        if debug != 0 {
            sh.highlight_important_area(s.ip, x - dx, y - dy, x + dx, y + dy, if three_d { z } else { MAP_Z_LOW_LIMIT });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::commands as c;
    use super::super::process::tests::{run_frame, start, world_with_script, Asm, A};
    use super::*;
    use crate::general::Rng;
    use crate::ped::ctx::{NoPedHooks, Px};
    use crate::ped::types::ped_type;
    use crate::world::tests::LANDSTAL_MODEL;

    #[derive(Default)]
    struct Spy {
        bullet_boxes: Vec<(f32, f32, f32, f32, f32, f32)>,
        highlights: usize,
    }
    impl ScriptHooks for Spy {
        fn test_for_sniper_bullet(&mut self, x1: f32, x2: f32, y1: f32, y2: f32, z1: f32, z2: f32) -> bool {
            self.bullet_boxes.push((x1, x2, y1, y2, z1, z2));
            true
        }
        fn highlight_important_area(&mut self, _id: u32, _x1: f32, _y1: f32, _x2: f32, _y2: f32, _z: f32) {
            self.highlights += 1;
        }
    }

    fn world(a: &Asm) -> (World, EntityRef) {
        let mut w = world_with_script(a);
        let p = w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped_type::PLAYER1, -1, None).unwrap();
        w.ped_mut(p).player = Some(Default::default());
        w.ents.player_ped = Some(p);
        w.get_mut(p).unwrap().base.set_position(Vec3::new(10.0, 10.0, 1.0));
        (w, p)
    }

    /// Runs the script and tells what its last test came out as.
    fn result(w: &mut World, spy: &mut Spy) -> bool {
        let i = start(w);
        run_frame(w, spy);
        w.ents.scripts.scripts[i].cond_result
    }

    #[test]
    fn the_table_knows_every_locate_command_by_its_name() {
        let s = spec_of(c::LOCATE_STOPPED_PLAYER_ON_FOOT_3D).unwrap();
        assert_eq!((s.stopped, s.subject, s.mode, s.target, s.three_d), (true, Subject::Player, Mode::OnFoot, Target::Coords, true));
        let s = spec_of(c::LOCATE_CHAR_IN_CAR_CAR_2D).unwrap();
        assert_eq!((s.stopped, s.subject, s.mode, s.target, s.three_d), (false, Subject::Char, Mode::InCar, Target::Car, false));
        let s = spec_of(c::LOCATE_CAR_3D).unwrap();
        assert_eq!((s.subject, s.mode, s.target), (Subject::Car, Mode::None, Target::Coords));
        assert!(spec_of(c::LOCATE_CHAR_ANY_MEANS_OBJECT_2D).is_some() && spec_of(c::WAIT).is_none());
    }

    #[test]
    fn the_player_in_a_box_on_foot_or_in_a_car() {
        let mut a = Asm::new(0);
        a.cmd(c::LOCATE_PLAYER_ON_FOOT_2D, &[A::I8(0), A::F(12.0), A::F(10.0), A::F(3.0), A::F(1.0), A::I8(1)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let (mut w, p) = world(&a);
        w.ents.scripts.space[..a.bytes.len()].copy_from_slice(&a.bytes);
        let mut spy = Spy::default();
        assert!(result(&mut w, &mut spy));
        assert_eq!(spy.highlights, 1);
        // in a car the same box does not do for ON_FOOT
        w.ped_mut(p).in_vehicle = true;
        assert!(!result(&mut w, &mut spy));
        // outside the box
        w.ped_mut(p).in_vehicle = false;
        w.get_mut(p).unwrap().base.set_position(Vec3::new(10.0, 12.5, 1.0));
        assert!(!result(&mut w, &mut spy));
    }

    #[test]
    fn a_stopped_test_needs_the_player_to_have_stopped() {
        let mut a = Asm::new(0);
        a.cmd(c::LOCATE_STOPPED_PLAYER_ANY_MEANS_3D, &[A::I8(0), A::F(10.0), A::F(10.0), A::F(1.0), A::F(2.0), A::F(2.0), A::F(2.0), A::I8(0)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let (mut w, p) = world(&a);
        w.ents.scripts.space[..a.bytes.len()].copy_from_slice(&a.bytes);
        let mut spy = Spy::default();
        assert!(result(&mut w, &mut spy));
        w.ped_mut(p).move_state = move_state::RUN;
        assert!(!result(&mut w, &mut spy));
    }

    #[test]
    fn a_char_is_judged_by_its_car_and_a_target_char_by_its_car_too() {
        let mut a = Asm::new(4);
        a.cmd(c::LOCATE_CHAR_ANY_MEANS_CHAR_2D, &[A::G(8), A::G(12), A::F(3.0), A::F(3.0), A::I8(0)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let (mut w, _) = world(&a);
        w.ents.scripts.space[..a.bytes.len()].copy_from_slice(&a.bytes);
        let mut rng = Rng::default();
        let mut mk = |w: &mut World, pos: Vec3| {
            let ped = w.create_ped(&mut Px { rng: &mut rng, hooks: &mut NoPedHooks }, ped_type::CIVMALE, -1, None).unwrap();
            w.get_mut(ped).unwrap().base.set_position(pos);
            ped
        };
        let (a_ped, b_ped) = (mk(&mut w, Vec3::new(0.0, 0.0, 0.0)), mk(&mut w, Vec3::new(50.0, 0.0, 0.0)));
        w.ents.scripts.set_global(8, a_ped.handle);
        w.ents.scripts.set_global(12, b_ped.handle);
        let mut spy = Spy::default();
        assert!(!result(&mut w, &mut spy));
        // b gets into a car that is next to a
        let car = w.new_car(LANDSTAL_MODEL, crate::vehicle::base::RANDOM_VEHICLE, &mut Rng::default()).unwrap();
        w.get_mut(car).unwrap().base.set_position(Vec3::new(1.0, 1.0, 0.0));
        let pd = w.ped_mut(b_ped);
        (pd.in_vehicle, pd.my_vehicle) = (true, Some(car));
        assert!(result(&mut w, &mut spy));
    }

    #[test]
    fn a_car_in_a_box_and_a_sniper_bullet_through_one() {
        let mut a = Asm::new(4);
        a.cmd(c::LOCATE_CAR_2D, &[A::G(8), A::F(5.0), A::F(5.0), A::F(2.0), A::F(2.0), A::I8(0)]);
        let j = a.here() as usize + 3;
        a.cmd(c::GOTO_IF_FALSE, &[A::I32(0)]);
        a.cmd(c::LOCATE_SNIPER_BULLET_3D, &[A::F(1.0), A::F(2.0), A::F(3.0), A::F(4.0), A::F(5.0), A::F(6.0), A::I8(0)]);
        let end = a.here();
        a.cmd(c::WAIT, &[A::I32(100000)]);
        a.bytes[j..j + 4].copy_from_slice(&end.to_le_bytes());
        let (mut w, _) = world(&a);
        w.ents.scripts.space[..a.bytes.len()].copy_from_slice(&a.bytes);
        let car = w.new_car(LANDSTAL_MODEL, crate::vehicle::base::RANDOM_VEHICLE, &mut Rng::default()).unwrap();
        w.get_mut(car).unwrap().base.set_position(Vec3::new(6.0, 4.0, 0.0));
        w.ents.scripts.set_global(8, car.handle);
        let mut spy = Spy::default();
        assert!(result(&mut w, &mut spy));
        // centre (1, 2, 3) with half sizes (4, 5, 6)
        assert_eq!(spy.bullet_boxes, vec![(-3.0, 5.0, -3.0, 7.0, -3.0, 9.0)]);
    }
}
