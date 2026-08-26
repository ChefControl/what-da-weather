import { useEffect, useRef } from 'react'
import type { Notice } from './api'

/**
 * Subscribe to the SSE stream (DESIGN.md D6). EventSource reconnects
 * automatically; the UI re-syncs current state via /api/status, so a missed
 * realtime nudge is acceptable by design.
 */
export function useNotifications(onNotice: (n: Notice) => void) {
  const handler = useRef(onNotice)
  handler.current = onNotice

  useEffect(() => {
    const source = new EventSource('/api/events')
    const listener = (e: MessageEvent) => {
      try {
        handler.current(JSON.parse(e.data as string) as Notice)
      } catch {
        // ignore malformed payloads
      }
    }
    source.addEventListener('recommendation', listener)
    return () => {
      source.removeEventListener('recommendation', listener)
      source.close()
    }
  }, [])
}

export function browserNotify(notice: Notice) {
  if ('Notification' in window && Notification.permission === 'granted') {
    new Notification(`${notice.activity_name} — go for it!`, {
      body: `${notice.city}: ${notice.reasoning}`,
      icon: undefined,
    })
  }
}
