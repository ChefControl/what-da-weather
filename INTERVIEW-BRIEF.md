# what-da-weather — Interview Brief

Prepared from the repository at commit 8282105.

Stack: Rust 1.97 / axum 0.8 · React 19 / Vite 8 · RabbitMQ 4.1 → Logstash 8.17 → Elasticsearch 8.17 · llama.cpp with Ministral-3-3B Q4_K_M · 33 backend tests, 4 frontend tests.

---

## 1. The 60-second pitch

Say this without notes. Every sentence opens a door you are ready to walk through.

> **what-da-weather** is a weather-driven activity recommender. You pick a city and an activity — matkot on the beach, nature sightseeing, or gaming indoors — and the service fetches current conditions from Open-Meteo, runs a hard-constraint gate in code, and then asks a locally hosted 3-billion-parameter LLM (Ministral, running on llama.cpp, CPU only) for a judgment over facts the code has already computed.
>
> Every evaluation is published to RabbitMQ with publisher confirms, consumed by Logstash into daily Elasticsearch indices, and Grafana reads both Elasticsearch for business data and Prometheus for infrastructure and LLM metrics. A scheduler re-evaluates three cities × three activities every minute, and an in-process notifier pushes a Server-Sent Event only on the edge where an activity *becomes* recommended.
>
> It is a Rust workspace with two binaries sharing a core library, a React and TypeScript front end served by axum from the same image, one `docker compose up` with no API keys, and GitHub Actions CI that lints, tests, builds the image, tags a semver release, and pushes to GHCR on every merge to main. The design document records every choice, its alternative, and the trigger that would make me revisit it.

### Three things to land in the first five minutes

- **The LLM never does arithmetic.** A 3B model measurably misses comparisons like `18 > 12`, so code evaluates every condition and the model judges stated facts. Its verdict is final. That split is the intellectual core of the project.
- **No data loss is three layers, one per failure class.** Producer→broker: confirms, retry, persistent messages on a durable queue. Downstream outage: Logstash persistent queue, then backpressure into RabbitMQ. Poison messages: dead-letter queue re-indexed into its own monitored index.
- **Every decision has a rejected alternative and a revisit trigger.** RabbitMQ over Kafka until independent consumer types multiply. SSE over Web Push until tab-closed delivery matters. Compose over Kubernetes because the reviewer has to run it.

---

## 2. Draw it from memory

Interviewers will hand you a marker. Practice these three drawings until each takes under two minutes.

### 2.1 The data path

Solid arrows are the pipeline. Dotted arrows are the two external calls an evaluation depends on. Say out loud that the app never writes to the database directly, because the spec said the write path must flow through the queue.

```
Browser UI  --REST-->  weather-api  --publish, confirm-->  RabbitMQ
Scheduler   --HTTP-->  weather-api                            |
                            |                                 | consume, ack
                            | ....weather....> Open-Meteo      v
                            | ....verdict....> llama.cpp    Logstash
                            |                                 | bulk index
                            |                                 v
Browser UI  <---SSE---------+                           Elasticsearch
                                                              | datasource
                                                              v
                                                           Grafana
```

### 2.2 One evaluation, step by step

Both the UI and the scheduler hit the same `POST /api/evaluate`, so both paths run identical code. The debug endpoint shares the same `derive_verdict` function so the debug view can never drift from production.

1. Client (UI or scheduler) → `POST /api/evaluate {city, activity}`
2. weather-api → Open-Meteo: geocode, then current weather (3 attempts each, backoff)
3. Open-Meteo → snapshot: temperature, wind, rain, visibility, is_day
4. **If a required constraint fails**: not recommended, `source: rules-gate`, LLM never consulted
5. **Else**: code computes the soft conditions, then calls llama.cpp with HOLD / do-NOT-hold lists (2 attempts, 60 s each)
   - Valid JSON → `{reasoning, recommended}`, `source: llm`
   - Down, timeout, or garbage → not recommended, `source: fallback`
6. weather-api → RabbitMQ: publish persistent, wait for confirm (10 s cap)
7. weather-api → Notifier: `observe(event)` — SSE fires only on the not-recommended → recommended edge
8. weather-api → client: `{event, published}`

### 2.3 The monitoring plane

Kept separate from the data path on purpose. Prometheus scrapes five targets: weather-api, scheduler, RabbitMQ's built-in prometheus plugin on port 15692, the elasticsearch-exporter, and llama-server with `--metrics`. Grafana reads Prometheus for infra and LLM panels and Elasticsearch for business panels, with two provisioned dashboards and five ops alert rules, all committed as code.

### 2.4 The repository in one breath

| Path | What it holds |
|---|---|
| `backend/crates/core` | Shared lib: config (serde, `deny_unknown_fields`), rules gate, weather provider trait plus Open-Meteo, LLM client and prompt builder, RabbitMQ publisher, retry helper, Prometheus metrics. |
| `backend/crates/weather-api` | axum binary: evaluate, debug evaluate, status, activities, SSE events, metrics, healthz, static SPA. Owns the notifier and the ES status merge. |
| `backend/crates/scheduler` | Tick binary: every N minutes walks cities × activities and calls the API. Exposes its own metrics and healthz. |
| `frontend/` | React 19 + TypeScript on Vite. Evaluate form, verdict card, toasts, SSE hook with lag handling, debug panel with sliders through the real gate→LLM path. |
| `logstash/`, `monitoring/` | Main and DLQ pipelines, persistent queue config, Prometheus scrape config, Grafana datasources, dashboards, alert rules — provisioned from files. |
| `scripts/` | Failure-injection script that stops Logstash mid-stream and asserts exact event ids arrive; OpenRouter model-fit eval harness with raw results. |

---

## 3. Decision ledger

For each choice: what, why, what you rejected, and what would make you change it. Answer in that order every time.

| Choice | Why | Rejected | Revisit when |
|---|---|---|---|
| **Rust with axum** | Compiler kills whole bug classes before review, which makes LLM-assisted iteration safe and regression testing cheap. axum is tokio/tower native, so metrics and SSE are frictionless. | actix-web (own runtime ecosystem); a dynamic language (more surface for tests to cover). | Not a revisit candidate for this scope. |
| **Two binaries, one lib** | Separate failure domains for API and scheduler, zero code duplication. Same image, different `command`. | A tokio task inside the API — a crashed scheduler would take the API down, and vice versa. | Never; cost is near zero. |
| **docker compose only** | The spec said simple and uniform to run. One command, no keys. | Kubernetes manifests: shows ops depth, raises the reviewer's cost to run. | Multi-node or autoscaling needs. Images are already 12-factor and stateless. |
| **Open-Meteo behind a trait** | Keyless, so the project runs with zero setup. The spec allowed alternatives. | OpenWeather — needs registration before anything works. | Provider quality or a paid tier. Slot a new impl behind `WeatherProvider`. |
| **RabbitMQ, not Kafka** | Trickle workload, one consumer. Per-message ack, retry, TTL, dead-lettering are native. ~1 GB less memory in a 7.65 GB Docker VM. | Kafka: replay and many consumer groups unused, DLQ and retry are hand-rolled, a poison record head-of-line blocks its partition. | Independent subscriber types multiply, or any consumer needs history bootstrap. Then one-log/many-cursors wins. |
| **Logstash as the consumer** | A broker is passive; something must move messages to the DB. Configure a battle-tested consumer instead of writing one. It handles acks, reconnects, bulk API, retries, backpressure, DLQ. | A hand-written Rust consumer: more code to test for the same guarantees. | Transformations grow beyond what filters express cleanly. |
| **Elasticsearch as the DB** | Write-once JSON documents over time, aggregated by time bucket. No joins, no updates. First-class Logstash output and Grafana datasource. `event_id` doubles as document id for idempotency. | Postgres: schema migrations with nothing relational to gain. | Relational questions appear (users, subscriptions, joins). |
| **Grafana, no Kibana** | One tool reads both Prometheus and Elasticsearch. Free alerting. Everything provisioned as code. | Kibana: only fronts ES, cannot read Prometheus, free tier lacks webhook alert connectors. | Deep log exploration or ES-specific tooling becomes the daily driver. |
| **In-process SSE notifier** | A realtime nudge is worthless when late, so durability buys nothing. Keep last verdict per pair in memory, push on the became-recommended edge only. ES stays the durable record. | Grafana alert webhook (ops tool becomes a product runtime dependency); a dedicated notify queue (over-engineering); Web Push (relays through Google/Mozilla, VAPID keys, service worker). | Mobile app, email, or alert history. That is the trigger for a real notification pipeline. |
| **llama.cpp, model by env var** | Docker on macOS has no GPU, so CPU inference bounds model size to ~2.5 GB of GGUF next to ELK. Model swap is config, not a rebuild. Cache keyed by file name so a URL swap never reuses stale weights. | Ollama (another daemon and API shape for the same runtime); larger models (do not fit the budget). | GPU availability, or a workload that needs tool use. |
| **Fallback is "not recommended"** | The advisor owns all preference nuance, including gaming's inverse semantics where a hardcoded `true` would invert the meaning. Without it the honest answer is no recommendation. Fallbacks never notify and never move the notifier baseline. | Gate-only "recommended" (first version); a rule-based verdict (second version, reverted). | Never guess. This one is a principle, not a trade-off. |
| **Code computes, LLM judges** | Small models cannot run comparison chains. Code partitions conditions into HOLD and do-NOT-hold lists and states "N of M hold". Reasoning precedes the boolean in the JSON so the verdict is conditioned on the analysis. | Free-text numbered rules (model did arithmetic, failed); a code-side consistency guard that overrides the model (built, then reverted). | A model strong enough that raw numbers suffice — which you would prove with the debug page and the eval harness first. |

---

## 4. Stories to tell

Behavioral questions want a decision, a reversal, a measurement. These are all in the git history.

### 4.1 The consistency guard you built and then reverted
*commits 2fd8f62 → c50ea08 → 4d4c42a*

**Situation.** The 1.5B model contradicted its own verdict on the gaming activity, where good weather means "not recommended."

**Action.** First cut: code computes each condition, derives the expected verdict from an all/any policy, and overrides a contradicting LLM verdict, flagged `source: corrected` and counted as an error. 9/9 on the probe matrix.

**Reversal.** I reverted it the same day. If code can decide, the LLM is decorative and the metric just measures how often I ignore it.

**Resolution.** Keep the fact presentation, drop the guard, and fix alignment instead: reasoning before the boolean in the output JSON, and inverse guidance keyed off the computed count instead of keywords that collide with the do-NOT-hold list. Still 9/9, every verdict `source: llm`.

**Lesson.** When you find yourself correcting a component, ask whether it should be there at all.

### 4.2 The fallback that flipped polarity twice
*commits 0fb5529 → 2fd8f62 → 04c26d1*

Version one: if the LLM is down and the hard gate passes, say "recommended: all hard constraints pass." Version two: serve a deterministic rule-based verdict. Final: say "not recommended, advisor unavailable" and never notify on it. The gaming activity forced the decision — with inverse semantics, a default of `true` is not conservative, it is wrong. The rule became "never guess," and the notifier ignores fallback verdicts so an outage can neither fire nor set up a became-recommended edge.

Note that the README still describes the second version in two places. Say that before they find it.

### 4.3 Discovering the model cannot compare numbers
*DESIGN.md §4, probed on the debug page at temperature 0*

I built a debug view with a slider per weather parameter that runs the identical gate→LLM path and returns the exact prompt. Sliding wind to 18 km/h against a "wind at most 12" rule, the 3B model kept passing it. So comparisons moved into code, and the prompt carries the results as grouped lists with "(none)" for empty — because per-line MET/NOT MET annotations made the model misparse negations. The debug page turned prompt engineering from vibes into a measurement.

### 4.4 Choosing the model with an OpenRouter harness
*scripts/llm-fit-eval, commit 9f7ace0*

Hardware envelope first: `llmfit` against the Docker VM shortlisted 1–3B models with GGUF quants. Then a stdlib-only Python port of the production request path — same system prompt and prompt builder at the time, same parser — run in parallel against every in-budget candidate on OpenRouter. Nine cases, three trials each, majority vote, then ten-trial stability on the finalists. Ministral-3-3B was the only in-budget model to solve the gaming "lovely day means stay out" inversion. Caveat I kept in view: OpenRouter serves higher-precision weights, so cloud scores are an upper bound, and I re-validated the Q4 quant locally before adopting it.

### 4.5 The 6 km visibility call
*commits 2c2ffee, 45b6ac4, dd7772c*

Ministral recommended sightseeing at 6 km visibility against a 10 km guidance because the other three conditions held, and it named the trade-off. I judged that a feature: sound, cited judgment on marginal cases is what an LLM is for. So the guidance became a norm — "should normally" — with explicit room for judgment. That softening let matkot at 19 °C slip through on a fabricated justification, and the fix was in the condition description itself ("outside this the beach session is unpleasant, not marginal"), because the description is the text the model weighs. Then, once the system prompt forbade citing rules or computed summaries because the reasoning is user-facing, the model declined the 6 km case on its own: "the views are the whole point." Plain-language framing made verdicts more faithful, not less.

### 4.6 A 15-finding review round
*commit 04c26d1*

The publisher cloned the channel out of the mutex so confirms stop serializing concurrent requests. A publish timeout became "unconfirmed" rather than "dropped" because the frame may already be on the queue and idempotent indexing makes a re-send safe. The notifier started ignoring out-of-order stale events. SSE lag is surfaced as a `lagged` event instead of swallowed. Alert thresholds were recalibrated to the system's actual scale. All published ports moved to loopback. The scheduler's client timeout was raised above the API's additive worst case.

Tell this as: reviews find real bugs when you ask adversarial questions of your own design.

### 4.7 Small things that show ops maturity

The Elasticsearch healthcheck waits for **yellow, not green**, because a single node can never allocate replicas and green would keep the dependency chain unhealthy forever. The LLM is deliberately **not a startup dependency** of the API, so verdicts degrade while 1.1 GB of weights download. Logstash assigns a **UUID and a tag** to any message missing `event_id`, so a stray message cannot clobber one shared document id. The DLQ pipeline has **its own DLQ disabled** so it can never dead-letter its own output.

---

## 5. Question drills

Read the question, answer aloud, then check yourself against the answer.

### Reliability and the pipeline

**Q. Walk me through exactly what "no data loss" means here and where the guarantee ends.**

Three layers, one per failure class. Producer→broker: publisher confirms, four attempts with exponential backoff from 400 ms, reconnect between attempts, persistent messages on a durable queue on a named volume, and a 10-second overall cap so a broker outage cannot hang the API. Downstream outage: Logstash acks to RabbitMQ only after the event is in its on-disk persistent queue, its ES output retries retryable errors indefinitely, and when the 512 MB queue fills it stops consuming so backlog accumulates in RabbitMQ. Poison messages: the dead-letter queue catches ES rejections and a second pipeline re-indexes them into a monitored index.

Where it ends: a process crash between computing the verdict and publishing loses that one event, since there is no outbox. That is acceptable because events are re-derivable observations and the scheduler re-evaluates in a minute. Browser notifications are realtime-only by design. Single-node disk failure is out of scope.

**Q. How do you get exactly-once semantics out of an at-least-once pipeline?**

I do not. Delivery is at-least-once everywhere: a publish timeout may leave a frame on the queue, Logstash may replay from its persistent queue after a crash, RabbitMQ may redeliver an unacked message. Storage is exactly-once because the producer's UUID is the Elasticsearch document id, so a redelivery overwrites itself. The daily index name comes from the producer's timestamp, not ingest time, so a redelivery always targets the same index.

**Q. Elasticsearch goes down for an hour. Narrate the system minute by minute.**

Logstash's ES output starts getting connection errors and retries. Events already consumed sit in the persistent queue on disk. The queue keeps filling from RabbitMQ until 512 MB, at which point the input stops consuming and messages-ready in RabbitMQ climb. The Grafana "queue backlog" rule fires after the backlog exceeds 10 for 10 minutes, and "service down" fires for the exporter target. The API keeps working: publishes still confirm because RabbitMQ is healthy, so the UI shows verdicts normally and `published: true`. `/api/status` logs a warning and serves the in-memory snapshot with `elasticsearch: false`. When ES returns, Logstash drains the persistent queue, then resumes consuming and drains RabbitMQ. Zero loss — and the failure-injection script proves the Logstash-down variant end to end.

**Q. What happens if RabbitMQ itself is down?**

The publisher's four attempts fail, each dropping the channel so the next attempt reconnects from scratch, all inside the 10-second cap. The handler logs an error, increments `publish_dropped_total`, still notifies, and returns 200 with `published: false`. The UI shows a "not persisted" badge on the verdict card. The scheduler treats it as success because the evaluation itself ran. This is the one place data is knowingly dropped, and it is visible in metrics and in the response.

**Q. Why does a publish timeout count as unconfirmed, not dropped?**

The timeout cancels the future at an arbitrary await point. The frame may already be on the durable queue with only the confirm outstanding. Calling that dropped would overstate loss. Because indexing is idempotent by event id, a delivered-but-unconfirmed event or a re-send is still exactly-once in storage.

**Q. Why publish before responding to the caller?**

So the caller learns whether the event reached the durable pipeline. The response carries `published`, the failure-injection script asserts on it, and the UI surfaces it. The cost is latency, bounded by the 10-second cap on top of the verdict path.

**Q. Why RabbitMQ over Kafka? Push me on it.**

One producer, one consumer, a handful of small messages per minute. What I need is per-message ack, retry, dead-lettering, and TTL — native in RabbitMQ, hand-rolled patterns in Kafka, where a poison record also head-of-line blocks its partition. Kafka's strengths, replay and many consumer groups with history bootstrap, go unused because the full event is the ES document, so the replayable history already lives in the database. Kafka also costs about a gigabyte of a 7.65 GB VM. The trigger to migrate is independent subscriber types multiplying.

If they push on "but Kafka has DLQ via Connect": that is a connector-level pattern, and I would be adding Connect to get what the broker gives me now.

**Q. Why not write your own consumer in Rust instead of Logstash?**

The consumer must handle subscription, acks after durable write, reconnects, bulk indexing, retries on 429 and 503, backpressure, and dead-lettering. Logstash does all of that in configuration I can read in one screen, and the assignment named it. Writing it would be more code to test for the same guarantees. Logstash costs RAM, and it earns it.

**Q. What is idempotent about the pipeline and what is not?**

Indexing is idempotent by document id. The notifier is not idempotent in the naive sense but is order-safe: it ignores an event older than the stored one for its pair, so out-of-order completion cannot fire an edge from stale data. Metrics counters are not idempotent — a retried publish attempt counts twice in `publish_failures_total`, which is what you want for a failure counter.

### The LLM

**Q. Why does code compute the conditions instead of giving the model raw numbers?**

Measured, not assumed: on the debug page a 3B model missed `18 > 12` against a wind limit. So code partitions the activity's soft conditions into "conditions that HOLD" and "conditions that do NOT hold," writes "(none)" for an empty list, and states "N of M hold." The model's job becomes judgment over facts, and the system prompt says to trust the computed results and never recompute. Grouped lists beat per-line annotations because small models misparse negations.

**Q. Why does reasoning come before the boolean in the JSON?**

Autoregressive generation: whatever comes first conditions what follows. With the boolean first, the model's nice-weather prior fixed the verdict before any analysis happened, and the reasoning then rationalized it. With reasoning first, the boolean is generated after and conditioned on the model's own analysis. Same idea as chain-of-thought, at the cost of a few dozen tokens. The prompt also asks the reasoning to end with an explicit "Conclusion: recommended." sentence so the boolean has an anchor.

**Q. How do you know the model is not just agreeing with the prompt?**

The probe matrix includes cases where the correct answer runs against the prior: gaming on a lovely day must be "not recommended," and gaming on a mild night must be "recommended." The 1B and 1.5B models failed exactly those. I also reviewed the reasoning prose for all nine cases, not only the boolean, and the judge pass in the harness scored coherence at 85 of 90 responses.

**Q. Why temperature 0, and what does that buy you?**

Determinism for probing. At temperature 0 the same prompt gives the same verdict, so a prompt change is a measurable diff on the nine cases. It also makes the debug page reproducible. The residual flakiness in the harness's 10-trial runs comes from OpenRouter's serving, not from sampling.

**Q. What does the fallback do when the LLM is down, and why that polarity?**

Two attempts with a 60-second timeout each and a 500 ms pause, then "not recommended, advisor unavailable," flagged `source: fallback`, counted in `llm_fallbacks_total`. The advisor owns preference nuance, including gaming's inverse semantics where an assumed `true` inverts the meaning, so the honest degraded answer is no recommendation. Fallbacks never notify and never move the notifier's baseline, so an outage cannot produce a became-recommended edge when it ends.

**Q. How would you swap the model, and what would you check?**

Change `LLM_MODEL_URL` and `LLM_MODEL_FILE` in `.env` and restart. The file name keys the cache so a new URL never reuses stale weights. Then run the nine-case matrix on the debug page at temperature 0, and if the candidate is hosted, run the OpenRouter harness first to shortlist. Check llama.cpp version support for the architecture, and confirm the context size fits the prompt with headroom.

**Q. How defensive is the JSON parsing?**

The request asks for `response_format: json_object`, and the parser still tries the trimmed text, then the substring from the first brace to the last, so prose or a markdown fence around the object still parses. `recommended` must be a real boolean — "yes" is rejected. Missing reasoning gets a placeholder. Anything else is `Unparseable`, which counts as an error kind, retries once, and then falls back.

**Q. Why is the reasoning forbidden from mentioning rules or lists?**

The reasoning is shown to an end user who never sees the prompt, so "rule 2 does not hold" tells them nothing. The system prompt requires plain weather terms with real numbers. Side effect: without a "3 of 4 conditions hold" crutch to cite as fake endorsement, the model judged marginal cases on the weather itself and became more faithful to the guidance.

### Rust and the code

**Q. Why is `WeatherProvider` a trait object but `LlmClient` a concrete struct?**

The provider was designed to be swappable — OpenWeather behind the same trait — so it is `Box<dyn WeatherProvider>` in app state. The LLM client speaks one OpenAI-compatible endpoint whatever the model, so swapping is a URL, not a type. The honest downside: without a trait I cannot fake the LLM in a handler test, and there are no handler-level tests today. Making it a trait is the first refactor I would do to test `derive_verdict`.

**Q. You use both `std::sync::Mutex` and `tokio::sync::Mutex`. Why?**

The notifier holds a std mutex only for a map lookup and insert, never across an await, so the cheaper std lock is correct and there is no risk of blocking the runtime. The publisher holds its lock while awaiting `open_channel`, so it needs the async mutex. After the review round it clones the channel out and releases the lock before the network publish, because lapin's channel is an Arc-backed handle safe for concurrent use.

**Q. Explain the retry helper's signature.**

`retry_if` takes an attempt count, a base delay, a predicate on the error, and an `FnMut` that returns a future. Exponential backoff with `saturating_pow`. It only retries while the predicate says the error is transient, so a 4xx from the weather provider fails fast instead of hammering a rate-limited API, while 5xx, timeouts, and mid-body decode aborts retry. The LLM client and the publisher have their own simpler loops because their retry semantics differ — reconnect between attempts for the publisher, fixed pause for the LLM.

**Q. How does the SSE endpoint work and what happens to a slow client?**

A tokio broadcast channel with capacity 64 in the notifier, wrapped as a `BroadcastStream` per connection, with axum's keep-alive. A guard struct increments an `sse_clients` gauge on connect and decrements in `Drop` when the stream ends. If a client falls behind by more than 64 messages, the stream yields `Lagged(n)`, which I send as a `lagged` event with the count, the server logs a warning, and the UI shows a banner saying alerts may have been missed. Loss is surfaced, never silent.

**Q. What is the error handling strategy?**

`thiserror` enums in the library where callers match on variants — `WeatherError` and `LlmError`, each with a classification method for retry or metrics. `anyhow` in binaries and at the config boundary where the only consumer is a log line. The API maps errors to an `ApiError` enum with an `IntoResponse` impl: unknown activity and unknown city are 404, provider failure is 502, all JSON bodies. Unknown `/api/*` routes return JSON 404 rather than falling through to the SPA's index.html.

**Q. How is configuration validated?**

YAML deserialized into strict types with `deny_unknown_fields` everywhere, so a typo fails startup instead of silently doing nothing. A `validate` pass enforces non-empty activities and cities, interval at least one minute, non-empty prompts, every constraint has min or max, min not above max. A unit test loads the shipped config so drift between schema and file fails CI. The scheduler interval can be overridden by an env var without editing the mounted file.

**Q. Why does the scheduler call the API over HTTP instead of using the core library directly?**

So both paths run identical code including publish, metrics, and notification, and the scheduler stays a thin tick loop with no broker or LLM configuration. It also keeps failure domains separate. The cost is one localhost HTTP hop per evaluation, negligible next to CPU inference.

**Q. A tick takes longer than the interval. What happens?**

The pairs run serially, and nine LLM calls on CPU can exceed 60 seconds. The interval uses `MissedTickBehavior::Delay`, so ticks never overlap and never burst to catch up — they run back to back. The client timeout is 210 seconds because the API's additive worst case is about 193: weather retries ~62, LLM 2 × 60 plus the pause, publish 10. A failed request is logged and counted, and the next tick retries the pair.

### Observability and operations

**Q. What are your dedicated LLM metrics and what do they tell an operator?**

`llm_requests_total{status}` for ok versus error, `llm_errors_total{kind}` split into timeout, connect, http, and unparseable, a `llm_latency_seconds` histogram with buckets from 100 ms to 60 s, and `llm_fallbacks_total`. The kinds matter: connect means the container is down, timeout means it is alive but too slow, unparseable means a prompt or model regression. Plus llama-server's own metrics via `--metrics`.

**Q. Walk me through the alert rules and why the thresholds are what they are.**

- **Service down**: `up == 0` for 2 minutes.
- **Queue backlog**: ready messages above 10 for 10 minutes. Calibrated to scale — at nine events per tick a dead Logstash crosses it quickly, whereas 100 would take hours.
- **LLM error rate**: above 0.005/s over 10 minutes (about three errors), because a total outage only produces about 0.03/s here so a textbook 0.05 could never fire, while a lone transient retry stays under.
- **DLQ non-empty**: any document in `weather-recs-dlq` for 1 minute. We control the producer, so an empty DLQ is itself a signal.
- **Staleness**: fewer than one evaluation in 30 minutes.

Be ready for the follow-up: no contact point is provisioned, so these fire in Grafana's alert list only. Adding a Slack or webhook contact point is a provisioning file.

**Q. Why are user notifications not routed through Grafana alerting?**

Monitoring stacks notify operators. User-facing notifications are a product feature owned by the application. Routing them through Grafana makes an ops tool a runtime dependency of the product's user flow, which inverts industry practice. It was the first design and I rejected it after review.

**Q. How does `/api/status` stay correct across restarts and indexing lag?**

It merges two sources. Elasticsearch gives the latest document per (city, activity) through a composite aggregation with a top-hits sub-aggregation, covering state from before the last API restart. The notifier's in-memory snapshot covers events Logstash has not indexed yet and full ES outages. The merge keeps the newer timestamp per pair, and the response says whether ES answered.

**Q. Describe the CI/CD pipeline.**

Three jobs. Backend: `cargo fmt --check`, `clippy -D warnings` on all targets, `cargo test`, with the rust-cache action. Frontend: `npm ci`, vitest, then build which includes `tsc --noEmit`. Image: needs both, builds the multi-stage Dockerfile with GHA layer cache, and on main only computes the next semver tag from conventional commits, pushes `latest`, sha, and `vX.Y.Z` tags to GHCR, then creates a GitHub release with generated notes. cargo-chef cooks dependencies in their own layer so only manifest changes invalidate it. Dependabot watches cargo, npm, actions, and docker weekly.

**Q. Why is the frontend served by axum rather than nginx?**

Single origin: no CORS, trivial SSE, one container fewer. The Dockerfile builds the Vite bundle in a node stage and copies `dist` into the runtime image, which has no Node. axum serves it with `ServeDir` and an index.html fallback for client routes, with an explicit JSON 404 for unknown API paths so the fallback never masks an API error.

**Q. What is your test strategy and what is missing?**

33 backend unit tests: gate semantics including inclusive bounds and daylight, config validation including the shipped file, prompt construction, verdict parsing across clean/fenced/missing/garbage inputs, retry classification, weather parsing against a wiremock server including retry-on-503 and no-retry-on-not-found, notifier edges including fallback and stale events, and the ES merge. Four vitest tests on formatting. A scripted failure-injection test against a live stack asserts exact event ids survive a Logstash outage.

Missing, and I would say so: handler tests, an LLM fake, and the failure-injection test in CI. The last one needs a compose stack in Actions and a model download, so I left it as a documented script.

---

## 6. Failure matrix

"Kill any component" is a favorite. Have the row ready.

| Component down | What the user sees | What the system does | Who notices |
|---|---|---|---|
| **Open-Meteo** | 502 "weather provider unavailable" after retries, or 404 for an unknown city. | 3 attempts per stage with backoff, 4xx fails fast. Nothing published because there is no snapshot to evaluate. | Scheduler error counter; staleness alert after 30 min. |
| **llama.cpp** | Verdict "not recommended, advisor unavailable" with a fallback badge, after up to two 60 s attempts. | Event still published with `source: fallback`. Notifier ignores it. | LLM error rate alert, fallback counter, service down alert. |
| **RabbitMQ** | Verdict shown normally plus a "not persisted" badge. | 4 attempts inside 10 s, then `publish_dropped_total`. Notifier still runs. The one knowing loss. | Service down alert, dropped counter; Logstash input reconnects on recovery. |
| **Logstash** | Nothing. Verdicts and notifications unaffected. | Backlog accumulates in RabbitMQ on disk. Drains on restart, idempotent ids. | Queue backlog alert after 10 min. Proven by the injection script. |
| **Elasticsearch** | Nothing on evaluate. Status endpoint serves memory only and says `elasticsearch: false`. | Logstash persistent queue absorbs up to 512 MB, then backpressure into RabbitMQ. | Service down and backlog alerts; Grafana business panels go blank. |
| **weather-api** | UI cannot load. EventSource reconnects automatically. | Restarts under `unless-stopped`. Notifier re-baselines silently — first observation of each pair never notifies. Status rebuilds from ES. | Service down alert, scheduler error counter. |
| **Scheduler** | No background re-evaluation; on-demand still works. | Restarts. First tick fires immediately on start. | Service down alert, staleness alert. |
| **Prometheus / Grafana** | Nothing. The product does not depend on the monitoring plane. | Restart. Dashboards and rules re-provision from files. | An operator opening Grafana. |

---

## 7. Where they can hurt you

Every one of these is real. Name the ones that matter before the interviewer does, with the fix in the same breath.

### Real design limits

- **[HIGH] No outbox.** A crash between verdict and publish loses that event. Defense: events are re-derivable observations, the scheduler re-evaluates within a minute, and the client never got a success. A transactional outbox would be the fix if events ever carried user actions.
- **[HIGH] No authentication anywhere.** Evaluate, debug, status, metrics, and SSE are open. Elasticsearch security is off, Grafana is admin/admin by default, the debug endpoint is always on. Mitigation: every published port binds to 127.0.0.1, and this is a single-host demo stack. Say what production would need — a reverse proxy with auth, ES security on, the debug route behind a flag.
- **[MED] The `trigger` field is client-controlled.** Any caller can label its event "scheduler." Harmless for metrics in a demo, wrong in production. Fix: derive it from the caller's identity or a shared secret header.
- **[MED] Serial scheduler and a single inference server.** Nine evaluations per tick contend with user requests on the same CPU-bound llama-server. Worst-case user latency is around two minutes, and the frontend fetch has no timeout or abort. Fix: bounded concurrency in the scheduler, a shorter LLM timeout for user-triggered requests, and llama-server slots sized to the CPU.
- **[MED] No Elasticsearch index template or lifecycle policy.** Mappings come from dynamic defaults, daily indices grow forever, replicas default to one on a single node so the cluster is yellow by design. Fix: a committed index template with replicas 0 and explicit keyword fields, plus an ILM rollover and delete policy.
- **[MED] Alerts have no contact point.** Rules are provisioned, delivery is not. They fire in Grafana's alert list only. Fix: a provisioned contact point and notification policy.
- **[MED] No handler-level tests and no LLM fake.** `LlmClient` is concrete, so `derive_verdict` and the evaluate handler are only exercised by hand. Fix: put the client behind a trait and test the three verdict sources with a fake.
- **[LOW] Notifier state is unbounded and volatile.** The API accepts free-text cities so the map grows with every new city string, and a restart forgets everything. Accepted for scale; would cap or persist for production.
- **[LOW] String comparison of timestamps in the status merge.** RFC 3339 strings in UTC compare correctly except when two timestamps in the same second are serialized with different fractional precision. In practice the same event round-trips as the identical string.
- **[LOW] The publisher opens a fresh connection per reconnect and keeps only the channel handle.** Works, verified by the injection script, but a connection pool or a long-lived connection with channel recovery would be cleaner.

### Documentation drift they can catch

These are cheap to fix before the interview and expensive to be caught on. Each is a "your docs say X but the code does Y" question.

- README, twice, describes the fallback as a rule-based verdict from the preferred conditions. The code returns "not recommended, advisor unavailable." The `VerdictSource::Fallback` doc comment in `event.rs` says the same stale thing.
- Alert rule comments in `rules.yml` reason from a 10-minute scheduler cadence. The default is now one minute, so the backlog threshold crosses in about two minutes and the staleness window is 30× the cadence. The thresholds still work; the justification is stale.
- DESIGN.md §3 says the user picks a city as free text. The UI became a dropdown of configured cities. The API still accepts free text.
- The eval harness README says it uses the same system prompt as production. It was faithful when written, but the production prompt changed afterwards — user-facing reasoning rules, "should normally" guidance, the "N of M" summary line. Say the harness measured the prompt at swap time and would need a re-sync to re-run.
- DESIGN.md's status line still reads "approved design, pre-implementation" while §9 marks items resolved.

---

## 8. The AI-assisted question

The commit trailers say "Co-Authored-By: Claude" and the README FAQ says it out loud. Assume they know.

**Lead with it.** Something like:

> "I built this with an AI pair programmer, deliberately, and the workflow is part of what I'm showing. I wrote the design document first so every decision was mine before any code existed, I chose Rust so the compiler catches whole bug classes before review, and I spent my time on the parts a model cannot do: choosing between alternatives, measuring the LLM, reviewing adversarially, and reverting when the first cut was wrong."

Then prove ownership with specifics, because that is what they are testing:

- Point to the **reversal**: the consistency guard was a working, tested feature and you removed it on a design principle.
- Point to the **measurement**: the debug page and the OpenRouter harness exist because you refused to trust prompt changes without numbers.
- Point to the **review round**: fifteen findings, several of them subtle — timeout as unconfirmed, channel cloned out of the mutex, stale-event ordering in the notifier.
- Be able to **walk any file live**. The drills above cover the questions an interviewer asks to test ownership: "why this lock," "why this polarity," "what happens if this times out."

If asked what the model got wrong: the first fallback polarity, the consistency guard, and a matkot verdict at 19 °C that slipped through on a fabricated justification until you fixed the condition description.

If asked what you would not delegate: the decision ledger and the prompt-alignment findings, which came from watching outputs, not from generating code.

---

## 9. Numbers to know cold

Precise figures signal that you built it. Round ones signal that you read about it.

| Thing | Value | Where it lives |
|---|---|---|
| Scheduler cadence, pairs per tick | 1 min · 3 × 3 = 9 | activities.yaml, `SCHEDULER_INTERVAL_MINUTES` |
| Weather client timeout, attempts, base delay | 10 s · 3 · 300 ms | weather.rs, per stage (geocode, then forecast) |
| LLM timeout, attempts, pause | 60 s · 2 · 500 ms | `LLM_TIMEOUT_SECS`, `LLM_ATTEMPTS` |
| LLM request shape | temp 0 · 250 tokens · json_object · ctx 2048 | llm.rs, compose llm command |
| Publish attempts, base backoff, overall cap | 4 · 400 ms · 10 s | publish.rs, main.rs |
| Scheduler HTTP client timeout | 210 s (API worst case ≈ 193 s) | scheduler main.rs |
| Logstash persistent queue, prefetch | 512 MB · 64 | logstash.yml, main.conf |
| SSE broadcast capacity | 64 | notifier.rs |
| Model, quant, size | Ministral-3-3B-Instruct-2512 · Q4_K_M · 2.15 GB | .env.example, compose |
| Hardware envelope for model selection | 7.65 GB RAM · 10 CPU · no GPU · ≈2.5 GB budget | DESIGN.md D7 |
| Eval harness | 9 cases · 3 trials · 10× stability · 9/9, 87/90 | scripts/llm-fit-eval |
| Incumbent score, runner-up | Qwen2.5-3B 8/9 · granite-4.0-h-micro 8/9 | harness README |
| JVM heaps | ES 512 MB · Logstash 256–512 MB | docker-compose.yml |
| Alert thresholds | backlog > 10 for 10 m · LLM errors > 0.005/s for 10 m · DLQ > 0 for 1 m · < 1 eval in 30 m | rules.yml |
| Matkot hard gate | daylight · 15–38 °C · wind ≤ 20 · vis ≥ 1 km · rain ≤ 2 mm | activities.yaml |
| Matkot soft conditions | 22–31 °C · wind ≤ 12 · rain 0 | activities.yaml |
| Gaming inverse conditions | night · ≥ 32 °C · ≤ 10 °C · wind ≥ 30 · rain ≥ 0.1 | activities.yaml |
| Tests | 33 backend · 4 frontend · 1 scripted injection | cargo test, vitest, scripts/ |
| Scrape targets, dashboards, alert rules | 5 · 2 · 5 | prometheus.yml, monitoring/grafana |
| Merged PRs, release | #19 #21 #24 #25 #26 · v1.0.0 | git log |

---

## 10. Before the interview

In priority order.

1. **Run the stack once more** and do the demo yourself: evaluate, watch a scheduled tick land in Grafana, run the injection script, open the debug page and slide wind past 12 km/h. Have screenshots in case Docker misbehaves on the day.
2. **Fix the five documentation drifts** listed above. Twenty minutes, and it removes the easiest gotchas.
3. **Rehearse the three drawings** until each is under two minutes with the timeouts annotated.
4. **Say the pitch aloud five times.** Then say it in 30 seconds.
5. **Pick your two stories**: the consistency guard reversal for "tell me about a decision you changed," the OpenRouter harness for "how do you make decisions with data."
6. **Prepare the "what next" answer**: LLM client behind a trait plus handler tests, index template plus lifecycle policy, alert contact point, bounded scheduler concurrency, a request-scoped LLM timeout for users. Concrete and small beats grand.
7. **Have questions for them** that come from this project: how they run local inference, how they test pipelines with failure injection, how they decide between queue and log.
