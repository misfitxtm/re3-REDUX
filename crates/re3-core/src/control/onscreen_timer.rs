//! `OnscreenTimer.cpp`: the countdown clock and the counter the script shows on the HUD.

pub const NUM_ONSCREEN_TIMER_ENTRIES: usize = 1;
const TEXT_LEN: usize = 10;

/// How a counter is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CounterDisplay {
    #[default]
    Number,
    Bar,
}

/// The script's variable memory, which the timers read and count down in place (`GetPointerToScriptVariable`).
pub trait ScriptVars {
    /// The 32 bit variable at a byte offset into the script's variable space.
    fn variable(&mut self, offset: u32) -> &mut i32;
}

/// `COnscreenTimerEntry`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OnscreenTimerEntry {
    /// 0 for none; otherwise where the countdown lives in the script's variables.
    pub timer_offset: u32,
    pub counter_offset: u32,
    pub timer_text: String,
    pub counter_text: String,
    pub ty: CounterDisplay,
    pub timer_processed: bool,
    pub counter_processed: bool,
    pub timer_buffer: String,
    pub counter_buffer: String,
}

/// `COnscreenTimer`.
#[derive(Debug, Clone, Default)]
pub struct OnscreenTimer {
    pub entries: [OnscreenTimerEntry; NUM_ONSCREEN_TIMER_ENTRIES],
    pub disabled: bool,
    pub processed: bool,
}

/// What a tick of the clock asks the sound for: the front end's clock tick with the seconds left.
pub type ClockTick = Option<i32>;

fn limited(text: Option<&str>) -> String {
    text.map_or_else(String::new, |t| t.chars().take(TEXT_LEN).collect())
}

impl OnscreenTimerEntry {
    /// `COnscreenTimerEntry::Process`: counts the clock down by `step_ms`; a tick sound in the last seconds.
    pub fn process(&mut self, vars: &mut dyn ScriptVars, step_ms: i32) -> ClockTick {
        if self.timer_offset == 0 {
            return None;
        }
        let timer = vars.variable(self.timer_offset);
        let old = *timer;
        let new = old - step_ms;
        if new < 0 {
            *timer = 0;
            self.timer_processed = false;
            self.timer_offset = 0;
            self.timer_text.clear();
            None
        } else {
            *timer = new;
            let old_seconds = old / 1000;
            (old_seconds < 12 && new / 1000 != old_seconds).then_some(new / 1000)
        }
    }

    /// `COnscreenTimerEntry::ProcessForDisplay`: formats what is drawn; false when the entry has nothing.
    pub fn process_for_display(&mut self, vars: &mut dyn ScriptVars) -> bool {
        self.timer_processed = false;
        self.counter_processed = false;
        if self.timer_offset == 0 && self.counter_offset == 0 {
            return false;
        }
        if self.timer_offset != 0 {
            self.timer_processed = true;
            let time = *vars.variable(self.timer_offset) as u32;
            self.timer_buffer = format!("{:02}:{:02}", time / 1000 / 60, time / 1000 % 60);
        }
        if self.counter_offset != 0 {
            self.counter_processed = true;
            self.counter_buffer = format!("{}", *vars.variable(self.counter_offset) as u32);
        }
        true
    }
}

impl OnscreenTimer {
    /// `COnscreenTimer::Init`.
    pub fn init(&mut self) {
        *self = Self::default();
    }

    /// `COnscreenTimer::Process`: a tick sound, if any entry asks for one.
    pub fn process(&mut self, vars: &mut dyn ScriptVars, step_ms: i32, replaying: bool) -> Vec<i32> {
        if replaying || self.disabled {
            return vec![];
        }
        self.entries.iter_mut().filter_map(|e| e.process(vars, step_ms)).collect()
    }

    /// `COnscreenTimer::ProcessForDisplay`.
    pub fn process_for_display(&mut self, vars: &mut dyn ScriptVars, wants_to_draw_hud: bool) {
        if wants_to_draw_hud {
            self.processed = false;
            for e in &mut self.entries {
                if e.process_for_display(vars) {
                    self.processed = true;
                }
            }
        }
    }

    /// `ClearCounter`.
    pub fn clear_counter(&mut self, offset: u32) {
        for e in self.entries.iter_mut().filter(|e| e.counter_offset == offset) {
            e.counter_offset = 0;
            e.counter_text.clear();
            e.ty = CounterDisplay::Number;
            e.counter_processed = false;
        }
    }

    /// `ClearClock`.
    pub fn clear_clock(&mut self, offset: u32) {
        for e in self.entries.iter_mut().filter(|e| e.timer_offset == offset) {
            e.timer_offset = 0;
            e.timer_text.clear();
            e.timer_processed = false;
        }
    }

    /// `AddCounter`: the first entry without a counter takes it.
    pub fn add_counter(&mut self, offset: u32, ty: CounterDisplay, text: Option<&str>) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.counter_offset == 0) {
            e.counter_offset = offset;
            e.counter_text = limited(text);
            e.ty = ty;
        }
    }

    /// `AddClock`.
    pub fn add_clock(&mut self, offset: u32, text: Option<&str>) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.timer_offset == 0) {
            e.timer_offset = offset;
            e.timer_text = limited(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Vars(HashMap<u32, i32>);
    impl ScriptVars for Vars {
        fn variable(&mut self, offset: u32) -> &mut i32 {
            self.0.entry(offset).or_insert(0)
        }
    }

    #[test]
    fn the_clock_counts_down_ticks_in_the_last_seconds_and_ends_at_zero() {
        let mut t = OnscreenTimer::default();
        let mut v = Vars::default();
        t.add_clock(8, Some("TIMER"));
        *v.variable(8) = 12_500;
        // above 12 s there is no tick
        assert!(t.process(&mut v, 600, false).is_empty());
        assert_eq!(*v.variable(8), 11_900);
        // crossing from 11 s to 10 s ticks with the seconds left
        *v.variable(8) = 11_100;
        assert_eq!(t.process(&mut v, 200, false), vec![10]);
        // running out removes the clock
        *v.variable(8) = 100;
        t.process(&mut v, 200, false);
        assert_eq!((*v.variable(8), t.entries[0].timer_offset, t.entries[0].timer_text.as_str()), (0, 0, ""));
    }

    #[test]
    fn a_replay_or_a_disabled_timer_does_not_count() {
        let mut t = OnscreenTimer::default();
        let mut v = Vars::default();
        t.add_clock(4, None);
        *v.variable(4) = 5000;
        t.process(&mut v, 100, true);
        t.disabled = true;
        t.process(&mut v, 100, false);
        assert_eq!(*v.variable(4), 5000);
    }

    #[test]
    fn clock_and_counter_are_formatted_for_the_hud_and_can_be_cleared() {
        let mut t = OnscreenTimer::default();
        let mut v = Vars::default();
        t.add_clock(4, Some("a very long timer label"));
        t.add_counter(8, CounterDisplay::Bar, Some("KILLS"));
        assert_eq!(t.entries[0].timer_text, "a very lon");
        *v.variable(4) = 125_000;
        *v.variable(8) = 17;
        t.process_for_display(&mut v, false);
        assert!(!t.processed);
        t.process_for_display(&mut v, true);
        assert!(t.processed);
        assert_eq!((t.entries[0].timer_buffer.as_str(), t.entries[0].counter_buffer.as_str()), ("02:05", "17"));
        t.clear_counter(8);
        assert_eq!((t.entries[0].counter_offset, t.entries[0].ty), (0, CounterDisplay::Number));
        t.clear_clock(4);
        t.process_for_display(&mut v, true);
        assert!(!t.processed);
    }
}
