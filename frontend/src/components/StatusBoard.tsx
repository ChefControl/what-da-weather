import type { EvaluationEvent } from '../api'
import { timeAgo, weatherSummary } from '../format'

interface Props {
  items: EvaluationEvent[]
  elasticsearch: boolean
}

export function StatusBoard({ items, elasticsearch }: Props) {
  if (items.length === 0) {
    return (
      <div className="card">
        <h2>Current status</h2>
        <p className="muted">
          No evaluations yet — the scheduler runs every few minutes, or check a city above.
        </p>
      </div>
    )
  }

  const cities = [...new Set(items.map((i) => i.city))].sort()

  return (
    <div className="card">
      <div className="board-header">
        <h2>Current status</h2>
        {!elasticsearch && <span className="badge warn">live view — history store unreachable</span>}
      </div>
      {cities.map((city) => {
        const cityItems = items.filter((i) => i.city === city)
        return (
          <div key={city} className="city-block">
            <h3>
              {city}
              <span className="muted weather-inline">
                {' '}
                {weatherSummary(cityItems[0].weather)}
              </span>
            </h3>
            <div className="status-grid">
              {cityItems.map((item) => (
                <div
                  key={`${item.city}/${item.activity}`}
                  className={`status-tile ${item.recommended ? 'ok' : 'nope'}`}
                  title={item.reasoning}
                >
                  <span className="tile-verdict">{item.recommended ? '✅' : '—'}</span>
                  <span className="tile-name">{item.activity_name}</span>
                  <span className="tile-age muted">{timeAgo(item.timestamp)}</span>
                </div>
              ))}
            </div>
          </div>
        )
      })}
    </div>
  )
}
