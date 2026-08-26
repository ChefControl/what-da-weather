interface Props {
  cities: string[]
  muted: string[]
  onToggle: (city: string) => void
}

/**
 * Per-browser notification city filter (localStorage-backed in App). Checked
 * means "notify me about this city"; everything is checked by default. Only
 * toasts and browser notifications are filtered — the map always shows every
 * measurement.
 */
export function NotifyFilter({ cities, muted, onToggle }: Props) {
  if (cities.length === 0) return null
  const active = cities.length - muted.filter((c) => cities.includes(c)).length
  return (
    <details className="notify-filter">
      <summary className="ghost">
        🔕 Alert cities ({active}/{cities.length})
      </summary>
      <div className="notify-filter-panel card">
        {cities.map((city) => (
          <label key={city}>
            <input
              type="checkbox"
              checked={!muted.includes(city)}
              onChange={() => onToggle(city)}
            />
            {city}
          </label>
        ))}
      </div>
    </details>
  )
}
