export interface Weather {
  temperature_c: number
  wind_kmh: number
  precipitation_mm: number
  visibility_km?: number
  weather_code: number
  is_day: boolean
}

export type VerdictSource = 'rules-gate' | 'llm' | 'fallback'

export interface EvaluationEvent {
  event_id: string
  timestamp: string
  trigger: string
  city: string
  country?: string | null
  latitude: number
  longitude: number
  activity: string
  activity_name: string
  weather: Weather
  gate_passed: boolean
  gate_failures: string[]
  recommended: boolean
  source: VerdictSource
  reasoning: string
  llm_latency_ms?: number | null
}

export interface EvaluateResponse {
  event: EvaluationEvent
  published: boolean
}

export interface ActivityMeta {
  key: string
  name: string
  required: string[]
  prompt: string
}

export interface ActivitiesResponse {
  activities: ActivityMeta[]
  cities: string[]
}

export interface Notice {
  type: string
  city: string
  activity: string
  activity_name: string
  reasoning: string
  timestamp: string
}

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  const resp = await fetch(url, init)
  if (!resp.ok) {
    let message = `${resp.status} ${resp.statusText}`
    try {
      const body = (await resp.json()) as { error?: string }
      if (body.error) message = body.error
    } catch {
      // non-JSON error body; keep the status text
    }
    throw new Error(message)
  }
  return (await resp.json()) as T
}

export function getActivities(): Promise<ActivitiesResponse> {
  return request('/api/activities')
}

export function evaluate(city: string, activity: string): Promise<EvaluateResponse> {
  return request('/api/evaluate', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ city, activity, trigger: 'user' }),
  })
}

/**
 * One `/api/status` item: the latest evaluation per (city, activity), i.e. an
 * EvaluationEvent as indexed. `city` is the geocoder's canonical spelling
 * ("Teverya"), which can differ from the config spelling ("Tiberias").
 */
export interface StatusItem {
  city: string
  activity: string
  activity_name: string
  timestamp: string
  latitude: number
  longitude: number
  recommended: boolean
  source: VerdictSource
  reasoning: string
  weather?: Weather
}

export interface StatusResponse {
  items: StatusItem[]
  elasticsearch: boolean
}

export function getStatus(): Promise<StatusResponse> {
  return request('/api/status')
}

export interface DebugResponse {
  activity: string
  activity_name: string
  weather: Weather
  gate_passed: boolean
  gate_failures: string[]
  recommended: boolean
  source: VerdictSource
  reasoning: string
  llm_latency_ms?: number | null
  prompt: string
}

export function debugEvaluate(activity: string, weather: Weather): Promise<DebugResponse> {
  return request('/api/debug/evaluate', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ activity, weather }),
  })
}
