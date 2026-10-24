//! `CDoor` and `CTrainDoor`: swinging car doors and sliding train doors.
use crate::entity::physical::Physical;
use crate::math::Matrix;
use glam::Vec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DoorState {
    #[default]
    Swinging,
    /// Really "at the max angle".
    Open,
    /// Really "at the min angle".
    Closed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Door {
    pub max_angle: f32,
    pub min_angle: f32,
    /// Direction of rotation for air resistance.
    pub dirn: i8,
    /// Axis the door rotates about.
    pub axis: i8,
    pub state: DoorState,
    pub angle: f32,
    pub prev_angle: f32,
    pub ang_vel: f32,
    pub speed: Vec3,
}

impl Door {
    pub fn init(&mut self, min_angle: f32, max_angle: f32, dirn: i8, axis: i8) {
        self.min_angle = min_angle;
        self.max_angle = max_angle;
        self.dirn = dirn;
        self.axis = axis;
    }

    pub fn open(&mut self, ratio: f32) {
        self.prev_angle = self.angle;
        let open = self.angle_when_open();
        if ratio < 1.0 {
            self.angle = open * ratio;
            if self.angle == 0.0 {
                self.ang_vel = 0.0;
            }
        } else {
            self.state = DoorState::Open;
            self.angle = open;
        }
    }

    /// Swings the door with the air resistance of the vehicle's motion.
    pub fn process(&mut self, vehicle: &Physical, matrix: &Matrix) {
        let speed = vehicle.speed_at(Vec3::new(1.0, 0.0, 0.0));
        let diff = matrix.rotate_vector_inverse(speed - self.speed);
        let mut d = match self.axis {
            0 => if self.dirn != 0 { diff.y + diff.z } else { -(diff.y + diff.z) },
            2 => if self.dirn != 0 { -(diff.y + diff.x) } else { diff.y - diff.x },
            // no y axis support, and the original leaves the value uninitialised
            _ => 0.0,
        };
        d = d.clamp(-0.2, 0.2);
        if d.abs() > 0.002 {
            self.ang_vel += d;
        }
        self.ang_vel *= 0.945;
        self.ang_vel = self.ang_vel.clamp(-0.3, 0.3);

        self.angle += self.ang_vel;
        self.state = DoorState::Swinging;
        if self.angle > self.max_angle {
            self.angle = self.max_angle;
            self.ang_vel *= -0.8;
            self.state = DoorState::Open;
        }
        if self.angle < self.min_angle {
            self.angle = self.min_angle;
            self.ang_vel *= -0.8;
            self.state = DoorState::Closed;
        }
        self.speed = speed;
    }

    pub fn angle_when_closed(&self) -> f32 {
        if self.max_angle.abs() < self.min_angle.abs() { self.max_angle } else { self.min_angle }
    }

    pub fn angle_when_open(&self) -> f32 {
        if self.max_angle.abs() < self.min_angle.abs() { self.min_angle } else { self.max_angle }
    }

    pub fn angle_open_ratio(&self) -> f32 {
        let open = self.angle_when_open();
        if open == 0.0 { 0.0 } else { self.angle / open }
    }

    /// Within about 28 degrees of fully open.
    pub fn is_fully_open(&self) -> bool {
        self.angle.abs() >= self.angle_when_open().abs() - 0.5
    }

    pub fn is_closed(&self) -> bool {
        self.angle == self.angle_when_closed()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TrainDoor {
    pub closed_posn: f32,
    pub open_posn: f32,
    pub dirn: i8,
    pub state: DoorState,
    pub axis: i8,
    pub posn: f32,
    pub prev_posn: f32,
}

impl TrainDoor {
    pub fn init(&mut self, open: f32, closed: f32, dirn: i8, axis: i8) {
        self.open_posn = open;
        self.closed_posn = closed;
        self.dirn = dirn;
        self.axis = axis;
    }

    pub fn open(&mut self, ratio: f32) {
        self.prev_posn = self.posn;
        let open = self.translation_when_open();
        if ratio < 1.0 {
            self.posn = open * ratio;
        } else {
            self.state = DoorState::Open;
            self.posn = open;
        }
    }

    pub fn translation_when_closed(&self) -> f32 {
        if self.closed_posn.abs() < self.open_posn.abs() { self.closed_posn } else { self.open_posn }
    }

    pub fn translation_when_open(&self) -> f32 {
        if self.closed_posn.abs() < self.open_posn.abs() { self.open_posn } else { self.closed_posn }
    }

    pub fn is_fully_open(&self) -> bool {
        self.posn.abs() >= self.translation_when_open().abs() - 0.5
    }

    pub fn is_closed(&self) -> bool {
        self.posn == self.translation_when_closed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn door() -> Door {
        let mut d = Door::default();
        d.init(0.0, 1.2, 1, 2);
        d
    }

    #[test]
    fn opening_sets_the_angle_by_ratio() {
        let mut d = door();
        d.open(0.5);
        assert!((d.angle - 0.6).abs() < 1e-6 && d.state == DoorState::Swinging && !d.is_fully_open());
        d.open(1.0);
        assert!(d.state == DoorState::Open && d.is_fully_open() && !d.is_closed());
        d.open(0.0);
        assert!(d.is_closed() && d.ang_vel == 0.0 && d.angle_open_ratio() == 0.0);
        // which end counts as "open" is the one further from zero
        let mut back = Door::default();
        back.init(-1.5, 0.0, 0, 0);
        assert_eq!((back.angle_when_open(), back.angle_when_closed()), (-1.5, 0.0));
    }

    #[test]
    fn a_moving_car_swings_the_door_and_it_bounces_at_the_stops() {
        let mut d = door();
        let car = Physical { mass: 1000.0, turn_mass: 4000.0, move_speed: Vec3::new(0.0, 1.0, 0.0), ..Default::default() };
        for _ in 0..200 {
            d.process(&car, &Matrix::IDENTITY);
            assert!(d.angle >= d.min_angle && d.angle <= d.max_angle);
        }
        // the first frame sees the whole speed as a change; afterwards only friction acts
        assert!(d.ang_vel.abs() <= 0.3);
    }

    #[test]
    fn train_doors_slide_between_positions() {
        let mut t = TrainDoor::default();
        t.init(1.5, 0.0, 1, 0);
        t.open(0.5);
        assert_eq!(t.posn, 0.75);
        assert!(!t.is_fully_open());
        t.open(1.0);
        assert!(t.is_fully_open() && t.state == DoorState::Open && !t.is_closed());
    }
}
