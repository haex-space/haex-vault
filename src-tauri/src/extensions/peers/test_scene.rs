//! Two vaults that are contacts of each other, for the tests of invitations (spec 057 US1).

use super::test_support::{exchange_cards, Admit, Vault, LIMIT};
use crate::sync::test_support::Member;

/// A vault `a` with one device and a contact vault `b` with two, which know each other's cards;
/// `a` knows where both devices of `b` listen. The test extension of `a` may read contacts.
pub(crate) struct Scene {
    pub a: Vault,
    pub b1: Vault,
    pub b2: Vault,
    /// The contact of `b` on `a`.
    pub b_on_a: String,
    /// The contact of `a` on `b1` and `b2`.
    pub a_on_b: String,
}

impl Scene {
    pub(crate) async fn start() -> Self {
        let a = Vault::of(Member::genesis(), Admit::Real(LIMIT)).await;
        let main = Member::genesis();
        let linked = Member::join(&main);
        main.add(&linked);
        linked.device.pull_from(&main.device);
        let b1 = Vault::of(main, Admit::Real(LIMIT)).await;
        let b2 = Vault::of(linked, Admit::Real(LIMIT)).await;
        let (b_on_a, a_on_b) = exchange_cards(&a, &b1).await;
        // The contacts of `b1` reach its linked device as the sync would bring them.
        b2.member.device.pull_from(&b1.member.device);
        a.learn(&b1).await;
        a.learn(&b2).await;
        a.grant("contacts", "read", "granted");
        Self {
            a,
            b1,
            b2,
            b_on_a,
            a_on_b,
        }
    }

    pub(crate) async fn shutdown(&self) {
        self.a.shutdown().await;
        self.b1.shutdown().await;
        self.b2.shutdown().await;
    }
}
