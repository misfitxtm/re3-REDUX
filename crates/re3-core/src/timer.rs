//! Frame timing (`CTimer`). The caller supplies the elapsed wall-clock time, so the clock source
//! (and tests) stay outside. Uses the bug-fixed update with logical 30 fps frame counting.

#[derive(Debug, Clone)]
pub struct Timer {
    pub time_in_ms: u32,
    pub time_in_ms_pause_mode: u32,
    pub time_in_ms_non_clipped: u32,
    pub previous_time_in_ms: u32,
    pub frame_counter: u32,
    pub time_scale: f32,
    /// Frame length in 1/50 s units, clamped to 3.
    pub time_step: f32,
    pub time_step_non_clipped: f32,
    pub logical_frame_counter: u32,
    pub logical_frames_passed: u32,
    pub user_pause: bool,
    pub code_pause: bool,
    /// Replay playback lifts the step and time clamps.
    pub playing_back: bool,
    /// Recording a chase scene fixes the step to 1 and the frame to 16 ms.
    pub recording_chase: bool,
    frame_time_logical: f64,
    frame_time_fraction: f64,
    frame_time_fraction_scaled: f64,
}

impl Default for Timer {
    /// `CTimer::Initialise`.
    fn default() -> Self {
        Self {
            time_in_ms: 0,
            time_in_ms_pause_mode: 1,
            time_in_ms_non_clipped: 0,
            previous_time_in_ms: 0,
            frame_counter: 0,
            time_scale: 1.0,
            time_step: 1.0,
            time_step_non_clipped: 1.0,
            logical_frame_counter: 0,
            logical_frames_passed: 0,
            user_pause: false,
            code_pause: false,
            playing_back: false,
            recording_chase: false,
            frame_time_logical: 0.0,
            frame_time_fraction: 0.0,
            frame_time_fraction_scaled: 0.0,
        }
    }
}

impl Timer {
    pub fn is_paused(&self) -> bool {
        self.user_pause || self.code_pause
    }

    pub fn time_step_in_seconds(&self) -> f32 {
        self.time_step / 50.0
    }

    pub fn time_step_in_milliseconds(&self) -> u32 {
        (self.time_step / 50.0 * 1000.0) as u32
    }

    pub fn default_time_step() -> f32 {
        50.0 / 30.0
    }

    /// Step relative to a 30 fps frame (`GetTimeStepFix`).
    pub fn time_step_fix(&self) -> f32 {
        self.time_step / Self::default_time_step()
    }

    pub fn is_slow_motion(&self) -> bool {
        self.time_scale < 1.0
    }

    /// `CTimer::Update` with `elapsed_ms` of real time since the last call.
    pub fn update(&mut self, elapsed_ms: f64) {
        self.previous_time_in_ms = self.time_in_ms;
        let frame_time = if self.is_paused() { elapsed_ms } else { elapsed_ms * self.time_scale as f64 };

        // count frames as if running at 30 fps
        self.logical_frames_passed = 0;
        self.frame_time_logical += elapsed_ms;
        while self.frame_time_logical >= 1000.0 / 30.0 {
            self.frame_time_logical -= 1000.0 / 30.0;
            self.logical_frames_passed += 1;
        }
        self.logical_frame_counter += self.logical_frames_passed;

        self.frame_time_fraction += elapsed_ms;
        self.frame_time_fraction_scaled += frame_time;
        self.time_in_ms_pause_mode = self.time_in_ms_pause_mode.wrapping_add(self.frame_time_fraction as u32);
        if self.is_paused() {
            self.time_step = 0.0;
        } else {
            self.time_in_ms = self.time_in_ms.wrapping_add(self.frame_time_fraction_scaled as u32);
            self.time_in_ms_non_clipped = self.time_in_ms_non_clipped.wrapping_add(self.frame_time_fraction_scaled as u32);
            self.time_step = (frame_time / 1000.0 * 50.0) as f32;
        }
        self.frame_time_fraction -= (self.frame_time_fraction as u32) as f64;
        self.frame_time_fraction_scaled -= (self.frame_time_fraction_scaled as u32) as f64;

        if self.time_step < 0.01 && !self.is_paused() {
            self.time_step = 0.01;
        }
        self.time_step_non_clipped = self.time_step;
        if !self.playing_back {
            self.time_step = self.time_step.min(3.0);
            if self.time_in_ms.wrapping_sub(self.previous_time_in_ms) > 60 {
                self.time_in_ms = self.previous_time_in_ms + 60;
            }
        }
        if self.recording_chase {
            self.time_step = 1.0;
            self.time_in_ms = self.previous_time_in_ms + 16;
        }
        self.frame_counter += 1;
    }

    /// `CTimer::Stop`.
    pub fn stop(&mut self) {
        self.previous_time_in_ms = self.time_in_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twenty_ms_frames_give_a_step_of_one() {
        let mut t = Timer::default();
        t.update(20.0);
        assert!((t.time_step - 1.0).abs() < 1e-6);
        assert_eq!((t.time_in_ms, t.frame_counter), (20, 1));
        // 30 fps = 33.3 ms: step 1.6667, one logical frame per real frame
        let mut t = Timer::default();
        for _ in 0..30 {
            t.update(1000.0 / 30.0 + 1e-9);
        }
        assert_eq!(t.logical_frame_counter, 30);
    }

    #[test]
    fn long_frames_are_clamped() {
        let mut t = Timer::default();
        t.update(500.0);
        assert_eq!(t.time_step, 3.0);
        assert_eq!(t.time_step_non_clipped, 25.0);
        assert_eq!(t.time_in_ms, 60);
        t.playing_back = true;
        t.update(500.0);
        assert_eq!(t.time_step, 25.0);
        assert_eq!(t.time_in_ms, 560);
    }

    #[test]
    fn pause_and_scale() {
        let mut t = Timer { time_scale: 0.5, ..Default::default() };
        t.update(40.0);
        assert!((t.time_step - 1.0).abs() < 1e-6, "slow motion halves the step");
        assert!(t.is_slow_motion());
        t.code_pause = true;
        let before = t.time_in_ms;
        t.update(40.0);
        assert_eq!((t.time_step, t.time_in_ms), (0.0, before));
        assert!(t.time_in_ms_pause_mode > 1, "pause-mode clock keeps running");
        t.code_pause = false;
        t.recording_chase = true;
        t.update(40.0);
        assert_eq!((t.time_step, t.time_in_ms), (1.0, t.previous_time_in_ms + 16));
    }

    #[test]
    fn tiny_frames_have_a_floor() {
        let mut t = Timer::default();
        t.update(0.001);
        assert_eq!(t.time_step, 0.01);
    }
}
