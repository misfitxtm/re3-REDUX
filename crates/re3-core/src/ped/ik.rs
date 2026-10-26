//! `CPedIK`: head, torso and arm orientation for looking and aiming.
use super::clump::PedClump;
use super::data::node;
use crate::anim::frame_flags;
use crate::general::{limit_radian_angle, radian_angle_between_points};
use crate::math::Matrix;
use glam::Vec3;
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LimbOrientation {
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct LimbMovementInfo {
    pub max_yaw: f32,
    pub min_yaw: f32,
    pub yaw_d: f32,
    pub max_pitch: f32,
    pub min_pitch: f32,
    pub pitch_d: f32,
}

const fn rad(d: f32) -> f32 {
    d * PI / 180.0
}

pub const TORSO_INFO: LimbMovementInfo = LimbMovementInfo { max_yaw: rad(50.0), min_yaw: rad(-50.0), yaw_d: rad(15.0), max_pitch: rad(45.0), min_pitch: rad(-45.0), pitch_d: rad(7.0) };
pub const HEAD_INFO: LimbMovementInfo = LimbMovementInfo { max_yaw: rad(90.0), min_yaw: rad(-90.0), yaw_d: rad(10.0), max_pitch: rad(45.0), min_pitch: rad(-45.0), pitch_d: rad(5.0) };
pub const HEAD_RESTORE_INFO: LimbMovementInfo = HEAD_INFO;
pub const UPPER_ARM_INFO: LimbMovementInfo = LimbMovementInfo { max_yaw: rad(20.0), min_yaw: rad(-100.0), yaw_d: rad(20.0), max_pitch: rad(70.0), min_pitch: rad(-70.0), pitch_d: rad(10.0) };
pub const LOWER_ARM_INFO: LimbMovementInfo = LimbMovementInfo { max_yaw: rad(80.0), min_yaw: rad(0.0), yaw_d: rad(20.0), max_pitch: rad(90.0), min_pitch: rad(-90.0), pitch_d: rad(5.0) };

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimbMoveStatus {
    /// The wanted angles were out of reach.
    SetToMax,
    OneAngleCouldntBeSetExactly,
    SetExactly,
}

pub mod ik_flag {
    pub const GUN_POINTED_SUCCESSFULLY: i32 = 1;
    pub const LOOKAROUND_HEAD_ONLY: i32 = 2;
    pub const AIMS_WITH_ARM: i32 = 4;
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PedIk {
    pub head_orient: LimbOrientation,
    pub torso_orient: LimbOrientation,
    pub upper_arm_orient: LimbOrientation,
    pub lower_arm_orient: LimbOrientation,
    pub flags: i32,
}

/// What the IK reads of the ped it works on.
pub struct IkPed<'a> {
    pub clump: &'a mut PedClump,
    pub rotation_cur: f32,
    pub pos: Vec3,
    /// `TheCamera.Cams[ActiveCam].Using3rdPersonMouseCam()`.
    pub third_person_mouse_cam: bool,
}

/// `CPedIK::ExtractYawAndPitchWorld` (the RenderWare `up` is our `forward`, `at` is `up`).
pub fn extract_yaw_and_pitch_world(m: &Matrix) -> (f32, f32) {
    let f = m.forward.dot(Vec3::Y).clamp(-1.0, 1.0);
    let mut yaw = f.acos();
    if m.forward.x > 0.0 {
        yaw = -yaw;
    }
    let f = m.right.dot(Vec3::Z).clamp(-1.0, 1.0);
    let mut pitch = f.acos();
    if m.forward.z > 0.0 {
        pitch = -pitch;
    }
    (yaw, pitch)
}

/// `CPedIK::ExtractYawAndPitchLocal`.
pub fn extract_yaw_and_pitch_local(m: &Matrix) -> (f32, f32) {
    let f = m.up.dot(Vec3::Z).clamp(-1.0, 1.0);
    let mut yaw = f.acos();
    if m.up.y > 0.0 {
        yaw = -yaw;
    }
    let f = m.right.dot(Vec3::X).clamp(-1.0, 1.0);
    let mut pitch = f.acos();
    if m.forward.x > 0.0 {
        pitch = -pitch;
    }
    (yaw, pitch)
}

impl PedIk {
    /// `MoveLimb`.
    pub fn move_limb(limb: &mut LimbOrientation, target_yaw: f32, target_pitch: f32, info: &LimbMovementInfo) -> LimbMoveStatus {
        let mut result = LimbMoveStatus::OneAngleCouldntBeSetExactly;
        if limb.yaw > target_yaw {
            limb.yaw -= info.yaw_d;
        } else if limb.yaw < target_yaw {
            limb.yaw += info.yaw_d;
        }
        if (limb.yaw - target_yaw).abs() < info.yaw_d {
            limb.yaw = target_yaw;
            result = LimbMoveStatus::SetExactly;
        }
        if limb.yaw > info.max_yaw || limb.yaw < info.min_yaw {
            limb.yaw = limb.yaw.clamp(info.min_yaw, info.max_yaw);
            result = LimbMoveStatus::SetToMax;
        }
        if limb.pitch > target_pitch {
            limb.pitch -= info.pitch_d;
        } else if limb.pitch < target_pitch {
            limb.pitch += info.pitch_d;
        }
        if (limb.pitch - target_pitch).abs() < info.pitch_d {
            limb.pitch = target_pitch;
        } else {
            result = LimbMoveStatus::OneAngleCouldntBeSetExactly;
        }
        if limb.pitch > info.max_pitch || limb.pitch < info.min_pitch {
            limb.pitch = limb.pitch.clamp(info.min_pitch, info.max_pitch);
            result = LimbMoveStatus::SetToMax;
        }
        result
    }

    /// `RotateTorso` for the frame `node` (non-skinned version).
    pub fn rotate_torso(&self, ped: &mut IkPed, node_frame: usize, limb: LimbOrientation, change_roll: bool) {
        let sk = &mut ped.clump.skeleton;
        let parent = sk.parent[node_frame];
        let mat = if parent >= 0 { sk.world_matrix(parent as usize) } else { Matrix::IDENTITY };
        let pos = sk.local[node_frame].pos;
        // RenderWare's right/up/at are our right/forward/up
        let up_vector = Vec3::new(mat.right.z, mat.forward.z, mat.up.z);
        let (c, s) = (ped.rotation_cur.cos(), ped.rotation_cur.sin());
        let right_vector = Vec3::new(-(c * mat.right.x + s * mat.right.y), -(c * mat.forward.x + s * mat.forward.y), -(c * mat.up.x + s * mat.up.y));
        let f = &mut sk.local[node_frame];
        if change_roll {
            let inversed_forward = crate::math::normalise(crate::math::cross(Vec3::Z, mat.forward));
            let dot = mat.up.dot(inversed_forward).clamp(-1.0, 1.0);
            let mut alpha = dot.acos();
            if mat.up.z < 0.0 {
                alpha = -alpha;
            }
            let forward_vector = Vec3::new(s * mat.right.x - c * mat.right.y, s * mat.forward.x - c * mat.forward.y, s * mat.up.x - c * mat.up.y);
            let (cur_yaw, _) = extract_yaw_and_pitch_world(&mat);
            f.rotate_about_axis(right_vector, limb.pitch.to_degrees());
            f.rotate_about_axis(up_vector, (limb.yaw - (cur_yaw - ped.rotation_cur)).to_degrees());
            f.rotate_about_axis(forward_vector, alpha.to_degrees());
        } else {
            f.rotate_about_axis(right_vector, limb.pitch.to_degrees());
            f.rotate_about_axis(up_vector, limb.yaw.to_degrees());
        }
        f.pos = pos;
    }

    /// `RestoreGunPosn`.
    pub fn restore_gun_posn(&mut self, ped: &mut IkPed) -> bool {
        let status = Self::move_limb(&mut self.torso_orient, 0.0, 0.0, &TORSO_INFO);
        let mid = ped.clump.nodes[node::MID];
        self.rotate_torso(ped, mid, self.torso_orient, false);
        status == LimbMoveStatus::SetExactly
    }

    /// `LookInDirection`.
    pub fn look_in_direction(&mut self, ped: &mut IkPed, target_yaw: f32, target_pitch: f32) -> bool {
        let mut success = true;
        let head = ped.clump.nodes[node::HEAD];
        if ped.clump.anim.frames[head].flag & frame_flags::IGNORE_ROTATION == 0 {
            ped.clump.anim.frames[head].flag |= frame_flags::IGNORE_ROTATION;
            let (y, p) = extract_yaw_and_pitch_local(&ped.clump.skeleton.local[head]);
            self.head_orient = LimbOrientation { yaw: y, pitch: p };
        }
        let parent = ped.clump.skeleton.parent[head];
        let world = if parent >= 0 { ped.clump.skeleton.world_matrix(parent as usize) } else { Matrix::IDENTITY };
        let (mut yaw, mut pitch) = extract_yaw_and_pitch_world(&world);
        yaw += self.torso_orient.yaw;
        let needed_yaw_turn = limit_radian_angle(target_yaw - yaw);
        pitch *= needed_yaw_turn.cos();
        let needed_pitch_turn = limit_radian_angle(target_pitch - pitch);
        let head_status = Self::move_limb(&mut self.head_orient, needed_yaw_turn, needed_pitch_turn, &HEAD_INFO);
        if head_status == LimbMoveStatus::SetToMax {
            success = false;
        }
        if head_status != LimbMoveStatus::SetExactly && self.flags & ik_flag::LOOKAROUND_HEAD_ONLY == 0 {
            let remaining = limit_radian_angle(target_yaw - ped.rotation_cur);
            // any status other than "set to max" counts as true in the original (enum value 0 is false)
            if Self::move_limb(&mut self.torso_orient, remaining, target_pitch, &TORSO_INFO) != LimbMoveStatus::SetToMax {
                success = true;
            }
        }
        self.set_head_rotation(ped, head);
        if self.flags & ik_flag::LOOKAROUND_HEAD_ONLY == 0 {
            let mid = ped.clump.nodes[node::MID];
            self.rotate_torso(ped, mid, self.torso_orient, false);
        }
        success
    }

    fn set_head_rotation(&self, ped: &mut IkPed, head: usize) {
        let local = &mut ped.clump.skeleton.local[head];
        let pos = local.pos;
        let mut m = Matrix::IDENTITY;
        m.rotate_z(self.head_orient.pitch);
        m.rotate_x(self.head_orient.yaw);
        m.pos = pos;
        *local = m;
    }

    /// `LookAtPosition`.
    pub fn look_at_position(&mut self, ped: &mut IkPed, pos: Vec3) -> bool {
        let yaw = radian_angle_between_points(pos.x, pos.y, ped.pos.x, ped.pos.y);
        let d = ped.pos - pos;
        let pitch = radian_angle_between_points(pos.z, (d.x * d.x + d.y * d.y).sqrt(), ped.pos.z, 0.0);
        self.look_in_direction(ped, yaw, pitch)
    }

    /// `PointGunInDirection`.
    pub fn point_gun_in_direction(&mut self, ped: &mut IkPed, target_yaw: f32, target_pitch: f32) -> bool {
        let mut result = true;
        let mut arm_pointed = false;
        let mut angle = limit_radian_angle(target_yaw - ped.rotation_cur);
        self.flags &= !ik_flag::GUN_POINTED_SUCCESSFULLY;
        self.flags |= ik_flag::LOOKAROUND_HEAD_ONLY;
        if self.flags & ik_flag::AIMS_WITH_ARM != 0 {
            arm_pointed = self.point_gun_in_direction_using_arm(ped, angle, target_pitch);
            angle = limit_radian_angle(angle - self.upper_arm_orient.yaw);
        }
        if arm_pointed {
            if self.flags & ik_flag::AIMS_WITH_ARM != 0 && self.torso_orient.yaw * self.upper_arm_orient.yaw < 0.0 {
                let pitch = self.torso_orient.pitch;
                Self::move_limb(&mut self.torso_orient, 0.0, pitch, &TORSO_INFO);
            }
        } else {
            match Self::move_limb(&mut self.torso_orient, angle, target_pitch, &TORSO_INFO) {
                LimbMoveStatus::SetToMax => result = false,
                LimbMoveStatus::SetExactly => self.flags |= ik_flag::GUN_POINTED_SUCCESSFULLY,
                _ => {}
            }
        }
        let mid = ped.clump.nodes[node::MID];
        let roll = ped.third_person_mouse_cam && self.flags & ik_flag::AIMS_WITH_ARM != 0;
        self.rotate_torso(ped, mid, self.torso_orient, roll);
        result
    }

    /// `PointGunInDirectionUsingArm`.
    pub fn point_gun_in_direction_using_arm(&mut self, ped: &mut IkPed, target_yaw: f32, target_pitch: f32) -> bool {
        let mut result = false;
        let arm = ped.clump.nodes[node::UPPERARMR];
        let parent = ped.clump.skeleton.parent[arm];
        let matrix = if parent >= 0 { ped.clump.skeleton.world_matrix(parent as usize) } else { Matrix::IDENTITY };
        let up_vector = Vec3::new(matrix.right.z, matrix.forward.z, matrix.up.z);
        let (_, pitch) = extract_yaw_and_pitch_world(&matrix);
        let right_vector = Vec3::Z;
        let forward_vector = Vec3::X;
        let ua_yaw = target_yaw - self.torso_orient.yaw - rad(15.0);
        let ua_pitch = limit_radian_angle(target_pitch - pitch);
        let ua_status = Self::move_limb(&mut self.upper_arm_orient, ua_yaw, ua_pitch, &UPPER_ARM_INFO);
        if ua_status == LimbMoveStatus::SetExactly {
            self.flags |= ik_flag::GUN_POINTED_SUCCESSFULLY;
            result = true;
        }
        if ua_status == LimbMoveStatus::SetToMax {
            let la_yaw = ua_yaw - self.upper_arm_orient.yaw;
            let la_status = if la_yaw > 0.0 {
                Self::move_limb(&mut self.lower_arm_orient, la_yaw, -rad(45.0), &LOWER_ARM_INFO)
            } else {
                Self::move_limb(&mut self.lower_arm_orient, la_yaw, 0.0, &LOWER_ARM_INFO)
            };
            if la_status == LimbMoveStatus::SetExactly {
                self.flags |= ik_flag::GUN_POINTED_SUCCESSFULLY;
                result = true;
            }
            // the forearm is the first child of the upper arm
            if let Some(child) = ped.clump.skeleton.parent.iter().position(|&p| p == arm as i32) {
                let f = &mut ped.clump.skeleton.local[child];
                let pos = f.pos;
                f.rotate_about_axis(forward_vector, self.lower_arm_orient.pitch.to_degrees());
                f.rotate_about_axis(right_vector, (-self.lower_arm_orient.yaw).to_degrees());
                f.pos = pos;
            }
        }
        let f = &mut ped.clump.skeleton.local[arm];
        let pos = f.pos;
        f.rotate_about_axis(right_vector, self.upper_arm_orient.pitch.to_degrees());
        f.rotate_about_axis(up_vector, self.upper_arm_orient.yaw.to_degrees());
        f.pos = pos;
        result
    }

    /// `PointGunAtPosition`.
    pub fn point_gun_at_position(&mut self, ped: &mut IkPed, position: Vec3) -> bool {
        let yaw = radian_angle_between_points(position.x, position.y, ped.pos.x, ped.pos.y);
        let d2 = ((ped.pos.x - position.x).powi(2) + (ped.pos.y - position.y).powi(2)).sqrt();
        let pitch = radian_angle_between_points(position.z, d2, ped.pos.z, 0.0);
        self.point_gun_in_direction(ped, yaw, pitch)
    }

    /// `RestoreLookAt`.
    pub fn restore_look_at(&mut self, ped: &mut IkPed) -> bool {
        let mut result = false;
        let head = ped.clump.nodes[node::HEAD];
        if ped.clump.anim.frames[head].flag & frame_flags::IGNORE_ROTATION != 0 {
            ped.clump.anim.frames[head].flag &= !frame_flags::IGNORE_ROTATION;
        } else {
            let (yaw, pitch) = extract_yaw_and_pitch_local(&ped.clump.skeleton.local[head]);
            if Self::move_limb(&mut self.head_orient, yaw, pitch, &HEAD_RESTORE_INFO) == LimbMoveStatus::SetExactly {
                result = true;
            }
        }
        self.set_head_rotation(ped, head);
        if self.flags & ik_flag::LOOKAROUND_HEAD_ONLY == 0 {
            Self::move_limb(&mut self.torso_orient, 0.0, 0.0, &TORSO_INFO);
            let mid = ped.clump.nodes[node::MID];
            self.rotate_torso(ped, mid, self.torso_orient, false);
        }
        result
    }
}
