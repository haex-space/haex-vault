//! A watched folder on the serving device (spec 056 FR-046, research R16): its own stream stays
//! open, `Done` says the watch runs, and every debounced batch of changes sends `Changed` without
//! naming any path. The request is checked again with every batch and every [`RECHECK`], so a
//! revoked grant or the switch going off ends the watch.

use std::future::Future;
use std::time::Duration;

use iroh::endpoint::SendStream;

use super::roots::Located;
use crate::files::peer::wire::{write_head, FailCode, Reply};

/// How often a quiet watch checks again whether it may go on.
pub const RECHECK: Duration = Duration::from_secs(10);

/// Watches `at` until the other side ends the stream or `still` says no more.
pub async fn run<F, Fut>(at: &Located, mut send: SendStream, still: F)
where
    F: Fn() -> Fut,
    Fut: Future<Output = bool>,
{
    let folder = match crate::remote_storage::local_fs::resolve(&at.root, &at.key) {
        Ok(folder) => folder,
        Err(error) => return super::answer(send, Reply::Failed(super::ops::fail_of(error))).await,
    };
    let (changed, mut changes) = tokio::sync::mpsc::channel::<()>(1);
    #[cfg(not(target_os = "ios"))]
    let watch = crate::files::local::watch::watch_folder(&folder, false, move |_| {
        // One waiting signal is enough: the other side reloads the whole folder.
        let _ = changed.try_send(());
    });
    #[cfg(target_os = "ios")]
    let watch: Result<(), String> = {
        let _ = (folder, changed);
        Err("no watching on this platform".to_owned())
    };
    let _watch = match watch {
        Ok(watch) => watch,
        Err(reason) => {
            log::debug!("files: a watch for another device failed: {reason}");
            return super::answer(send, Reply::Failed(FailCode::Unsupported)).await;
        }
    };
    if write_head(&mut send, &Reply::Done).await.is_err() {
        return;
    }
    let mut tick = tokio::time::interval(RECHECK);
    tick.tick().await;
    loop {
        tokio::select! {
            change = changes.recv() => {
                if change.is_none() || !still().await {
                    break;
                }
                if write_head(&mut send, &Reply::Changed).await.is_err() {
                    break;
                }
            }
            _ = tick.tick() => if !still().await { break },
            _ = send.stopped() => break,
        }
    }
    let _ = send.finish();
}
