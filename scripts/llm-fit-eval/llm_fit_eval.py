#!/usr/bin/env python3
"""LLM fit eval for what-da-weather via OpenRouter.

Faithful port of backend/crates/core/src/llm.rs prompt construction
(system prompt, build_prompt, parse_verdict) and rules.rs constraint
semantics. Sends the SAME payload shape the app sends today
(temperature 0, max_tokens 250, response_format json_object), but to
OpenRouter, across several models feasible under DESIGN.md D7
(CPU-only, ~2.5 GB GGUF budget). Stdlib only; threads for parallelism.
"""
import json
import os
import sys
import time
import urllib.request
import urllib.error
from concurrent.futures import ThreadPoolExecutor, as_completed

API_KEY = os.environ.get("OPENROUTER_API_KEY", "")
URL = "https://openrouter.ai/api/v1/chat/completions"

SYSTEM_PROMPT = (
    'You are a concise weather-activity advisor. The user names an activity, guidance on how '
    'to decide, the current weather, and condition checks ALREADY COMPUTED by the system. '
    'Trust the computed results exactly - never recompute or second-guess them. Apply the '
    'guidance to those results and decide. Respond with a single JSON object of the form '
    '{"reasoning": "one or two sentences citing the deciding conditions and ending with the '
    'conclusion", "recommended": true|false} and nothing else - reasoning FIRST, then '
    'recommended. The recommended value must state the conclusion your reasoning reached.'
)

# ---- activities.yaml, verbatim (soft conditions only; all eval cases pass the hard gate) ----
ACTIVITIES = {
    "matkot": {
        "name": "Matkot at the beach",
        "prompt": 'Matkot is a beach paddle-ball game played outdoors on the sand. Recommend it (recommended=true) ONLY if the "do NOT hold" list is (none). If any condition does not hold, recommended=false and the reasoning must cite it.',
        "conditions": [
            {"param": "temperature_c", "min": 22, "max": 31, "description": "Warm beach temperature (22-31 C)"},
            {"param": "wind_kmh", "max": 12, "description": "Calm enough for the very light ball (wind at most 12 km/h)"},
            {"param": "precipitation_mm", "max": 0, "description": "Completely dry"},
        ],
    },
    "nature": {
        "name": "Nature sightseeing",
        "prompt": 'Nature sightseeing means walking outdoor trails and viewpoints. Recommend it (recommended=true) ONLY if the "do NOT hold" list is (none). If any condition does not hold, recommended=false and the reasoning must cite it.',
        "conditions": [
            {"param": "temperature_c", "min": 15, "max": 30, "description": "Comfortable walking temperature (15-30 C)"},
            {"param": "wind_kmh", "max": 25, "description": "No strong wind"},
            {"param": "precipitation_mm", "max": 0.5, "description": "Essentially dry"},
            {"param": "visibility_km", "min": 10, "description": "Long views (at least 10 km visibility)"},
        ],
    },
    "gaming": {
        "name": "Gaming (indoors)",
        "prompt": 'Gaming happens indoors and is always physically possible. The computed conditions describe weather that argues for STAYING IN. Look at the "HOLD right now" list: if it is (none), recommended MUST be false - the weather is pleasant, better to go outside than game. If anything is in the "HOLD right now" list, recommended=true, citing it.',
        "conditions": [
            {"param": "is_day", "max": 0, "description": "It is nighttime - outdoor time is over for the day"},
            {"param": "temperature_c", "min": 32, "description": "Very hot outside"},
            {"param": "temperature_c", "max": 10, "description": "Cold outside"},
            {"param": "wind_kmh", "min": 30, "description": "Strong wind outside"},
            {"param": "precipitation_mm", "min": 0.1, "description": "Raining outside"},
        ],
    },
}

# ---- 9-case suite: all pass the hard gate, so the LLM decides every one ----
def W(t, w, p, v, day):
    return {"temperature_c": t, "wind_kmh": w, "precipitation_mm": p, "visibility_km": v, "is_day": day}

CASES = [
    {"id": "M1-perfect-beach",   "activity": "matkot", "weather": W(26, 8, 0, 20, True),  "expected": True},
    {"id": "M2-too-windy",       "activity": "matkot", "weather": W(26, 18, 0, 20, True), "expected": False},
    {"id": "M3-too-cool",        "activity": "matkot", "weather": W(18, 5, 0, 20, True),  "expected": False},
    {"id": "N1-perfect-hike",    "activity": "nature", "weather": W(22, 10, 0, 25, True), "expected": True},
    {"id": "N2-hazy",            "activity": "nature", "weather": W(20, 10, 0, 6, True),  "expected": False},
    {"id": "N3-drizzle",         "activity": "nature", "weather": W(24, 15, 1.2, 15, True), "expected": False},
    {"id": "G1-lovely-day",      "activity": "gaming", "weather": W(24, 10, 0, 20, True), "expected": False},
    {"id": "G2-heatwave",        "activity": "gaming", "weather": W(35, 5, 0, 20, True),  "expected": True},
    {"id": "G3-mild-night",      "activity": "gaming", "weather": W(22, 8, 0, 20, False), "expected": True},
]

# ---- rules.rs constraint_satisfied ----
def get_param(w, p):
    if p == "is_day":
        return 1.0 if w["is_day"] else 0.0
    return float(w[p])

def constraint_satisfied(c, w):
    v = get_param(w, c["param"])
    if "min" in c and v < c["min"]:
        return False
    if "max" in c and v > c["max"]:
        return False
    return True

# ---- llm.rs build_prompt, verbatim formatting ----
def build_prompt(activity_name, guidance, conditions, w):
    prompt = (
        f"Activity: {activity_name}\n"
        f"Guidance: {guidance}\n"
        f"Current weather at the location: temperature {w['temperature_c']:.1f} C, "
        f"wind {w['wind_kmh']:.1f} km/h, precipitation {w['precipitation_mm']:.1f} mm, "
        f"visibility {w['visibility_km']:.1f} km, "
        f"{'daytime' if w['is_day'] else 'nighttime'}.\n"
    )
    if conditions:
        met = [c for c in conditions if constraint_satisfied(c, w)]
        unmet = [c for c in conditions if not constraint_satisfied(c, w)]
        fmt = lambda v: "(none)" if not v else "; ".join(c["description"] for c in v)
        prompt += (
            "Condition checks, already computed by the system (trust them):\n"
            f"Conditions that HOLD right now: {fmt(met)}\n"
            f"Conditions that do NOT hold: {fmt(unmet)}\n"
        )
    prompt += (
        'Apply the guidance to the computed checks and decide. '
        'Answer with JSON only, reasoning first: {"reasoning": "one or two sentences citing '
        'the deciding conditions", "recommended": true or false}'
    )
    return prompt

# ---- llm.rs parse_verdict ----
def parse_verdict(text):
    candidates = [text.strip()]
    s, e = text.find("{"), text.rfind("}")
    if s != -1 and e > s:
        candidates.append(text[s:e + 1])
    for cand in candidates:
        try:
            v = json.loads(cand)
        except Exception:
            continue
        if isinstance(v, dict) and isinstance(v.get("recommended"), bool):
            r = v.get("reasoning")
            reasoning = r.strip() if isinstance(r, str) and r.strip() else "(no reasoning provided)"
            return {"recommended": v["recommended"], "reasoning": reasoning}
    return None

# ---- OpenRouter call, mirroring llm.rs request shape (2 attempts, like prod) ----
def call_model(model, messages, response_format=True, max_tokens=250, timeout=60):
    body = {
        "model": model,
        "temperature": 0.0,
        "max_tokens": max_tokens,
        "messages": messages,
    }
    if response_format:
        body["response_format"] = {"type": "json_object"}
    req = urllib.request.Request(
        URL,
        data=json.dumps(body).encode(),
        headers={"Authorization": f"Bearer {API_KEY}", "Content-Type": "application/json"},
    )
    t0 = time.time()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            data = json.loads(resp.read())
    except urllib.error.HTTPError as e:
        err = e.read().decode(errors="replace")[:300]
        # some providers reject response_format -> retry without, like a config fallback
        if response_format and e.code in (400, 404, 422):
            return call_model(model, messages, response_format=False, max_tokens=max_tokens, timeout=timeout)
        return {"error": f"HTTP {e.code}: {err}", "latency_ms": int((time.time() - t0) * 1000)}
    except Exception as e:
        return {"error": f"{type(e).__name__}: {e}", "latency_ms": int((time.time() - t0) * 1000)}
    lat = int((time.time() - t0) * 1000)
    try:
        content = data["choices"][0]["message"]["content"] or ""
    except Exception:
        return {"error": f"bad response shape: {json.dumps(data)[:300]}", "latency_ms": lat}
    if not content and isinstance(data["choices"][0]["message"].get("reasoning"), str):
        content = data["choices"][0]["message"]["reasoning"]
    return {"content": content, "latency_ms": lat, "used_response_format": response_format}


def run_one(model, case, trial):
    act = ACTIVITIES[case["activity"]]
    user = build_prompt(act["name"], act["prompt"], act["conditions"], case["weather"])
    messages = [{"role": "system", "content": SYSTEM_PROMPT}, {"role": "user", "content": user}]
    last = None
    for attempt in range(2):  # LLM_ATTEMPTS default 2, like prod
        r = call_model(model, messages)
        last = r
        if "error" in r:
            time.sleep(0.5)
            continue
        verdict = parse_verdict(r["content"])
        if verdict is None:
            last = {**r, "error": f"unparseable: {r['content'][:200]!r}"}
            time.sleep(0.5)
            continue
        return {
            "model": model, "case": case["id"], "trial": trial,
            "expected": case["expected"], "got": verdict["recommended"],
            "correct": verdict["recommended"] == case["expected"],
            "reasoning": verdict["reasoning"], "latency_ms": r["latency_ms"],
            "raw": r["content"],
        }
    return {
        "model": model, "case": case["id"], "trial": trial,
        "expected": case["expected"], "got": None, "correct": False,
        "reasoning": None, "error": last.get("error", "unknown"),
        "latency_ms": last.get("latency_ms"),
    }


def main():
    models = sys.argv[1].split(",") if len(sys.argv) > 1 else []
    trials = int(sys.argv[2]) if len(sys.argv) > 2 else 3
    out_path = sys.argv[3] if len(sys.argv) > 3 else "results.json"
    jobs = [(m, c, t) for m in models for c in CASES for t in range(trials)]
    results = []
    with ThreadPoolExecutor(max_workers=40) as ex:
        futs = {ex.submit(run_one, m, c, t): (m, c["id"], t) for (m, c, t) in jobs}
        done = 0
        for f in as_completed(futs):
            results.append(f.result())
            done += 1
            if done % 20 == 0:
                print(f"  {done}/{len(jobs)} calls done", file=sys.stderr)
    with open(out_path, "w") as fh:
        json.dump(results, fh, indent=1)
    # summary
    print(f"\n{'model':42s} {'score(maj)':>10s} {'strict':>7s} {'errors':>6s} {'p50 ms':>7s}")
    for m in models:
        rs = [r for r in results if r["model"] == m]
        maj = strict = 0
        for c in CASES:
            cr = [r for r in rs if r["case"] == c["id"]]
            n_correct = sum(1 for r in cr if r["correct"])
            if n_correct * 2 > len(cr):
                maj += 1
            if n_correct == len(cr):
                strict += 1
        errs = sum(1 for r in rs if r.get("error"))
        lats = sorted(r["latency_ms"] for r in rs if r.get("latency_ms"))
        p50 = lats[len(lats) // 2] if lats else -1
        print(f"{m:42s} {maj:>7d}/9 {strict:>5d}/9 {errs:>6d} {p50:>7d}")
    # per-case failure detail
    print("\nFailures (per model/case, first failing trial shown):")
    for m in models:
        for c in CASES:
            cr = [r for r in results if r["model"] == m and r["case"] == c["id"]]
            bad = [r for r in cr if not r["correct"]]
            if bad:
                b = bad[0]
                why = b.get("error") or f"got={b['got']} want={b['expected']} :: {b['reasoning']}"
                print(f"  {m} | {c['id']} | {len(bad)}/{len(cr)} wrong | {why[:220]}")

if __name__ == "__main__":
    main()
