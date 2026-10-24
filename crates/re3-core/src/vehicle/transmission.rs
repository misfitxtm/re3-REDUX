//! `cTransmission`: gear table and drive acceleration.
use super::handling::flags::{BOOST_1G, BOOST_2G};
use crate::timer::Timer;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Gear {
    pub max_velocity: f32,
    pub shift_up_velocity: f32,
    pub shift_down_velocity: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Transmission {
    /// Gear 0 is reverse, 1-5 forward.
    pub gears: [Gear; 6],
    pub drive_type: u8,
    pub engine_type: u8,
    pub num_gears: i8,
    pub flags: u8,
    pub engine_acceleration: f32,
    pub max_velocity: f32,
    pub max_cruise_velocity: f32,
    pub max_reverse_velocity: f32,
    pub cur_velocity: f32,
}

impl Transmission {
    /// `InitGearRatios`.
    pub fn init_gear_ratios(&mut self) {
        self.gears = [Gear::default(); 6];
        let n = self.num_gears as i32;
        for i in 1..=n {
            let prev = self.gears[(i - 1) as usize];
            let max = i as f32 / n as f32 * self.max_velocity;
            self.gears[i as usize].max_velocity = max;
            let diff = max - prev.max_velocity;
            if i >= n {
                self.gears[i as usize].shift_up_velocity = self.max_velocity;
            } else {
                self.gears[(i + 1) as usize].shift_down_velocity = diff * 0.42 + prev.max_velocity;
                self.gears[i as usize].shift_up_velocity = diff * 0.6667 + prev.max_velocity;
            }
        }
        self.gears[0] = Gear { max_velocity: self.max_reverse_velocity, shift_up_velocity: -0.01, shift_down_velocity: self.max_reverse_velocity };
        self.gears[1].shift_down_velocity = -0.01;
    }

    /// `CalculateGearForSimpleCar`.
    pub fn gear_for_simple_car(&mut self, speed: f32, gear: &mut u8) {
        let g = self.gears[*gear as usize];
        self.cur_velocity = speed;
        if speed > g.shift_up_velocity {
            *gear += 1;
        } else if speed < g.shift_down_velocity {
            *gear = gear.saturating_sub(1);
        }
    }

    /// `CalculateDriveAcceleration`. `time` is the time left in a gear change.
    pub fn drive_acceleration(&mut self, gas: f32, gear: &mut u8, time: &mut f32, velocity: f32, cheat: bool, timer: &Timer) -> f32 {
        if velocity < self.max_reverse_velocity || velocity > self.max_velocity {
            return 0.0;
        }
        self.cur_velocity = velocity;
        assert!(*gear as i8 <= self.num_gears);

        let g = self.gears[*gear as usize];
        if velocity > g.shift_up_velocity {
            if *gear != 0 || gas > 0.0 {
                *gear += 1;
                *time = 0.0;
                return self.drive_acceleration(gas, gear, time, velocity, false, timer);
            }
        } else if velocity < g.shift_down_velocity && *gear != 0 && (*gear != 1 || gas < 0.0) {
            *gear -= 1;
            *time = 0.0;
            return self.drive_acceleration(gas, gear, time, velocity, false, timer);
        }

        if *time > 0.0 {
            // changing gear: no power
            *time -= timer.time_step_in_seconds();
            return 0.0;
        }
        let (speed_mul, accel_mul);
        if *gear < 1 {
            accel_mul = if self.flags & BOOST_2G as u8 != 0 { 2.0 } else { 1.0 };
            speed_mul = -1.0;
        } else if self.num_gears == 1 {
            accel_mul = 1.0;
            speed_mul = 1.0;
        } else {
            // integer division in the original: 1.0 below top gear's step, 0.0 in the top gear
            let f = 1.0 - ((*gear as i32 - 1) / (self.num_gears as i32 - 1)) as f32;
            speed_mul = 3.0 * f * f + 1.0;
            accel_mul = if self.flags & BOOST_2G as u8 != 0 {
                match *gear {
                    1 => if self.flags & BOOST_1G as u8 != 0 { 3.0 } else { 2.0 },
                    2 => 1.3,
                    _ => 1.0,
                }
            } else if self.flags & BOOST_1G as u8 != 0 && *gear == 1 {
                3.0
            } else {
                1.0
            };
        }
        let boost = if cheat { 1.2 } else { 1.0 };
        let gear_max = self.gears[*gear as usize].max_velocity;
        let target = gear_max * speed_mul * boost;
        let accel = (target - velocity) * (self.engine_acceleration * accel_mul) / target.abs();
        if velocity.abs() < (gear_max * boost).abs() {
            gas * accel * timer.time_step
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timer(step: f32) -> Timer {
        let mut t = Timer::default();
        t.time_step = step;
        t
    }

    fn five_speed() -> Transmission {
        let mut t = Transmission { num_gears: 5, max_velocity: 1.0, max_reverse_velocity: -0.2, engine_acceleration: 0.01, drive_type: b'R', ..Default::default() };
        t.init_gear_ratios();
        t
    }

    #[test]
    fn gear_table_splits_the_top_speed_evenly() {
        let t = five_speed();
        for i in 1..=5 {
            assert!((t.gears[i].max_velocity - i as f32 / 5.0).abs() < 1e-6);
        }
        // shifting up happens 2/3 into a gear, down 42% into the previous one
        assert!((t.gears[1].shift_up_velocity - 0.2 * 0.6667).abs() < 1e-6);
        assert!((t.gears[2].shift_down_velocity - 0.2 * 0.42).abs() < 1e-6);
        assert_eq!(t.gears[5].shift_up_velocity, 1.0);
        assert_eq!((t.gears[0].max_velocity, t.gears[0].shift_up_velocity, t.gears[1].shift_down_velocity), (-0.2, -0.01, -0.01));
    }

    #[test]
    fn simple_cars_shift_with_speed() {
        let mut t = five_speed();
        let mut gear = 1;
        t.gear_for_simple_car(0.5, &mut gear);
        assert_eq!(gear, 2);
        t.gear_for_simple_car(0.0, &mut gear);
        assert_eq!(gear, 1);
        gear = 0;
        t.gear_for_simple_car(-1.0, &mut gear);
        assert_eq!(gear, 0);
    }

    #[test]
    fn drive_acceleration_pushes_toward_the_gear_speed_and_stops_at_the_limit() {
        let mut t = five_speed();
        let (mut gear, mut time) = (1u8, 0.0f32);
        let a = t.drive_acceleration(1.0, &mut gear, &mut time, 0.05, false, &timer(1.0));
        assert!(a > 0.0 && gear == 1);
        // no gas, no push
        assert_eq!(t.drive_acceleration(0.0, &mut gear, &mut time, 0.05, false, &timer(1.0)), 0.0);
        // beyond the top speed nothing is applied
        assert_eq!(t.drive_acceleration(1.0, &mut gear, &mut time, 1.5, false, &timer(1.0)), 0.0);
        // fast enough for second: it changes gear on its own and resets the shift timer
        time = 1.0;
        t.drive_acceleration(1.0, &mut gear, &mut time, 0.3, false, &timer(1.0));
        assert_eq!(gear, 2);
        assert_eq!(time, 0.0);
    }

    #[test]
    fn a_gear_change_in_progress_cuts_the_power() {
        let mut t = five_speed();
        let (mut gear, mut time) = (1u8, 0.5f32);
        assert_eq!(t.drive_acceleration(1.0, &mut gear, &mut time, 0.05, false, &timer(1.0)), 0.0);
        assert!((time - 0.48).abs() < 1e-6);
    }
}
