//! The documents `browser.entry.fill` fills into (spec 065 FR-037, research R7a, T091): the tab's
//! own page and every frame of a website the agent may operate over `https`. Payment providers put
//! their card fields into such frames. A frame in the tab's process runs holzi's script in its own
//! isolated world through the tab's session; a frame of another website runs in its own process
//! (its own target), so holzi attaches to it for the call and detaches afterwards. A frame of a
//! website the agent may only read, or of none it may use, is left out: nothing is offered for it
//! and nothing written into it.

use std::collections::BTreeSet;

use serde_json::Value;

use super::Acting;
use crate::agent_browser::calls::{browser_error, site_of};
use crate::agent_browser::cdp::{page, target, Connection};
use crate::agent_browser::read;
use crate::agent_browser::sites::may_operate;
use crate::agent_browser::tab::Tab;
use crate::chat::tools::native_action::NativeError;

/// One document holzi may fill into.
pub(super) struct Doc {
    pub session: String,
    pub frame: String,
    /// The frame's website, for the user's question; `None` for the tab's own page.
    pub site: Option<String>,
}

/// The documents of a call, the tab's own page first, and the sessions holzi attached for them.
pub(super) struct Docs {
    pub list: Vec<Doc>,
    attached: Vec<String>,
}

impl Docs {
    /// Detaches the sessions holzi attached to frames of other processes.
    pub async fn release(self, connection: &Connection) {
        for session in self.attached {
            if let Err(error) = target::detach(connection, &session).await {
                log::debug!("agent browser: detaching a frame session: {error}");
            }
        }
    }
}

impl Acting<'_> {
    /// The tab's own page and the frames holzi may fill on `tab`.
    pub(super) async fn documents(&self, tab: &Tab) -> Result<Docs, NativeError> {
        let frames = page::frames(self.connection, &tab.session)
            .await
            .map_err(browser_error)?;
        let separate: BTreeSet<String> = self
            .session
            .targets_of(&tab.id)
            .into_iter()
            .filter(|target| target != &tab.target)
            .collect();
        let mut list = vec![Doc {
            session: tab.session.clone(),
            frame: frames[0].0.clone(),
            site: None,
        }];
        let docs_in = |session: &str, frames: &[(String, String)], list: &mut Vec<Doc>| {
            for (frame, url) in frames {
                if !separate.contains(frame) && self.frame_operable(url) {
                    list.push(Doc {
                        session: session.to_owned(),
                        frame: frame.clone(),
                        site: Some(site_of(url)),
                    });
                }
            }
        };
        docs_in(&tab.session, &frames[1..], &mut list);
        let mut attached = Vec::new();
        for frame_target in &separate {
            let Ok((kind, url)) = target::info(self.connection, frame_target).await else {
                continue;
            };
            if kind != "iframe" || !self.frame_operable(&url) {
                continue;
            }
            let Ok(session) = target::attach(self.connection, frame_target).await else {
                continue;
            };
            attached.push(session.clone());
            if let Ok(inner) = page::frames(self.connection, &session).await {
                // The frame's own document is its target; frames inside it may be in this process.
                if let Some((own, own_url)) = inner.first() {
                    if self.frame_operable(own_url) {
                        list.push(Doc {
                            session: session.clone(),
                            frame: own.clone(),
                            site: Some(site_of(own_url)),
                        });
                    }
                }
                docs_in(&session, &inner[1..], &mut list);
            }
        }
        Ok(Docs { list, attached })
    }

    /// Whether holzi may fill into a frame showing `url`: `https` on a website the agent may
    /// operate.
    pub(super) fn frame_operable(&self, url: &str) -> bool {
        url.starts_with("https://")
            && may_operate(
                url,
                &self.rights.sites,
                self.rights.all,
                &self.session.once_allowed_sites(),
            )
    }

    /// Runs the page script with `args` in `doc`.
    pub(super) async fn run_in(&self, doc: &Doc, args: &Value) -> Result<Value, NativeError> {
        read::script_in(self.connection, &doc.session, &doc.frame, args)
            .await
            .map_err(browser_error)
    }
}
