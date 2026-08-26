# Post-task enhancement — v1.1 Activity map

> Planned enhancement, designed after the assignment was submitted. DESIGN.md remains the
> ledger for the delivered system; this file is the agreed design for the next increment.
> Ships as `feat:` → **1.1.0** — additive, nothing breaks.

**Status:** design agreed, pre-implementation
**Date:** 2026-08-27

---

## 1. What it is

A third UI view (alongside Main and Debug): a map of Israel partitioned into **Voronoi
cells around 12 main cities**, colored by the selected activity's verdict at the *nearest
measured city*. The claim every pixel makes is honest and one sentence long: "the closest
measurement says X." No interpolation, no fabricated data between cities.

## 2. How the design got here (rejected shapes)

- **District polygons** — rejected: one verdict per district lies (the South spans
  Beer Sheva to Eilat); districts are administratively loose in Israel anyway.
- **Municipal boundary polygons** — rejected: honest but tiny at country zoom, and drags
  in an OSM boundary-extraction pipeline plus a config↔GeoJSON id contract.
- **Verdict heatmap (interpolated)** — rejected: a verdict is a decision, not a field
  quantity. Interpolating booleans paints "50% recommended" on places nothing evaluated;
  threshold effects (gates) make averaged weather and averaged verdicts disagree; the
  three-state legend (gray "no verdict") cannot be interpolated at all.
- **Voronoi cells** — adopted: the honest version of the heatmap instinct. Regions without
  averaging; "colored by nearest measurement" is the whole defense.

## 3. Decisions

| Decision | Choice | Why |
|---|---|---|
| Map semantics | Per-activity view, activity selector | Matches the existing (city × activity) model; simple legend |
| Geometry | Voronoi cells from city points, clipped to a country outline | No boundary extraction; cells recompute from points |
| Cities | Tel Aviv, Jerusalem, Haifa, Beer Sheva, Eilat, Netanya, Ashdod, Ashkelon, Rishon LeZion, Petah Tikva, Tiberias, Nazareth | Coastal/inland/desert/north spread → cells differ meaningfully |
| Tick interval | `SCHEDULER_INTERVAL_MINUTES` default 1 → 2 | Worst case 12×3 = 36 LLM calls ≈ 54s serial at ~1.5s/verdict — fits a 2-minute tick with headroom |
| LLM contention | Scheduler **spreads** calls across the tick window (one every `interval/N`) | User calls interleave with ≤ ~one verdict of queueing; no priority queue, no second LLM; also flattens the per-tick latency spike in Prometheus |
| Country outline | OSM Israel relation, as-is, simplified and committed as static GeoJSON | Single committed file; ODbL attribution shown in the UI |
| Voronoi computation | Runtime, in the frontend (`d3-delaunay`) from the lat/lons already in `/api/status` | 12 points is microseconds; city list changes in config propagate to the map automatically — no generated artifact to drift |
| Cell colors | Three states: recommended / not recommended / gray "no verdict" (fallback + no data) | Preserves the D6/D7 distinction between "advisor said no" and "advisor unavailable" |
| Interaction | Hover tooltip (city, verdict, one-line reasoning); click jumps to the classic view prefilled with that city+activity; SSE became-recommended **flashes the cell** | The map is a navigator, not a second reasoning UI; the flash makes "dashboard that alerts" visible live |
| Notifications at 12 cities | Client-side city filter (localStorage), default notify-all | A sunrise can flip a dozen pairs in one tick; filtering is client state, no backend digest infra |
| UI placement | Third view via the existing view toggle | The delivered dashboard stays the front door |
| Versioning | `feat:` commit → 1.1.0 | Additive; version numbers keep meaning compatibility, not marketing |

## 4. Backend surface touched

Deliberately minimal:

- `config/activities.yaml`: scheduler city list grows to the 12.
- Scheduler: interval default 2, plus call-spreading across the tick window.
- Nothing else: no new endpoints, no event/ES schema change, Grafana untouched.
  The map is a projection of state the system already produces.

## 5. Revisit triggers

- **Finer granularity than 12 cities** (denser map, all ~80 municipalities): revisit the
  tick budget — llama.cpp parallel slots or a longer interval.
- **Measured user-latency contention** with the spread scheduler: revisit true
  prioritization (user calls preempt scheduler calls).
- **Notification filter proves insufficient**: revisit backend digest/coalescing.
