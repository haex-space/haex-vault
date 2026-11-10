//! Spec 081 FR-016: the chat model computes first, speech output waits for its unit.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::{units_of_layers, ComputeUse, Unit};

#[test]
fn a_claim_holds_its_units_until_dropped() {
    let use_ = ComputeUse::new();
    assert!(!use_.busy(Unit::Processor));
    let claim = use_.claim(&[Unit::Processor]);
    assert!(use_.busy(Unit::Processor));
    assert!(!use_.busy(Unit::GraphicsCard), "the other unit stays free");
    let second = use_.claim(&[Unit::Processor, Unit::GraphicsCard]);
    drop(claim);
    assert!(
        use_.busy(Unit::Processor),
        "the second claim still holds it"
    );
    drop(second);
    assert!(!use_.busy(Unit::Processor));
    assert!(!use_.busy(Unit::GraphicsCard));
}

#[test]
fn speech_waits_until_the_chat_model_is_done_with_its_unit() {
    let use_ = ComputeUse::new();
    let (claimed, chat_computes) = std::sync::mpsc::channel();
    let mut started = Instant::now();
    std::thread::scope(|scope| {
        // A lock guard stays on the thread that took it.
        scope.spawn(|| {
            let _claim = use_.claim(&[Unit::Processor]);
            claimed.send(()).expect("claimed");
            std::thread::sleep(Duration::from_millis(100));
        });
        chat_computes.recv().expect("the chat model computes");
        started = Instant::now();
        assert!(use_.claim_speech(Unit::Processor, &|| false).is_some());
    });
    assert!(started.elapsed() >= Duration::from_millis(80));
}

/// A chat call that begins while speech computes a part waits for that part only.
#[test]
fn a_chat_call_waits_for_the_part_speech_is_computing() {
    let use_ = ComputeUse::new();
    let speech = use_.claim_speech(Unit::Processor, &|| false).expect("free");
    let (done, chat_started) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let _claim = use_.claim(&[Unit::Processor]);
            done.send(Instant::now()).expect("started");
        });
        std::thread::sleep(Duration::from_millis(50));
        assert!(
            chat_started.try_recv().is_err(),
            "the chat call waits for the part"
        );
        let released = Instant::now();
        drop(speech);
        let began = chat_started
            .recv_timeout(Duration::from_secs(5))
            .expect("then it computes");
        assert!(began >= released);
    });
}

#[test]
fn a_free_unit_does_not_wait() {
    let use_ = ComputeUse::new();
    let _claim = use_.claim(&[Unit::GraphicsCard]);
    let started = Instant::now();
    assert!(use_.claim_speech(Unit::Processor, &|| false).is_some());
    assert!(started.elapsed() < Duration::from_millis(20));
}

#[test]
fn a_stop_ends_the_wait() {
    let use_ = ComputeUse::new();
    let _claim = use_.claim(&[Unit::Processor]);
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(50));
            stop.store(true, Ordering::Release);
        });
        assert!(use_
            .claim_speech(Unit::Processor, &|| stop.load(Ordering::Acquire))
            .is_none());
    });
}

#[test]
fn the_layers_on_the_graphics_card_say_where_a_model_computes() {
    assert_eq!(units_of_layers(0, 32, true), [Unit::Processor]);
    assert_eq!(
        units_of_layers(999, 32, false),
        [Unit::Processor],
        "no card"
    );
    assert_eq!(units_of_layers(33, 32, true), [Unit::GraphicsCard]);
    assert_eq!(units_of_layers(999, 32, true), [Unit::GraphicsCard]);
    assert_eq!(
        units_of_layers(20, 32, true),
        [Unit::Processor, Unit::GraphicsCard]
    );
}
