use super::*;

#[test]
fn the_development_origins_join_frame_src_and_nothing_else_changes() {
    let csp = "default-src 'self'; frame-src haex-ext://localhost; script-src 'self'";
    assert_eq!(
        with_dev_frames(csp),
        "default-src 'self'; frame-src haex-ext://localhost http://localhost:* \
         http://127.0.0.1:*; script-src 'self'"
    );
}

#[test]
fn a_policy_without_frame_src_gets_one_with_what_frames_fell_back_to() {
    assert_eq!(
        with_dev_frames("default-src 'self';"),
        "default-src 'self'; frame-src 'self' http://localhost:* http://127.0.0.1:*"
    );
    assert_eq!(
        with_dev_frames("default-src 'self'; child-src haex-ext://localhost"),
        "default-src 'self'; child-src haex-ext://localhost; \
         frame-src haex-ext://localhost http://localhost:* http://127.0.0.1:*"
    );
    assert_eq!(
        with_dev_frames("script-src 'self'"),
        "script-src 'self'; frame-src http://localhost:* http://127.0.0.1:*"
    );
}
