import { describe, expect, it } from 'vitest'
import { fitProjection, pointInRing, ringToPath, type Ring } from './geometry'
import outline from './israel-outline.json'

// A 1°x1° square near Israel's latitude.
const SQUARE: Ring = [
  [34, 31],
  [35, 31],
  [35, 32],
  [34, 32],
  [34, 31],
]

describe('fitProjection', () => {
  it('maps the bounding box corners onto the padded viewport', () => {
    const proj = fitProjection(SQUARE, 400, 400, 10)
    expect(proj.x(34)).toBeCloseTo(10)
    expect(proj.y(32)).toBeCloseTo(10) // north edge at the top
    expect(proj.y(31)).toBeCloseTo(proj.height - 10)
    expect(proj.x(35)).toBeCloseTo(proj.width - 10)
  })

  it('compresses longitude by cos(latitude) so the aspect is true', () => {
    const proj = fitProjection(SQUARE, 4000, 400, 0)
    const xSpan = proj.x(35) - proj.x(34)
    const ySpan = proj.y(31) - proj.y(32)
    expect(xSpan / ySpan).toBeCloseTo(Math.cos(31.5 * (Math.PI / 180)), 5)
  })
})

describe('ringToPath', () => {
  it('renders a closed path', () => {
    const proj = fitProjection(SQUARE, 100, 100, 0)
    const d = ringToPath(SQUARE, proj)
    expect(d.startsWith('M')).toBe(true)
    expect(d.endsWith('Z')).toBe(true)
    expect(d.match(/L/g)).toHaveLength(SQUARE.length - 1)
  })
})

describe('pointInRing', () => {
  it('accepts inside points and rejects outside points', () => {
    expect(pointInRing(SQUARE, 34.5, 31.5)).toBe(true)
    expect(pointInRing(SQUARE, 33.9, 31.5)).toBe(false)
    expect(pointInRing(SQUARE, 34.5, 32.1)).toBe(false)
  })

  it('places the scheduler cities inside the committed outline and foreign cities outside', () => {
    const ring = outline.coordinates[0] as Ring
    // Tel Aviv, Jerusalem, Eilat
    expect(pointInRing(ring, 34.78, 32.08)).toBe(true)
    expect(pointInRing(ring, 35.22, 31.77)).toBe(true)
    expect(pointInRing(ring, 34.95, 29.56)).toBe(true)
    // London, Athens
    expect(pointInRing(ring, -0.13, 51.51)).toBe(false)
    expect(pointInRing(ring, 23.73, 37.98)).toBe(false)
  })
})
