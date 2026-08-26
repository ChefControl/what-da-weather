// Client-side notification city filter. The backend broadcasts every
// became-recommended nudge; which of them turn into toasts / browser
// notifications is per-browser state, so it lives in localStorage — no
// backend digest infrastructure for a preference.
//
// Stored as the list of MUTED cities: the default (empty) means notify-all,
// and a city added to the config later is included by default instead of
// silently starting muted.

const STORAGE_KEY = 'wdw.mutedCities'

/** Parse a stored value defensively: anything malformed means "mute nothing". */
export function parseMuted(raw: string | null): string[] {
  if (!raw) return []
  try {
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.filter((c): c is string => typeof c === 'string')
  } catch {
    return []
  }
}

export function toggleCity(muted: string[], city: string): string[] {
  return muted.includes(city) ? muted.filter((c) => c !== city) : [...muted, city]
}

export function loadMuted(): string[] {
  try {
    return parseMuted(localStorage.getItem(STORAGE_KEY))
  } catch {
    return []
  }
}

export function saveMuted(muted: string[]) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(muted))
  } catch {
    // Storage unavailable (private mode, quota): the filter just doesn't persist.
  }
}
