use super::*;
use re3_formats::ifp::{Animation, Keyframe, Sequence as FileSeq, TrackKind};

fn kf(t: f32, rot: [f32; 4], tr: [f32; 3]) -> Keyframe {
    Keyframe { rotation: rot, translation: tr, scale: [1.0; 3], time: t }
}

fn file_anim(name: &str, bones: &[&str], kind: TrackKind, frames: &[(f32, [f32; 4], [f32; 3])]) -> Animation {
    Animation {
        name: name.into(),
        sequences: bones
            .iter()
            .map(|b| FileSeq { name: (*b).into(), bone_tag: None, kind, keyframes: frames.iter().map(|&(t, r, v)| kf(t, r, v)).collect() })
            .collect(),
    }
}

fn skeleton(game: Game, names: &[&str]) -> AnimClump {
    AnimClump::new(game, names.iter().map(|n| FrameData { flag: 0, reset_pos: Vec3::ZERO, name: (*n).into(), node_id: -1 }).collect())
}

// the file stores conjugates, so (0,0,0,1) is the identity either way
const ID: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

#[test]
fn file_data_becomes_runtime_form() {
    // second rotation is stored in the opposite hemisphere: the loader flips it back
    let a = file_anim("walk", &["Root"], TrackKind::RotationTranslation, &[(0.0, [0.1, 0.0, 0.0, 0.99], [0.0; 3]), (0.5, [-0.1, 0.0, 0.0, -0.99], [1.0, 0.0, 0.0]), (1.0, [0.1, 0.0, 0.0, 0.99], [2.0, 0.0, 0.0])]);
    let h = Hierarchy::from_file(&a);
    let s = &h.sequences[0];
    assert!(s.has_translation);
    // conjugated, and times are now deltas
    assert_eq!(s.keyframes[0].rotation, Quat::new(-0.1, 0.0, 0.0, 0.99));
    assert_eq!(s.keyframes.iter().map(|k| k.delta_time).collect::<Vec<_>>(), [0.0, 0.5, 0.5]);
    assert!(s.keyframes[0].rotation.dot(&s.keyframes[1].rotation) > 0.0);
    assert_eq!(h.total_length, 1.0);
}

fn walk_manager(game: Game) -> (AnimManager, AnimClump) {
    let mut m = AnimManager::new(game);
    let anim = file_anim("walk", &["Root"], TrackKind::RotationTranslation, &[(0.0, ID, [0.0; 3]), (1.0, ID, [2.0, 0.0, 0.0])]);
    let idle = file_anim("idle", &["Root"], TrackKind::Rotation, &[(0.0, ID, [0.0; 3]), (1.0, ID, [0.0; 3])]);
    m.blocks.push(Block { name: "ped".into(), animations: vec![Arc::new(Hierarchy::from_file(&anim)), Arc::new(Hierarchy::from_file(&idle))] });
    let clump = skeleton(game, &["Root"]);
    let def = GroupDefinition {
        name: "t",
        block: "ped",
        anims: &["walk", "idle"],
        descs: &[(0, flags::REPEAT | flags::MOVEMENT | flags::HAS_TRANSLATION), (1, flags::REPEAT)],
    };
    m.create_group(0, &def, &clump).unwrap();
    (m, clump)
}

#[test]
fn playback_interpolates_translation_and_loops() {
    for game in [Game::Gta3, Game::Vc] {
        let (m, mut clump) = walk_manager(game);
        m.add_animation(&mut clump, 0, 0).unwrap();
        // velocity extraction is off (no velocity slot), so the pose carries the translation
        let (out, _) = clump.update_animations(0.0, true);
        assert_eq!(out[0].translation, Some(Vec3::ZERO), "{game:?}");
        let mut x = vec![];
        for _ in 0..5 {
            let (out, _) = clump.update_animations(0.3, true);
            x.push(out[0].translation.unwrap().x);
        }
        // GTA III poses the frame with the previous update's step (so it lags one step behind);
        // Vice City computes the step first. The 1 s animation covers 2 units and wraps.
        let lag = if game == Game::Gta3 { 0 } else { 1 };
        for (i, v) in x.iter().enumerate() {
            let t = 0.3 * (i + lag) as f32;
            assert!((v - 2.0 * (t % 1.0)).abs() < 1e-4, "{game:?} step {i}: {v} vs {}", 2.0 * (t % 1.0));
        }
        let rot = clump.update_animations(0.1, true).0[0].rotation.unwrap();
        assert!((rot.magnitude_squared() - 1.0).abs() < 1e-5);
    }
}

#[test]
fn non_repeating_animation_finishes_once_and_fires_its_callback() {
    for game in [Game::Gta3, Game::Vc] {
        let (m, mut clump) = walk_manager(game);
        m.add_animation(&mut clump, 0, 1).unwrap();
        let a = clump.association_mut(1).unwrap();
        a.flags &= !flags::REPEAT;
        a.flags |= flags::FADE_OUT_WHEN_DONE;
        a.set_finish_callback(42);
        let mut fired = vec![];
        for _ in 0..40 {
            let (_, ev) = clump.update_animations(0.1, true);
            fired.extend(ev);
        }
        assert!(fired.iter().filter(|e| e.kind == CallbackKind::Finish).count() == 1, "{game:?} {fired:?}");
        assert_eq!(fired[0].token, 42);
        // faded out and removed
        assert!(clump.associations.is_empty(), "{game:?}");
    }
}

#[test]
fn blend_animation_crossfades() {
    let (m, mut clump) = walk_manager(Game::Gta3);
    m.blend_animation(&mut clump, 0, 1, 4.0).unwrap(); // idle, first: full blend immediately
    assert_eq!(clump.associations[0].blend_amount, 1.0);
    m.blend_animation(&mut clump, 0, 0, 4.0).unwrap(); // walk fades in, idle fades out
    assert_eq!((clump.associations[0].anim_id, clump.associations[0].blend_amount, clump.associations[0].blend_delta), (0, 0.0, 4.0));
    let idle = clump.association(1).unwrap();
    assert!(idle.blend_delta < 0.0 && idle.flags & flags::DELETE_FADED_OUT != 0);
    for _ in 0..10 {
        clump.update_animations(0.1, true);
    }
    assert_eq!(clump.associations.len(), 1);
    assert_eq!(clump.associations[0].anim_id, 0);
    assert_eq!(clump.associations[0].blend_amount, 1.0);
    // re-blending the same animation changes nothing structural
    m.blend_animation(&mut clump, 0, 0, 4.0).unwrap();
    assert_eq!(clump.associations.len(), 1);
}

#[test]
fn velocity_extraction_removes_root_motion_from_the_pose() {
    let (m, mut clump) = walk_manager(Game::Gta3);
    clump.velocity = Some(Vec3::ZERO);
    m.add_animation(&mut clump, 0, 0).unwrap();
    clump.update_animations(0.0, true);
    let (out, _) = clump.update_animations(0.5, true);
    // Y translation is extracted into the velocity, X only with the x-translation flag
    let v = clump.velocity.unwrap();
    assert!(v.y.abs() < 1e-6 && v.z == 0.0);
    assert!(out[0].translation.unwrap().y.abs() < 1e-6);
}

#[test]
fn queries() {
    let (m, mut clump) = walk_manager(Game::Gta3);
    m.add_animation(&mut clump, 0, 1).unwrap();
    m.add_animation(&mut clump, 0, 0).unwrap();
    assert_eq!(clump.associations[0].anim_id, 0, "newest first");
    let (main, second, blend) = clump.main_association();
    assert_eq!((main.unwrap().anim_id, second.unwrap().anim_id, blend), (0, 1, 1.0));
    assert!(clump.main_partial_association().is_none());
    assert_eq!(clump.nth_association(1, false).unwrap().anim_id, 1);
    clump.set_blend_deltas(flags::MOVEMENT, -2.0);
    assert_eq!(clump.association(0).unwrap().blend_delta, -2.0);
    assert_eq!(clump.association(1).unwrap().blend_delta, 0.0);
    clump.remove_associations(flags::MOVEMENT);
    assert_eq!(clump.associations.len(), 1);
    clump.remove_associations(0);
    assert!(clump.associations.is_empty());
}

#[test]
fn bones_match_by_name_or_tag() {
    let mut c = skeleton(Game::Vc, &["Root", "Smid", "Spine"]);
    c.frames[2].node_id = 5;
    assert_eq!(c.find_frame("smid"), Some(1));
    assert_eq!(c.find_frame("nope"), None);
    assert_eq!(c.find_bone(5), Some(2));
    let h = Arc::new(Hierarchy {
        name: "a".into(),
        total_length: 1.0,
        sequences: vec![Sequence { name: "ignored".into(), bone_tag: 5, has_translation: false, keyframes: vec![KeyFrame { rotation: Quat::new(0., 0., 0., 1.), delta_time: 0.0, translation: Vec3::ZERO }] }],
    });
    assert_eq!(Association::new(Game::Vc, &c, h.clone()).nodes[2].sequence, Some(0));
    assert_eq!(Association::new(Game::Gta3, &c, h).nodes[2].sequence, None);
}
