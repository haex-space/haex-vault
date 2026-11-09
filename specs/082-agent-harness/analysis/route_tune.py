"""Routing accuracy with the prompt holzi sends (spec 082 T047): the domains of `domains.json` and one
domain per extension, described by its tool titles, built as `chat/tools/route.rs` builds them. Tunes
on the development set only; `--final` adds the everyday set once, after the descriptions are fixed.
Usage:
python3 -I route_tune.py <llama-server> <models dir> <domains.json> <mail_tools.json> <devset.json>
    <set.json> [--final] <stem>...
"""
import json, re, subprocess, sys, time, urllib.request

SERVER, MODELS, DOMAINS, MAIL, DEV, SET = sys.argv[1:7]
FINAL = "--final" in sys.argv[7:]
# `name=description` replaces one domain's description, to try a wording on the dev set.
OVERRIDES = dict(a.split("=", 1) for a in sys.argv[7:] if "=" in a)
STEMS = [a for a in sys.argv[7:] if a != "--final" and "=" not in a]
PORT = 18096

declared = json.load(open(DOMAINS))
mail = json.load(open(MAIL))["extension"]
domains = [(d["key"], d["name"], d["description"]) for d in declared]
titles = [t["title"]["en"].lower() for t in mail["tools"]]
domains.append(("x_mail", "mail", f'{mail["title"]}: {", ".join(titles)}'))
domains = [(k, n, OVERRIDES.get(n, d)) for k, n, d in domains]

PROMPT = ("Which area of the app holzi does the user's request need? Areas:\n"
          + "".join(f"{name}: {description}\n" for _, name, description in domains)
          + "none: anything else: general knowledge, arithmetic, the time, small talk\n"
          + "Answer with one area name, or two if the request could mean either, comma-separated, "
            "nothing else.")


def parse(reply):
    """As `parse_route`: names or keys in order, at most two; returns names."""
    found = []
    for word in re.split(r"[^\w]+", reply):
        word = word.lower()
        for key, name, _ in domains:
            if word in (name.lower(), key.lower()) and name not in found:
                found.append(name)
        if len(found) == 2:
            break
    return found or ["none"]


PREFIX_TO_NAME = {key: name for key, name, _ in domains}


def label_of(req):
    e = req["expect"]
    if isinstance(e, str):
        return None
    if "answer" in e:
        return "none"
    chain = e["calls"] if "calls" in e else e["anyOf"][0]["calls"]
    tool = chain[0]["tool"]
    if tool.startswith("x_mail_"):
        return "mail"
    return PREFIX_TO_NAME.get(tool.split("_")[0])


dev = [tuple(x) for x in json.load(open(DEV))]
everyday = [(label_of(r), r["text"]) for r in json.load(open(SET))["requests"]]
everyday = [x for x in everyday if x[0]]


def post(body):
    req = urllib.request.Request(f"http://127.0.0.1:{PORT}/v1/chat/completions",
                                 data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.load(r)


print(PROMPT, "\n")
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
        times = []

        def route(text):
            t0 = time.time()
            r = post({"messages": [{"role": "system", "content": PROMPT},
                                   {"role": "user", "content": text}],
                      "max_tokens": 12, "temperature": 0,
                      "chat_template_kwargs": {"enable_thinking": False}})
            times.append(time.time() - t0)
            return parse(r["choices"][0]["message"]["content"])

        sets = [("dev", dev)] + ([("everyday", everyday)] if FINAL else [])
        line = []
        misses = []
        for name, items in sets:
            got = [(lab, text, route(text)) for lab, text in items]
            ok = sum(1 for lab, _, r in got if lab in r)
            line.append(f"{name} {ok}/{len(items)} ({ok / len(items):.0%})")
            if name == "dev":
                misses = [(lab, text, r) for lab, text, r in got if lab not in r]
        times.sort()
        print(f"{stem:34s} {'   '.join(line)}   median {times[len(times) // 2] * 1000:.0f} ms")
        for lab, text, got in misses[:8]:
            print(f"{'':36s}miss: {lab} <- {got}: {text}")
    finally:
        p.terminate()
        p.wait(timeout=60)
