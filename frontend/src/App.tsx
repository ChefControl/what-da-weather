import { useEffect, useState } from 'react'
import {
  evaluate,
  getActivities,
  type ActivitiesResponse,
  type EvaluateResponse,
  type Notice,
} from './api'
import { EvaluateForm } from './components/EvaluateForm'
import { Toasts } from './components/Toasts'
import { VerdictCard } from './components/VerdictCard'
import { browserNotify, useNotifications } from './useNotifications'

export default function App() {
  const [meta, setMeta] = useState<ActivitiesResponse>({ activities: [], cities: [] })
  const [result, setResult] = useState<EvaluateResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [notices, setNotices] = useState<Notice[]>([])
  const [notifPermission, setNotifPermission] = useState(
    'Notification' in window ? Notification.permission : 'unsupported',
  )

  useEffect(() => {
    getActivities()
      .then(setMeta)
      .catch((e: Error) => setError(`Failed to load activities: ${e.message}`))
  }, [])

  useNotifications((notice) => {
    setNotices((prev) => [...prev, notice])
    browserNotify(notice)
  })

  const handleEvaluate = (city: string, activity: string) => {
    setBusy(true)
    setError(null)
    evaluate(city, activity)
      .then(setResult)
      .catch((e: Error) => setError(e.message))
      .finally(() => setBusy(false))
  }

  const requestNotifications = () => {
    if ('Notification' in window) {
      void Notification.requestPermission().then(setNotifPermission)
    }
  }

  return (
    <div className="app">
      <header>
        <h1>
          🌦️ What Da Weather
          <span className="subtitle">should you go out, or boot up?</span>
        </h1>
        {notifPermission === 'default' && (
          <button className="ghost" onClick={requestNotifications}>
            🔔 Enable notifications
          </button>
        )}
      </header>

      {error && <div className="card error-banner">⚠️ {error}</div>}

      <main>
        <EvaluateForm
          activities={meta.activities}
          cities={meta.cities}
          busy={busy}
          onSubmit={handleEvaluate}
        />
        {result && <VerdictCard event={result.event} published={result.published} />}
      </main>

      <Toasts
        notices={notices}
        onDismiss={(index) => setNotices((prev) => prev.filter((_, i) => i !== index))}
      />

      <footer className="muted">
        Weather by Open-Meteo · verdicts by a local LLM · alerts push only when an activity{' '}
        <em>becomes</em> recommended
      </footer>
    </div>
  )
}
