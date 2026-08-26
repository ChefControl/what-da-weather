# LLM fit eval (OpenRouter harness)

Evidence behind the D7 model swap to **Ministral-3-3B-Instruct-2512 Q4_K_M**
(commit "Adopt Ministral-3-3B-Instruct-2512 Q4_K_M as the default model").

`llm_fit_eval.py` is a stdlib-only Python port of the production request path in
`backend/crates/core/src/llm.rs` — same system prompt, same `build_prompt`
formatting (verified against that file's unit tests), same defensive
`parse_verdict`, temperature 0, `max_tokens` 250, `response_format
json_object`, 2 attempts — pointed at OpenRouter instead of the local
llama.cpp server so many candidate models can be tested in parallel.

## Suite

9 cases, all passing the hard gate so the LLM decides every verdict:
per activity one clear-yes, one clear-no, and one tricky case (borderline
value, or Gaming's inverse preference). Ground truths derive mechanically from
`config/activities.yaml` semantics.

## Running

```
OPENROUTER_API_KEY=... python3 llm_fit_eval.py "model-a,model-b" 3 out.json
```

Args: comma-separated OpenRouter model ids, trials per case, output path.

## Results (2026-08-26)

Candidates were limited to DESIGN.md D7's budget (CPU-only Docker,
~2.5 GB GGUF → ≤ ~4B params). Incumbent Qwen2.5-3B-Instruct (not hosted on
OpenRouter) scored 8/9 on the same cases via the local debug page.

| Model | ~Q4 size | Score (maj/3) | 10-trial stability |
|---|---|---|---|
| **mistralai/ministral-3b-2512** | **2.15 GB** | **9/9** | **87/90** |
| ibm-granite/granite-4.0-h-micro | ~2 GB | 8/9 | 80/90 (G1 0/10) |
| google/gemma-3-4b-it | ~2.5 GB | 7/9 | — |
| meta-llama/llama-3.2-3b-instruct | 2.0 GB | 6/9 | — |
| meta-llama/llama-3.2-1b-instruct | 0.8 GB | 3/9 | — |
| qwen/qwen3-30b-a3b, google/gemma-3-12b (over-budget references) | — | 9/9 | — |

Ministral was the only in-budget model to solve `G1-lovely-day` (Gaming's
"pleasant weather → recommended=false" inversion), the case that also capped
the incumbent at 8/9. A context-aware judge pass found its reasoning coherent
in 85/90 responses; its one residual weakness is N3-style cases (a single
unmet condition among many met), flaky in ~2/10 repeats.

Raw per-trial output: `results/results_round1.json` (7 models × 9 cases × 3
trials) and `results/stability_10x.json` (finalists × 9 cases × 10 trials).

## Caveats

- OpenRouter served fp8/bf16-class weights, so scores are an upper bound for
  the local Q4_K_M quant; the swap was re-validated locally through
  `/api/debug/evaluate` before adoption.
- Ministral 3 support needs llama.cpp ≥ b7216 (Dec 2025).
- Ministral-3-3B is Apache 2.0 with an official mistralai GGUF repo.
