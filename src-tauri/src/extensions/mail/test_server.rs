//! A small IMAP and SMTP server on this device for the mail tests (no TLS, which holzi allows only
//! towards this device). It knows what the tests need: LOGIN, LIST, STATUS, SELECT, CREATE,
//! UID SEARCH (also with criteria), UID FETCH, UID STORE, UID MOVE, UID EXPUNGE, APPEND, LOGOUT;
//! EHLO, AUTH PLAIN, MAIL, RCPT, DATA, QUIT.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

pub const USER: &str = "anna";
pub const PASSWORD: &str = "geheim-7f1c";

#[derive(Debug, Clone)]
pub struct Stored {
    pub uid: u32,
    pub flags: Vec<String>,
    pub raw: Vec<u8>,
}

#[derive(Default)]
pub struct State {
    pub boxes: BTreeMap<String, Vec<Stored>>,
    pub next_uid: u32,
    /// What SMTP received: the envelope recipients and the data.
    pub sent: Vec<(Vec<String>, Vec<u8>)>,
    /// Every command line the IMAP server read.
    pub commands: Vec<String>,
    /// Whether the server offers IDLE.
    pub no_idle: bool,
    /// Whether the server lacks UIDPLUS (and with it UID EXPUNGE).
    pub no_uidplus: bool,
    /// Wakes the sessions in IDLE when a message arrives.
    pub arrived: Option<tokio::sync::broadcast::Sender<()>>,
}

/// A new message in `mailbox`, as if it arrived from outside; sessions in IDLE hear of it.
pub fn deliver(state: &Shared, mailbox: &str, subject: &str) {
    let mut s = state.lock().unwrap();
    let uid = s.next_uid;
    s.next_uid += 1;
    s.boxes.entry(mailbox.to_owned()).or_default().push(Stored {
        uid,
        flags: vec![],
        raw: message(subject, false),
    });
    if let Some(arrived) = &s.arrived {
        let _ = arrived.send(());
    }
}

pub type Shared = Arc<Mutex<State>>;

#[path = "test_server_manage.rs"]
mod manage;
#[path = "test_server_smtp.rs"]
mod smtp;

/// Builds a deterministic RFC 822 message for the IMAP and SMTP tests.
pub fn message(subject: &str, with_attachment: bool) -> Vec<u8> {
    let mut text = format!(
        "From: Anna <anna@example.org>\r\nTo: Ben <ben@example.org>\r\nSubject: {subject}\r\n\
         Message-ID: <{subject}@example.org>\r\nReferences: <a@x>\r\n <b@x>\r\nMIME-Version: 1.0\r\n"
    );
    if with_attachment {
        text.push_str(
            "Content-Type: multipart/mixed; boundary=\"B\"\r\n\r\n--B\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\r\nHallo grün\r\n--B\r\n\
             Content-Type: application/pdf; name=\"a.pdf\"\r\nContent-Disposition: attachment; filename=\"a.pdf\"\r\n\
             Content-Transfer-Encoding: base64\r\n\r\nUERGLWRhdGE=\r\n--B--\r\n",
        );
    } else {
        text.push_str("Content-Type: text/plain; charset=utf-8\r\n\r\nHallo grün\r\n");
    }
    text.into_bytes()
}

/// Creates the shared test server state with two initial messages.
pub fn state() -> Shared {
    let mut state = State {
        next_uid: 10,
        arrived: Some(tokio::sync::broadcast::channel(16).0),
        ..State::default()
    };
    state.boxes.insert(
        "INBOX".into(),
        vec![
            Stored {
                uid: 7,
                flags: vec!["\\Seen".into()],
                raw: message("first", false),
            },
            Stored {
                uid: 9,
                flags: vec![],
                raw: message("second", true),
            },
        ],
    );
    state.boxes.insert("Archive".into(), Vec::new());
    Arc::new(Mutex::new(state))
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The arguments of a command line, quoted strings unquoted.
fn words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c == ' ' {
            chars.next();
        } else if c == '"' {
            chars.next();
            let mut word = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => word.extend(chars.next()),
                    '"' => break,
                    c => word.push(c),
                }
            }
            out.push(word);
        } else if c == '(' {
            let mut word = String::new();
            for c in chars.by_ref() {
                word.push(c);
                if c == ')' {
                    break;
                }
            }
            out.push(word);
        } else {
            let mut word = String::new();
            while let Some(&c) = chars.peek() {
                if c == ' ' {
                    break;
                }
                word.push(c);
                chars.next();
            }
            out.push(word);
        }
    }
    out
}

fn uids_of(set: &str, messages: &[Stored]) -> Vec<u32> {
    let mut uids = Vec::new();
    for part in set.split(',') {
        if let Some((a, b)) = part.split_once(':') {
            let high = messages.iter().map(|m| m.uid).max().unwrap_or(0);
            let a: u32 = a.parse().unwrap_or(1);
            let b: u32 = if b == "*" {
                high
            } else {
                b.parse().unwrap_or(0)
            };
            let (a, b) = (a.min(b), a.max(b));
            uids.extend(
                messages
                    .iter()
                    .map(|m| m.uid)
                    .filter(|u| (a..=b).contains(u)),
            );
        } else if let Ok(uid) = part.parse() {
            uids.push(uid);
        }
    }
    uids
}

fn envelope_fetch(seq: usize, m: &Stored) -> Vec<u8> {
    let has_attachment = m.raw.windows(9).any(|w| w == b"multipart");
    let structure = if has_attachment {
        "((\"text\" \"plain\" (\"charset\" \"utf-8\") NIL NIL \"8bit\" 12 1)(\"application\" \"pdf\" (\"name\" \"a.pdf\") NIL NIL \"base64\" 12) \"mixed\")"
    } else {
        "(\"text\" \"plain\" (\"charset\" \"utf-8\") NIL NIL \"8bit\" 12 1)"
    };
    let subject = String::from_utf8_lossy(&m.raw)
        .lines()
        .find_map(|l| l.strip_prefix("Subject: ").map(str::to_owned))
        .unwrap_or_default();
    let header =
        format!("Message-ID: <{subject}@example.org>\r\nReferences: <a@x>\r\n <b@x>\r\n\r\n");
    let flags = m.flags.join(" ");
    format!(
        "* {seq} FETCH (UID {uid} FLAGS ({flags}) INTERNALDATE \"01-Jan-2026 10:00:00 +0000\" RFC822.SIZE {size} \
         ENVELOPE (NIL {subj} ((\"Anna\" NIL \"anna\" \"example.org\")) NIL NIL ((\"Ben\" NIL \"ben\" \"example.org\")) NIL NIL \"<parent@example.org>\" \"<{subject}@example.org>\") \
         BODYSTRUCTURE {structure} BODY[HEADER.FIELDS (MESSAGE-ID IN-REPLY-TO REFERENCES)] {{{len}}}\r\n{header})\r\n",
        uid = m.uid,
        size = m.raw.len(),
        subj = quoted(&subject),
        len = header.len(),
    )
    .into_bytes()
}

async fn imap_session(stream: TcpStream, state: Shared) {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let _ = write.write_all(b"* OK test server ready\r\n").await;
    let mut selected = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
            return;
        }
        let line = line.trim_end().to_owned();
        state.lock().unwrap().commands.push(line.clone());
        let (tag, rest) = line.split_once(' ').unwrap_or((&line, ""));
        let (command, args) = rest.split_once(' ').unwrap_or((rest, ""));
        let mut out: Vec<u8> = Vec::new();
        let ok = |text: &str| format!("{tag} OK {text}\r\n").into_bytes();
        match command.to_ascii_uppercase().as_str() {
            "CAPABILITY" => {
                let s = state.lock().unwrap();
                let plus = if s.no_uidplus { "" } else { " UIDPLUS" };
                let idle = if s.no_idle { "" } else { " IDLE" };
                out.extend(format!("* CAPABILITY IMAP4rev1 MOVE{plus}{idle}\r\n").into_bytes());
            }
            "NOOP" => {
                let n = state
                    .lock()
                    .unwrap()
                    .boxes
                    .get(&selected)
                    .map_or(0, Vec::len);
                out.extend(format!("* {n} EXISTS\r\n").into_bytes());
            }
            "IDLE" => {
                let mut arrived = state.lock().unwrap().arrived.as_ref().unwrap().subscribe();
                let _ = write.write_all(b"+ idling\r\n").await;
                loop {
                    let mut done = String::new();
                    tokio::select! {
                        read = reader.read_line(&mut done) => {
                            if read.unwrap_or(0) == 0 {
                                return;
                            }
                            break;
                        }
                        _ = arrived.recv() => {
                            let n = state.lock().unwrap().boxes.get(&selected).map_or(0, Vec::len);
                            let _ = write.write_all(format!("* {n} EXISTS\r\n").as_bytes()).await;
                        }
                    }
                }
            }
            "LOGIN" => {
                let w = words(args);
                if w.first().map(String::as_str) != Some(USER)
                    || w.get(1).map(String::as_str) != Some(PASSWORD)
                {
                    out.extend(format!("{tag} NO [AUTHENTICATIONFAILED] wrong\r\n").into_bytes());
                    let _ = write.write_all(&out).await;
                    continue;
                }
            }
            "CREATE" => {
                let name = words(args).remove(0);
                let exists = {
                    let mut s = state.lock().unwrap();
                    let exists = s.boxes.contains_key(&name);
                    if !exists {
                        s.boxes.insert(name, Vec::new());
                    }
                    exists
                };
                if exists {
                    let refusal = format!("{tag} NO [ALREADYEXISTS] Mailbox already exists\r\n");
                    let _ = write.write_all(refusal.as_bytes()).await;
                    continue;
                }
            }
            "LIST" => {
                for name in state.lock().unwrap().boxes.keys() {
                    // A special-use mailbox (RFC 6154) carries its role.
                    let role = if name == "Archive" { " \\Archive" } else { "" };
                    out.extend(
                        format!("* LIST (\\HasNoChildren{role}) \"/\" {}\r\n", quoted(name))
                            .into_bytes(),
                    );
                }
            }
            "STATUS" => {
                let w = words(args);
                let s = state.lock().unwrap();
                let messages = s.boxes.get(&w[0]).cloned().unwrap_or_default();
                let unseen = messages
                    .iter()
                    .filter(|m| !m.flags.iter().any(|f| f == "\\Seen"))
                    .count();
                out.extend(
                    format!(
                        "* STATUS {} (MESSAGES {} UNSEEN {unseen} UIDVALIDITY 1 UIDNEXT {})\r\n",
                        quoted(&w[0]),
                        messages.len(),
                        s.next_uid
                    )
                    .into_bytes(),
                );
            }
            "SELECT" => {
                selected = words(args).remove(0);
                let (n, next) = {
                    let s = state.lock().unwrap();
                    (s.boxes.get(&selected).map_or(0, Vec::len), s.next_uid)
                };
                out.extend(
                    format!(
                        "* {n} EXISTS\r\n* OK [UIDVALIDITY 1] ok\r\n* OK [UIDNEXT {next}] ok\r\n"
                    )
                    .into_bytes(),
                );
                out.extend(format!("{tag} OK [READ-WRITE] SELECT done\r\n").into_bytes());
                let _ = write.write_all(&out).await;
                continue;
            }
            "UID" => {
                let (sub, rest) = args.split_once(' ').unwrap_or((args, ""));
                let mut s = state.lock().unwrap();
                let messages = s.boxes.get(&selected).cloned().unwrap_or_default();
                match sub.to_ascii_uppercase().as_str() {
                    "SEARCH" => {
                        let set = rest.trim_start_matches("UID ").trim();
                        let found: Vec<u32> = if manage::is_criteria(rest) {
                            manage::search(rest, &messages)
                        } else if rest.starts_with("UID ") {
                            uids_of(set, &messages)
                        } else {
                            // A sequence set.
                            let (a, b) = set.split_once(':').unwrap_or((set, set));
                            let (a, b): (usize, usize) =
                                (a.parse().unwrap_or(1), b.parse().unwrap_or(0));
                            messages
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| (a..=b).contains(&(i + 1)))
                                .map(|(_, m)| m.uid)
                                .collect()
                        };
                        let list: Vec<String> = found.iter().map(u32::to_string).collect();
                        out.extend(format!("* SEARCH {}\r\n", list.join(" ")).into_bytes());
                    }
                    "FETCH" => {
                        let (set, query) = rest.split_once(' ').unwrap_or((rest, ""));
                        for uid in uids_of(set, &messages) {
                            let Some((seq, m)) =
                                messages.iter().enumerate().find(|(_, m)| m.uid == uid)
                            else {
                                continue;
                            };
                            if query.contains("ENVELOPE") {
                                out.extend(envelope_fetch(seq + 1, m));
                            } else if query.contains("BODY.PEEK[]") {
                                out.extend(format!("* {} FETCH (UID {uid} FLAGS ({}) INTERNALDATE \"01-Jan-2026 10:00:00 +0000\" RFC822.SIZE {} BODY[] {{{}}}\r\n", seq + 1, m.flags.join(" "), m.raw.len(), m.raw.len()).into_bytes());
                                out.extend(&m.raw);
                                out.extend(b")\r\n");
                            } else {
                                out.extend(
                                    format!(
                                        "* {} FETCH (UID {uid} RFC822.SIZE {})\r\n",
                                        seq + 1,
                                        m.raw.len()
                                    )
                                    .into_bytes(),
                                );
                            }
                        }
                    }
                    "STORE" => {
                        let w = words(rest);
                        let flags: Vec<String> = w[2]
                            .trim_matches(['(', ')'])
                            .split(' ')
                            .filter(|f| !f.is_empty())
                            .map(str::to_owned)
                            .collect();
                        let uids = uids_of(&w[0], &messages);
                        let add = w[1].starts_with('+');
                        if let Some(list) = s.boxes.get_mut(&selected) {
                            for m in list.iter_mut().filter(|m| uids.contains(&m.uid)) {
                                m.flags.retain(|f| !flags.contains(f));
                                if add {
                                    m.flags.extend(flags.clone());
                                }
                            }
                        }
                    }
                    "MOVE" => {
                        let w = words(rest);
                        let uids = uids_of(&w[0], &messages);
                        let moved: Vec<Stored> = messages
                            .iter()
                            .filter(|m| uids.contains(&m.uid))
                            .cloned()
                            .collect();
                        s.boxes
                            .get_mut(&selected)
                            .unwrap()
                            .retain(|m| !uids.contains(&m.uid));
                        s.boxes.entry(w[1].clone()).or_default().extend(moved);
                    }
                    "EXPUNGE" => out.extend(manage::expunge(&mut s, &selected, rest)),
                    _ => out.extend(format!("{tag} BAD unknown\r\n").into_bytes()),
                }
            }
            "APPEND" => {
                // APPEND "box" (flags) {n}
                let w = words(args);
                let len: usize = args
                    .rsplit('{')
                    .next()
                    .and_then(|n| n.trim_end_matches('}').parse().ok())
                    .unwrap_or(0);
                let _ = write.write_all(b"+ go ahead\r\n").await;
                let mut data = vec![0u8; len];
                let _ = reader.read_exact(&mut data).await;
                let mut rest = String::new();
                let _ = reader.read_line(&mut rest).await;
                let flags = w
                    .iter()
                    .find(|x| x.starts_with('('))
                    .map(|f| {
                        f.trim_matches(['(', ')'])
                            .split(' ')
                            .filter(|x| !x.is_empty())
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                let mut s = state.lock().unwrap();
                let uid = s.next_uid;
                s.next_uid += 1;
                s.boxes.entry(w[0].clone()).or_default().push(Stored {
                    uid,
                    flags,
                    raw: data,
                });
            }
            "LOGOUT" => {
                let _ = write
                    .write_all(format!("* BYE\r\n{tag} OK bye\r\n").as_bytes())
                    .await;
                return;
            }
            _ => {
                let _ = write
                    .write_all(format!("{tag} BAD unknown\r\n").as_bytes())
                    .await;
                continue;
            }
        }
        out.extend(ok("done"));
        let _ = write.write_all(&out).await;
    }
}

/// Starts both servers on `runtime`; returns the IMAP and the SMTP port.
pub fn start(runtime: &tokio::runtime::Runtime, state: Shared) -> (u16, u16) {
    runtime.block_on(async {
        let imap = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let smtp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ports = (
            imap.local_addr().unwrap().port(),
            smtp.local_addr().unwrap().port(),
        );
        let imap_state = Arc::clone(&state);
        tokio::spawn(async move {
            while let Ok((stream, _)) = imap.accept().await {
                tokio::spawn(imap_session(stream, Arc::clone(&imap_state)));
            }
        });
        tokio::spawn(async move {
            while let Ok((stream, _)) = smtp.accept().await {
                tokio::spawn(smtp::session(stream, Arc::clone(&state)));
            }
        });
        ports
    })
}
