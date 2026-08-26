import { useCallback, useEffect, useRef, useState } from 'react'
import {
  evaluate,
  getActivities,
  getStatus,
  type ActivitiesResponse,
  type EvaluateResponse,
  type Notice,
  type StatusItem,
} from './api'
import { DebugPanel } from './components/DebugPanel'
import { EvaluateForm, type Prefill } from './components/EvaluateForm'
import { MapView } from './components/MapView'
import { NotifyFilter } from './components/NotifyFilter'
import { Toasts } from './components/Toasts'
import { VerdictCard } from './components/VerdictCard'
import { loadMuted, saveMuted, toggleCity } from './notifyFilter'
import { browserNotify, useNotifications } from './useNotifications'

type View = 'main' | 'map' | 'debug'

export default function App() {
  const [meta, setMeta] = useState<ActivitiesResponse>({ activities: [], cities: [] })
  const [result, setResult] = useState<EvaluateResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [notices, setNotices] = useState<Notice[]>([])
  const [notifPermission, setNotifPermission] = useState(
    'Notification' in window ? Notification.permission : 'unsupported',
  )
  const [view, setView] = useState<View>('main')
  const [items, setItems] = useState<StatusItem[]>([])
  const [flash, setFlash] = useState<Record<string, number>>({})
  const [muted, setMuted] = useState<string[]>(loadMuted)
  const [prefill, setPrefill] = useState<Prefill | null>(null)
  const seq = useRef(0)

  useEffect(() => {
    getActivities()
      .then(setMeta)
      .catch((e: Error) => setError(`Failed to load activities: ${e.message}`))
  }, [])

  // The latest verdict per (city, activity) powers the map and the city list
  // of the notification filter. Polling covers became-NOT-recommended flips,
  // which the SSE stream deliberately never carries (D6: nudges only).
  const refreshStatus = useCallback(() => {
    getStatus()
      .then((r) => setItems(r.items))
      .catch(() => {
        // Transient: the next poll retries; the map keeps its last snapshot.
      })
  }, [])

  useEffect(() => {
    refreshStatus()
    const timer = setInterval(refreshStatus, 30_000)
    return () => clearInterval(timer)
  }, [refreshStatus])

  useNotifications(
    (notice) => {
      // The map always reflects the event: patch the snapshot optimistically
      // and flash the cell, then re-poll for the authoritative document.
      seq.current += 1
      setFlash((prev) => ({ ...prev, [`${notice.city}|${notice.activity}`]: seq.current }))
      setItems((prev) =>
        prev.map((item) =>
          item.city === notice.city && item.activity === notice.activity
            ? {
                ...item,
                recommended: true,
                source: 'llm',
                reasoning: notice.reasoning,
                timestamp: notice.timestamp,
              }
            : item,
        ),
      )
      setTimeout(refreshStatus, 2000)
      // The mute filter only gates the alerting surfaces, never the map.
      if (!muted.includes(notice.city)) {
        setNotices((prev) => [...prev, notice])
        browserNotify(notice)
      }
    },
    (missed) => {
      setError(`Notification stream fell behind; ${missed} alert(s) may have been missed.`)
    },
  )

  const handleEvaluate = (city: string, activity: string) => {
    setBusy(true)
    setError(null)
    evaluate(city, activity)
      .then((r) => {
        setResult(r)
        refreshStatus()
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => setBusy(false))
  }

  const handleInspect = (city: string, activity: string) => {
    seq.current += 1
    setPrefill({ city, activity, seq: seq.current })
    setView('main')
  }

  const requestNotifications = () => {
    if ('Notification' in window) {
      void Notification.requestPermission().then(setNotifPermission)
    }
  }

  // Canonical geocoder spellings from the measurements themselves, so the
  // filter matches what notices actually say; config spellings are the
  // fallback before the first snapshot arrives.
  const filterCities = items.length
    ? [...new Set(items.map((i) => i.city))].sort()
    : meta.cities

  const VIEWS: { key: View; label: string }[] = [
    { key: 'main', label: '🏠 Main' },
    { key: 'map', label: '🗺 Map' },
    { key: 'debug', label: '🛠 Debug' },
  ]

  return (
    <div className="app">
      <header>
        <h1>
          🌦️ What Da Weather
          <span className="subtitle">should you go out, or boot up?</span>
        </h1>
        <div className="header-actions">
          {notifPermission === 'default' && (
            <button className="ghost" onClick={requestNotifications}>
              🔔 Enable notifications
            </button>
          )}
          <NotifyFilter
            cities={filterCities}
            muted={muted}
            onToggle={(city) => {
              const next = toggleCity(muted, city)
              setMuted(next)
              saveMuted(next)
            }}
          />
          <div className="view-switch">
            {VIEWS.map((v) => (
              <button
                key={v.key}
                className={`ghost ${view === v.key ? 'active' : ''}`}
                onClick={() => setView(v.key)}
              >
                {v.label}
              </button>
            ))}
          </div>
        </div>
      </header>

      {error && <div className="card error-banner">⚠️ {error}</div>}

      <main>
        {view === 'main' && (
          <>
            <EvaluateForm
              activities={meta.activities}
              cities={meta.cities}
              busy={busy}
              prefill={prefill}
              onSubmit={handleEvaluate}
            />
            {result && <VerdictCard event={result.event} published={result.published} />}
          </>
        )}
        {view === 'map' && (
          <MapView
            activities={meta.activities}
            items={items}
            flash={flash}
            onInspect={handleInspect}
          />
        )}
        {view === 'debug' && <DebugPanel activities={meta.activities} />}
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
