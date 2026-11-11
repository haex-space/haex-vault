"""Routing accuracy for spec 087 (research R11): spec 082's `route_tune.py` with a second development set of
requests that name only a person or a thing (`routeset.json`, each with every area that may hold it; right when all
of them are among the routed areas), and a routing prompt that can take one more rule.
Tunes on the development sets only; `--final` adds the everyday set once, after the wording is fixed.
Usage:
python3 -I route087.py <llama-server> <models dir> <domains.json> <mail_tools.json> <devset.json> <routeset.json>
    <set.json> [--final] [rule=<sentence>] [answer=<sentence>] [<area>=<description>] <stem>...
"""
import json, re, subprocess, sys, time, urllib.request

SERVER, MODELS, DOMAINS, MAIL, DEV, ROUTESET, SET = sys.argv[1:8]
FINAL = "--final" in sys.argv[8:]
# `name=description` replaces one domain's description; `rule=` adds a sentence before the answer's form,
# `answer=` replaces the sentence on the answer's form.
OVERRIDES = dict(a.split("=", 1) for a in sys.argv[8:] if "=" in a)
RULE = OVERRIDES.pop("rule", "")
ANSWER = OVERRIDES.pop("answer", "Answer with one area name, or two if the request could mean either, "
                                 "comma-separated, nothing else.")
STEMS = [a for a in sys.argv[8:] if a != "--final" and "=" not in a]
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
          + (RULE + " " if RULE else "") + ANSWER)


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


# Every item: (the areas that must all be routed, text).
dev = [([lab], text) for lab, text in json.load(open(DEV))]
routeset = [(labs, text) for labs, text in json.load(open(ROUTESET))]
everyday = [([label_of(r)], r["text"]) for r in json.load(open(SET))["requests"]]
everyday = [x for x in everyday if x[0][0]]


def post(body):
    req = urllib.request.Request(f"http://127.0.0.1:{PORT}/v1/chat/completions",
                                 data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.load(r)


print(PROMPT, end="\n\n")
def port_taken():
    try:
        urllib.request.urlopen(f"http://127.0.0.1:{PORT}/health", timeout=2)
    except urllib.error.HTTPError:
        return True
    except (urllib.error.URLError, OSError):
        return False
    return True


for stem in STEMS:
    # Another server on the port would answer for this model.
    if port_taken():
        sys.exit(f"port {PORT} is taken; stop the server on it first")
    p = subprocess.Popen([SERVER, "-m", f"{MODELS}/{stem}.gguf", "-ngl", "999", "-c", "4096",
                          "--port", str(PORT), "--no-webui", "-np", "1"],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(300):
            # A server that ended (its port taken by another) must not leave the answers to that one.
            if p.poll() is not None:
                sys.exit(f"llama-server for {stem} ended with {p.returncode}; is port {PORT} taken?")
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{PORT}/health", timeout=2)
                break
            except Exception:
                time.sleep(1)
        else:
            sys.exit(f"llama-server for {stem} did not come up")
        times = []

        def route(text):
            t0 = time.time()
            r = post({"messages": [{"role": "system", "content": PROMPT},
                                   {"role": "user", "content": text}],
                      "max_tokens": 12, "temperature": 0,
                      "chat_template_kwargs": {"enable_thinking": False}})
            times.append(time.time() - t0)
            return parse(r["choices"][0]["message"]["content"])

        sets = [("dev", dev), ("route", routeset)] + ([("everyday", everyday)] if FINAL else [])
        line = []
        misses = []
        for name, items in sets:
            got = [(labs, text, route(text)) for labs, text in items]
            ok = sum(1 for labs, _, r in got if all(lab in r for lab in labs))
            wide = sum(1 for labs, _, r in got if len(r) > len(labs))
            line.append(f"{name} {ok}/{len(items)} ({ok / len(items):.0%}, {wide} wider)")
            if name != "everyday":
                misses += [(labs, text, r) for labs, text, r in got if not all(lab in r for lab in labs)]
        times.sort()
        print(f"{stem:34s} {'   '.join(line)}   median {times[len(times) // 2] * 1000:.0f} ms")
        for labs, text, got in misses:
            print(f"{'':36s}miss: {','.join(labs)} <- {got}: {text}")
    finally:
        p.terminate()
        p.wait(timeout=60)
