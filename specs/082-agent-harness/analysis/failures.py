"""Root causes of everyday failures per model. Usage: python3 -I failures.py <set.json> <report.json>..."""
import json, sys, collections
SET, REPORTS = sys.argv[1], sys.argv[2:]
requests = {r["id"]: r for r in json.load(open(SET))["requests"]}

def expected_tools(req):
    exp = req["expect"]
    if isinstance(exp, str):
        return set(), [exp]
    chains = []
    if "calls" in exp:
        chains = [exp["calls"]]
    elif "anyOf" in exp:
        chains = [alt.get("calls", []) for alt in exp["anyOf"]]
    names = set()
    for chain in chains:
        for c in chain:
            names.add(c.get("tool") or c.get("name"))
    return {n for n in names if n}, list(exp.keys())

def cause(fail, req, safety_kinds):
    calls = [c["tool"] for c in fail["calls"]]
    want, kinds = expected_tools(req)
    if kinds == ["answer"]:
        return "expected_answer:" + ("called " + calls[0] if calls else fail["result"])
    if kinds in (["refusal"], ["question"]):
        return f"expected_{kinds[0]}:" + ("called " + calls[0] if calls else fail["result"])
    if fail["result"] == "safety_error":
        return "safety:" + ",".join(sorted(safety_kinds.get(fail["id"], ["?"])))
    if not calls:
        return "no_call"
    hit = [c for c in calls if c in want]
    detour = any(c.startswith("wm_") for c in calls) and not any(w.startswith("wm_") for w in want)
    used_find = "find_actions" in calls
    if want and not hit:
        if detour: return "never_right_tool:wm_detour"
        if used_find: return "never_right_tool:after_find_actions"
        if "browser_search" in calls or "run_command" in calls: return "never_right_tool:web_or_command"
        return "never_right_tool:other"
    if hit and detour: return "right_tool_but_wm_detour"
    return fail["result"]

totals = collections.Counter()
by_cat = collections.defaultdict(collections.Counter)
for m in REPORTS:
    d = json.load(open(m))
    sk = collections.defaultdict(list)
    for s in d["safetyErrors"]:
        sk[s["request"]].append(s["kind"])
    c = collections.Counter()
    for f in d["failures"]:
        req = requests[f["id"]]
        k = cause(f, req, sk)
        c[k] += 1; totals[k] += 1
        by_cat[req["category"]][m] += 1
    print(f"\n{m}: pass {d['total']['pass']}/106")
    for k, n in c.most_common():
        print(f"  {n:3d}  {k}")
print(f"\nALL {len(REPORTS)} reports:")
for k, n in totals.most_common():
    print(f"  {n:3d}  {k}")
print(f"\nfailures per category (sum over the reports, of {len(REPORTS)} x requests in category):")
cats = collections.Counter(r["category"] for r in requests.values())
rows = sorted(by_cat.items(), key=lambda kv: -sum(kv[1].values()))
for cat, cnt in rows:
    print(f"  {cat:18s} {sum(cnt.values()):3d} / {len(REPORTS)*cats[cat]:3d}")
