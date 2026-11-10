"""Accuracy and cost of recognising a request's domain: a routing call per model (llama-server) and a keyword
matcher over the declared tool titles and descriptions. Usage:
python3 -I router_eval.py <llama-server> <models dir> <set.json> <tools.json> <mail_tools.json> <devset.json> <stem>...
"""
import json, re, socket, subprocess, sys, time, urllib.request, collections, math
SERVER, MODELS, SET, TOOLS, MAIL, DEV = sys.argv[1:7]
STEMS = sys.argv[7:]
PORT = 18095
DOMAINS = {
    "mail": "read, find, answer, send, sort and delete e-mails",
    "contacts": "people's phone numbers, addresses, birthdays and e-mail addresses",
    "files": "find and open documents, photos and other files",
    "passwords": "saved logins, passwords and PINs",
    "settings": "the app's language, appearance and installed models",
    "apps": "open, close or switch apps, windows and tabs",
    "web": "search the internet or open a website",
    "sync": "rules that sync folders with other devices",
    "none": "anything else: general knowledge, arithmetic, the time, small talk",
}
PROMPT = ("Which area of the app holzi does the user's request need? Areas:\n" +
          "\n".join(f"{k}: {v}" for k, v in DOMAINS.items()) +
          "\nAnswer with one area name, or two if the request could mean either, comma-separated, nothing else.")
PREFIX_TO_DOMAIN = {"wm": "apps", "settings": "settings", "browser": "web", "files": "files", "contacts": "contacts",
                    "passwords": "passwords", "syncRules": "sync", "x": "mail"}

def label_of(req):
    e = req["expect"]
    if isinstance(e, str):
        return None  # refusal or question: any routing is acceptable
    if "answer" in e:
        return "none"
    chain = e["calls"] if "calls" in e else e["anyOf"][0]["calls"]
    return PREFIX_TO_DOMAIN.get(chain[0]["tool"].split("_")[0])

everyday = [(label_of(r), r["text"]) for r in json.load(open(SET))["requests"]]
everyday = [x for x in everyday if x[0]]
dev = [tuple(x) for x in json.load(open(DEV))]

# --- keyword matcher over declarations -------------------------------------------------------------
STOP = set("""der die das den dem des ein eine einen einem einer und oder mir mich mein meine meinen meiner du dir ich
ist sind war was wie wo wer wann bitte mal noch auch zu im in am an auf aus von vom mit für fürs the a an and or of to
in on at for from my me i you is are was what how where who when please it this that with do does did have has""".split())
def words(text):
    return [w for w in re.findall(r"[a-zäöüß0-9]+", text.lower()) if w not in STOP and len(w) > 2]
docs = collections.defaultdict(list)
for a in json.load(open(TOOLS)):
    d = PREFIX_TO_DOMAIN.get(a["toolName"].split("_")[0])
    if d:
        docs[d] += words(a["description"]) + words(a["titles"]["de"]) + words(a["titles"]["en"])
mail = json.load(open(MAIL))["extension"]
docs["mail"] += words(mail["title"]["de"] if isinstance(mail["title"], dict) else mail["title"])
for t in mail["tools"]:
    docs["mail"] += words(t["description"]) + words(t["title"]["de"]) + words(t["title"]["en"])
vocab = {d: set(ws) for d, ws in docs.items()}
df = collections.Counter(w for ws in vocab.values() for w in ws)
def kw_route(text):
    scores = {}
    for d, ws in vocab.items():
        s = 0.0
        for q in set(words(text)):
            hit = q in ws or (len(q) >= 4 and any(w.startswith(q[:max(4, len(q) - 2)]) for w in ws))
            if hit:
                s += math.log(1 + len(vocab) / df.get(q, 1))
        scores[d] = s
    best = max(scores, key=scores.get)
    return [best] if scores[best] >= 1.5 else ["none"]

def score(route, items):
    ok = sum(1 for lab, text in items if lab in route(text))
    return ok, len(items)

# --- routing call -------------------------------------------------------------------------------------
def post(body):
    req = urllib.request.Request(f"http://127.0.0.1:{PORT}/v1/chat/completions", data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.load(r)

MAX_ROUTED = 2  # as `route.rs`: at most two domains, in the order the reply names them

def parse_route(reply):
    """The domains the reply names, in reply order, at most MAX_ROUTED, as `parse_route` in route.rs does. `none`
    names nothing there: it counts only when no area is named, so a reply naming an area and `none` is that area."""
    named = []
    for word in re.findall(r"[a-z0-9_]+", reply.lower()):
        if word in DOMAINS and word != "none" and word not in named:
            named.append(word)
    if named:
        return named[:MAX_ROUTED]
    return ["none"] if re.search(r"\bnone\b", reply.lower()) else ["?"]

def model_route_factory(times):
    def route(text):
        t0 = time.time()
        r = post({"messages": [{"role": "system", "content": PROMPT}, {"role": "user", "content": text}],
                  "max_tokens": 12, "temperature": 0, "chat_template_kwargs": {"enable_thinking": False}})
        times.append(time.time() - t0)
        return parse_route(r["choices"][0]["message"]["content"])
    return route

def report(name, route, extra=""):
    a, n = score(route, dev); b, m = score(route, everyday)
    print(f"{name:34s} dev {a}/{n} ({a/n:.0%})   everyday {b}/{m} ({b/m:.0%}) {extra}")

report("keyword matcher (declarations)", kw_route)
def port_in_use():
    with socket.socket() as s:
        s.settimeout(1)
        return s.connect_ex(("127.0.0.1", PORT)) == 0

for stem in STEMS:
    if port_in_use():
        # Another server on the port would answer /health and its routes would be reported under this stem.
        sys.exit(f"port {PORT} is in use: stop the server that holds it first")
    p = subprocess.Popen([SERVER, "-m", f"{MODELS}/{stem}.gguf", "-ngl", "999", "-c", "4096", "--port", str(PORT),
                          "--no-webui", "-np", "1"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(300):
            if p.poll() is not None:
                sys.exit(f"{SERVER} exited with {p.returncode} before it was ready ({stem})")
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{PORT}/health", timeout=2); break
            except Exception:
                time.sleep(1)
        else:
            sys.exit(f"{SERVER} did not become ready for {stem}")
        times = []
        route = model_route_factory(times)
        wrong = []
        def logged(text, _r=route):
            return _r(text)
        report(f"routing call: {stem}", route)
        times.sort()
        print(f"{'':34s} median {times[len(times)//2]*1000:.0f} ms, 90th {times[int(len(times)*0.9)]*1000:.0f} ms per call")
        misses = [(lab, text, route(text)) for lab, text in dev if lab not in route(text)]
        for lab, text, got in misses[:6]:
            print(f"{'':36s}miss: {lab} <- {got}: {text}")
    finally:
        p.terminate(); p.wait(timeout=60)
