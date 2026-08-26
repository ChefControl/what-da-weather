import { describe, expect, it } from 'vitest'
import { parseMuted, toggleCity } from './notifyFilter'

describe('parseMuted', () => {
  it('defaults to notify-all', () => {
    expect(parseMuted(null)).toEqual([])
    expect(parseMuted('')).toEqual([])
  })

  it('round-trips a stored list', () => {
    expect(parseMuted('["Eilat","Teverya"]')).toEqual(['Eilat', 'Teverya'])
  })

  it('treats malformed storage as notify-all instead of throwing', () => {
    expect(parseMuted('not json')).toEqual([])
    expect(parseMuted('{"a":1}')).toEqual([])
    expect(parseMuted('[1,"Eilat",null]')).toEqual(['Eilat'])
  })
})

describe('toggleCity', () => {
  it('mutes and unmutes without touching other cities', () => {
    const muted = toggleCity([], 'Eilat')
    expect(muted).toEqual(['Eilat'])
    expect(toggleCity(muted, 'Haifa')).toEqual(['Eilat', 'Haifa'])
    expect(toggleCity(['Eilat', 'Haifa'], 'Eilat')).toEqual(['Haifa'])
  })
})
