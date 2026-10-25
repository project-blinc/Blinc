use blinc_animation::{Easing, KeyframeProperties, MultiKeyframeAnimation};

fn animation() -> MultiKeyframeAnimation {
    MultiKeyframeAnimation::new(100)
        .iterations(2)
        .keyframe(0.0, KeyframeProperties::opacity(0.0), Easing::Linear)
        .keyframe(1.0, KeyframeProperties::opacity(1.0), Easing::Linear)
}

#[test]
fn playback_identity_changes_on_construction_and_explicit_start() {
    let mut animation = animation();
    let created = animation.playback_id();
    assert_ne!(created, MultiKeyframeAnimation::new(100).playback_id());
    animation.start();
    let running = animation.playback_id();
    assert_ne!(running, created);
    animation.tick(40.0);
    assert!((animation.current_properties().opacity.unwrap() - 0.4).abs() < 1e-5);
    assert_eq!(animation.playback_id(), running);
    animation.tick(70.0);
    assert!(animation.is_playing());
    assert_eq!(animation.current_properties().opacity, Some(0.0));
    assert_eq!(animation.playback_id(), running);
    animation.tick(101.0);
    assert!(!animation.is_playing());
    assert_eq!(animation.current_properties().opacity, Some(1.0));
    assert_eq!(animation.playback_id(), running);
    animation.stop();
    assert_eq!(animation.playback_id(), running);
    animation.start();
    assert_ne!(animation.playback_id(), running);
    assert_eq!(animation.progress(), 0.0);
    assert_eq!(animation.current_properties().opacity, Some(0.0));
}

#[test]
fn playback_snapshot_clones_share_identity_until_started_independently() {
    let mut source = animation();
    source.start();
    source.tick(25.0);
    let mut snapshot = source.clone();
    assert_eq!(snapshot.playback_id(), source.playback_id());
    assert_eq!(snapshot.progress(), source.progress());
    snapshot.tick(10.0);
    assert_eq!(snapshot.playback_id(), source.playback_id());
    assert_ne!(snapshot.progress(), source.progress());
    snapshot.start();
    assert_ne!(snapshot.playback_id(), source.playback_id());
    assert_eq!(source.progress(), 0.25);
}

#[test]
fn playback_ids_remain_distinct_across_threads() {
    let workers: Vec<_> = (0..2)
        .map(|_| {
            std::thread::spawn(|| {
                (0..32)
                    .map(|_| {
                        let mut animation = animation();
                        animation.start();
                        animation.playback_id()
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let ids: Vec<_> = workers
        .into_iter()
        .flat_map(|worker| worker.join().unwrap())
        .collect();
    let unique: std::collections::HashSet<_> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len());
    assert!(!unique.contains(&0));
}
