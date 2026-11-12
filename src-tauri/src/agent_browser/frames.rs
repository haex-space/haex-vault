//! The elements of frames (spec 065 FR-053, research R20). Players, payment forms and the box of a
//! "are you human" check sit in frames, often of another website and then of another process, which
//! the script of the main document cannot reach. The list of a page therefore goes on through its
//! frames: their elements are numbered after those of the main document, in the order of the
//! frames, each with the website of its frame; the tab remembers which numbers belong to which
//! frame, and a call that names such a number runs in that frame.
//!
//! Only frames of a website the agent may operate are listed (https only); the websites of the
//! others are named as `frames_not_operable`, so the agent can say what to allow. A frame that
//! cannot be reached is left out; it never fails the reading of the page.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde_json::{json, Value};

use super::calls::site_of;
use super::cdp::page::{self, FrameNode};
use super::cdp::{target, Connection};
use super::error::Result;
use super::read;
use super::session::Session;
use super::sites::may_operate;
use super::tab::Tab;
use crate::mcp_server::grants::BrowserRights;

/// The most frames one reading looks into; a page full of advertisements has more than it can use.
pub(super) const MAX_FRAMES: usize = 20;
/// The most websites `frames_not_operable` names.
const MAX_NAMED: usize = 10;

/// What decides whether a frame is listed.
pub(super) struct FrameAccess<'a> {
    pub session: &'a Session,
    pub rights: &'a BrowserRights,
}

impl FrameAccess<'_> {
    /// Whether the agent may operate a frame showing `url`: `https` on a website it may operate.
    pub(super) fn operable(&self, url: &str) -> bool {
        url.starts_with("https://")
            && may_operate(
                url,
                &self.rights.sites,
                self.rights.all,
                &self.session.once_allowed_sites(),
            )
    }
}

/// One step from a frame up towards the tab: the element that holds `frame`, measured in the view
/// of the process whose session is that of the target `host` (the tab's own when `None`). The
/// browser reports such a box in the root frame of the process, so only the frame itself and each
/// process boundary above it count, not the frames in between.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Hop {
    pub host: Option<String>,
    pub frame: String,
}

/// The element numbers of one frame in the last reading of a tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FrameSlice {
    /// The number of its first element, and how many numbers it has.
    pub first: u32,
    pub count: u32,
    /// The frame, and the target whose session runs scripts in it (the tab's own when `None`).
    pub frame: String,
    pub host: Option<String>,
    /// From the frame up to the tab, innermost first, to add up where it lies on the page.
    pub chain: Vec<Hop>,
}

impl FrameSlice {
    /// The number inside the frame for the number `n` of the list, when `n` is one of its own.
    pub(super) fn local(&self, n: u32) -> Option<u32> {
        (self.first..self.first + self.count)
            .contains(&n)
            .then(|| n - self.first + 1)
    }
}

/// What reading the frames of a page found.
#[derive(Debug, Default)]
pub(super) struct Found {
    pub elements: Vec<Value>,
    pub slices: Vec<FrameSlice>,
    /// Websites of frames the agent may not operate.
    pub not_operable: Vec<String>,
}

/// Sessions holzi attached to targets of other processes for a moment; each ends with
/// [`Attached::release`].
pub(super) struct Attached<'a> {
    connection: &'a Connection,
    sessions: BTreeMap<String, String>,
}

impl<'a> Attached<'a> {
    pub(super) fn new(connection: &'a Connection) -> Self {
        Self {
            connection,
            sessions: BTreeMap::new(),
        }
    }

    /// The session of `host`, attaching to it the first time; the tab's own session for `None`.
    async fn session(&mut self, tab: &Tab, host: Option<&str>) -> Result<String> {
        let Some(host) = host else {
            return Ok(tab.session.clone());
        };
        if let Some(session) = self.sessions.get(host) {
            return Ok(session.clone());
        }
        let session = target::attach(self.connection, host).await?;
        self.sessions.insert(host.to_owned(), session.clone());
        Ok(session)
    }

    pub(super) async fn release(self) {
        for session in self.sessions.into_values() {
            if let Err(error) = target::detach(self.connection, &session).await {
                log::debug!("agent browser: detaching a frame session: {error}");
            }
        }
    }
}

/// Where the corner of a frame lies on the page: the sum of the places of the elements that hold
/// it, up to the tab.
pub(super) async fn origin(
    attached: &mut Attached<'_>,
    tab: &Tab,
    chain: &[Hop],
) -> Result<(f64, f64)> {
    let (mut x, mut y) = (0.0, 0.0);
    for hop in chain {
        let session = attached.session(tab, hop.host.as_deref()).await?;
        let (hop_x, hop_y) = page::owner_origin(attached.connection, &session, &hop.frame).await?;
        x += hop_x;
        y += hop_y;
    }
    Ok((x, y))
}

/// Runs the page script with `args` in the frame of `slice`.
pub(super) async fn script(
    attached: &mut Attached<'_>,
    tab: &Tab,
    slice: &FrameSlice,
    args: &Value,
) -> Result<Value> {
    let session = attached.session(tab, slice.host.as_deref()).await?;
    read::script_in(attached.connection, &session, &slice.frame, args).await
}

/// A document whose frames are still to be looked at.
struct Pending {
    session: String,
    host: Option<String>,
    nodes: Vec<FrameNode>,
    /// The way from this document's frame up to the tab (empty for the tab's own document).
    base: Vec<Hop>,
}

impl Pending {
    /// The way from `frame` of this process up to the tab.
    fn chain(&self, frame: &str) -> Vec<Hop> {
        let mut hops = vec![Hop {
            host: self.host.clone(),
            frame: frame.to_owned(),
        }];
        hops.extend(self.base.iter().cloned());
        hops
    }
}

/// Reads the frame `frame` shown at `url` through `session` and adds what it holds to `found`;
/// `next` is the number the last element so far has.
struct Reader<'a> {
    connection: &'a Connection,
    tab: &'a Tab,
    attached: Attached<'a>,
    found: Found,
    next: u32,
}

impl Reader<'_> {
    async fn add(
        &mut self,
        session: &str,
        host: Option<String>,
        frame: &str,
        url: &str,
        chain: Vec<Hop>,
    ) {
        let Ok(reply) =
            read::script_in(self.connection, session, frame, &json!({ "noText": true })).await
        else {
            return;
        };
        let elements = reply["elements"].as_array().cloned().unwrap_or_default();
        if elements.is_empty() {
            return;
        }
        let count = reply["total"]
            .as_u64()
            .map_or(elements.len(), |total| total as usize) as u32;
        let site = site_of(reply["url"].as_str().unwrap_or(url));
        let slice = FrameSlice {
            first: self.next + 1,
            count,
            frame: frame.to_owned(),
            host,
            chain,
        };
        let place = origin(&mut self.attached, self.tab, &slice.chain)
            .await
            .ok();
        for mut element in elements {
            let local = element["n"].as_u64().unwrap_or(0) as u32;
            element["n"] = json!(self.next + local);
            element["frame"] = json!(site);
            shift_box(&mut element, place);
            self.found.elements.push(element);
        }
        self.next += count;
        self.found.slices.push(slice);
    }
}

/// The elements of the frames of `tab`, numbered from `after` + 1 on.
pub(super) async fn read(
    connection: &Connection,
    tab: &Tab,
    access: &FrameAccess<'_>,
    after: u32,
) -> Found {
    let Ok(top) = page::frame_nodes(connection, &tab.session).await else {
        return Found::default();
    };
    // Frames of another process each have a target of their own and are no part of the tree.
    let mut apart: BTreeSet<String> = access
        .session
        .targets_of(&tab.id)
        .into_iter()
        .filter(|target| target != &tab.target)
        .collect();
    if top.len() < 2 && apart.is_empty() {
        return Found::default();
    }
    let mut reader = Reader {
        connection,
        tab,
        attached: Attached::new(connection),
        found: Found::default(),
        next: after,
    };
    let mut queue = VecDeque::from([Pending {
        session: tab.session.clone(),
        host: None,
        nodes: top,
        base: Vec::new(),
    }]);
    let mut looked = 0;
    let mut unreachable = BTreeSet::new();
    'documents: while let Some(pending) = queue.pop_front() {
        // The frames of the document's own process.
        for node in &pending.nodes[1..] {
            if !access.operable(&node.url) {
                if node.url.starts_with("http") {
                    unreachable.insert(site_of(&node.url));
                }
                continue;
            }
            if looked == MAX_FRAMES {
                break 'documents;
            }
            looked += 1;
            let chain = pending.chain(&node.id);
            reader
                .add(
                    &pending.session,
                    pending.host.clone(),
                    &node.id,
                    &node.url,
                    chain,
                )
                .await;
        }
        // The frames of other processes this document holds.
        for frame in apart.clone() {
            if !page::holds_frame(connection, &pending.session, &frame).await {
                continue;
            }
            apart.remove(&frame);
            let url = match target::info(connection, &frame).await {
                Ok((kind, url)) if kind == "iframe" => url,
                _ => continue,
            };
            if !access.operable(&url) {
                if url.starts_with("http") {
                    unreachable.insert(site_of(&url));
                }
                continue;
            }
            if looked == MAX_FRAMES {
                break 'documents;
            }
            looked += 1;
            let Ok(session) = reader.attached.session(tab, Some(&frame)).await else {
                continue;
            };
            let Ok(inner) = page::frame_nodes(connection, &session).await else {
                continue;
            };
            let chain = pending.chain(&frame);
            reader
                .add(
                    &session,
                    Some(frame.clone()),
                    &inner[0].id,
                    &url,
                    chain.clone(),
                )
                .await;
            queue.push_back(Pending {
                session,
                host: Some(frame),
                nodes: inner,
                base: chain,
            });
        }
    }
    reader.attached.release().await;
    let mut found = reader.found;
    found.not_operable = unreachable.into_iter().take(MAX_NAMED).collect();
    found
}

/// Moves the `box` of an element of a frame by the place of the frame, or drops it when that is
/// not known: a box that is off by the frame's place would send a click to the wrong spot.
fn shift_box(element: &mut Value, place: Option<(f64, f64)>) {
    let Some(fields) = element.as_object_mut() else {
        return;
    };
    let Some(b) = fields.get_mut("box").and_then(Value::as_object_mut) else {
        return;
    };
    match place {
        Some((x, y)) => {
            for (key, by) in [("x", x), ("y", y)] {
                let moved = b.get(key).and_then(Value::as_f64).unwrap_or(0.0) + by;
                b.insert(key.to_owned(), json!(moved.round() as i64));
            }
        }
        None => {
            fields.remove("box");
        }
    }
}

/// The arguments of the page script that name an element by its number.
const KEYS: [&str; 7] = [
    "target",
    "focusElement",
    "focus",
    "formOf",
    "fileOf",
    "choose",
    "scrollTo",
];

/// Where `args` of the page script name an element: its number.
fn slot(args: &mut Value) -> Option<&mut Value> {
    let key = KEYS
        .iter()
        .copied()
        .find(|key| args.get(*key).is_some_and(Value::is_number));
    if let Some(key) = key {
        return args.get_mut(key);
    }
    args.get_mut("media")?.get_mut("element")
}

/// The frame a call's element number belongs to, with the arguments rewritten to the number inside
/// that frame. `None` when the number is one of the main document.
pub(super) fn route(slices: &[FrameSlice], args: &mut Value) -> Option<FrameSlice> {
    let slot = slot(args)?;
    let number = u32::try_from(slot.as_u64()?).ok()?;
    let slice = slices.iter().find(|slice| slice.local(number).is_some())?;
    *slot = json!(slice.local(number)?);
    Some(slice.clone())
}

#[cfg(test)]
#[path = "frames_tests.rs"]
mod tests;
