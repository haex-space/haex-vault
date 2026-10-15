//! The frame shim (contracts/bridge.md §Rahmen-Shim): a small inline script holzi puts into every
//! HTML document of an extension, right after `<head>`, so it runs before the extension's own
//! scripts. It talks to holzi over its own port and reports navigation, title, close guards,
//! `window.close()`, holzi's shortcuts and clicks into the frame.
//!
//! WebKitGTK fires `load` on the frame element for a fragment navigation inside the frame, so
//! holzi cannot take every `load` for a new document. The shim answers each init with
//! `hello { fresh }`: `fresh` only on the first init of its document, and only then holzi hands
//! the SDK a new port.
//!
//! The shim runs in the extension's realm, so every message it sends can be forged by the
//! extension; holzi applies them only to the extension's own tab (bridge.md). Shortcuts arrive as
//! fields (`code`, `ctrl`, `alt`, `shift`, `meta`) that holzi derives from its own chords, so the
//! shim compares fields and holds no copy of the chord rules (`src/lib/wm/keybindings.ts`).
//!
//! ponytail: attention ("the tab wants attention") has no web equivalent the shim could observe;
//! extensions ask for it through the SDK (`extension_tab_attention`), not through the shim.

/// The shim script, without the surrounding `<script>` tags. Its SHA-256 goes into the CSP.
pub const SHIM: &str = r#"(() => {
  'use strict';
  if (window.top === window) return;
  const host = window.parent;
  const listen = window.addEventListener.bind(window);
  const unlisten = window.removeEventListener.bind(window);
  let port = null;
  let shortcuts = [];
  const send = (message) => { if (port) port.postMessage(message); };

  const where = () => {
    const hash = window.location.hash.replace(/^#/, '');
    const q = hash.indexOf('?');
    return { path: (q < 0 ? hash : hash.slice(0, q)) || '/', query: q < 0 ? '' : hash.slice(q + 1) };
  };
  let lastNav = null;
  let replaceNext = false;
  const reportNav = () => {
    const { path, query } = where();
    const key = path + '?' + query;
    if (key === lastNav) { replaceNext = false; return; }
    lastNav = key;
    send({ type: 'nav', path, query, replace: replaceNext });
    replaceNext = false;
  };
  const push = history.pushState.bind(history);
  const replace = history.replaceState.bind(history);
  history.pushState = function (...args) { const r = push(...args); reportNav(); return r; };
  history.replaceState = function (...args) { replaceNext = true; const r = replace(...args); reportNav(); return r; };
  listen('hashchange', reportNav);
  listen('popstate', reportNav);

  let lastTitle = null;
  const reportTitle = () => {
    const text = String(document.title || '').slice(0, 200);
    if (text === lastTitle) return;
    lastTitle = text;
    send({ type: 'title', text });
  };
  // The document, not its root element: as an init script (development page) the shim can run
  // before the parser created `<html>` (WebView2, Android), and `observe(null)` would throw.
  new MutationObserver(reportTitle).observe(document, {
    subtree: true, childList: true, characterData: true,
  });

  const guards = new Set();
  let propertyGuard = null;
  let lastGuard = null;
  const reportGuard = () => {
    const active = guards.size > 0 || typeof propertyGuard === 'function';
    if (active === lastGuard) return;
    lastGuard = active;
    send({ type: 'closeGuard', active });
  };
  window.addEventListener = function (type, listener, options) {
    if (type === 'beforeunload' && listener) { guards.add(listener); reportGuard(); }
    return listen(type, listener, options);
  };
  window.removeEventListener = function (type, listener, options) {
    if (type === 'beforeunload') { guards.delete(listener); reportGuard(); }
    return unlisten(type, listener, options);
  };
  Object.defineProperty(window, 'onbeforeunload', {
    configurable: true,
    get: () => propertyGuard,
    set: (value) => { propertyGuard = value; reportGuard(); },
  });
  window.close = () => send({ type: 'close' });

  listen('keydown', (event) => {
    const hit = shortcuts.find((s) => s.code === event.code && s.ctrl === event.ctrlKey
      && s.alt === event.altKey && s.shift === event.shiftKey && s.meta === event.metaKey);
    if (!hit) return;
    event.preventDefault();
    event.stopPropagation();
    send({ type: 'shortcut', id: hit.id });
  }, true);

  // A click inside the frame never reaches holzi's own document, so the window around the frame
  // could not come to the front; the shim says so, holzi raises the window.
  listen('pointerdown', () => send({ type: 'focus' }), true);

  const readShortcuts = (list) => (Array.isArray(list) ? list : []).filter((s) => s
    && typeof s.id === 'string' && typeof s.code === 'string' && typeof s.ctrl === 'boolean'
    && typeof s.alt === 'boolean' && typeof s.shift === 'boolean' && typeof s.meta === 'boolean');

  const onHost = (event) => {
    const data = event.data || {};
    if (data.type === 'navigate' && typeof data.path === 'string') {
      const query = typeof data.query === 'string' ? data.query : '';
      lastNav = data.path + '?' + query;
      window.location.hash = '#' + data.path + (query ? '?' + query : '');
    } else if (data.type === 'shortcuts') {
      shortcuts = readShortcuts(data.list);
    }
  };
  listen('message', (event) => {
    if (event.source !== host) return;
    const data = event.data;
    if (!data || data.type !== 'holzi:frame:init' || !event.ports || !event.ports[0]) return;
    const fresh = port === null;
    if (port) port.close();
    port = event.ports[0];
    port.onmessage = onHost;
    shortcuts = readShortcuts(data.shortcuts);
    lastNav = null;
    lastTitle = null;
    lastGuard = null;
    send({ type: 'hello', fresh });
    reportNav();
    reportTitle();
    reportGuard();
  });
})();"#;

/// The shim for the page of a development version (US12, research R16 addendum): holzi does not
/// serve that page, so its main window runs the same shim as an init script in every frame. It
/// acts only in a frame on a development server's address (`dev::server_url`), the frames
/// developer mode allows; a frame holzi serves has another address and gets the shim injected.
/// A sandboxed frame's origin is opaque, so the guard reads protocol and host of its URL.
pub fn dev_init_script() -> String {
    let elsewhere = crate::extensions::dev::LOOPBACK
        .map(|host| format!("at.hostname !== '{host}'"))
        .join(" && ");
    format!(
        "(() => {{ const at = window.location; \
         if (at.protocol !== 'http:' || ({elsewhere})) return; {SHIM} }})();"
    )
}

/// Index of `needle` in `haystack` at or after `from`, ignoring ASCII case.
pub(crate) fn find_ascii_case_insensitive(
    haystack: &str,
    needle: &str,
    from: usize,
) -> Option<usize> {
    let hay = haystack.as_bytes();
    let needle = needle.as_bytes();
    (from..=hay.len().checked_sub(needle.len())?)
        .find(|&i| hay[i..i + needle.len()].eq_ignore_ascii_case(needle))
}

/// End of the start tag that begins at `start` (the index after its `>`), skipping quoted
/// attribute values.
pub(crate) fn tag_end(html: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (i, c) in html[start..].char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(start + i + 1),
            _ => {}
        }
    }
    None
}

/// Puts the shim into an HTML document: right after the `<head>` start tag, else after `<html>`,
/// else at the very beginning.
pub fn inject(html: &str) -> String {
    let script = format!("<script>{SHIM}</script>");
    let after = ["<head", "<html"].iter().find_map(|tag| {
        let at = find_ascii_case_insensitive(html, tag, 0)?;
        let next = html.as_bytes().get(at + tag.len())?;
        (next.is_ascii_whitespace() || *next == b'>')
            .then(|| tag_end(html, at))
            .flatten()
    });
    let at = after.unwrap_or(0);
    format!("{}{script}{}", &html[..at], &html[at..])
}

#[cfg(test)]
#[path = "shim_tests.rs"]
mod tests;
