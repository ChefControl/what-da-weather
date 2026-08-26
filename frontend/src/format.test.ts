import { describe, expect, it } from 'vitest'
import { sourceLabel, verdictLabel, weatherSummary } from './format'

describe('verdictLabel', () => {
  it('labels both verdicts', () => {
    expect(verdictLabel(true)).toBe('Recommended')
    expect(verdictLabel(false)).toBe('Not recommended')
  })
})

describe('sourceLabel', () => {
  it('covers every verdict source', () => {
    expect(sourceLabel('llm')).toBe('LLM verdict')
    expect(sourceLabel('fallback')).toContain('fallback')
    expect(sourceLabel('rules-gate')).toContain('hard constraint')
  })
})

describe('weatherSummary', () => {
  it('renders a compact one-liner', () => {
    const s = weatherSummary({
      temperature_c: 28.53,
      wind_kmh: 12.3,
      humidity_pct: 55,
      precipitation_mm: 0,
      cloud_cover_pct: 20,
      visibility_km: 24.14,
      weather_code: 1,
      is_day: true,
    })
    expect(s).toBe('28.5°C · wind 12 km/h · humidity 55% · rain 0.0 mm · clouds 20% · visibility 24 km')
  })

  it('tolerates events indexed before the visibility field existed', () => {
    const s = weatherSummary({
      temperature_c: 20,
      wind_kmh: 5,
      humidity_pct: 40,
      precipitation_mm: 0,
      cloud_cover_pct: 10,
      weather_code: 1,
      is_day: true,
    })
    expect(s).toBe('20.0°C · wind 5 km/h · humidity 40% · rain 0.0 mm · clouds 10%')
  })
})
