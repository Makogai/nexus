import { describe, expect, it } from 'vitest'
import { fuzzyFilter, fuzzyScore } from '../src/lib/fuzzy'
import { duration, relativeTime, sessionStatus, sessionTone, shortPath } from '../src/lib/format'
import type { SessionView } from '../src/lib/types'

describe('fuzzy', () => {
  it('matches characters in order and rejects others', () => {
    expect(fuzzyScore('lcs', 'Launch Claude session')).not.toBeNull()
    expect(fuzzyScore('xyz', 'Launch Claude session')).toBeNull()
    expect(fuzzyScore('', 'anything')).toBe(0)
  })

  it('ranks word-start and consecutive matches first', () => {
    const items = ['Open settings', 'Switch workspace', 'Open shell', 'Show sessions']
    expect(fuzzyFilter(items, 'os', (x) => x)[0]).toBe('Open shell')
    expect(fuzzyFilter(items, 'work', (x) => x)).toEqual(['Switch workspace'])
  })
})

describe('format', () => {
  const now = 1_000_000_000_000
  it('relative time', () => {
    expect(relativeTime(null)).toBe('never')
    expect(relativeTime(now - 10_000, now)).toBe('just now')
    expect(relativeTime(now - 5 * 60_000, now)).toBe('5 min ago')
    expect(relativeTime(now - 3 * 3_600_000, now)).toBe('3 h ago')
    expect(relativeTime(now - 24 * 3_600_000, now)).toBe('yesterday')
  })

  it('duration', () => {
    expect(duration(0, 42_000)).toBe('42s')
    expect(duration(0, 125_000)).toBe('2m 5s')
    expect(duration(0, 3_720_000)).toBe('1h 2m')
  })

  it('shortens paths keeping head and tail', () => {
    expect(shortPath('C:\\Users\\dev\\code\\app', 'C:\\Users\\dev')).toBe('~\\code\\app')
    const long = 'D:\\very\\long\\path\\with\\many\\segments\\that\\goes\\on\\project-name'
    const s = shortPath(long, undefined, 30)
    expect(s.startsWith('D:\\…\\')).toBe(true)
    expect(s.endsWith('project-name')).toBe(true)
    expect(s.length).toBeLessThanOrEqual(34)
  })

  it('describes session states truthfully', () => {
    const base = { running: false, state: 'exited', exitCode: 0, error: null } as unknown as SessionView
    expect(sessionStatus(base)).toBe('Exited')
    expect(sessionStatus({ ...base, exitCode: 2 })).toBe('Exited (code 2)')
    expect(sessionTone({ ...base, exitCode: 2 })).toBe('warn')
    expect(sessionStatus({ ...base, state: 'interrupted' })).toContain('Interrupted')
    expect(sessionStatus({ ...base, state: 'failed', error: 'boom' })).toBe('Failed: boom')
    expect(sessionTone({ ...base, running: true, state: 'running' })).toBe('live')
  })
})
