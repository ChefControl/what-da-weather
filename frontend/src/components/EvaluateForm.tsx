import { FormEvent, useEffect, useState } from 'react'
import type { ActivityMeta } from '../api'

/** A map-cell click lands here: `seq` distinguishes repeated clicks. */
export interface Prefill {
  city: string
  activity: string
  seq: number
}

interface Props {
  activities: ActivityMeta[]
  cities: string[]
  busy: boolean
  prefill?: Prefill | null
  onSubmit: (city: string, activity: string) => void
}

export function EvaluateForm({ activities, cities, busy, prefill, onSubmit }: Props) {
  const [city, setCity] = useState('')
  const [activity, setActivity] = useState('')
  const selectedCity = city || cities[0] || ''
  const selectedActivity = activity || activities[0]?.key || ''

  useEffect(() => {
    if (prefill) {
      setCity(prefill.city)
      setActivity(prefill.activity)
    }
  }, [prefill])

  // Map cities carry the geocoder's canonical spelling ("Teverya"), which may
  // not be in the configured list — surface it as an extra option rather than
  // rendering an empty select.
  const cityOptions =
    selectedCity && !cities.includes(selectedCity) ? [...cities, selectedCity] : cities

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    if (selectedCity && selectedActivity) onSubmit(selectedCity, selectedActivity)
  }

  return (
    <form className="card evaluate-form" onSubmit={handleSubmit}>
      <h2>Check now</h2>
      <label>
        City
        <select value={selectedCity} onChange={(e) => setCity(e.target.value)}>
          {cityOptions.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
      </label>
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
      <button type="submit" disabled={busy || !selectedCity}>
        {busy ? 'Evaluating…' : 'Evaluate'}
      </button>
    </form>
  )
}
