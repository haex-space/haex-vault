//! Cuts the streamed answer into sentences, so each one is spoken as soon as it is complete
//! (spec 066 FR-022, research R8): at `.`, `!`, `?`, `…`, `:` followed by a space, and at every
//! line break; not inside numbers ("3.5"), times ("10:30"), URLs or abbreviations ("z. B.", "e.g.",
//! "Dr."), and not after a list number ("1."). Table rows and the lines of a fenced code block are
//! cut only at line breaks, so `speakable` sees them whole.

/// Abbreviations that end with a period but not a sentence (lower case, without the period). Single
/// letters ("z", "b") and dotted ones ("e.g", "z.b") are recognized without a list.
const ABBREVIATIONS: &[&str] = &[
    "dr", "prof", "nr", "ca", "bzw", "vgl", "evtl", "inkl", "ggf", "mr", "mrs", "ms", "st", "vs",
    "no", "sr", "sra", "srta", "dott", "sig", "mme", "mlle", "jr", "abs", "tel",
];

/// German months: "am 3. Oktober" is a date, not the end of a sentence.
const MONTHS_DE: &[&str] = &[
    "januar",
    "februar",
    "märz",
    "april",
    "mai",
    "juni",
    "juli",
    "august",
    "september",
    "oktober",
    "november",
    "dezember",
];

#[derive(Debug, Default)]
pub struct Sentences {
    buf: String,
    in_fence: Option<Fence>,
}

/// The delimiter of a CommonMark fenced code block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Fence {
    marker: char,
    length: usize,
}

/// Whether a line opens a CommonMark fenced code block.
pub(super) fn fence_opener(line: &str) -> Option<Fence> {
    let (fence, info) = fence_run(line)?;
    if fence.marker == '`' && info.contains('`') {
        return None;
    }
    Some(fence)
}

/// Whether a line closes `fence`: same marker, at least its length, and whitespace after it.
pub(super) fn fence_closer(line: &str, fence: Fence) -> bool {
    fence_run(line).is_some_and(|(closing, tail)| {
        closing.marker == fence.marker && closing.length >= fence.length && tail.trim().is_empty()
    })
}

fn fence_run(line: &str) -> Option<(Fence, &str)> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let marker = rest.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let length = rest
        .bytes()
        .take_while(|byte| *byte == marker as u8)
        .count();
    (length >= 3).then_some((Fence { marker, length }, &rest[length..]))
}

impl Sentences {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a token delta; returns the sentences it completed, trimmed, in order.
    pub fn push(&mut self, delta: &str) -> Vec<String> {
        self.buf.push_str(delta);
        self.drain(false)
    }

    /// The end of the answer: whatever is left is the last sentence.
    pub fn finish(&mut self) -> Vec<String> {
        self.drain(true)
    }

    fn drain(&mut self, at_end: bool) -> Vec<String> {
        let mut out = Vec::new();
        loop {
            let first_line = self.buf.split('\n').next().unwrap_or_default();
            let start = first_line.trim_start_matches([' ', '\t']);
            let opening = self.in_fence.is_none() && fence_opener(first_line).is_some();
            let closing = self
                .in_fence
                .is_some_and(|fence| fence_closer(first_line, fence));
            let line_mode = self.in_fence.is_some() || opening || closing || start.starts_with('|');
            let Some((end, rest)) = next_cut(&self.buf, line_mode, at_end) else {
                break;
            };
            let raw_piece = self.buf[..end].to_owned();
            let piece = raw_piece.trim().to_owned();
            self.buf = self.buf[rest..].trim_start_matches([' ', '\t']).to_owned();
            if self
                .in_fence
                .is_some_and(|fence| fence_closer(&raw_piece, fence))
            {
                self.in_fence = None;
                out.push(raw_piece);
            } else if self.in_fence.is_none()
                && let Some(fence) = fence_opener(&raw_piece)
            {
                self.in_fence = Some(fence);
                out.push(raw_piece);
            } else if !piece.is_empty() {
                if raw_piece.trim_start().starts_with("```")
                    || raw_piece.trim_start().starts_with("~~~")
                {
                    out.push(raw_piece);
                } else {
                    out.push(piece);
                }
            }
            if self.buf.is_empty() {
                break;
            }
        }
        out
    }
}

fn is_run(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '…')
}

fn is_closer(c: char) -> bool {
    matches!(c, ')' | ']' | '"' | '\'' | '”' | '“' | '»' | '«' | '’')
}

/// Where the next sentence ends (`end`) and the one after it starts (`rest`), or `None` when that
/// is not decided yet: the buffer ends right after a sentence mark that more text could still
/// turn into an abbreviation or a number.
fn next_cut(buf: &str, line_mode: bool, at_end: bool) -> Option<(usize, usize)> {
    let chars: Vec<(usize, char)> = buf.char_indices().collect();
    let byte = |k: usize| chars.get(k).map_or(buf.len(), |(i, _)| *i);
    let mut k = 0;
    while k < chars.len() {
        let (i, c) = chars[k];
        if c == '\n' {
            return Some((i, i + 1));
        }
        if line_mode || !(is_run(c) || c == ':') {
            k += 1;
            continue;
        }
        let mut e = k + 1;
        if c != ':' {
            while e < chars.len() && is_run(chars[e].1) {
                e += 1;
            }
        }
        while e < chars.len() && is_closer(chars[e].1) {
            e += 1;
        }
        let Some(&(_, next)) = chars.get(e) else {
            return at_end.then_some((buf.len(), buf.len()));
        };
        if !next.is_whitespace() {
            k = e;
            continue;
        }
        if c == '.' && e == k + 1 {
            match period_ends(buf, i, &buf[byte(e)..], at_end) {
                Some(true) => {}
                Some(false) => {
                    k = e;
                    continue;
                }
                None => return None,
            }
        }
        return Some((byte(e), byte(e)));
    }
    if at_end && !buf.trim().is_empty() {
        return Some((buf.len(), buf.len()));
    }
    None
}

/// Whether the single period at byte `at` (followed by whitespace, then `after`) ends a sentence;
/// `None` when the next word is needed but has not arrived.
fn period_ends(buf: &str, at: usize, after: &str, at_end: bool) -> Option<bool> {
    let before = &buf[..at];
    let token = before
        .rsplit(char::is_whitespace)
        .next()
        .unwrap_or_default()
        .trim_start_matches(['(', '[', '"', '\'', '„', '“', '«']);
    if token.contains("://") || token.starts_with("www.") {
        return Some(true);
    }
    let lower = token.to_lowercase();
    if token.chars().count() == 1 && token.chars().all(char::is_alphabetic) {
        return Some(false);
    }
    if lower.contains('.') && lower.split('.').all(|part| part.chars().count() <= 2) {
        return Some(false);
    }
    if ABBREVIATIONS.contains(&lower.as_str()) {
        return Some(false);
    }
    let next_text = after.trim_start();
    if after[..after.len() - next_text.len()].contains('\n') {
        return Some(true);
    }
    let next_word: String = next_text
        .chars()
        .take_while(|c| c.is_alphabetic())
        .collect();
    let word_complete = next_text.chars().count() > next_word.chars().count();
    if !token.is_empty() && token.chars().all(|c| c.is_ascii_digit()) {
        if before.trim() == token {
            return Some(false);
        }
        if !word_complete && !at_end {
            return None;
        }
        if MONTHS_DE.contains(&next_word.to_lowercase().as_str()) {
            return Some(false);
        }
    }
    match next_text.chars().next() {
        None if !at_end => None,
        Some(first) if first.is_lowercase() => Some(false),
        _ => Some(true),
    }
}
