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
  return [
    `${w.temperature_c.toFixed(1)}°C`,
    `wind ${w.wind_kmh.toFixed(0)} km/h`,
    `humidity ${w.humidity_pct.toFixed(0)}%`,
    `rain ${w.precipitation_mm.toFixed(1)} mm`,
    `clouds ${w.cloud_cover_pct.toFixed(0)}%`,
    `visibility ${w.visibility_km.toFixed(0)} km`,
  ].join(' · ')
}

export function timeAgo(iso: string, now: Date = new Date()): string {
  const then = new Date(iso).getTime()
  if (Number.isNaN(then)) return 'unknown'
  const seconds = Math.max(0, Math.floor((now.getTime() - then) / 1000))
  if (seconds < 60) return 'just now'
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes} min ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours} h ago`
  const days = Math.floor(hours / 24)
  return `${days} d ago`
}
