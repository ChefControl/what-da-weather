import type { VerdictSource, Weather } from './api'

export function verdictLabel(recommended: boolean): string {
  return recommended ? 'Recommended' : 'Not recommended'
}

export function sourceLabel(source: VerdictSource): string {
  switch (source) {
    case 'llm':
      return 'LLM verdict'
    case 'fallback':
      return 'Rule fallback (LLM unavailable)'
    case 'rules-gate':
      return 'Blocked by hard constraint'
  }
}

export function weatherSummary(w: Weather): string {
  const parts = [
    `${w.temperature_c.toFixed(1)}°C`,
    `wind ${w.wind_kmh.toFixed(0)} km/h`,
    `humidity ${w.humidity_pct.toFixed(0)}%`,
    `rain ${w.precipitation_mm.toFixed(1)} mm`,
    `clouds ${w.cloud_cover_pct.toFixed(0)}%`,
  ]
  // Events indexed before the visibility parameter existed lack the field.
  if (Number.isFinite(w.visibility_km)) {
    parts.push(`visibility ${(w.visibility_km as number).toFixed(0)} km`)
  }
  return parts.join(' · ')
}
