//! A ped's skeleton: the animation state plus the local matrices the IK and renderer use.
use crate::anim::AnimClump;
use crate::math::Matrix;

/// Frames in the same order as `AnimClump::frames`, each with its parent frame.
#[derive(Debug, Clone)]
pub struct Skeleton {
    pub parent: Vec<i32>,
    pub local: Vec<Matrix>,
}

impl Skeleton {
    pub fn new(parent: Vec<i32>) -> Self {
        let local = vec![Matrix::IDENTITY; parent.len()];
        Self { parent, local }
    }

    /// `CPedIK::GetWorldMatrix(frame)`: the frame's matrix composed with all its parents.
    pub fn world_matrix(&self, frame: usize) -> Matrix {
        let mut m = self.local[frame];
        let mut p = self.parent[frame];
        while p >= 0 {
            m = self.local[p as usize] * m;
            p = self.parent[p as usize];
        }
        m
    }

    /// Position of a frame in the ped's own space (`GetComponentPosition`).
    pub fn component_position(&self, frame: usize) -> glam::Vec3 {
        self.world_matrix(frame).pos
    }
}

#[derive(Debug, Clone)]
pub struct PedClump {
    pub anim: AnimClump,
    pub skeleton: Skeleton,
    /// Skeleton frame of each `PedNode`.
    pub nodes: [usize; super::data::node::MAX],
}
