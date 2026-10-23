//! The throttle of the admission (spec 057 T054, research R7, FR-033), on an injected clock.

use super::*;

fn endpoint(n: u16) -> [u8; 32] {
    let mut id = [0; 32];
    id[..2].copy_from_slice(&n.to_be_bytes());
    id
}

#[test]
fn an_endpoint_gets_five_attempts_per_window() {
    let (throttle, start) = (Throttle::default(), Instant::now());
    for _ in 0..ATTEMPTS_PER_ENDPOINT {
        assert!(throttle.handshake(endpoint(1), start).is_some());
    }
    assert!(throttle.handshake(endpoint(1), start).is_none());
    assert!(
        throttle
            .handshake(endpoint(1), start + WINDOW - Duration::from_millis(1))
            .is_none(),
        "still the same window"
    );
    assert!(
        throttle.handshake(endpoint(2), start).is_some(),
        "another endpoint has its own count"
    );
    assert!(
        throttle.handshake(endpoint(1), start + WINDOW).is_some(),
        "a new window"
    );
}

#[test]
fn three_failed_joins_ban_an_endpoint_for_a_minute() {
    let (throttle, start) = (Throttle::default(), Instant::now());
    for n in 1..FAILED_JOINS {
        throttle.join_failed(endpoint(1), start);
        assert!(!throttle.banned(endpoint(1), start), "{n} failed joins");
    }
    throttle.join_failed(endpoint(1), start);
    assert!(throttle.banned(endpoint(1), start));
    assert!(throttle.banned(endpoint(1), start + BAN - Duration::from_millis(1)));
    assert!(!throttle.banned(endpoint(2), start));
    assert!(!throttle.banned(endpoint(1), start + BAN));
    // The count started over with the ban.
    throttle.join_failed(endpoint(1), start + BAN);
    assert!(!throttle.banned(endpoint(1), start + BAN));
}

#[test]
fn at_most_sixteen_devices_that_are_not_verified_wait_at_once() {
    let (throttle, start) = (Throttle::default(), Instant::now());
    let mut waiting: Vec<Slot> = (0..PENDING)
        .map(|_| throttle.unverified(start).expect("a pending slot"))
        .collect();
    assert!(throttle.unverified(start).is_none());
    // Contacts and own devices only pass the handshake and the per-endpoint count, which the
    // waiting strangers do not touch.
    let contact = throttle.handshake(endpoint(7), start);
    assert!(contact.is_some(), "a contact is not locked out");
    waiting.pop();
    assert!(
        throttle.unverified(start).is_some(),
        "a place came back with its slot"
    );
}

#[test]
fn at_most_twenty_new_devices_that_are_not_verified_per_window() {
    let (throttle, start) = (Throttle::default(), Instant::now());
    for _ in 0..UNVERIFIED_PER_WINDOW {
        // Each is admitted or gone at once; only the count of new ones binds.
        drop(
            throttle
                .unverified(start)
                .expect("within the window's count"),
        );
    }
    assert!(throttle.unverified(start).is_none());
    assert!(
        throttle.handshake(endpoint(7), start).is_some(),
        "verified devices are limited per endpoint only"
    );
    assert!(throttle.unverified(start + WINDOW).is_some());
}

#[test]
fn handshakes_at_once_are_bounded_and_given_back() {
    let (throttle, start) = (Throttle::default(), Instant::now());
    let mut running: Vec<Slot> = (0..HANDSHAKES)
        .map(|n| {
            throttle
                .handshake(endpoint(n as u16), start)
                .expect("a handshake slot")
        })
        .collect();
    assert!(throttle.handshake(endpoint(999), start).is_none());
    running.clear();
    assert!(throttle.handshake(endpoint(999), start).is_some());
}

#[test]
fn the_map_of_endpoints_keeps_at_most_its_limit() {
    let (throttle, start) = (Throttle::default(), Instant::now());
    for n in 0..(2 * MAX_ENDPOINTS) as u16 {
        drop(throttle.handshake(endpoint(n), start + Duration::from_millis(u64::from(n))));
    }
    assert_eq!(throttle.endpoints(), MAX_ENDPOINTS);
    // The least recently seen went first; the latest are kept.
    let latest = (2 * MAX_ENDPOINTS - 1) as u16;
    let at = start + Duration::from_millis(u64::from(latest));
    for _ in 1..ATTEMPTS_PER_ENDPOINT {
        drop(throttle.handshake(endpoint(latest), at));
    }
    assert!(throttle.handshake(endpoint(latest), at).is_none());
}

#[test]
fn a_ban_is_not_pushed_out_by_other_endpoints() {
    let (throttle, start) = (Throttle::default(), Instant::now());
    for _ in 0..FAILED_JOINS {
        throttle.join_failed(endpoint(0), start);
    }
    // The map is full of endpoints seen as long ago as the banned one; a new one pushes out one
    // of those, not the ban.
    for n in 1..MAX_ENDPOINTS as u16 {
        drop(throttle.handshake(endpoint(n), start));
    }
    let later = start + Duration::from_secs(59);
    drop(throttle.handshake(endpoint(5000), later));
    assert!(throttle.banned(endpoint(0), later));
}
