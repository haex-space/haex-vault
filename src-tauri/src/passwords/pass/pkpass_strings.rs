//! The `pass.strings` files of a pass (spec 092, research R6): `"key" = "value";` lines in the
//! old property-list text format, in UTF-16 with a byte order mark or in UTF-8, with `/* … */` and
//! `// …` comments. A pass names a text by a key in `pass.json`, and the file of the language of
//! the window gives it; the text of `pass.json` itself is the base fallback.

use std::collections::HashMap;

/// The most pairs read from one file: a pass has a few dozen texts.
const MAX_PAIRS: usize = 2000;

/// Decodes the bytes of a strings file to text: UTF-16 (either order) with its mark, else UTF-8.
fn decode(bytes: &[u8]) -> String {
    let utf16 = |rest: &[u8], big: bool| {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                if big {
                    u16::from_be_bytes(*pair)
                } else {
                    u16::from_le_bytes(*pair)
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFE, 0xFF, rest @ ..] => utf16(rest, true),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, false),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// The pairs of a strings file. A line that is not a pair is skipped.
pub fn parse(bytes: &[u8]) -> HashMap<String, String> {
    let text = decode(bytes);
    let mut chars = text.chars().peekable();
    let mut pairs = HashMap::new();
    loop {
        skip_blank(&mut chars);
        let Some(key) = quoted(&mut chars) else {
            // Not a pair: drop the rest of the statement and go on.
            if chars.peek().is_none() {
                break;
            }
            for c in chars.by_ref() {
                if c == ';' || c == '\n' {
                    break;
                }
            }
            continue;
        };
        skip_blank(&mut chars);
        if chars.next() != Some('=') {
            continue;
        }
        skip_blank(&mut chars);
        let Some(value) = quoted(&mut chars) else {
            continue;
        };
        skip_blank(&mut chars);
        if chars.peek() == Some(&';') {
            chars.next();
        }
        pairs.insert(key, value);
        if pairs.len() >= MAX_PAIRS {
            break;
        }
    }
    pairs
}

fn skip_blank(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    loop {
        match chars.peek() {
            Some(c) if c.is_whitespace() => {
                chars.next();
            }
            Some('/') => {
                let mut ahead = chars.clone();
                ahead.next();
                match ahead.peek() {
                    Some('/') => {
                        for c in chars.by_ref() {
                            if c == '\n' {
                                break;
                            }
                        }
                    }
                    Some('*') => {
                        chars.next();
                        chars.next();
                        let mut last = ' ';
                        for c in chars.by_ref() {
                            if last == '*' && c == '/' {
                                break;
                            }
                            last = c;
                        }
                    }
                    _ => return,
                }
            }
            _ => return,
        }
    }
}

/// A double-quoted text with `\"`, `\\`, `\n`, `\t`, `\r` and `\UXXXX` escapes.
fn quoted(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    if chars.peek() != Some(&'"') {
        return None;
    }
    chars.next();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'U' | 'u' => {
                    let hex: String = (0..4).filter_map(|_| chars.next()).collect();
                    if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        out.push(c);
                    }
                }
                other => out.push(other),
            },
            other => out.push(other),
        }
    }
    None
}
