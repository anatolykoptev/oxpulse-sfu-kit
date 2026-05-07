//! Phase 8 T8 — `with_extra_dc` generic builder + `with_voice_dc` convenience.
//!
//! Tests:
//!   1. `with_voice_dc(200)` registers id=6, unordered, MaxPacketLifetime 200 ms.
//!   2. `with_extra_dc` chains three distinct DC registrations.
//!   3. `with_chat_dcs()` still opens id=4 (chat-data) and id=5 (chat-ctrl).

#![cfg(feature = "test-utils")]

use oxpulse_sfu_kit::{ChannelConfig, Client};

#[test]
fn with_voice_dc_opens_id_6_unreliable_lifetime_200() {
    let client = Client::new_for_test().with_voice_dc(200);
    let cfg = client.dc_config_for("voice").expect("voice dc registered");
    assert_eq!(cfg.id(), 6, "voice DC must use id=6");
    assert!(!cfg.ordered(), "voice DC must be unordered");
    assert_eq!(
        cfg.max_packet_lifetime_ms(),
        Some(200),
        "lifetime must be 200 ms"
    );
    assert!(
        cfg.max_retransmits().is_none(),
        "voice DC must not set max_retransmits"
    );
}

#[test]
fn with_extra_dc_chains_three_distinct_ids() {
    let client = Client::new_for_test()
        .with_extra_dc("a", 10, ChannelConfig::reliable_ordered())
        .with_extra_dc("b", 11, ChannelConfig::reliable_ordered())
        .with_extra_dc("c", 12, ChannelConfig::reliable_ordered());
    assert_eq!(client.dc_count(), 3);
}

#[test]
fn with_chat_dcs_still_opens_id_4_and_5() {
    let client = Client::new_for_test().with_chat_dcs();
    let chat = client
        .dc_config_for("chat-data")
        .expect("chat-data registered");
    let ctrl = client
        .dc_config_for("chat-ctrl")
        .expect("chat-ctrl registered");

    assert_eq!(chat.id(), 4);
    assert!(chat.ordered(), "chat-data must be ordered");
    assert!(
        chat.max_packet_lifetime_ms().is_none(),
        "chat-data is reliable: no lifetime cap"
    );
    assert!(
        chat.max_retransmits().is_none(),
        "chat-data is reliable: no retransmit cap"
    );

    assert_eq!(ctrl.id(), 5);
    assert!(!ctrl.ordered(), "chat-ctrl must be unordered");
    assert_eq!(ctrl.max_retransmits(), Some(0));
    assert!(
        ctrl.max_packet_lifetime_ms().is_none(),
        "chat-ctrl uses max_retransmits, not lifetime"
    );
}
