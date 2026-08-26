import { FormEvent, useState } from 'react'
import type { ActivityMeta } from '../api'

interface Props {
  activities: ActivityMeta[]
  cities: string[]
  busy: boolean
  onSubmit: (city: string, activity: string) => void
}

export function EvaluateForm({ activities, cities, busy, onSubmit }: Props) {
  const [city, setCity] = useState('')
  const [activity, setActivity] = useState('')
  const selectedCity = city || cities[0] || ''
  const selectedActivity = activity || activities[0]?.key || ''

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
          {cities.map((c) => (
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
