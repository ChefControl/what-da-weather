import type { Notice } from '../api'

interface Props {
  notices: Notice[]
  onDismiss: (index: number) => void
}

export function Toasts({ notices, onDismiss }: Props) {
  if (notices.length === 0) return null
  return (
    <div className="toasts">
      {notices.map((n, i) => (
        <div key={`${n.city}-${n.activity}-${n.timestamp}`} className="toast">
          <strong>
            🎉 {n.activity_name} in {n.city} just became recommended!
          </strong>
          <p>{n.reasoning}</p>
          <button onClick={() => onDismiss(i)} aria-label="Dismiss">
            ×
          </button>
        </div>
      ))}
    </div>
  )
}
