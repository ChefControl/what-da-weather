import { useEffect, useRef } from 'react'
import type { Notice } from './api'

/**
 * Subscribe to the SSE stream (DESIGN.md D6). EventSource reconnects
 * automatically. There is no replay: a nudge broadcast while disconnected is
 * gone for good — but the server does tell us when we fell behind an open
 * stream (a "lagged" event), and we surface that instead of staying silent.
 */
export function useNotifications(onNotice: (n: Notice) => void, onLagged?: (missed: number) => void) {
  const handler = useRef(onNotice)
  handler.current = onNotice
  const laggedHandler = useRef(onLagged)
  laggedHandler.current = onLagged

  useEffect(() => {
    const source = new EventSource('/api/events')
    const listener = (e: MessageEvent) => {
      try {
        handler.current(JSON.parse(e.data as string) as Notice)
      } catch {
        // ignore malformed payloads
      }
    }
    const lagged = (e: MessageEvent) => {
      laggedHandler.current?.(Number(e.data) || 0)
    }
    source.addEventListener('recommendation', listener)
    source.addEventListener('lagged', lagged)
    return () => {
      source.removeEventListener('recommendation', listener)
      source.removeEventListener('lagged', lagged)
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
