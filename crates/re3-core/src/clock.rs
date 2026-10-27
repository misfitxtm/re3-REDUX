//! `Clock.cpp`: the game clock, which runs `ms_per_game_minute` real milliseconds per game minute.

/// `CClock`'s statics.
#[derive(Debug, Clone, PartialEq)]
pub struct Clock {
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u16,
    pub stored_hours: u8,
    pub stored_minutes: u8,
    pub stored_seconds: u16,
    pub ms_per_game_minute: u32,
    pub last_clock_tick: u32,
    pub clock_has_been_stored: bool,
}

impl Default for Clock {
    /// `CGame` starts the clock at a game minute per real second.
    fn default() -> Self {
        Self::new(1000, 0)
    }
}

impl Clock {
    /// `CClock::Initialise`: noon.
    pub fn new(scale: u32, now: u32) -> Self {
        Self { hours: 12, minutes: 0, seconds: 0, stored_hours: 0, stored_minutes: 0, stored_seconds: 0, ms_per_game_minute: scale, last_clock_tick: now, clock_has_been_stored: false }
    }

    /// `CClock::Update`: `skip_forward` is the debug key (eight minutes a frame), `fast_time` is `gbFastTime`.
    /// Returns true when a day has just gone by (`CStats::DaysPassed++`).
    pub fn update(&mut self, now: u32, skip_forward: bool, fast_time: bool) -> bool {
        let mut day_passed = false;
        if skip_forward {
            self.minutes = self.minutes.wrapping_add(8);
            self.last_clock_tick = now;
            if self.minutes >= 60 {
                self.hours += 1;
                self.minutes = 0;
                if self.hours >= 24 {
                    self.hours = 0;
                }
            }
        } else if now.wrapping_sub(self.last_clock_tick) > self.ms_per_game_minute || fast_time {
            self.minutes += 1;
            self.last_clock_tick = self.last_clock_tick.wrapping_add(self.ms_per_game_minute);
            if fast_time {
                self.last_clock_tick = now;
            }
            if self.minutes >= 60 {
                self.hours += 1;
                self.minutes = 0;
                if self.hours >= 24 {
                    day_passed = true;
                    self.hours = 0;
                }
            }
        }
        self.seconds = (60u32.wrapping_mul(now.wrapping_sub(self.last_clock_tick)) / self.ms_per_game_minute) as u16;
        day_passed
    }

    /// `SetGameClock`.
    pub fn set_game_clock(&mut self, h: u8, m: u8, now: u32) {
        self.hours = h;
        self.minutes = m;
        self.seconds = 0;
        self.last_clock_tick = now;
    }

    /// `GetGameClockMinutesUntil`: game minutes until the next time `h:m`, within a day.
    pub fn minutes_until(&self, h: u8, m: u8) -> i32 {
        let now = self.hours as i32 * 60 + self.minutes as i32;
        let mut then = h as i32 * 60 + m as i32;
        if then < now {
            then += 24 * 60;
        }
        then - now
    }

    /// `GetIsTimeInRange`: the hours from `h1` up to (not including) `h2`, which may wrap past midnight.
    pub fn is_time_in_range(&self, h1: u8, h2: u8) -> bool {
        if h1 > h2 { self.hours >= h1 || self.hours < h2 } else { self.hours >= h1 && self.hours < h2 }
    }

    /// `StoreClock`.
    pub fn store(&mut self) {
        (self.stored_hours, self.stored_minutes, self.stored_seconds) = (self.hours, self.minutes, self.seconds);
        self.clock_has_been_stored = true;
    }

    /// `RestoreClock`.
    pub fn restore(&mut self) {
        (self.hours, self.minutes, self.seconds) = (self.stored_hours, self.stored_minutes, self.stored_seconds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minute_goes_by_every_scale_ms_and_a_day_after_midnight() {
        let mut c = Clock::new(1000, 0);
        assert_eq!((c.hours, c.minutes), (12, 0));
        assert!(!c.update(1000, false, false), "not yet more than a minute");
        assert_eq!(c.minutes, 0);
        c.update(1001, false, false);
        assert_eq!((c.minutes, c.last_clock_tick), (1, 1000));
        c.set_game_clock(23, 59, 5000);
        assert!(c.update(6001, false, false));
        assert_eq!((c.hours, c.minutes), (0, 0));
    }

    #[test]
    fn the_seconds_follow_the_fraction_of_the_minute() {
        let mut c = Clock::new(1000, 0);
        c.update(500, false, false);
        assert_eq!(c.seconds, 30);
    }

    #[test]
    fn the_debug_key_and_fast_time_move_the_clock_on() {
        let mut c = Clock::new(1000, 0);
        c.update(10, true, false);
        assert_eq!(c.minutes, 8);
        c.set_game_clock(12, 55, 20);
        c.update(30, true, false);
        assert_eq!((c.hours, c.minutes), (13, 0));
        c.update(40, false, true);
        assert_eq!((c.minutes, c.last_clock_tick), (1, 40));
    }

    #[test]
    fn time_ranges_wrap_past_midnight() {
        let mut c = Clock::new(1000, 0);
        c.hours = 23;
        assert!(c.is_time_in_range(22, 5) && !c.is_time_in_range(8, 19));
        c.hours = 3;
        assert!(c.is_time_in_range(22, 5));
        c.hours = 19;
        assert!(!c.is_time_in_range(8, 19) && c.is_time_in_range(19, 22));
        c.set_game_clock(10, 30, 0);
        assert_eq!((c.minutes_until(10, 45), c.minutes_until(10, 15)), (15, 23 * 60 + 45));
    }

    #[test]
    fn a_stored_clock_comes_back() {
        let mut c = Clock::new(1000, 0);
        c.set_game_clock(7, 7, 0);
        c.store();
        c.set_game_clock(1, 1, 0);
        c.restore();
        assert_eq!((c.hours, c.minutes, c.clock_has_been_stored), (7, 7, true));
    }
}
