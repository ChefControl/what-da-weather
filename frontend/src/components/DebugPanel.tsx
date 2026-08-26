import { useState } from 'react'
import { debugEvaluate, type ActivityMeta, type DebugResponse, type Weather } from '../api'
import { sourceLabel, verdictLabel } from '../format'

interface Props {
  activities: ActivityMeta[]
}

interface SliderSpec {
  key: keyof Weather
  label: string
  min: number
  max: number
  step: number
  unit: string
}

const SLIDERS: SliderSpec[] = [
  { key: 'temperature_c', label: 'Temperature', min: -10, max: 45, step: 0.5, unit: '°C' },
  { key: 'wind_kmh', label: 'Wind', min: 0, max: 80, step: 1, unit: 'km/h' },
  { key: 'humidity_pct', label: 'Humidity', min: 0, max: 100, step: 1, unit: '%' },
  { key: 'precipitation_mm', label: 'Precipitation', min: 0, max: 20, step: 0.1, unit: 'mm' },
  { key: 'cloud_cover_pct', label: 'Cloud cover', min: 0, max: 100, step: 1, unit: '%' },
  { key: 'visibility_km', label: 'Visibility', min: 0, max: 50, step: 0.5, unit: 'km' },
]

const PLEASANT_DAY: Weather = {
  temperature_c: 26,
  wind_kmh: 8,
  humidity_pct: 50,
  precipitation_mm: 0,
  cloud_cover_pct: 20,
  visibility_km: 25,
  weather_code: 0,
  is_day: true,
}

export function DebugPanel({ activities }: Props) {
  const [weather, setWeather] = useState<Weather>(PLEASANT_DAY)
  const [activity, setActivity] = useState('')
  const [result, setResult] = useState<DebugResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const selectedActivity = activity || activities[0]?.key || ''

  const setParam = (key: keyof Weather, value: number) =>
    setWeather((w) => ({ ...w, [key]: value }))

  const run = () => {
    setBusy(true)
    setError(null)
    debugEvaluate(selectedActivity, weather)
      .then(setResult)
      .catch((e: Error) => setError(e.message))
      .finally(() => setBusy(false))
  }

  return (
    <div className="debug-panel">
      <div className="card">
        <h2>🛠 Debug the AI</h2>
        <p className="muted">
          Synthetic weather goes through the identical gate → LLM path. Nothing is published or
          notified.
        </p>
        <label>
          Activity
          <select value={selectedActivity} onChange={(e) => setActivity(e.target.value)}>
            {activities.map((a) => (
              <option key={a.key} value={a.key}>
                {a.name}
              </option>
            ))}
          </select>
        </label>

        {SLIDERS.map((s) => (
          <div key={s.key} className="slider-row">
            <span className="slider-label">{s.label}</span>
            <input
              type="range"
              min={s.min}
              max={s.max}
              step={s.step}
              value={weather[s.key] as number}
              onChange={(e) => setParam(s.key, Number(e.target.value))}
            />
            <span className="slider-value">
              {weather[s.key] as number} {s.unit}
            </span>
          </div>
        ))}

        <div className="slider-row">
          <span className="slider-label">Sun is up</span>
          <input
            type="checkbox"
            checked={weather.is_day}
            onChange={(e) => setWeather((w) => ({ ...w, is_day: e.target.checked }))}
          />
          <span className="slider-value">{weather.is_day ? 'daytime' : 'nighttime'}</span>
        </div>

        <button onClick={run} disabled={busy}>
          {busy ? 'Asking the AI…' : 'Ask the AI'}
        </button>
      </div>

      {error && <div className="card error-banner">⚠️ {error}</div>}

      {result && (
        <div className={`card verdict-card ${result.recommended ? 'ok' : 'nope'}`}>
          <div className="verdict-headline">
            <span className="verdict-emoji">{result.recommended ? '✅' : '⛔'}</span>
            <div>
              <h2>{result.activity_name}</h2>
              <p className="verdict-label">{verdictLabel(result.recommended)}</p>
            </div>
          </div>
          <p className="reasoning">{result.reasoning}</p>
          {result.gate_failures.length > 0 && (
            <ul className="gate-failures">
              {result.gate_failures.map((f) => (
                <li key={f}>{f}</li>
              ))}
            </ul>
          )}
          <p className="meta-line">
            <span className={`badge source-${result.source}`}>{sourceLabel(result.source)}</span>
            {result.llm_latency_ms != null && (
              <span className="badge">LLM {(result.llm_latency_ms / 1000).toFixed(1)}s</span>
            )}
          </p>
          <details className="prompt-details">
            <summary>Prompt sent to the LLM</summary>
            <pre>{result.prompt}</pre>
          </details>
        </div>
      )}
    </div>
  )
}
