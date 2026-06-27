use super::flood_mode::{
    apply, evaluate, FloodMode, FloodObservation, FloodThresholds, FloodTransition,
};
use std::time::{Duration, Instant};

fn thresholds() -> FloodThresholds {
    FloodThresholds {
        global_rate_per_sec: 100,
        distinct_sources_threshold: 10,
        auto_expiry: Duration::from_secs(1800),
    }
}

fn obs(global: usize, distinct: usize) -> FloodObservation {
    FloodObservation {
        global_count: global,
        distinct_sources: distinct,
    }
}

#[test]
fn quiet_stays_quiet_below_thresholds() {
    let now = Instant::now();
    let t = evaluate(&FloodMode::Quiet, obs(50, 3), thresholds(), now);
    assert_eq!(t, FloodTransition::NoChange);
}

#[test]
fn quiet_stays_quiet_if_only_global_exceeded() {
    let now = Instant::now();
    let t = evaluate(&FloodMode::Quiet, obs(500, 3), thresholds(), now);
    assert_eq!(t, FloodTransition::NoChange);
}

#[test]
fn quiet_stays_quiet_if_only_distinct_exceeded() {
    let now = Instant::now();
    let t = evaluate(&FloodMode::Quiet, obs(50, 50), thresholds(), now);
    assert_eq!(t, FloodTransition::NoChange);
}

#[test]
fn quiet_transitions_to_ddos_when_both_thresholds_crossed() {
    let now = Instant::now();
    let cfg = thresholds();
    let t = evaluate(&FloodMode::Quiet, obs(200, 15), cfg, now);
    match t {
        FloodTransition::EnteredDdos {
            source_count,
            expires_at,
        } => {
            assert_eq!(source_count, 15);
            assert_eq!(expires_at, now + cfg.auto_expiry);
        }
        other => panic!("expected EnteredDdos, got {other:?}"),
    }
}

#[test]
fn single_source_also_transitions_to_ddos() {
    let now = Instant::now();
    let single = FloodMode::SingleSource {
        did: "did:key:abc".to_string(),
    };
    let t = evaluate(&single, obs(200, 12), thresholds(), now);
    assert!(matches!(t, FloodTransition::EnteredDdos { .. }));
}

#[test]
fn ddos_stays_ddos_while_thresholds_held() {
    let now = Instant::now();
    let cfg = thresholds();
    let ddos = FloodMode::Ddos {
        source_count: 20,
        expires_at: now + Duration::from_secs(900),
    };
    let t = evaluate(&ddos, obs(300, 25), cfg, now);
    assert_eq!(t, FloodTransition::NoChange);
}

#[test]
fn ddos_does_not_re_enter_with_new_expiry_while_active() {
    let started = Instant::now();
    let cfg = thresholds();
    let ddos = FloodMode::Ddos {
        source_count: 15,
        expires_at: started + Duration::from_secs(1800),
    };
    let later = started + Duration::from_secs(60);
    let t = evaluate(&ddos, obs(400, 30), cfg, later);
    assert_eq!(t, FloodTransition::NoChange);
}

#[test]
fn ddos_expires_when_deadline_passed() {
    let started = Instant::now();
    let cfg = thresholds();
    let ddos = FloodMode::Ddos {
        source_count: 15,
        expires_at: started + Duration::from_secs(1800),
    };
    let later = started + Duration::from_secs(1801);
    let t = evaluate(&ddos, obs(0, 0), cfg, later);
    assert_eq!(t, FloodTransition::Expired);
}

#[test]
fn ddos_expiry_takes_precedence_even_if_thresholds_still_high() {
    let started = Instant::now();
    let cfg = thresholds();
    let ddos = FloodMode::Ddos {
        source_count: 15,
        expires_at: started + Duration::from_secs(1800),
    };
    let later = started + Duration::from_secs(1900);
    let t = evaluate(&ddos, obs(500, 50), cfg, later);
    assert_eq!(t, FloodTransition::Expired);
}

#[test]
fn apply_no_change_returns_input() {
    let now = Instant::now();
    let single = FloodMode::SingleSource {
        did: "did:key:abc".to_string(),
    };
    assert_eq!(
        apply(single.clone(), &FloodTransition::NoChange),
        single,
        "NoChange must be a passthrough"
    );
    let _ = now;
}

#[test]
fn apply_entered_ddos_yields_ddos_state() {
    let now = Instant::now();
    let exp = now + Duration::from_secs(1800);
    let next = apply(
        FloodMode::Quiet,
        &FloodTransition::EnteredDdos {
            source_count: 42,
            expires_at: exp,
        },
    );
    assert_eq!(
        next,
        FloodMode::Ddos {
            source_count: 42,
            expires_at: exp,
        }
    );
}

#[test]
fn apply_expired_resets_to_quiet() {
    let now = Instant::now();
    let prev = FloodMode::Ddos {
        source_count: 9,
        expires_at: now + Duration::from_secs(10),
    };
    assert_eq!(apply(prev, &FloodTransition::Expired), FloodMode::Quiet);
}

#[test]
fn is_ddos_helper() {
    assert!(!FloodMode::Quiet.is_ddos());
    assert!(!FloodMode::SingleSource {
        did: "did:key:abc".to_string()
    }
    .is_ddos());
    let now = Instant::now();
    assert!(FloodMode::Ddos {
        source_count: 1,
        expires_at: now,
    }
    .is_ddos());
}
