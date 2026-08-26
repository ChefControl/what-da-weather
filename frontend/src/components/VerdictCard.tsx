import type { EvaluationEvent } from '../api'
import { sourceLabel, verdictLabel, weatherSummary } from '../format'

interface Props {
  event: EvaluationEvent
  published: boolean
}

export function VerdictCard({ event, published }: Props) {
  return (
    <div className={`card verdict-card ${event.recommended ? 'ok' : 'nope'}`}>
      <div className="verdict-headline">
        <span className="verdict-emoji">{event.recommended ? '✅' : '⛔'}</span>
        <div>
          <h2>
            {event.activity_name} in {event.city}
          </h2>
          <p className="verdict-label">{verdictLabel(event.recommended)}</p>
        </div>
      </div>
      <p className="weather-line">{weatherSummary(event.weather)}</p>
      <p className="reasoning">{event.reasoning}</p>
      {event.gate_failures.length > 0 && (
        <ul className="gate-failures">
          {event.gate_failures.map((f) => (
            <li key={f}>{f}</li>
          ))}
        </ul>
      )}
      <p className="meta-line">
        <span className={`badge source-${event.source}`}>{sourceLabel(event.source)}</span>
        {event.llm_latency_ms != null && <span className="badge">LLM {(event.llm_latency_ms / 1000).toFixed(1)}s</span>}
        {!published && <span className="badge warn">not persisted (queue unavailable)</span>}
      </p>
    </div>
  )
}
