#[test]
fn streaming_chunk_size_is_one_mib() {
    assert_eq!(crate::peer_storage::streaming::CHUNK_SIZE, 1024 * 1024);
}

#[test]
fn streaming_channel_depth_is_eight() {
    assert_eq!(crate::peer_storage::streaming::CHANNEL_DEPTH, 8);
}

#[test]
fn streaming_multi_stream_threshold_is_sixteen_mib() {
    assert_eq!(
        crate::peer_storage::streaming::MULTI_STREAM_THRESHOLD,
        16 * 1024 * 1024
    );
}

#[test]
fn streaming_max_parallel_streams_is_four() {
    assert_eq!(
        crate::peer_storage::streaming::MAX_PARALLEL_STREAMS_PER_FILE,
        4
    );
}

#[test]
fn pipeline_error_display_io() {
    use crate::peer_storage::streaming::PipelineError;
    let e = PipelineError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "file gone",
    ));
    let s = e.to_string();
    assert!(s.starts_with("io:"), "expected 'io:' prefix, got: {s}");
    assert!(s.contains("file gone"));
}

#[test]
fn pipeline_error_display_stream() {
    use crate::peer_storage::streaming::PipelineError;
    let e = PipelineError::Stream("unexpected EOF".to_string());
    let s = e.to_string();
    assert!(
        s.starts_with("stream:"),
        "expected 'stream:' prefix, got: {s}"
    );
    assert!(s.contains("unexpected EOF"));
}

#[test]
fn pipeline_error_display_cancelled() {
    use crate::peer_storage::streaming::PipelineError;
    let e = PipelineError::Cancelled;
    assert_eq!(e.to_string(), "cancelled");
}

#[test]
fn pipeline_error_is_std_error() {
    use crate::peer_storage::streaming::PipelineError;
    // Verify the trait bound compiles and is well-formed.
    fn accepts_error<E: std::error::Error>(_e: E) {}
    accepts_error(PipelineError::Cancelled);
    accepts_error(PipelineError::Stream("x".into()));
}

#[test]
fn recv_stats_default_is_zero() {
    use crate::peer_storage::streaming::RecvStats;
    let s = RecvStats::default();
    assert_eq!(s.bytes, 0);
}

#[test]
fn recv_options_default_has_no_fields_set() {
    use crate::peer_storage::streaming::RecvOptions;
    let opts = RecvOptions::default();
    assert!(opts.on_progress.is_none());
    assert!(opts.cancel_token.is_none());
    assert!(opts.pause_flag.is_none());
}
