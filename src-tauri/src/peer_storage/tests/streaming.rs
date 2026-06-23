// Integration tests: streaming pipelines via PeerEndpoint operations.
//
// The streaming pipeline section in the original `tests.rs` only contained
// shared harness scaffolding (`MultipartHarness` / `setup_multipart_harness`),
// which now lives in `helpers.rs`. The actual streaming pipeline tests are
// organised by feature into `pipe_reader`, `download_chunks`,
// `download_resume`, and `multi_stream` submodules.
