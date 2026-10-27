//! The script virtual machine (`Script*.cpp`): `main.scm` bytecode, its variables and the running scripts (GTA III).
use glam::Vec3;

pub mod checks;
mod cmds_100;
mod cmds_200;
mod cmds_300;
mod locate;
mod locate_table;
pub mod commands;
pub mod hooks;
mod process;

pub use hooks::ScriptHooks;

pub const SIZE_MAIN_SCRIPT: usize = 128 * 1024;
pub const SIZE_MISSION_SCRIPT: usize = 32 * 1024;
pub const SIZE_SCRIPT_SPACE: usize = SIZE_MAIN_SCRIPT + SIZE_MISSION_SCRIPT;
pub const MAX_NUM_SCRIPTS: usize = 128;
pub const MAX_STACK_DEPTH: usize = 6;
pub const NUM_LOCAL_VARS: usize = 16;
pub const NUM_TIMERS: usize = 2;
pub const KEY_LENGTH_IN_SCRIPT: usize = 8;
pub const USED_OBJECT_NAME_LENGTH: usize = 24;
pub const MAX_NUM_USED_OBJECTS: usize = 200;
pub const MAX_NUM_MISSION_SCRIPTS: usize = 120;
pub const MAX_NUM_CONTACTS: usize = 16;

/// `CLEANUP_*`: what kind of entity a mission has made.
pub const CLEANUP_UNUSED: u8 = 0;
pub const CLEANUP_CAR: u8 = 1;
pub const CLEANUP_CHAR: u8 = 2;
pub const CLEANUP_OBJECT: u8 = 3;
pub const MAX_CLEANUP: usize = 50;

/// `CMissionCleanup`: the entities a mission made, to be removed when it ends.
#[derive(Debug, Clone, PartialEq)]
pub struct MissionCleanup {
    pub entities: [(u8, i32); MAX_CLEANUP],
    pub count: u8,
}

impl Default for MissionCleanup {
    fn default() -> Self {
        Self { entities: [(CLEANUP_UNUSED, 0); MAX_CLEANUP], count: 0 }
    }
}

impl MissionCleanup {
    /// `AddEntityToList`.
    pub fn add_entity_to_list(&mut self, id: i32, ty: u8) {
        if let Some(e) = self.entities.iter_mut().find(|e| e.0 == CLEANUP_UNUSED) {
            *e = (ty, id);
            self.count += 1;
        }
    }

    /// `RemoveEntityFromList`.
    pub fn remove_entity_from_list(&mut self, id: i32, ty: u8) {
        for e in self.entities.iter_mut().filter(|e| **e == (ty, id)) {
            *e = (CLEANUP_UNUSED, 0);
            self.count -= 1;
        }
    }
}

/// How an argument is stored in the bytecode (`ARGUMENT_*`).
pub mod arg {
    pub const END: u8 = 0;
    pub const INT32: u8 = 1;
    pub const GLOBALVAR: u8 = 2;
    pub const LOCALVAR: u8 = 3;
    pub const INT8: u8 = 4;
    pub const INT16: u8 = 5;
    pub const FLOAT: u8 = 6;
}

/// `m_nAndOrState`: how many more conditions are joined, and by what.
pub mod andor {
    pub const NONE: u16 = 0;
    pub const ANDS_1: u16 = 1;
    pub const ANDS_8: u16 = 8;
    pub const ORS_1: u16 = 21;
    pub const ORS_8: u16 = 28;
}

/// The jump target of a script: a negative one is an offset into the mission script.
pub fn jump_target(param: i32) -> u32 {
    if param >= 0 { param as u32 } else { (SIZE_MAIN_SCRIPT as i32 - param) as u32 }
}

pub fn f32_of(bits: i32) -> f32 {
    f32::from_bits(bits as u32)
}

pub fn bits_of(f: f32) -> i32 {
    f.to_bits() as i32
}

/// `CRunningScript`. The lists it sits in are kept by [`Scripts`].
#[derive(Debug, Clone, PartialEq)]
pub struct RunningScript {
    pub name: [u8; 8],
    pub ip: u32,
    pub stack: [u32; MAX_STACK_DEPTH],
    pub stack_pointer: u16,
    pub locals: [i32; NUM_LOCAL_VARS + NUM_TIMERS],
    pub cond_result: bool,
    pub is_mission_script: bool,
    pub skip_wake_time: bool,
    pub wake_time: u32,
    pub and_or_state: u16,
    pub not_flag: bool,
    pub deatharrest_enabled: bool,
    pub deatharrest_executed: bool,
    pub mission_flag: bool,
}

impl Default for RunningScript {
    /// `CRunningScript::Init`.
    fn default() -> Self {
        let mut name = [0u8; 8];
        name[..6].copy_from_slice(b"noname");
        Self {
            name,
            ip: 0,
            stack: [0; MAX_STACK_DEPTH],
            stack_pointer: 0,
            locals: [0; NUM_LOCAL_VARS + NUM_TIMERS],
            cond_result: false,
            is_mission_script: false,
            skip_wake_time: false,
            wake_time: 0,
            and_or_state: 0,
            not_flag: false,
            deatharrest_enabled: true,
            deatharrest_executed: false,
            mission_flag: false,
        }
    }
}

impl RunningScript {
    /// `UpdateTimers`: the two local timers count milliseconds.
    pub fn update_timers(&mut self, time_step_ms: f32) {
        for t in &mut self.locals[NUM_LOCAL_VARS..] {
            *t = (*t as f32 + time_step_ms) as i32;
        }
    }

    /// `UpdateCompareFlag`: the result of a test, as the NOT and AND/OR state of the script say.
    pub fn update_compare_flag(&mut self, mut flag: bool) {
        use andor::*;
        if self.not_flag {
            flag = !flag;
        }
        let s = self.and_or_state;
        if s == NONE {
            self.cond_result = flag;
            return;
        }
        if (ANDS_1..=ANDS_8).contains(&s) {
            self.cond_result &= flag;
            if s == ANDS_1 {
                self.and_or_state = NONE;
                return;
            }
        } else if (ORS_1..=ORS_8).contains(&s) {
            self.cond_result |= flag;
            if s == ORS_1 {
                self.and_or_state = NONE;
                return;
            }
        } else {
            return;
        }
        self.and_or_state -= 1;
    }

    pub fn name_str(&self) -> String {
        String::from_utf8_lossy(&self.name).trim_end_matches('\0').to_owned()
    }
}

/// A variable or constant in the bytecode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Operand {
    Value(i32),
    Global(u16),
    Local(u16),
}

/// `CTheScripts` and the other statics of the script code.
#[derive(Debug, Clone)]
pub struct Scripts {
    pub space: Vec<u8>,
    pub scripts: Vec<RunningScript>,
    next: Vec<Option<usize>>,
    prev: Vec<Option<usize>>,
    pub active: Option<usize>,
    pub idle: Option<usize>,
    /// `ScriptParams`.
    pub params: [i32; 32],
    pub on_a_mission_flag: u32,
    pub cleanup: MissionCleanup,
    /// `UsedObjectArray`: the names of the models the script creates, and the ids they came to have.
    pub used_objects: Vec<(String, i32)>,
    pub multi_script_array: [i32; MAX_NUM_MISSION_SCRIPTS],
    pub number_of_mission_scripts: u16,
    pub largest_mission_script_size: u32,
    pub main_script_size: u32,
    pub store_vehicle_index: i32,
    pub store_vehicle_was_random: bool,
    pub base_brief_id_for_contact: [i32; MAX_NUM_CONTACTS],
    pub on_a_mission_for_contact_flag: [i32; MAX_NUM_CONTACTS],
    pub fail_current_mission: u8,
    pub already_running_a_mission_script: bool,
    pub commands_executed: u16,
    pub scripts_updated: u16,
}

impl Default for Scripts {
    /// `CTheScripts::Init` without the script file: every script is idle.
    fn default() -> Self {
        let mut s = Self {
            space: vec![0; SIZE_SCRIPT_SPACE],
            scripts: vec![RunningScript::default(); MAX_NUM_SCRIPTS],
            next: vec![None; MAX_NUM_SCRIPTS],
            prev: vec![None; MAX_NUM_SCRIPTS],
            active: None,
            idle: None,
            params: [0; 32],
            on_a_mission_flag: 0,
            cleanup: Default::default(),
            used_objects: vec![],
            multi_script_array: [0; MAX_NUM_MISSION_SCRIPTS],
            number_of_mission_scripts: 0,
            largest_mission_script_size: 0,
            main_script_size: 0,
            store_vehicle_index: -1,
            store_vehicle_was_random: true,
            base_brief_id_for_contact: [0; MAX_NUM_CONTACTS],
            on_a_mission_for_contact_flag: [0; MAX_NUM_CONTACTS],
            fail_current_mission: 0,
            already_running_a_mission_script: false,
            commands_executed: 0,
            scripts_updated: 0,
        };
        for i in 0..MAX_NUM_SCRIPTS {
            s.add_to_list(i, false);
        }
        s
    }
}

impl Scripts {
    /// `CTheScripts::Init` with the script file: the main script goes in at the bottom of the script space.
    pub fn from_main_script(main_scm: &[u8]) -> Self {
        let mut s = Self::default();
        let n = main_scm.len().min(SIZE_MAIN_SCRIPT);
        s.space[..n].copy_from_slice(&main_scm[..n]);
        s.read_object_names_from_script();
        s.read_multi_script_file_offsets_from_script();
        s
    }

    /// `ReadObjectNamesFromScript`: the table of model names behind the script's negative object numbers.
    pub fn read_object_names_from_script(&mut self) {
        let mut ip = self.size_of_variable_space() as u32 + 8;
        let count = self.read2(&mut ip) as usize;
        ip += 2;
        self.used_objects = (0..count.min(MAX_NUM_USED_OBJECTS))
            .map(|i| {
                let at = ip as usize + i * USED_OBJECT_NAME_LENGTH;
                let raw = &self.space[at..at + USED_OBJECT_NAME_LENGTH];
                (String::from_utf8_lossy(raw.split(|&b| b == 0).next().unwrap_or(&[])).into_owned(), 0)
            })
            .collect();
    }

    /// `ReadMultiScriptFileOffsetsFromScript`: where each mission sits in the script file.
    pub fn read_multi_script_file_offsets_from_script(&mut self) {
        let mut ip = self.size_of_variable_space() as u32 + 3;
        let object_size = self.read4(&mut ip);
        ip = object_size as u32 + 8;
        self.main_script_size = self.read4(&mut ip) as u32;
        self.largest_mission_script_size = self.read4(&mut ip) as u32;
        self.number_of_mission_scripts = self.read2(&mut ip) as u16;
        ip += 2;
        for i in 0..(self.number_of_mission_scripts as usize).min(MAX_NUM_MISSION_SCRIPTS) {
            self.multi_script_array[i] = self.read4(&mut ip);
        }
    }

    // ---- the lists (`AddScriptToList`, `RemoveScriptFromList`) ----

    fn head(&mut self, active: bool) -> &mut Option<usize> {
        if active { &mut self.active } else { &mut self.idle }
    }

    /// Puts script `i` at the head of the active or the idle list.
    pub fn add_to_list(&mut self, i: usize, active: bool) {
        let old = *self.head(active);
        self.next[i] = old;
        self.prev[i] = None;
        if let Some(o) = old {
            self.prev[o] = Some(i);
        }
        *self.head(active) = Some(i);
    }

    /// Takes script `i` out of the list it is in.
    pub fn remove_from_list(&mut self, i: usize, active: bool) {
        let (p, n) = (self.prev[i], self.next[i]);
        match p {
            Some(p) => self.next[p] = n,
            None => *self.head(active) = n,
        }
        if let Some(n) = n {
            self.prev[n] = p;
        }
    }

    pub fn next_of(&self, i: usize) -> Option<usize> {
        self.next[i]
    }

    /// `StartNewScript`: an idle script is cleaned and put at the head of the active list.
    pub fn start_new_script(&mut self, ip: u32) -> usize {
        let i = self.idle.expect("no idle script");
        self.remove_from_list(i, false);
        self.scripts[i] = RunningScript { ip, ..Default::default() };
        self.add_to_list(i, true);
        i
    }

    /// The ids of the active scripts, newest first.
    pub fn active_scripts(&self) -> Vec<usize> {
        std::iter::successors(self.active, |&i| self.next[i]).collect()
    }

    // ---- reading the bytecode ----

    pub fn read1(&self, ip: &mut u32) -> i8 {
        let v = self.space[*ip as usize] as i8;
        *ip += 1;
        v
    }

    pub fn read2(&self, ip: &mut u32) -> i16 {
        let i = *ip as usize;
        *ip += 2;
        i16::from_le_bytes([self.space[i], self.space[i + 1]])
    }

    pub fn read4(&self, ip: &mut u32) -> i32 {
        let i = *ip as usize;
        *ip += 4;
        i32::from_le_bytes([self.space[i], self.space[i + 1], self.space[i + 2], self.space[i + 3]])
    }

    /// Numbers in the script are 16-bit fixed point.
    pub fn read_float(&self, ip: &mut u32) -> f32 {
        self.read2(ip) as f32 / 16.0
    }

    /// A text key: up to eight characters.
    pub fn read_text_label(&self, ip: &mut u32) -> String {
        let i = *ip as usize;
        *ip += KEY_LENGTH_IN_SCRIPT as u32;
        let raw = &self.space[i..i + KEY_LENGTH_IN_SCRIPT];
        String::from_utf8_lossy(raw.split(|&b| b == 0).next().unwrap_or(&[])).into_owned()
    }

    /// `GetSizeOfVariableSpace`: the first jump of the script skips over the variables.
    pub fn size_of_variable_space(&self) -> i32 {
        let mut ip = 3;
        self.read4(&mut ip)
    }

    fn global_at(&self, offset: u16) -> i32 {
        debug_assert!(offset as i32 >= 8 && (offset as i32) < self.size_of_variable_space(), "bad global {offset}");
        let o = offset as usize;
        i32::from_le_bytes([self.space[o], self.space[o + 1], self.space[o + 2], self.space[o + 3]])
    }

    pub fn set_global(&mut self, offset: u16, v: i32) {
        let o = offset as usize;
        self.space[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    pub fn global(&self, offset: u32) -> i32 {
        self.global_at(offset as u16)
    }

    /// One argument, as it stands in the bytecode.
    pub fn read_operand(&self, ip: &mut u32) -> Operand {
        match self.read1(ip) as u8 {
            arg::INT32 => Operand::Value(self.read4(ip)),
            arg::GLOBALVAR => Operand::Global(self.read2(ip) as u16),
            arg::LOCALVAR => Operand::Local(self.read2(ip) as u16),
            arg::INT8 => Operand::Value(self.read1(ip) as i32),
            arg::INT16 => Operand::Value(self.read2(ip) as i32),
            arg::FLOAT => Operand::Value(bits_of(self.read_float(ip))),
            t => panic!("bad script argument type {t} at {}", *ip - 1),
        }
    }

    /// The number an argument stands for.
    pub fn value_of(&self, s: &RunningScript, o: Operand) -> i32 {
        match o {
            Operand::Value(v) => v,
            Operand::Global(g) => self.global_at(g),
            Operand::Local(l) => s.locals[l as usize],
        }
    }

    pub fn store_operand(&mut self, s: &mut RunningScript, o: Operand, v: i32) {
        match o {
            Operand::Global(g) => self.set_global(g, v),
            Operand::Local(l) => s.locals[l as usize] = v,
            Operand::Value(_) => panic!("script stores into a constant"),
        }
    }

    /// `CollectParameters`: the next `total` arguments into `params`.
    pub fn collect_parameters(&mut self, s: &mut RunningScript, total: usize) {
        for i in 0..total {
            let o = self.read_operand(&mut s.ip);
            self.params[i] = self.value_of(s, o);
        }
    }

    /// `CollectNextParameterWithoutIncreasingPC`.
    pub fn peek_parameter(&self, s: &RunningScript, mut ip: u32) -> i32 {
        let o = self.read_operand(&mut ip);
        self.value_of(s, o)
    }

    /// `StoreParameters`: the first `number` of `params` into the variables that follow.
    pub fn store_parameters(&mut self, s: &mut RunningScript, number: usize) {
        for i in 0..number {
            let o = self.read_operand(&mut s.ip);
            let v = self.params[i];
            self.store_operand(s, o, v);
        }
    }

    pub fn param_f(&self, i: usize) -> f32 {
        f32_of(self.params[i])
    }

    pub fn param_v3(&self, i: usize) -> Vec3 {
        Vec3::new(self.param_f(i), self.param_f(i + 1), self.param_f(i + 2))
    }

    pub fn set_param_f(&mut self, i: usize, f: f32) {
        self.params[i] = bits_of(f);
    }

    pub fn set_param_v3(&mut self, i: usize, v: Vec3) {
        self.set_param_f(i, v.x);
        self.set_param_f(i + 1, v.y);
        self.set_param_f(i + 2, v.z);
    }

    /// `IsPlayerOnAMission`.
    pub fn is_player_on_a_mission(&self) -> bool {
        self.on_a_mission_flag != 0 && self.global(self.on_a_mission_flag) == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_main_scm_header_and_tables() {
        let Ok(dir) = std::env::var("GTA3_DIR") else { return };
        let bytes = std::fs::read(format!("{dir}/data/main.scm")).or_else(|_| std::fs::read(format!("{dir}/data/MAIN.SCM"))).unwrap();
        let s = Scripts::from_main_script(&bytes);
        // the first command jumps over the variables
        assert_eq!(i16::from_le_bytes([s.space[0], s.space[1]]), crate::script::commands::GOTO as i16);
        let vars = s.size_of_variable_space();
        assert!(vars > 100 && (vars as usize) < SIZE_MAIN_SCRIPT, "variable space {vars}");
        assert!(s.used_objects.len() > 10 && s.used_objects[1].0.chars().all(|c| c.is_ascii_graphic()));
        assert!(s.number_of_mission_scripts > 50 && s.main_script_size as usize <= SIZE_MAIN_SCRIPT);
        assert!(s.multi_script_array[0] > 0);
    }

    /// Runs the real main script from its start until it reaches something not ported, and says what.
    #[test]
    fn real_main_scm_how_far_it_gets() {
        let Ok(dir) = std::env::var("GTA3_DIR") else { return };
        let bytes = std::fs::read(format!("{dir}/data/main.scm")).or_else(|_| std::fs::read(format!("{dir}/data/MAIN.SCM"))).unwrap();
        let mut w = crate::world::tests::world_with_cars();
        w.ents.scripts = Scripts::from_main_script(&bytes);
        w.ents.scripts.start_new_script(0);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for _ in 0..50 {
                w.ents.timer.time_in_ms += 20;
                w.process_scripts(&mut crate::ped::ctx::Px { rng: &mut crate::general::Rng::default(), hooks: &mut crate::ped::ctx::NoPedHooks }, &mut hooks::NoScriptHooks);
            }
        }));
        if let Err(e) = r {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            println!("main.scm stopped: {msg} after {} commands", w.ents.scripts.commands_executed);
        }
    }
}
