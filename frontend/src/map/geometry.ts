// Pure geometry for the activity map: a country outline plus the scheduler's
// city points in, SVG coordinates out. No interpolation anywhere — the map's
// honesty rests on projecting measured points and nothing else.

/** A closed ring of [longitude, latitude] pairs. */
export type Ring = [number, number][]

export interface Projection {
  x: (lon: number) => number
  y: (lat: number) => number
  /** Viewport size the ring fits inside (including padding). */
  width: number
  height: number
}

/**
 * Equirectangular projection fitted to a ring's bounding box. Longitudes are
 * compressed by cos(mid-latitude) so the country keeps its true aspect; for a
 * span the size of Israel the distortion vs a real conic is invisible.
 */
export function fitProjection(ring: Ring, maxWidth: number, maxHeight: number, pad = 10): Projection {
  const lons = ring.map((p) => p[0])
  const lats = ring.map((p) => p[1])
  const minLon = Math.min(...lons)
  const maxLon = Math.max(...lons)
  const minLat = Math.min(...lats)
  const maxLat = Math.max(...lats)
  const aspect = Math.cos(((minLat + maxLat) / 2) * (Math.PI / 180))
  const lonSpan = (maxLon - minLon) * aspect
  const latSpan = maxLat - minLat
  const scale = Math.min((maxWidth - 2 * pad) / lonSpan, (maxHeight - 2 * pad) / latSpan)
  return {
    x: (lon) => pad + (lon - minLon) * aspect * scale,
    y: (lat) => pad + (maxLat - lat) * scale,
    width: lonSpan * scale + 2 * pad,
    height: latSpan * scale + 2 * pad,
  }
}

/** Render a ring as a closed SVG path in projected coordinates. */
export function ringToPath(ring: Ring, proj: Projection): string {
  return (
    ring
      .map((p, i) => `${i === 0 ? 'M' : 'L'}${proj.x(p[0]).toFixed(1)},${proj.y(p[1]).toFixed(1)}`)
      .join('') + 'Z'
  )
}

/**
 * Ray-casting point-in-polygon on raw [lon, lat] coordinates. Used to keep
 * one-off evaluations of cities outside the outline (a user typing "London")
 * from becoming Voronoi sites.
 */
export function pointInRing(ring: Ring, lon: number, lat: number): boolean {
  let inside = false
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const [xi, yi] = ring[i]
    const [xj, yj] = ring[j]
    if (yi > lat !== yj > lat && lon < ((xj - xi) * (lat - yi)) / (yj - yi) + xi) {
      inside = !inside
    }
  }
  return inside
}
