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
  const selected = activity || activities[0]?.key || ''

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    if (city.trim() && selected) onSubmit(city.trim(), selected)
  }

  return (
    <form className="card evaluate-form" onSubmit={handleSubmit}>
      <h2>Check now</h2>
      <label>
        City
        <input
          type="text"
          value={city}
          list="city-suggestions"
          placeholder="e.g. Tel Aviv"
          onChange={(e) => setCity(e.target.value)}
          required
        />
        <datalist id="city-suggestions">
          {cities.map((c) => (
            <option key={c} value={c} />
          ))}
        </datalist>
      </label>
      <label>
        Activity
        <select value={selected} onChange={(e) => setActivity(e.target.value)}>
          {activities.map((a) => (
            <option key={a.key} value={a.key}>
              {a.name}
            </option>
          ))}
        </select>
      </label>
      <button type="submit" disabled={busy || !city.trim()}>
        {busy ? 'Evaluating…' : 'Evaluate'}
      </button>
    </form>
  )
}
