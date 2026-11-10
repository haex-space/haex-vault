"""Routing accuracy when an extension's name says nothing about its topic (follow-up of spec 082).

The app named an extension's domain by its prefix (`x_<slug>_`): haex-mail became `haexmail`, and an extension
called Thunderbird would be `thunderbird`. The routing counts a reply only when it repeats that name. This
measures, on the development set only, how the routing prompt holds up under several names; `mail` under the title
Thunderbird is the rule chosen from it (the word the tool titles share, `topic_word` in `chat/tools/domains.rs`):

  names     the app's style: "<name>: <title>: <tool titles>", a reply counts by name or key
  names+d   the same, with the manifest description in the line
  words     names+d, and a reply also counts by a word of the extension's title or manifest description
  numbers   every area numbered, "<n>: <name or title>: <description>", the reply is numbers

Each style runs under several (label, title) pairs; pass a style list (e.g. `names`) before the stems to run
only those.

Usage:
python3 -I route_names.py <llama-server> <models dir> <domains.json> <mail_tools.json> <devset.json> [<style>,...] <stem>...
"""
import json, re, subprocess, sys, time, urllib.request

SERVER, MODELS, DOMAINS, MAIL, DEV = sys.argv[1:6]
STEMS = [a for a in sys.argv[6:] if a not in ("names", "names+d", "words", "numbers") and "," not in a]
PORT = 18097
MANIFEST_DESCRIPTION = "Mail client for IMAP and SMTP accounts"
# (label, title): the label the routing prompt shows for the extension. "mail" and "messages" under the title
# Thunderbird stand for a label holzi gives itself; "mail" is what the tool titles share.
NAMINGS = [("mail", "haex-mail"), ("haexmail", "haex-mail"), ("thunderbird", "Thunderbird"),
           ("mail", "Thunderbird"), ("messages", "Thunderbird")]
STYLES = sys.argv[6].split(",") if len(sys.argv) > 6 and sys.argv[6] in ("names", "names+d", "words", "numbers") \
    or (len(sys.argv) > 6 and "," in sys.argv[6]) else ["names", "names+d", "words", "numbers"]
STOP = {"and", "for", "the", "with", "of", "a", "an", "to", "or", "in", "on", "your", "its", "app"}

declared = json.load(open(DOMAINS))
mail = json.load(open(MAIL))["extension"]
tool_titles = ", ".join(t["title"]["en"].lower() for t in mail["tools"])
dev = [tuple(x) for x in json.load(open(DEV))]


def areas(naming, style):
    """(key, label, line text, extra words) per area; the mail extension's key is always `x_mail`."""
    name, title = naming
    out = [(d["key"], d["name"], d["description"], set()) for d in declared]
    if style == "names":
        text = f"{title}: {tool_titles}"
    else:
        text = f"{title}, {MANIFEST_DESCRIPTION.lower()}: {tool_titles}"
    extra = set()
    if style == "words":
        extra = {w for w in re.split(r"[^\w]+", f"{title} {MANIFEST_DESCRIPTION}".lower())
                 if len(w) > 2 and w not in STOP}
    out.append(("x_mail", name, text, extra))
    return out


def prompt(listed, style):
    head = "Which area of the app holzi does the user's request need? Areas:\n"
    if style == "numbers":
        lines = "".join(f"{i}: {label}: {text}\n" for i, (_, label, text, _) in enumerate(listed, 1))
        lines += "0: none: anything else: general knowledge, arithmetic, the time, small talk\n"
        tail = ("Answer with one area number, or two if the request could mean either, comma-separated, "
                "nothing else.")
    else:
        lines = "".join(f"{label}: {text}\n" for _, label, text, _ in listed)
        lines += "none: anything else: general knowledge, arithmetic, the time, small talk\n"
        tail = ("Answer with one area name, or two if the request could mean either, comma-separated, "
                "nothing else.")
    return head + lines + tail


def parse(reply, listed, style):
    found = []
    for word in re.split(r"[^\w]+", reply.lower()):
        if not word:
            continue
        for i, (key, label, _, extra) in enumerate(listed, 1):
            if style == "numbers":
                hit = word == str(i)
            else:
                hit = word in (label.lower(), key.lower()) or word in extra
            if hit and key not in found:
                found.append(key)
        if len(found) == 2:
            break
    return found


def label_key(label):
    """The devset labels name holzi's domains by name and the mail extension as `mail`."""
    if label == "mail":
        return "x_mail"
    if label == "none":
        return "none"
    return next(d["key"] for d in declared if d["name"] == label)


def post(body):
    req = urllib.request.Request(f"http://127.0.0.1:{PORT}/v1/chat/completions",
                                 data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.load(r)


for stem in STEMS:
    p = subprocess.Popen([SERVER, "-m", f"{MODELS}/{stem}.gguf", "-ngl", "999", "-c", "4096",
                          "--port", str(PORT), "--no-webui", "-np", "1"],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(300):
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{PORT}/health", timeout=2)
                break
            except Exception:
                time.sleep(1)
        print(stem)
        for style in STYLES:
            cells = []
            for naming in NAMINGS:
                listed = areas(naming, style)
                system = prompt(listed, style)
                ok = mail_ok = mail_n = 0
                misses = []
                for label, text in dev:
                    r = post({"messages": [{"role": "system", "content": system},
                                           {"role": "user", "content": text}],
                              "max_tokens": 12, "temperature": 0,
                              "chat_template_kwargs": {"enable_thinking": False}})
                    reply = r["choices"][0]["message"]["content"]
                    got = parse(reply, listed, style) or ["none"]
                    want = label_key(label)
                    hit = want in got
                    ok += hit
                    if want == "x_mail":
                        mail_n += 1
                        mail_ok += hit
                        if not hit and len(misses) < 3:
                            misses.append(reply.strip()[:30])
                cells.append(f"{naming[0]}/{naming[1][:5]:5s} {ok:2d}/{len(dev)} mail {mail_ok:2d}/{mail_n}")
                if misses:
                    cells[-1] += f" ({'; '.join(misses)})"
            print(f"  {style:8s} " + "   ".join(cells), flush=True)
    finally:
        p.terminate()
        p.wait(timeout=60)
