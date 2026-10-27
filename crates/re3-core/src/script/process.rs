//! `CTheScripts::Process`, `CRunningScript::Process` and the commands 0 to 99 (variables, tests, jumps, the player's position).
use super::commands as c;
use super::{bits_of, f32_of, jump_target, RunningScript, ScriptHooks, MAX_STACK_DEPTH};
use crate::entity::EntityRef;
use crate::ped::ctx::Px;
use crate::player_info::wb_state;
use crate::world::World;
use glam::Vec3;

const MAP_Z_LOW_LIMIT: f32 = -100.0;

impl World {
    // ---- parameters ----

    pub(crate) fn collect(&mut self, s: &mut RunningScript, n: usize) {
        self.ents.scripts.collect_parameters(s, n);
    }

    pub(crate) fn sp(&self, i: usize) -> i32 {
        self.ents.scripts.params[i]
    }

    pub(crate) fn spf(&self, i: usize) -> f32 {
        self.ents.scripts.param_f(i)
    }

    pub(crate) fn spv(&self, i: usize) -> Vec3 {
        self.ents.scripts.param_v3(i)
    }

    pub(crate) fn store(&mut self, s: &mut RunningScript, n: usize) {
        self.ents.scripts.store_parameters(s, n);
    }

    /// `CTheScripts::UpdateObjectIndices`: each name the script uses is matched with a model of that name.
    pub fn update_script_object_indices(&mut self) {
        for i in 1..self.ents.scripts.used_objects.len() {
            let name = self.ents.scripts.used_objects[i].0.clone();
            if let Some((id, _)) = self.ents.models.find_by_name(&name.to_ascii_lowercase()) {
                self.ents.scripts.used_objects[i].1 = id;
            }
        }
    }

    // ---- the loop ----

    /// `CTheScripts::Process`: every active script runs until it waits.
    pub fn process_scripts(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks) {
        if self.ents.timer.playing_back {
            return;
        }
        self.ents.scripts.commands_executed = 0;
        self.ents.scripts.scripts_updated = 0;
        let time_step = self.ents.timer.time_step / 50.0 * 1000.0;
        self.update_upside_down_timers();
        self.process_stuck_cars();
        let sc = &mut self.ents.scripts;
        sc.fail_current_mission = sc.fail_current_mission.saturating_sub(1);
        let mut cur = self.ents.scripts.active;
        while let Some(i) = cur {
            let next = self.ents.scripts.next_of(i);
            self.ents.scripts.scripts_updated += 1;
            let mut s = std::mem::take(&mut self.ents.scripts.scripts[i]);
            s.update_timers(time_step);
            self.process_script(px, sh, i, &mut s);
            self.ents.scripts.scripts[i] = s;
            cur = next;
        }
    }

    /// `CRunningScript::Process`.
    fn process_script(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, idx: usize, s: &mut RunningScript) {
        if s.is_mission_script {
            self.do_deatharrest_check(px, sh, s);
        }
        if s.mission_flag && self.ents.scripts.fail_current_mission == 1 && s.stack_pointer == 1 {
            s.stack_pointer -= 1;
            s.ip = s.stack[s.stack_pointer as usize];
        }
        if self.now() >= s.wake_time {
            while self.process_one_command(px, sh, idx, s) == 0 {}
            return;
        }
        if !s.skip_wake_time || !sh.cross_just_down() {
            return;
        }
        s.wake_time = 0;
        sh.skip_current_messages();
    }

    /// `CRunningScript::DoDeatharrestCheck`: a mission is cut short when the player dies or is arrested.
    fn do_deatharrest_check(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript) {
        if !s.deatharrest_enabled || !self.ents.scripts.is_player_on_a_mission() {
            return;
        }
        let wb = self.ents.player_info.wb_state;
        let (died, arrested) = (wb == wb_state::WASTED, wb == wb_state::BUSTED);
        if !died && !arrested && !self.ents.script_checks.upside_down.are_any_cars_upside_down() {
            return;
        }
        debug_assert!(s.stack_pointer > 0);
        s.stack_pointer = s.stack_pointer.min(1);
        s.stack_pointer -= 1;
        s.ip = s.stack[s.stack_pointer as usize];
        // the original works out which of the contact's messages to show and then shows none
        px.rng.range_i32(0, 5);
        sh.clear_small_messages_only();
        let flag = self.ents.scripts.on_a_mission_flag as usize;
        self.ents.scripts.space[flag..flag + 4].copy_from_slice(&0i32.to_le_bytes());
        s.deatharrest_executed = true;
        s.wake_time = 0;
    }

    /// `CRunningScript::ProcessOneCommand`: 0 to go on with the next command, 1 to stop for now.
    fn process_one_command(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, idx: usize, s: &mut RunningScript) -> i8 {
        self.ents.scripts.commands_executed += 1;
        let raw = self.ents.scripts.read2(&mut s.ip) as u16;
        s.not_flag = raw & 0x8000 != 0;
        let command = raw & 0x7FFF;
        if let Some(spec) = super::locate::spec_of(command) {
            self.process_locate_command(sh, s, spec);
            return 0;
        }
        if command == c::LOCATE_SNIPER_BULLET_2D || command == c::LOCATE_SNIPER_BULLET_3D {
            self.process_locate_sniper_bullet(sh, s, command == c::LOCATE_SNIPER_BULLET_3D);
            return 0;
        }
        match command {
            0..=99 => self.process_commands_0_to_99(px, sh, idx, s, command),
            100..=119 => {
                self.arithmetic_between_variables(s, command);
                0
            }
            120..=199 => self.process_commands_100_to_199(px, sh, s, command),
            200..=299 => self.process_commands_200_to_299(px, sh, s, command),
            300..=399 => self.process_commands_300_to_399(px, sh, s, command),
            _ => panic!("script command {command} is not ported (script {}, at {})", s.name_str(), s.ip),
        }
    }

    /// The commands that change a variable by another variable: `ADD`, `SUB`, `MULT` and `DIV`, eight of each.
    fn arithmetic_between_variables(&mut self, s: &mut RunningScript, command: u16) {
        let index = command - c::ADD_INT_VAR_TO_INT_VAR;
        let (op, float) = (index / 8, index % 2 == 1);
        let sc = &mut self.ents.scripts;
        let dest = sc.read_operand(&mut s.ip);
        let src = sc.read_operand(&mut s.ip);
        let (a, b) = (sc.value_of(s, dest), sc.value_of(s, src));
        let r = if float {
            let (a, b) = (f32_of(a), f32_of(b));
            bits_of(match op {
                0 => a + b,
                1 => a - b,
                2 => a * b,
                _ => a / b,
            })
        } else {
            match op {
                0 => a.wrapping_add(b),
                1 => a.wrapping_sub(b),
                2 => a.wrapping_mul(b),
                _ => a.checked_div(b).unwrap_or(0),
            }
        };
        sc.store_operand(s, dest, r);
    }

    /// `SET`, `ADD`, `SUB`, `MULT` and `DIV` of a variable by a number (commands 4 to 23).
    fn arithmetic_with_number(&mut self, s: &mut RunningScript, command: u16) {
        let index = command - c::SET_VAR_INT;
        let (op, float) = (index / 4, index % 2 == 1);
        let dest = self.ents.scripts.read_operand(&mut s.ip);
        self.collect(s, 1);
        let sc = &mut self.ents.scripts;
        let (a, b) = (sc.value_of(s, dest), sc.params[0]);
        let r = if float {
            let (a, b) = (f32_of(a), f32_of(b));
            bits_of(match op {
                0 => b,
                1 => a + b,
                2 => a - b,
                3 => a * b,
                _ => a / b,
            })
        } else {
            match op {
                0 => b,
                1 => a.wrapping_add(b),
                2 => a.wrapping_sub(b),
                3 => a.wrapping_mul(b),
                _ => a.checked_div(b).unwrap_or(0),
            }
        };
        sc.store_operand(s, dest, r);
    }

    /// The tests of two numbers (commands 24 to 75): the two arguments are variables or numbers in any mix.
    fn compare_numbers(&mut self, s: &mut RunningScript, command: u16) {
        let sc = &mut self.ents.scripts;
        let (x, y) = (sc.read_operand(&mut s.ip), sc.read_operand(&mut s.ip));
        let (a, b) = (sc.value_of(s, x), sc.value_of(s, y));
        let (fa, fb) = (f32_of(a), f32_of(b));
        let r = match command {
            24..=31 => a > b,
            32..=39 => fa > fb,
            40..=47 => a >= b,
            48..=55 => fa >= fb,
            56..=60 => a == b,
            61..=65 => a != b,
            66..=70 => fa == fb,
            _ => fa != fb,
        };
        s.update_compare_flag(r);
    }

    fn process_commands_0_to_99(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, idx: usize, s: &mut RunningScript, command: u16) -> i8 {
        match command {
            c::NOP => 0,
            c::WAIT => {
                self.collect(s, 1);
                s.wake_time = self.now().wrapping_add(self.sp(0) as u32);
                s.skip_wake_time = false;
                1
            }
            c::GOTO => {
                self.collect(s, 1);
                s.ip = jump_target(self.sp(0));
                0
            }
            c::SHAKE_CAM => {
                self.collect(s, 1);
                sh.camera_shake(self.sp(0) as f32 / 1000.0);
                0
            }
            c::SET_VAR_INT..=c::DIV_FLOAT_LVAR_BY_VAL => {
                self.arithmetic_with_number(s, command);
                0
            }
            c::IS_INT_VAR_GREATER_THAN_NUMBER..=c::IS_FLOAT_VAR_NOT_EQUAL_TO_FLOAT_LVAR => {
                self.compare_numbers(s, command);
                0
            }
            c::GOTO_IF_TRUE | c::GOTO_IF_FALSE => {
                self.collect(s, 1);
                if s.cond_result == (command == c::GOTO_IF_TRUE) {
                    s.ip = jump_target(self.sp(0));
                }
                0
            }
            c::TERMINATE_THIS_SCRIPT => {
                if s.mission_flag {
                    self.ents.scripts.already_running_a_mission_script = false;
                }
                self.ents.scripts.remove_from_list(idx, true);
                self.ents.scripts.add_to_list(idx, false);
                1
            }
            c::START_NEW_SCRIPT => {
                self.collect(s, 1);
                debug_assert!(self.sp(0) >= 0);
                let new = self.ents.scripts.start_new_script(self.sp(0) as u32);
                let sc = &mut self.ents.scripts;
                let mut i = 0;
                // the arguments run to an end marker and become the locals of the new script
                while sc.space[s.ip as usize] != super::arg::END {
                    let o = sc.read_operand(&mut s.ip);
                    let v = sc.value_of(s, o);
                    sc.scripts[new].locals[i] = v;
                    i += 1;
                }
                s.ip += 1;
                0
            }
            c::GOSUB => {
                self.collect(s, 1);
                debug_assert!((s.stack_pointer as usize) < MAX_STACK_DEPTH);
                s.stack[s.stack_pointer as usize] = s.ip;
                s.stack_pointer += 1;
                s.ip = jump_target(self.sp(0));
                0
            }
            c::RETURN => {
                debug_assert!(s.stack_pointer > 0);
                s.stack_pointer -= 1;
                s.ip = s.stack[s.stack_pointer as usize];
                0
            }
            c::LINE => {
                self.collect(s, 6);
                0
            }
            c::CREATE_PLAYER => {
                self.collect(s, 4);
                self.script_create_player(px, sh, s);
                0
            }
            c::GET_PLAYER_COORDINATES => {
                self.collect(s, 1);
                let pos = self.player_info_pos();
                self.ents.scripts.set_param_v3(0, pos);
                self.store(s, 3);
                0
            }
            c::SET_PLAYER_COORDINATES => {
                self.collect(s, 4);
                self.script_set_player_coordinates(px);
                0
            }
            c::IS_PLAYER_IN_AREA_2D => {
                self.collect(s, 6);
                let (x1, y1, x2, y2) = (self.spf(1), self.spf(2), self.spf(3), self.spf(4));
                let inside = self.player_vehicle_or_ped().is_some_and(|e| self.ent(e).base.is_within_area_2d(x1, y1, x2, y2));
                s.update_compare_flag(inside);
                if self.sp(5) != 0 {
                    sh.highlight_important_area(s.ip, x1, y1, x2, y2, MAP_Z_LOW_LIMIT);
                }
                0
            }
            c::IS_PLAYER_IN_AREA_3D => {
                self.collect(s, 8);
                let (a, b) = (Vec3::new(self.spf(1), self.spf(2), self.spf(3)), Vec3::new(self.spf(4), self.spf(5), self.spf(6)));
                let inside = self.player_vehicle_or_ped().is_some_and(|e| self.ent(e).base.is_within_area(a, b));
                s.update_compare_flag(inside);
                if self.sp(7) != 0 {
                    sh.highlight_important_area(s.ip, a.x, a.y, b.x, b.y, (a.z + b.z) / 2.0);
                }
                0
            }
            c::ADD_INT_VAR_TO_INT_VAR..=c::SUB_FLOAT_LVAR_FROM_FLOAT_LVAR => {
                self.arithmetic_between_variables(s, command);
                0
            }
            _ => panic!("script command {command} is not ported (script {}, at {})", s.name_str(), s.ip),
        }
    }

    /// The car the player is in, or the player.
    fn player_vehicle_or_ped(&self) -> Option<EntityRef> {
        self.find_player_vehicle().or(self.ents.player_ped)
    }

    /// `COMMAND_CREATE_PLAYER`: only the player in slot 0 exists.
    fn script_create_player(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript) {
        let index = self.sp(0);
        debug_assert!(index < 1);
        sh.load_player_model();
        let Ok(p) = self.setup_player_ped(px, None) else { return };
        self.ped_mut(p).char_created_by = crate::ped::data::MISSION_CHAR;
        self.deactivate_player_ped();
        let mut pos = self.spv(1);
        if pos.z <= MAP_Z_LOW_LIMIT {
            pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
        }
        pos.z += self.base_offset_of(p);
        self.get_mut(p).expect("player").base.set_position(pos);
        px.hooks.clear_space_for_mission_entity(pos, p);
        let _ = self.reactivate_player_ped();
        self.ents.scripts.params[0] = index;
        self.store(s, 1);
    }

    /// `GetDistanceFromCentreOfMassToBaseOfModel`.
    pub(crate) fn base_offset_of(&self, r: EntityRef) -> f32 {
        self.col_model_of(r).map_or(0.0, |c| self.ent(r).base.distance_from_centre_of_mass_to_base_of_model(&c))
    }

    /// `COMMAND_SET_PLAYER_COORDINATES`.
    fn script_set_player_coordinates(&mut self, px: &mut Px) {
        let mut pos = self.spv(1);
        if pos.z <= MAP_Z_LOW_LIMIT {
            pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
        }
        let Some(p) = self.ents.player_ped else { return };
        match self.find_player_vehicle() {
            None => {
                pos.z += self.base_offset_of(p);
                self.teleport(p, pos);
                px.hooks.clear_space_for_mission_entity(pos, p);
            }
            Some(v) => {
                pos.z += self.base_offset_of(v);
                if self.ents.indices.is_boat_model(self.ent(v).base.model_index) {
                    self.teleport_boat(v, pos);
                } else {
                    self.teleport_car(v, pos);
                }
                px.hooks.clear_space_for_mission_entity(pos, v);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::general::Rng;
    use crate::ped::ctx::NoPedHooks;
    use crate::script::ScriptHooks;

    /// One argument of a command, as it is written in the bytecode.
    #[derive(Clone, Copy)]
    pub(crate) enum A {
        I32(i32),
        I8(i8),
        I16(i16),
        F(f32),
        G(u16),
        L(u16),
    }

    /// A tiny assembler for test scripts.
    pub(crate) struct Asm {
        pub bytes: Vec<u8>,
    }

    impl Asm {
        /// A script with room for `vars` global variables (from offset 8), code from the offset after them.
        pub fn new(vars: usize) -> Self {
            let mut a = Self { bytes: vec![] };
            let start = 8 + vars as i32 * 4;
            a.cmd(c::GOTO, &[A::I32(start)]);
            a.bytes.resize(start as usize, 0);
            a
        }

        pub fn here(&self) -> i32 {
            self.bytes.len() as i32
        }

        pub fn cmd(&mut self, id: u16, args: &[A]) -> &mut Self {
            self.bytes.extend_from_slice(&id.to_le_bytes());
            for a in args {
                match *a {
                    A::I32(v) => {
                        self.bytes.push(1);
                        self.bytes.extend_from_slice(&v.to_le_bytes());
                    }
                    A::G(g) => {
                        self.bytes.push(2);
                        self.bytes.extend_from_slice(&g.to_le_bytes());
                    }
                    A::L(l) => {
                        self.bytes.push(3);
                        self.bytes.extend_from_slice(&l.to_le_bytes());
                    }
                    A::I8(v) => {
                        self.bytes.push(4);
                        self.bytes.push(v as u8);
                    }
                    A::I16(v) => {
                        self.bytes.push(5);
                        self.bytes.extend_from_slice(&v.to_le_bytes());
                    }
                    A::F(f) => {
                        self.bytes.push(6);
                        self.bytes.extend_from_slice(&((f * 16.0) as i16).to_le_bytes());
                    }
                }
            }
            self
        }

        /// A command whose argument list ends with the end marker.
        pub fn cmd_end(&mut self, id: u16, args: &[A]) -> &mut Self {
            self.cmd(id, args);
            self.bytes.push(0);
            self
        }
    }

    pub(crate) fn world_with_script(a: &Asm) -> World {
        let mut w = crate::world::tests::world_with_cars();
        w.ents.scripts.space[..a.bytes.len()].copy_from_slice(&a.bytes);
        w
    }

    pub(crate) fn start(w: &mut World) -> usize {
        w.ents.scripts.start_new_script(0)
    }

    pub(crate) fn run_frame(w: &mut World, sh: &mut dyn ScriptHooks) {
        w.process_scripts(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, sh);
    }

    #[derive(Default)]
    pub(crate) struct Spy {
        pub shakes: Vec<f32>,
        pub small_cleared: u32,
        pub highlights: Vec<(f32, f32, f32, f32)>,
    }
    impl ScriptHooks for Spy {
        fn camera_shake(&mut self, strength: f32) {
            self.shakes.push(strength);
        }
        fn clear_small_messages_only(&mut self) {
            self.small_cleared += 1;
        }
        fn highlight_important_area(&mut self, _id: u32, x1: f32, y1: f32, x2: f32, y2: f32, _z: f32) {
            self.highlights.push((x1, y1, x2, y2));
        }
    }

    const X: u16 = 8;
    const Y: u16 = 12;

    fn global(w: &World, g: u16) -> i32 {
        w.ents.scripts.global(g as u32)
    }

    #[test]
    fn a_script_runs_until_it_waits_then_goes_on_and_ends() {
        let mut a = Asm::new(2);
        a.cmd(c::SET_VAR_INT, &[A::G(X), A::I8(5)]);
        a.cmd(c::ADD_VAL_TO_INT_VAR, &[A::G(X), A::I16(3)]);
        a.cmd(c::IS_INT_VAR_GREATER_THAN_NUMBER, &[A::G(X), A::I8(7)]);
        let jump = a.here() as usize + 3;
        a.cmd(c::GOTO_IF_FALSE, &[A::I32(0)]);
        a.cmd(c::SET_VAR_INT, &[A::G(Y), A::I8(1)]);
        a.cmd(c::WAIT, &[A::I16(100)]);
        a.cmd(c::SET_VAR_INT, &[A::G(Y), A::I8(2)]);
        let end = a.here();
        a.cmd(c::TERMINATE_THIS_SCRIPT, &[]);
        a.bytes[jump..jump + 4].copy_from_slice(&end.to_le_bytes());
        let mut w = world_with_script(&a);
        let mut spy = Spy::default();
        let i = start(&mut w);
        run_frame(&mut w, &mut spy);
        assert_eq!((global(&w, X), global(&w, Y)), (8, 1));
        assert_eq!(w.ents.scripts.active, Some(i));
        run_frame(&mut w, &mut spy);
        assert_eq!(global(&w, Y), 1, "still waiting");
        w.ents.timer.time_in_ms = 100;
        run_frame(&mut w, &mut spy);
        assert_eq!(global(&w, Y), 2);
        assert_eq!((w.ents.scripts.active, w.ents.scripts.idle), (None, Some(i)));
    }

    #[test]
    fn a_false_test_takes_the_jump_and_not_inverts_it() {
        let mut a = Asm::new(2);
        a.cmd(c::SET_VAR_INT, &[A::G(X), A::I8(5)]);
        a.cmd(c::IS_INT_VAR_GREATER_THAN_NUMBER, &[A::G(X), A::I8(7)]);
        let j1 = a.here() as usize + 3;
        a.cmd(c::GOTO_IF_FALSE, &[A::I32(0)]);
        a.cmd(c::SET_VAR_INT, &[A::G(Y), A::I8(1)]);
        let l1 = a.here();
        // the same test with NOT: 5 > 7 is false, so NOT makes it true
        a.cmd(c::IS_INT_VAR_GREATER_THAN_NUMBER | 0x8000, &[A::G(X), A::I8(7)]);
        let j2 = a.here() as usize + 3;
        a.cmd(c::GOTO_IF_FALSE, &[A::I32(0)]);
        a.cmd(c::ADD_VAL_TO_INT_VAR, &[A::G(Y), A::I8(10)]);
        let l2 = a.here();
        a.cmd(c::WAIT, &[A::I32(100000)]);
        a.bytes[j1..j1 + 4].copy_from_slice(&l1.to_le_bytes());
        a.bytes[j2..j2 + 4].copy_from_slice(&l2.to_le_bytes());
        let mut w = world_with_script(&a);
        start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        assert_eq!(global(&w, Y), 10);
    }

    #[test]
    fn numbers_in_every_form_and_both_kinds_of_variable() {
        let mut a = Asm::new(2);
        a.cmd(c::SET_VAR_FLOAT, &[A::G(X), A::F(2.5)]);
        a.cmd(c::MULT_FLOAT_VAR_BY_VAL, &[A::G(X), A::F(4.0)]);
        a.cmd(c::SET_LVAR_INT, &[A::L(0), A::I32(100_000)]);
        a.cmd(c::SUB_VAL_FROM_INT_LVAR, &[A::L(0), A::I16(-5)]);
        a.cmd(c::DIV_INT_LVAR_BY_VAL, &[A::L(0), A::I8(7)]);
        a.cmd(c::SET_VAR_INT, &[A::G(Y), A::L(0)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_script(&a);
        let i = start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        assert_eq!(f32_of(global(&w, X)), 10.0);
        assert_eq!(global(&w, Y), 100_005 / 7);
        assert_eq!(w.ents.scripts.scripts[i].locals[0], 100_005 / 7);
    }

    #[test]
    fn variables_can_be_combined_with_variables_of_either_kind() {
        let mut a = Asm::new(2);
        a.cmd(c::SET_VAR_INT, &[A::G(X), A::I8(6)]);
        a.cmd(c::SET_LVAR_INT, &[A::L(1), A::I8(4)]);
        a.cmd(c::ADD_INT_LVAR_TO_INT_VAR, &[A::G(X), A::L(1)]);
        a.cmd(c::MULT_INT_VAR_BY_INT_LVAR, &[A::G(X), A::L(1)]);
        a.cmd(c::SUB_INT_LVAR_FROM_INT_VAR, &[A::G(X), A::L(1)]);
        a.cmd(c::DIV_INT_VAR_BY_INT_LVAR, &[A::G(X), A::L(1)]);
        a.cmd(c::SET_VAR_FLOAT, &[A::G(Y), A::F(1.5)]);
        a.cmd(c::SET_LVAR_FLOAT, &[A::L(2), A::F(0.5)]);
        a.cmd(c::ADD_FLOAT_LVAR_TO_FLOAT_VAR, &[A::G(Y), A::L(2)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_script(&a);
        start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        // ((6 + 4) * 4 - 4) / 4
        assert_eq!(global(&w, X), 9);
        assert_eq!(f32_of(global(&w, Y)), 2.0);
    }

    #[test]
    fn subroutines_return_and_new_scripts_start_with_their_arguments() {
        let mut a = Asm::new(2);
        let call = a.here() as usize + 3;
        a.cmd(c::GOSUB, &[A::I32(0)]);
        let spawn_at = a.here() as usize + 3;
        a.cmd_end(c::START_NEW_SCRIPT, &[A::I32(0), A::I8(42), A::G(X)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let sub = a.here();
        a.cmd(c::SET_VAR_INT, &[A::G(X), A::I8(7)]);
        a.cmd(c::RETURN, &[]);
        let child = a.here();
        a.cmd(c::SET_VAR_INT, &[A::G(Y), A::L(0)]);
        a.cmd(c::ADD_INT_VAR_TO_INT_LVAR, &[A::L(1), A::G(X)]);
        a.cmd(c::TERMINATE_THIS_SCRIPT, &[]);
        a.bytes[call..call + 4].copy_from_slice(&sub.to_le_bytes());
        a.bytes[spawn_at..spawn_at + 4].copy_from_slice(&child.to_le_bytes());
        let mut w = world_with_script(&a);
        let parent = start(&mut w);
        let mut spy = Spy::default();
        run_frame(&mut w, &mut spy);
        // the child was made at the head of the list and did not run in the frame it was born
        assert_eq!(global(&w, Y), 0);
        let child_id = w.ents.scripts.active.unwrap();
        assert_ne!(child_id, parent);
        assert_eq!(w.ents.scripts.scripts[child_id].locals[..2], [42, 7]);
        run_frame(&mut w, &mut spy);
        assert_eq!(global(&w, Y), 42);
        assert_eq!(w.ents.scripts.active_scripts(), vec![parent]);
    }

    #[test]
    fn the_and_or_state_joins_the_next_tests() {
        // two ANDed tests (the ANDOR command starts them from true)
        let mut s = RunningScript { and_or_state: 2, cond_result: true, ..Default::default() };
        s.update_compare_flag(true);
        assert_eq!((s.cond_result, s.and_or_state), (true, 1));
        s.update_compare_flag(false);
        assert_eq!((s.cond_result, s.and_or_state), (false, 0));
        // two ORed tests (from false)
        s.and_or_state = 22;
        s.cond_result = false;
        s.update_compare_flag(false);
        s.update_compare_flag(true);
        assert_eq!((s.cond_result, s.and_or_state), (true, 0));
        // a plain test replaces the result
        s.update_compare_flag(false);
        assert!(!s.cond_result);
    }

    #[test]
    fn the_player_commands_and_the_highlighted_area() {
        let mut a = Asm::new(4);
        a.cmd(c::GET_PLAYER_COORDINATES, &[A::I8(0), A::G(8), A::G(12), A::G(16)]);
        a.cmd(c::IS_PLAYER_IN_AREA_2D, &[A::I8(0), A::F(0.0), A::F(0.0), A::F(20.0), A::F(20.0), A::I8(1)]);
        let j = a.here() as usize + 3;
        a.cmd(c::GOTO_IF_FALSE, &[A::I32(0)]);
        a.cmd(c::SET_VAR_INT, &[A::G(20), A::I8(1)]);
        let end = a.here();
        a.cmd(c::WAIT, &[A::I32(100000)]);
        a.bytes[j..j + 4].copy_from_slice(&end.to_le_bytes());
        let mut w = world_with_script(&a);
        let p = w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, crate::ped::types::ped_type::PLAYER1, -1, None).unwrap();
        w.ped_mut(p).player = Some(Default::default());
        w.ents.player_ped = Some(p);
        w.get_mut(p).unwrap().base.set_position(Vec3::new(5.0, 6.0, 1.0));
        start(&mut w);
        let mut spy = Spy::default();
        run_frame(&mut w, &mut spy);
        assert_eq!((f32_of(global(&w, 8)), f32_of(global(&w, 12)), f32_of(global(&w, 16))), (5.0, 6.0, 1.0));
        assert_eq!(global(&w, 20), 1);
        assert_eq!(spy.highlights, vec![(0.0, 0.0, 20.0, 20.0)]);
    }

    #[test]
    fn a_mission_script_is_cut_short_when_the_player_dies() {
        let mut a = Asm::new(4);
        let waiting = a.here();
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let handler = a.here();
        a.cmd(c::SET_VAR_INT, &[A::G(12), A::I8(9)]);
        a.cmd(c::TERMINATE_THIS_SCRIPT, &[]);
        let mut w = world_with_script(&a);
        let i = start(&mut w);
        w.ents.scripts.on_a_mission_flag = 8;
        w.ents.scripts.set_global(8, 1);
        let s = &mut w.ents.scripts.scripts[i];
        (s.is_mission_script, s.stack_pointer, s.stack[0], s.ip) = (true, 1, handler as u32, waiting as u32);
        let mut spy = Spy::default();
        run_frame(&mut w, &mut spy);
        assert_eq!((global(&w, 8), global(&w, 12)), (1, 0), "waiting");
        w.ents.player_info.wb_state = wb_state::WASTED;
        run_frame(&mut w, &mut spy);
        // the mission flag is cleared and the script runs its clean-up
        assert_eq!((global(&w, 8), global(&w, 12), spy.small_cleared), (0, 9, 1));
        assert!(w.ents.scripts.scripts[i].deatharrest_executed);
        assert_eq!(w.ents.scripts.active, None);
    }

    #[test]
    fn a_script_that_turned_off_the_death_check_is_left_alone() {
        let mut a = Asm::new(1);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_script(&a);
        let i = start(&mut w);
        w.ents.scripts.on_a_mission_flag = 8;
        w.ents.scripts.set_global(8, 1);
        let s = &mut w.ents.scripts.scripts[i];
        (s.is_mission_script, s.deatharrest_enabled, s.wake_time) = (true, false, 100000);
        w.ents.player_info.wb_state = wb_state::BUSTED;
        run_frame(&mut w, &mut Spy::default());
        assert!(!w.ents.scripts.scripts[i].deatharrest_executed && global(&w, 8) == 1);
    }

    #[test]
    fn the_camera_shakes_for_the_time_given() {
        let mut a = Asm::new(0);
        a.cmd(c::SHAKE_CAM, &[A::I16(500)]);
        a.cmd(c::TERMINATE_THIS_SCRIPT, &[]);
        let mut w = world_with_script(&a);
        start(&mut w);
        let mut spy = Spy::default();
        run_frame(&mut w, &mut spy);
        assert_eq!(spy.shakes, vec![0.5]);
    }

    #[test]
    fn the_local_timers_count_milliseconds() {
        let mut s = RunningScript::default();
        s.update_timers(20.0);
        s.update_timers(20.0);
        assert_eq!((s.locals[16], s.locals[17]), (40, 40));
    }

    #[test]
    fn the_variable_space_size_comes_from_the_first_jump_and_text_labels_stop_at_the_end() {
        let a = Asm::new(10);
        let w = world_with_script(&a);
        assert_eq!(w.ents.scripts.size_of_variable_space(), 48);
        let mut w = w;
        w.ents.scripts.space[100..108].copy_from_slice(b"GA_1\0\0\0\0");
        let mut ip = 100;
        assert_eq!((w.ents.scripts.read_text_label(&mut ip), ip), ("GA_1".to_owned(), 108));
    }
}
