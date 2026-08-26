import { useMemo, useState } from 'react'
import { Delaunay } from 'd3-delaunay'
import type { ActivityMeta, StatusItem } from '../api'
import { sourceLabel, verdictLabel } from '../format'
import { fitProjection, pointInRing, ringToPath, type Ring } from '../map/geometry'
import outline from '../map/israel-outline.json'

// Voronoi activity map (post-task-enhancement.md): every pixel is colored by
// the verdict at the NEAREST measured city — regions without averaging. The
// cells are computed here at runtime from the lat/lons /api/status already
// carries, so a config city-list change propagates with no generated artifact.

const MAP_HEIGHT = 620
const MAP_MAX_WIDTH = 420

const RING = outline.coordinates[0] as Ring
const PROJ = fitProjection(RING, MAP_MAX_WIDTH, MAP_HEIGHT)
const OUTLINE_PATH = ringToPath(RING, PROJ)

/** Gray covers both "advisor unavailable" (D6 fallback) and "never measured". */
type CellState = 'ok' | 'nope' | 'none'

interface Site {
  city: string
  x: number
  y: number
  /** Latest event per activity key for this city. */
  byActivity: Map<string, StatusItem>
}

function cellState(item: StatusItem | undefined): CellState {
  if (!item || item.source === 'fallback') return 'none'
  return item.recommended ? 'ok' : 'nope'
}

interface Props {
  activities: ActivityMeta[]
  items: StatusItem[]
  /** (city|activity) -> sequence number; a new value replays the cell flash. */
  flash: Record<string, number>
  onInspect: (city: string, activity: string) => void
}

export function MapView({ activities, items, flash, onInspect }: Props) {
  const [activityKey, setActivityKey] = useState('')
  const selected = activityKey || activities[0]?.key || ''
  const [hover, setHover] = useState<{ site: Site; x: number; y: number } | null>(null)

  // Sites are cities inside the outline; one-off evaluations elsewhere
  // ("London") stay off the map instead of warping every cell toward them.
  const sites = useMemo(() => {
    const byCity = new Map<string, Site>()
    for (const item of items) {
      if (!Number.isFinite(item.latitude) || !Number.isFinite(item.longitude)) continue
      if (!pointInRing(RING, item.longitude, item.latitude)) continue
      let site = byCity.get(item.city)
      if (!site) {
        site = {
          city: item.city,
          x: PROJ.x(item.longitude),
          y: PROJ.y(item.latitude),
          byActivity: new Map(),
        }
        byCity.set(item.city, site)
      }
      site.byActivity.set(item.activity, item)
    }
    return [...byCity.values()]
  }, [items])

  const cells = useMemo(() => {
    if (sites.length === 0) return []
    const delaunay = Delaunay.from(
      sites,
      (s) => s.x,
      (s) => s.y,
    )
    const voronoi = delaunay.voronoi([0, 0, PROJ.width, PROJ.height])
    return sites.map((site, i) => ({ site, path: voronoi.renderCell(i) }))
  }, [sites])

  const counts = useMemo(() => {
    const c = { ok: 0, nope: 0, none: 0 }
    for (const site of sites) c[cellState(site.byActivity.get(selected))]++
    return c
  }, [sites, selected])

  const hoverItem = hover?.site.byActivity.get(selected)

  return (
    <div className="card map-view">
      <div className="board-header">
        <h2>🗺 Activity map</h2>
        <div className="view-switch">
          {activities.map((a) => (
            <button
              key={a.key}
              className={`ghost ${a.key === selected ? 'active' : ''}`}
              onClick={() => setActivityKey(a.key)}
            >
              {a.name}
            </button>
          ))}
        </div>
      </div>
      <p className="muted map-claim">
        Each region is colored by the latest verdict at its nearest measured city — no
        interpolation, no data invented between measurements. Click a region to inspect that city.
      </p>

      <div className="map-layout">
        <div className="map-canvas" onMouseLeave={() => setHover(null)}>
          <svg
            viewBox={`0 0 ${PROJ.width} ${PROJ.height}`}
            width={PROJ.width}
            height={PROJ.height}
            role="img"
            aria-label="Map of Israel, regions colored by activity verdict"
          >
            <defs>
              <clipPath id="country-clip">
                <path d={OUTLINE_PATH} />
              </clipPath>
            </defs>
            {/* Base fill: the country (with its cutouts) is visible from the
                first paint, before any measurement has arrived to carve cells. */}
            <path d={OUTLINE_PATH} className="map-base" />
            <g clipPath="url(#country-clip)">
              {cells.map(({ site, path }) => {
                if (!path) return null
                const state = cellState(site.byActivity.get(selected))
                const flashSeq = flash[`${site.city}|${selected}`]
                return (
                  <path
                    // A new flash sequence remounts the node so the CSS
                    // animation replays on repeat nudges for the same cell.
                    key={`${site.city}:${flashSeq ?? 0}`}
                    d={path}
                    className={`map-cell ${state} ${flashSeq ? 'flash' : ''}`}
                    onMouseMove={(e) => {
                      const box = e.currentTarget.closest('.map-canvas')!.getBoundingClientRect()
                      setHover({ site, x: e.clientX - box.left, y: e.clientY - box.top })
                    }}
                    onClick={() => onInspect(site.city, selected)}
                  />
                )
              })}
            </g>
            <path d={OUTLINE_PATH} className="map-outline" />
            {sites.map((site) => {
              // Dense center (Tel Aviv / Petah Tikva): a label whose eastern
              // neighbor is close flips to the left of its dot instead of
              // running into it.
              const crowdedRight = sites.some(
                (o) => o !== site && Math.abs(o.y - site.y) < 10 && o.x - site.x > 0 && o.x - site.x < 85,
              )
              return (
                <g key={site.city} className="map-city" pointerEvents="none">
                  <circle cx={site.x} cy={site.y} r={2.5} />
                  <text
                    x={crowdedRight ? site.x - 5 : site.x + 5}
                    y={site.y + 3}
                    textAnchor={crowdedRight ? 'end' : 'start'}
                  >
                    {site.city}
                  </text>
                </g>
              )
            })}
          </svg>
          {hover && (
            <div
              className="map-tooltip"
              style={{
                left: Math.min(hover.x + 14, PROJ.width - 10),
                top: hover.y + 14,
                transform: hover.x > PROJ.width / 2 ? 'translateX(-100%)' : undefined,
              }}
            >
              <strong>{hover.site.city}</strong>
              {hoverItem ? (
                <>
                  <span className={`tooltip-verdict ${cellState(hoverItem)}`}>
                    {hoverItem.source === 'fallback'
                      ? sourceLabel('fallback')
                      : verdictLabel(hoverItem.recommended)}
                  </span>
                  <p>{hoverItem.reasoning}</p>
                  <span className="muted">
                    as of {new Date(hoverItem.timestamp).toLocaleTimeString()}
                  </span>
                </>
              ) : (
                <p className="muted">No verdict yet for this activity.</p>
              )}
            </div>
          )}
          {sites.length === 0 && (
            <p className="muted map-empty">
              No measurements yet — the scheduler fills the map within one tick.
            </p>
          )}
        </div>

        <div className="map-side">
          <h3>Legend</h3>
          <ul className="map-legend">
            <li>
              <span className="swatch ok" /> Recommended ({counts.ok})
            </li>
            <li>
              <span className="swatch nope" /> Not recommended ({counts.nope})
            </li>
            <li>
              <span className="swatch none" /> No verdict ({counts.none})
            </li>
          </ul>
          <p className="muted">
            Red is a real “no” from the advisor. Gray means no verdict at all — the city
            hasn’t been measured yet, or the LLM was unavailable — so a missing answer never
            masquerades as a negative one. A region flashes the moment its activity{' '}
            <em>becomes</em> recommended.
          </p>
          <p className="muted map-attribution">
            Outline ©{' '}
            <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noreferrer">
              OpenStreetMap contributors
            </a>{' '}
            (ODbL), land-clipped via{' '}
            <a href="https://www.geoboundaries.org" target="_blank" rel="noreferrer">
              geoBoundaries
            </a>{' '}
            (CC BY)
          </p>
        </div>
      </div>
    </div>
  )
}
