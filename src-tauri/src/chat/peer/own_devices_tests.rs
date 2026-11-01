//! Own devices that are never online together end with the same conversation (spec 072 T028,
//! FR-016, US2 scenario 6, quickstart A4): every rumor is also wrapped to the own other devices.

use super::test_support::{mock_relay, two_vaults};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_own_devices_never_online_together_hold_the_same_rows() {
    let (_relay, url) = mock_relay().await;
    let (anna, ben) = two_vaults(&[url]).await;
    let anna_phone = anna.link();
    ben.learn(&anna).await;
    ben.start().await;

    // The laptop writes while the phone is closed.
    anna.start().await;
    anna.send(&ben, "vom Laptop");
    ben.wait_for(&anna, "the laptop's message", |m| !m.is_empty())
        .await;
    anna.stop();

    // Ben answers while both of Anna's devices are closed.
    ben.send(&anna, "an Anna");
    ben.wait_for(&anna, "sent", |m| {
        m.iter()
            .any(|m| m.own && m.delivery != Some(super::messages::DeliveryState::Waiting))
    })
    .await;

    // The phone opens alone: it gets its own copy of the laptop's message and Ben's answer.
    anna_phone.start().await;
    let on_phone = anna_phone
        .wait_for(&ben, "both messages on the phone", |m| m.len() == 2)
        .await;
    let laptop = anna.messages_with(&ben);
    let laptop_ids: Vec<_> = laptop.iter().map(|m| m.id.clone()).collect();
    assert!(
        on_phone.iter().any(|m| m.own && laptop_ids.contains(&m.id)),
        "the laptop's message has the same id on the phone"
    );
    assert!(on_phone
        .iter()
        .any(|m| !m.own && m.text.as_deref() == Some("an Anna")));
}
