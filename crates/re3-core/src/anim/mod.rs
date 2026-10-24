//! Skeletal animation: keyframe sequences, blending and the per-clump update
//! (`animation/*.cpp`, PC build: uncompressed keyframes).
//!
//! The original keeps animations on RenderWare frames and uses intrusive lists and callback
//! function pointers; here a clump's skeleton is an [`AnimClump`], associations live in a `Vec`
//! (front = most recently added) and finish/delete callbacks are reported as [`Event`]s.
use crate::math::Quat;
use glam::Vec3;
use re3_formats::{datafile::Game, ifp, rw::Error};
use std::sync::Arc;

pub mod tables_gta3;
pub mod tables_vc;

/// Association flags whose values are the same in both games.
pub mod flags {
    pub const RUNNING: u32 = 1;
    pub const REPEAT: u32 = 2;
    pub const DELETE_FADED_OUT: u32 = 4;
    pub const FADE_OUT_WHEN_DONE: u32 = 8;
    pub const PARTIAL: u32 = 0x10;
    pub const MOVEMENT: u32 = 0x20;
    pub const HAS_TRANSLATION: u32 = 0x40;

    /// Differs between the games (0x1000 in GTA III, 0x80 in Vice City).
    pub fn has_x_translation(game: super::Game) -> u32 {
        if game == super::Game::Vc { 0x80 } else { 0x1000 }
    }
}

/// `AnimBlendFrameData::flag` bits.
pub mod frame_flags {
    pub const IGNORE_ROTATION: u8 = 2;
    pub const IGNORE_TRANSLATION: u8 = 4;
    pub const VELOCITY_EXTRACTION: u8 = 8;
    pub const VELOCITY_EXTRACTION_3D: u8 = 0x10;
    /// Vice City only: keyframes must be re-synchronised after an off-screen update.
    pub const UPDATE_KEYFRAMES: u8 = 0x20;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyFrame {
    pub rotation: Quat,
    /// Time since the previous key frame (the first one keeps its absolute time).
    pub delta_time: f32,
    pub translation: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    pub name: String,
    /// Skeleton bone id, or -1 (match by name).
    pub bone_tag: i32,
    pub has_translation: bool,
    pub keyframes: Vec<KeyFrame>,
}

impl Sequence {
    fn from_file(s: &ifp::Sequence) -> Self {
        let has_translation = s.kind != ifp::TrackKind::Rotation;
        let mut keyframes: Vec<KeyFrame> = s
            .keyframes
            .iter()
            .map(|k| KeyFrame {
                // the file stores the conjugate of the rotation the game uses
                rotation: Quat::new(k.rotation[0], k.rotation[1], k.rotation[2], k.rotation[3]).conjugate(),
                delta_time: k.time,
                translation: if has_translation { Vec3::from(k.translation) } else { Vec3::ZERO },
            })
            .collect();
        for l in (1..keyframes.len()).rev() {
            keyframes[l].delta_time -= keyframes[l - 1].delta_time;
        }
        Self { name: s.name.clone(), bone_tag: s.bone_tag.unwrap_or(-1), has_translation, keyframes }
    }

    /// Flips quaternions so neighbouring key frames lie in the same hemisphere.
    fn remove_quaternion_flips(&mut self) {
        let Some(first) = self.keyframes.first() else { return };
        let mut last = first.rotation;
        for kf in self.keyframes.iter_mut().skip(1) {
            if last.dot(&kf.rotation) < 0.0 {
                kf.rotation = kf.rotation.scaled(-1.0);
            }
            last = kf.rotation;
        }
    }
}

/// A named animation: one sequence per animated bone.
#[derive(Debug, Clone, PartialEq)]
pub struct Hierarchy {
    pub name: String,
    pub sequences: Vec<Sequence>,
    pub total_length: f32,
}

impl Hierarchy {
    fn from_file(a: &ifp::Animation) -> Self {
        let mut sequences: Vec<Sequence> = a.sequences.iter().map(Sequence::from_file).collect();
        for s in &mut sequences {
            s.remove_quaternion_flips();
        }
        let total_length = sequences.iter().map(|s| s.keyframes.iter().map(|k| k.delta_time).sum::<f32>()).fold(0.0, f32::max);
        Self { name: a.name.clone(), sequences, total_length }
    }
}

/// Playback state of one sequence on one skeleton bone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Node {
    pub theta: f32,
    pub inv_sin: f32,
    /// next key frame
    pub frame_a: i32,
    /// previous key frame
    pub frame_b: i32,
    pub remaining_time: f32,
    /// Index into the hierarchy's sequences, if this bone is animated.
    pub sequence: Option<usize>,
}

impl Default for Node {
    fn default() -> Self {
        Self { theta: 0.0, inv_sin: 0.0, frame_a: -1, frame_b: -1, remaining_time: 0.0, sequence: None }
    }
}

/// What a node needs to know about the association that owns it.
#[derive(Clone, Copy)]
struct AssocView {
    running: bool,
    repeating: bool,
    partial: bool,
    time_step: f32,
    blend_amount: f32,
    game: Game,
}

impl AssocView {
    fn blend(&self, weight: f32) -> f32 {
        if self.partial { self.blend_amount } else { self.blend_amount * weight }
    }
}

impl Node {
    fn calc_deltas(&mut self, seq: &Sequence) {
        let (a, b) = (&seq.keyframes[self.frame_a as usize], &seq.keyframes[self.frame_b as usize]);
        let cos = a.rotation.dot(&b.rotation).min(1.0);
        self.theta = cos.acos();
        self.inv_sin = if self.theta == 0.0 { 0.0 } else { 1.0 / self.theta.sin() };
    }

    /// Advances to the next key frame; returns true if the animation looped.
    fn next_key_frame(&mut self, seq: &Sequence, repeating: bool) -> bool {
        if seq.keyframes.len() <= 1 {
            return false;
        }
        let n = seq.keyframes.len() as i32;
        let mut looped = false;
        self.frame_b = self.frame_a;
        while self.remaining_time <= 0.0 {
            self.frame_a += 1;
            if self.frame_a >= n {
                if !repeating {
                    self.frame_a -= 1;
                    self.remaining_time = 0.0;
                    return false;
                }
                looped = true;
                self.frame_a = 0;
            }
            self.remaining_time += seq.keyframes[self.frame_a as usize].delta_time;
        }
        self.frame_b = self.frame_a - 1;
        if self.frame_b < 0 {
            self.frame_b += n;
        }
        self.calc_deltas(seq);
        looped
    }

    /// Sets the node to animation time `t`; false if the end was passed on a non-repeating animation.
    fn find_key_frame(&mut self, seq: &Sequence, t: f32, repeating: bool, game: Game) -> bool {
        let n = seq.keyframes.len() as i32;
        if n < 1 {
            return false;
        }
        let kf = |i: i32| seq.keyframes[i as usize].delta_time;
        self.frame_a = 0;
        self.frame_b = 0;
        let mut t = t;
        if game == Game::Vc {
            if n == 1 {
                self.remaining_time = 0.0;
            } else {
                loop {
                    self.frame_a += 1;
                    if t <= kf(self.frame_a) {
                        break;
                    }
                    t -= kf(self.frame_a);
                    if self.frame_a + 1 >= n {
                        if !repeating {
                            self.calc_deltas(seq);
                            self.remaining_time = 0.0;
                            return false;
                        }
                        self.frame_a = 0;
                    }
                    self.frame_b = self.frame_a;
                }
                self.remaining_time = kf(self.frame_a) - t;
            }
        } else if n >= 2 {
            self.frame_a += 1;
            while t > kf(self.frame_a) {
                t -= kf(self.frame_a);
                self.frame_b = self.frame_a;
                self.frame_a += 1;
                if self.frame_a >= n {
                    if !repeating {
                        return false;
                    }
                    self.frame_a = 0;
                    self.frame_b = 0;
                }
            }
            self.remaining_time = kf(self.frame_a) - t;
        }
        self.calc_deltas(seq);
        true
    }

    /// Current blended `(translation, rotation, looped)`.
    fn update(&mut self, seq: &Sequence, a: AssocView, weight: f32) -> (Vec3, Quat, bool) {
        let mut looped = false;
        let (mut trans, mut rot) = (Vec3::ZERO, Quat::new(0.0, 0.0, 0.0, 0.0));
        if a.running {
            self.remaining_time -= a.time_step;
            if self.remaining_time <= 0.0 {
                looped = self.next_key_frame(seq, a.repeating);
            }
        }
        let blend = a.blend(weight);
        if blend > 0.0 {
            let (ka, kb) = (&seq.keyframes[self.frame_a as usize], &seq.keyframes[self.frame_b as usize]);
            let t = if ka.delta_time == 0.0 { 0.0 } else { (ka.delta_time - self.remaining_time) / ka.delta_time };
            if seq.has_translation {
                trans = kb.translation + (ka.translation - kb.translation) * t;
                trans *= blend;
            }
            rot = Quat::slerp(&kb.rotation, &ka.rotation, self.theta, self.inv_sin, t).scaled(blend);
        }
        (trans, rot, looped)
    }

    fn current_translation(&self, seq: &Sequence, a: AssocView, weight: f32) -> Vec3 {
        let blend = a.blend(weight);
        if blend > 0.0 && seq.has_translation {
            let (ka, kb) = (&seq.keyframes[self.frame_a as usize], &seq.keyframes[self.frame_b as usize]);
            let t = if ka.delta_time == 0.0 && a.game == Game::Vc { 0.0 } else { (ka.delta_time - self.remaining_time) / ka.delta_time };
            return (kb.translation + (ka.translation - kb.translation) * t) * blend;
        }
        Vec3::ZERO
    }

    fn end_translation(&self, seq: &Sequence, a: AssocView, weight: f32) -> Vec3 {
        let blend = a.blend(weight);
        if blend > 0.0 && seq.has_translation {
            return seq.keyframes[seq.keyframes.len() - 1].translation * blend;
        }
        Vec3::ZERO
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackKind {
    None,
    /// Fires once when a non-repeating animation reaches its end (or when it is deleted).
    Finish,
    /// Fires when the association is deleted after fading out.
    Delete,
}

/// A finish/delete callback that fired; the game dispatches on `token`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub kind: CallbackKind,
    pub anim_id: i32,
    pub token: u64,
}

/// One playing (or template) animation on a skeleton.
#[derive(Debug, Clone)]
pub struct Association {
    pub game: Game,
    pub hierarchy: Arc<Hierarchy>,
    /// One node per skeleton bone.
    pub nodes: Vec<Node>,
    pub blend_amount: f32,
    pub blend_delta: f32,
    pub current_time: f32,
    pub speed: f32,
    pub time_step: f32,
    pub anim_id: i32,
    pub flags: u32,
    /// Vice City: the animation group this belongs to.
    pub group_id: i32,
    pub callback: CallbackKind,
    pub callback_token: u64,
}

impl Association {
    fn blank(game: Game, hierarchy: Arc<Hierarchy>) -> Self {
        Self {
            game,
            hierarchy,
            nodes: vec![],
            blend_amount: 1.0,
            blend_delta: 0.0,
            current_time: 0.0,
            speed: 1.0,
            time_step: 0.0,
            anim_id: -1,
            flags: 0,
            group_id: -1,
            callback: CallbackKind::None,
            callback_token: 0,
        }
    }

    /// `Init(clump, hierarchy)`: match each sequence to a skeleton bone.
    pub fn new(game: Game, clump: &AnimClump, hierarchy: Arc<Hierarchy>) -> Self {
        let mut a = Self::blank(game, hierarchy);
        a.nodes = vec![Node::default(); clump.frames.len()];
        for (i, seq) in a.hierarchy.sequences.iter().enumerate() {
            let frame = if seq.bone_tag == -1 || game == Game::Gta3 { clump.find_frame(&seq.name) } else { clump.find_bone(seq.bone_tag) };
            if let Some(f) = frame
                && !seq.keyframes.is_empty() {
                    a.nodes[f].sequence = Some(i);
                }
        }
        a
    }

    /// A fresh playable copy of a template association (`Init(CAnimBlendAssociation&)`).
    pub fn copy_of(&self) -> Self {
        let mut a = Self::blank(self.game, self.hierarchy.clone());
        a.nodes = self.nodes.clone();
        a.flags = self.flags;
        a.anim_id = self.anim_id;
        a.group_id = self.group_id;
        a
    }

    pub fn is_running(&self) -> bool {
        self.flags & flags::RUNNING != 0
    }
    pub fn is_repeating(&self) -> bool {
        self.flags & flags::REPEAT != 0
    }
    pub fn is_partial(&self) -> bool {
        self.flags & flags::PARTIAL != 0
    }
    pub fn is_movement(&self) -> bool {
        self.flags & flags::MOVEMENT != 0
    }
    pub fn has_translation(&self) -> bool {
        self.flags & flags::HAS_TRANSLATION != 0
    }
    pub fn has_x_translation(&self) -> bool {
        self.flags & flags::has_x_translation(self.game) != 0
    }
    pub fn time_left(&self) -> f32 {
        self.hierarchy.total_length - self.current_time
    }
    /// Vice City only.
    pub fn progress(&self) -> f32 {
        self.current_time / self.hierarchy.total_length
    }

    pub fn set_blend(&mut self, amount: f32, delta: f32) {
        self.blend_amount = amount;
        self.blend_delta = delta;
    }

    pub fn set_finish_callback(&mut self, token: u64) {
        self.callback = CallbackKind::Finish;
        self.callback_token = token;
    }

    pub fn set_delete_callback(&mut self, token: u64) {
        self.callback = CallbackKind::Delete;
        self.callback_token = token;
    }

    fn view(&self) -> AssocView {
        AssocView {
            running: self.is_running(),
            repeating: self.is_repeating(),
            partial: self.is_partial(),
            time_step: self.time_step,
            blend_amount: self.blend_amount,
            game: self.game,
        }
    }

    fn sync_nodes_to(&mut self, t: f32) {
        let (view, hier) = (self.view(), self.hierarchy.clone());
        for n in &mut self.nodes {
            if let Some(s) = n.sequence {
                n.find_key_frame(&hier.sequences[s], t, view.repeating, self.game);
            }
        }
    }

    pub fn set_current_time(&mut self, time: f32) {
        let total = self.hierarchy.total_length;
        self.current_time = time;
        // shortcut: a zero-length animation would loop forever here, so it is left at `time`
        while total > 0.0 && self.current_time >= total {
            if !self.is_repeating() {
                if self.game == Game::Vc {
                    self.current_time = total;
                    break;
                }
                return;
            }
            self.current_time -= total;
        }
        self.sync_nodes_to(self.current_time);
    }

    pub fn sync_animation(&mut self, other: &Association) {
        self.set_current_time(other.current_time / other.hierarchy.total_length * self.hierarchy.total_length);
    }

    pub fn start(&mut self, time: f32) {
        self.flags |= flags::RUNNING;
        self.set_current_time(time);
    }

    /// Vice City computes the step separately, before the frames are updated.
    pub fn update_time_step(&mut self, time_delta: f32, rel_speed: f32) {
        if self.is_running() {
            self.time_step = (if self.is_movement() { rel_speed * self.hierarchy.total_length } else { self.speed }) * time_delta;
        }
    }

    pub fn update_time(&mut self, time_delta: f32, rel_speed: f32, events: &mut Vec<Event>) {
        if !self.is_running() {
            return;
        }
        let total = self.hierarchy.total_length;
        if self.game == Game::Vc {
            if self.current_time >= total {
                self.flags &= !flags::RUNNING;
                return;
            }
        } else {
            self.time_step = (if self.is_movement() { rel_speed * total } else { self.speed }) * time_delta;
        }
        self.current_time += self.time_step;
        if self.current_time >= total {
            if self.is_repeating() {
                self.current_time -= total;
            } else {
                self.current_time = total;
                if self.game == Game::Gta3 {
                    self.flags &= !flags::RUNNING;
                }
                if self.flags & flags::FADE_OUT_WHEN_DONE != 0 {
                    self.flags |= flags::DELETE_FADED_OUT;
                    self.blend_delta = -4.0;
                }
                if self.callback == CallbackKind::Finish {
                    self.callback = CallbackKind::None;
                    events.push(Event { kind: CallbackKind::Finish, anim_id: self.anim_id, token: self.callback_token });
                }
            }
        }
    }

    /// Returns false if the association faded out and must be removed.
    pub fn update_blend(&mut self, time_delta: f32, events: &mut Vec<Event>) -> bool {
        self.blend_amount += self.blend_delta * time_delta;
        if self.blend_amount <= 0.0 && self.blend_delta < 0.0 {
            self.blend_amount = 0.0;
            self.blend_delta = self.blend_delta.max(0.0);
            if self.flags & flags::DELETE_FADED_OUT != 0 {
                if matches!(self.callback, CallbackKind::Finish | CallbackKind::Delete) {
                    events.push(Event { kind: self.callback, anim_id: self.anim_id, token: self.callback_token });
                }
                return false;
            }
        }
        if self.blend_amount > 1.0 {
            self.blend_amount = 1.0;
            self.blend_delta = self.blend_delta.min(0.0);
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameData {
    pub flag: u8,
    /// Rest-pose position, added to the animated translation.
    pub reset_pos: Vec3,
    pub name: String,
    /// Skeleton bone id (-1 if unknown).
    pub node_id: i32,
}

/// The result of animating one frame: set `None` when the frame ignores that component.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameOut {
    pub rotation: Option<Quat>,
    pub translation: Option<Vec3>,
}

/// A skeleton with the animations playing on it (`CAnimBlendClumpData`).
#[derive(Debug, Clone)]
pub struct AnimClump {
    pub game: Game,
    /// In skeleton traversal order; frame 0 extracts velocity.
    pub frames: Vec<FrameData>,
    /// Newest first.
    pub associations: Vec<Association>,
    /// Set to enable root velocity extraction; receives the extracted velocity (2D uses x, y).
    pub velocity: Option<Vec3>,
}

pub const MAX_BLENDED: usize = 16;

impl AnimClump {
    pub fn new(game: Game, frames: Vec<FrameData>) -> Self {
        let mut c = Self { game, frames, associations: vec![], velocity: None };
        if let Some(f) = c.frames.first_mut() {
            f.flag |= frame_flags::VELOCITY_EXTRACTION;
        }
        c
    }

    /// Builds the skeleton from a model's frames (depth-first, skipping the clump's root frame).
    pub fn from_dff(game: Game, dff: &re3_formats::dff::Dff) -> Self {
        fn visit(dff: &re3_formats::dff::Dff, parent: i32, out: &mut Vec<FrameData>) {
            for (i, f) in dff.frames.iter().enumerate() {
                if f.parent == parent {
                    out.push(FrameData {
                        flag: 0,
                        reset_pos: Vec3::from(f.position),
                        name: f.name.clone(),
                        node_id: f.hanim.as_ref().map_or(-1, |h| h.node_id),
                    });
                    visit(dff, i as i32, out);
                }
            }
        }
        let mut frames = vec![];
        visit(dff, 0, &mut frames);
        Self::new(game, frames)
    }

    /// Last frame whose name matches (case-insensitive).
    pub fn find_frame(&self, name: &str) -> Option<usize> {
        self.frames.iter().rposition(|f| f.name.eq_ignore_ascii_case(name))
    }

    pub fn find_bone(&self, tag: i32) -> Option<usize> {
        self.frames.iter().rposition(|f| f.node_id == tag)
    }

    pub fn association(&self, anim_id: i32) -> Option<&Association> {
        self.associations.iter().find(|a| a.anim_id == anim_id)
    }

    pub fn association_mut(&mut self, anim_id: i32) -> Option<&mut Association> {
        self.associations.iter_mut().find(|a| a.anim_id == anim_id)
    }

    pub fn first_association(&self, mask: u32) -> Option<&Association> {
        self.associations.iter().find(|a| a.flags & mask != 0)
    }

    pub fn set_blend_deltas(&mut self, mask: u32, delta: f32) {
        for a in &mut self.associations {
            if mask == 0 || a.flags & mask != 0 {
                a.blend_delta = delta;
            }
        }
    }

    /// Removes associations (all if `mask` is 0).
    pub fn remove_associations(&mut self, mask: u32) {
        self.associations.retain(|a| !(mask == 0 || a.flags & mask != 0));
    }

    /// The most blended full-body association, plus the runner-up and its blend.
    pub fn main_association(&self) -> (Option<&Association>, Option<&Association>, f32) {
        let (mut main, mut second): (Option<&Association>, Option<&Association>) = (None, None);
        let (mut main_blend, mut second_blend) = (0.0, 0.0);
        for a in self.associations.iter().filter(|a| !a.is_partial()) {
            if a.blend_amount > main_blend {
                second_blend = main_blend;
                main_blend = a.blend_amount;
                second = main;
                main = Some(a);
            } else if a.blend_amount > second_blend {
                second_blend = a.blend_amount;
                second = Some(a);
            }
        }
        (main, second, second_blend)
    }

    pub fn main_partial_association(&self) -> Option<&Association> {
        let mut best: Option<&Association> = None;
        let mut blend = 0.0;
        for a in self.associations.iter().filter(|a| a.is_partial()) {
            if a.blend_amount > blend {
                blend = a.blend_amount;
                best = Some(a);
            }
        }
        best
    }

    /// The `n`th association of the requested kind in list order.
    pub fn nth_association(&self, n: usize, partial: bool) -> Option<&Association> {
        self.associations.iter().filter(|a| a.is_partial() == partial).nth(n)
    }

    /// Advances all animations by `dt`; returns the new pose per frame and any callbacks that fired.
    /// `render` is only meaningful in Vice City: off-screen clumps update just the root velocity.
    pub fn update_animations(&mut self, dt: f32, render: bool) -> (Vec<FrameOut>, Vec<Event>) {
        let game = self.game;
        let mut events = vec![];
        let mut out = vec![FrameOut::default(); self.frames.len()];
        if self.associations.is_empty() {
            return (out, events);
        }
        // blend first; collect the associations that still contribute
        let mut keep = vec![];
        let (mut total_length, mut total_blend) = (0.0f32, 0.0f32);
        let mut foobar = false; // "some contributing animation is not a movement animation"
        let assocs = std::mem::take(&mut self.associations);
        let mut survivors = vec![];
        for mut a in assocs {
            if !a.update_blend(dt, &mut events) {
                continue;
            }
            let contributes = game == Game::Gta3 || !a.hierarchy.sequences.is_empty();
            if contributes {
                if game == Game::Gta3 || keep.len() < 11 {
                    keep.push(survivors.len());
                }
                if a.is_movement() {
                    total_length += a.hierarchy.total_length / a.speed * a.blend_amount;
                    total_blend += a.blend_amount;
                } else {
                    foobar = true;
                }
            }
            survivors.push(a);
        }
        self.associations = survivors;
        let rel_speed = if total_length == 0.0 { 1.0 } else { total_blend / total_length };
        if game == Game::Vc {
            for a in &mut self.associations {
                a.update_time_step(dt, rel_speed);
            }
        }

        let update_keyframes = game == Game::Vc && render && self.frames.first().is_some_and(|f| f.flag & frame_flags::UPDATE_KEYFRAMES != 0);
        if update_keyframes {
            // catch the nodes up after off-screen updates
            for &k in &keep {
                let a = &mut self.associations[k];
                let (t, rep, hier) = (a.current_time - a.time_step, a.is_repeating(), a.hierarchy.clone());
                for (i, n) in a.nodes.iter_mut().enumerate() {
                    if (self.frames[i].flag & frame_flags::VELOCITY_EXTRACTION == 0 || self.velocity.is_none())
                        && let Some(s) = n.sequence {
                            n.find_key_frame(&hier.sequences[s], t, rep, Game::Vc);
                        }
                }
            }
        }
        #[allow(clippy::needless_range_loop)] // update_frame needs &mut self
        for i in 0..self.frames.len() {
            let extract = self.frames[i].flag & frame_flags::VELOCITY_EXTRACTION != 0 && self.velocity.is_some();
            if game == Game::Vc && !render {
                if extract {
                    out[i] = self.update_frame(i, &keep, foobar, Extraction::Planar);
                }
                continue;
            }
            let mode = match (extract, self.frames[i].flag & frame_flags::VELOCITY_EXTRACTION_3D != 0) {
                (false, _) => Extraction::None,
                (true, false) => Extraction::Planar,
                (true, true) => Extraction::Full,
            };
            out[i] = self.update_frame(i, &keep, foobar, mode);
        }
        if game == Game::Vc
            && let Some(f) = self.frames.first_mut() {
                if render {
                    f.flag &= !frame_flags::UPDATE_KEYFRAMES;
                } else {
                    f.flag |= frame_flags::UPDATE_KEYFRAMES;
                }
            }
        for a in &mut self.associations {
            a.update_time(dt, rel_speed, &mut events);
        }
        (out, events)
    }

    /// `FrameUpdateCallBack*` for one frame: blend the contributing nodes into one pose.
    fn update_frame(&mut self, frame: usize, keep: &[usize], foobar: bool, mode: Extraction) -> FrameOut {
        let (flag, reset) = (self.frames[frame].flag, self.frames[frame].reset_pos);
        let mut total_blend_amount = 0.0;
        if foobar {
            for &k in keep {
                let a = &self.associations[k];
                if seq_of(a, frame).is_some() && a.is_partial() {
                    total_blend_amount += a.blend_amount;
                }
            }
        }
        let weight = 1.0 - total_blend_amount;
        let (mut pos, mut rot) = (Vec3::ZERO, Quat::new(0.0, 0.0, 0.0, 0.0));
        let (mut cur, mut trans, mut end) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
        let mut looped = false;
        // translation the nodes are about to leave behind (read before they advance)
        if mode != Extraction::None {
            for &k in keep {
                let a = &self.associations[k];
                if let Some(seq) = seq_of(a, frame)
                    && seq.has_translation && a.has_translation() {
                        let v = a.nodes[frame].current_translation(seq, a.view(), weight);
                        cur.y += v.y;
                        if mode == Extraction::Full {
                            cur.x += v.x;
                            cur.z += v.z;
                        } else if a.has_x_translation() {
                            cur.x += v.x;
                        }
                    }
            }
        }
        for &k in keep {
            let a = &mut self.associations[k];
            let Some(s) = a.nodes[frame].sequence else { continue };
            let (view, hier) = (a.view(), a.hierarchy.clone());
            let seq = &hier.sequences[s];
            let (vec, q, nodelooped) = a.nodes[frame].update(seq, view, weight);
            if rot.dot(&q) < 0.0 {
                rot = Quat::new(rot.x - q.x, rot.y - q.y, rot.z - q.z, rot.w - q.w);
            } else {
                rot = Quat::new(rot.x + q.x, rot.y + q.y, rot.z + q.z, rot.w + q.w);
            }
            if seq.has_translation {
                pos += vec;
                if mode != Extraction::None && a.has_translation() {
                    trans.y += vec.y;
                    if mode == Extraction::Full {
                        trans.x += vec.x;
                        trans.z += vec.z;
                    } else if a.has_x_translation() {
                        trans.x += vec.x;
                    }
                    looped |= nodelooped;
                    if nodelooped {
                        let e = a.nodes[frame].end_translation(seq, view, weight);
                        end.y += e.y;
                        if mode == Extraction::Full {
                            end.x += e.x;
                            end.z += e.z;
                        } else if a.has_x_translation() {
                            end.x += e.x;
                        }
                    }
                }
            }
        }
        let mut out = FrameOut::default();
        if flag & frame_flags::IGNORE_ROTATION == 0 {
            rot.normalise();
            out.rotation = Some(rot);
        }
        if flag & frame_flags::IGNORE_TRANSLATION == 0 {
            match mode {
                Extraction::None => out.translation = Some(pos + reset),
                Extraction::Planar => {
                    let mut v = Vec3::new(trans.x - cur.x, trans.y - cur.y, 0.0);
                    if looped {
                        v.x += end.x;
                        v.y += end.y;
                    }
                    self.velocity = Some(v);
                    let mut p = Vec3::new(pos.x - trans.x, pos.y - trans.y, pos.z);
                    if p.z >= -0.8 {
                        if p.z < -0.4 {
                            p.z += (2.5 * p.z + 2.0) * reset.z;
                        } else {
                            p.z += reset.z;
                        }
                    }
                    p.x += reset.x;
                    p.y += reset.y;
                    out.translation = Some(p);
                }
                Extraction::Full => {
                    let mut v = trans - cur;
                    if looped {
                        v += end;
                    }
                    self.velocity = Some(v);
                    out.translation = Some((pos - trans) + reset);
                }
            }
        }
        out
    }
}

fn seq_of(a: &Association, frame: usize) -> Option<&Sequence> {
    a.nodes[frame].sequence.map(|s| &a.hierarchy.sequences[s])
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Extraction {
    None,
    Planar,
    Full,
}

/// A block of hierarchies loaded from one animation file.
#[derive(Debug, Clone)]
pub struct Block {
    pub name: String,
    pub animations: Vec<Arc<Hierarchy>>,
}

/// An animation group: associations for the animations a character type can play.
#[derive(Debug, Clone, Default)]
pub struct AssocGroup {
    pub associations: Vec<Association>,
}

/// One entry of the group tables (see `tables_*.rs`).
pub struct GroupDefinition<'a> {
    pub name: &'a str,
    pub block: &'a str,
    pub anims: &'a [&'a str],
    pub descs: &'a [(u32, u32)],
}

#[derive(Debug)]
pub struct AnimManager {
    pub game: Game,
    pub blocks: Vec<Block>,
    pub groups: Vec<AssocGroup>,
}

impl AnimManager {
    pub fn new(game: Game) -> Self {
        Self { game, blocks: vec![], groups: vec![] }
    }

    /// `LoadAnimFile`. A block with the same name is replaced (Vice City reloads streamed blocks).
    pub fn load_anim_file(&mut self, data: &[u8]) -> Result<(), Error> {
        for b in ifp::parse_ifp(data)? {
            let block = Block { name: b.name, animations: b.animations.iter().map(|a| Arc::new(Hierarchy::from_file(a))).collect() };
            match self.blocks.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&block.name)) {
                Some(slot) => *slot = block,
                None => self.blocks.push(block),
            }
        }
        Ok(())
    }

    pub fn remove_last_anim_file(&mut self) {
        self.blocks.pop();
    }

    pub fn block(&self, name: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| b.name.eq_ignore_ascii_case(name))
    }

    pub fn animation(&self, block: &Block, name: &str) -> Option<Arc<Hierarchy>> {
        block.animations.iter().find(|h| h.name.eq_ignore_ascii_case(name)).cloned()
    }

    /// Builds group `index` for `def` against a template skeleton (`CreateAssociations`).
    /// Animations missing from the block are skipped; their id slots keep the previous order.
    pub fn create_group(&mut self, index: usize, def: &GroupDefinition, template: &AnimClump) -> Result<(), String> {
        let block = self.block(def.block).ok_or_else(|| format!("animation block {} is not loaded", def.block))?;
        let mut associations = Vec::with_capacity(def.anims.len());
        for (j, name) in def.anims.iter().enumerate() {
            let hier = self.animation(block, name).ok_or_else(|| format!("animation {name} missing from block {}", def.block))?;
            let mut a = Association::new(self.game, template, hier);
            a.anim_id = j as i32;
            if let Some(&(_, f)) = def.descs.get(j) {
                a.flags |= f;
            }
            if self.game == Game::Vc {
                a.group_id = index as i32;
            }
            associations.push(a);
        }
        if self.groups.len() <= index {
            self.groups.resize_with(index + 1, AssocGroup::default);
        }
        self.groups[index] = AssocGroup { associations };
        Ok(())
    }

    /// Template association of a group by id.
    pub fn assoc(&self, group: usize, anim: usize) -> Option<&Association> {
        self.groups.get(group)?.associations.get(anim)
    }

    pub fn assoc_by_name(&self, group: usize, name: &str) -> Option<&Association> {
        self.groups.get(group)?.associations.iter().find(|a| a.hierarchy.name.eq_ignore_ascii_case(name))
    }

    /// `CreateAnimAssociation`: a playable copy.
    pub fn create_association(&self, group: usize, anim: usize) -> Option<Association> {
        self.assoc(group, anim).map(Association::copy_of)
    }

    /// `AddAnimation`: starts an animation, syncing movement animations with the current one.
    pub fn add_animation(&self, clump: &mut AnimClump, group: usize, anim: usize) -> Option<usize> {
        let mut a = self.create_association(group, anim)?;
        if a.is_movement() {
            match clump.associations.iter().find(|x| x.is_movement()) {
                Some(sync) => {
                    a.sync_animation(sync);
                    a.flags |= flags::RUNNING;
                }
                None => a.start(0.0),
            }
        } else {
            a.start(0.0);
        }
        clump.associations.insert(0, a);
        Some(0)
    }

    fn add_animation_and_sync(&self, clump: &mut AnimClump, sync: Option<&Association>, group: usize, anim: usize) -> Option<()> {
        let mut a = self.create_association(group, anim)?;
        match sync {
            Some(s) if a.is_movement() => {
                a.sync_animation(s);
                a.flags |= flags::RUNNING;
            }
            _ => a.start(0.0),
        }
        clump.associations.insert(0, a);
        Some(())
    }

    /// `BlendAnimation`: fade `anim` in (rate `delta`) and everything of the same kind out.
    /// Returns the index of the association in the clump's list.
    pub fn blend_animation(&self, clump: &mut AnimClump, group: usize, anim: usize, delta: f32) -> Option<usize> {
        let template = self.assoc(group, anim)?;
        let (is_movement, is_partial) = (template.is_movement(), template.is_partial());
        let mut remove_prev = false;
        let mut found: Option<usize> = None;
        let mut movement: Option<usize> = None;
        for (i, a) in clump.associations.iter_mut().enumerate() {
            if is_movement && a.is_movement() {
                movement = Some(i);
            }
            if a.anim_id == anim as i32 {
                found = Some(i);
            } else if is_partial == a.is_partial() {
                if a.blend_amount > 0.0 {
                    let blend_delta = -delta * a.blend_amount;
                    if blend_delta < a.blend_delta || !is_partial {
                        a.blend_delta = blend_delta;
                    }
                } else {
                    a.blend_delta = -1.0;
                }
                a.flags |= flags::DELETE_FADED_OUT;
                remove_prev = true;
            }
        }
        if let Some(i) = found {
            let a = &mut clump.associations[i];
            a.blend_delta = (1.0 - a.blend_amount) * delta;
            if !a.is_running() && a.current_time == a.hierarchy.total_length {
                a.start(0.0);
            }
            return Some(i);
        }
        let sync = movement.map(|i| clump.associations[i].clone());
        self.add_animation_and_sync(clump, sync.as_ref(), group, anim)?;
        let a = &mut clump.associations[0];
        if !remove_prev && !is_partial {
            a.blend_amount = 1.0;
        } else {
            a.blend_amount = 0.0;
            a.blend_delta = delta;
        }
        Some(0)
    }
}

#[cfg(test)]
mod tests;
