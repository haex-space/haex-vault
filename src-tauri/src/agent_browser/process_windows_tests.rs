//! Tests for the pipe on Windows (spec 065, T030). The command line and the `lpReserved2` block run
//! on every system; the start of a real process runs in the Windows job of the CI.

use super::process_windows::{command_line, crt_block, CRT_OPEN_PIPE};

#[test]
fn the_crt_block_lists_flags_then_handles() {
    let block = crt_block(&[(0, usize::MAX), (CRT_OPEN_PIPE, 0x1234)]);
    let width = std::mem::size_of::<usize>();
    assert_eq!(block.len(), 4 + 2 + 2 * width);
    assert_eq!(&block[..4], &2u32.to_le_bytes(), "the count");
    assert_eq!(&block[4..6], &[0, 0x09], "one flag byte per descriptor");
    assert_eq!(&block[6..6 + width], &usize::MAX.to_le_bytes());
    assert_eq!(&block[6 + width..], &0x1234usize.to_le_bytes());
}

#[test]
fn arguments_are_quoted_as_the_system_reads_them() {
    let args = [
        "--remote-debugging-pipe".to_owned(),
        r"--user-data-dir=C:\Users\Anna Maria\AppData\Local\Temp\holzi-agent-browser\v".to_owned(),
        String::new(),
        r#"say "hi""#.to_owned(),
        r"trailing\".to_owned(),
        r"C:\path with space\".to_owned(),
    ];
    assert_eq!(
        command_line(
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            &args
        ),
        concat!(
            r#""C:\Program Files\Google\Chrome\Application\chrome.exe" "#,
            r#"--remote-debugging-pipe "#,
            r#""--user-data-dir=C:\Users\Anna Maria\AppData\Local\Temp\holzi-agent-browser\v" "#,
            r#""" "#,
            r#""say \"hi\"" "#,
            r#"trailing\ "#,
            r#""C:\path with space\\""#
        )
    );
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_browser_answers_over_descriptors_3_and_4() {
    use std::path::Path;

    use serde_json::json;

    use crate::agent_browser::locate::{find, Os, System};
    use crate::agent_browser::process::args;

    let Some(browser) = find(Os::Windows, None, &System) else {
        eprintln!("skipped: no Chromium-based browser on this runner");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let (connection, process) =
        super::process_windows::spawn(Path::new(&browser), &args(Os::Windows, dir.path(), false))
            .expect("started");
    let version = connection
        .call("Browser.getVersion", json!({}), None)
        .await
        .expect("an answer over the pipe");
    assert!(version["product"].as_str().is_some());
    let _ = connection.call("Browser.close", json!({}), None).await;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), process.wait()).await;
    process.kill();
}
