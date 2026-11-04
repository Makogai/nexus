import { describe, expect, it } from 'vitest'
import { initialLaunchSelection, profileDefaults } from '../src/lib/launch'
import type { ClaudeProfile } from '../src/lib/types'

function profile(id: string, over: Partial<ClaudeProfile> = {}): ClaudeProfile {
  return {
    id,
    name: id,
    description: '',
    color: '#7c6cf2',
    configDir: null,
    executable: null,
    extraArgs: [],
    managedByNexus: false,
    inheritProviderEnv: false,
    isolateInstructions: false,
    defaultWorkProfile: null,
    defaultEffort: null,
    defaultModel: null,
    createdAt: 0,
    updatedAt: 0,
    lastUsedAt: null,
    archivedAt: null,
    ...over,
  }
}

const work = profile('work', { defaultWorkProfile: 'professional', defaultEffort: 'high' })
const game = profile('game', { defaultWorkProfile: 'prototype' })
const plain = profile('plain')
const all = [work, game, plain]

describe('initialLaunchSelection', () => {
  it('uses the explicitly requested profile and its own defaults', () => {
    const last = { profileId: 'work', workProfile: 'critical', effort: 'max' }
    expect(initialLaunchSelection(all, last, 'game', 'balanced')).toEqual({
      profileId: 'game',
      workProfile: 'prototype',
      effort: '',
    })
  })

  it('restores last-used rigor only for the same profile', () => {
    const last = { profileId: 'work', workProfile: 'critical', effort: 'max' }
    expect(initialLaunchSelection(all, last, 'work', 'balanced')).toEqual({
      profileId: 'work',
      workProfile: 'critical',
      effort: 'max',
    })
    expect(initialLaunchSelection(all, last, undefined, 'balanced').workProfile).toBe('critical')
  })

  it('falls back to the first profile and NEXUS defaults', () => {
    expect(initialLaunchSelection(all, null, 'deleted-profile', 'balanced')).toEqual({
      profileId: 'work',
      workProfile: 'professional',
      effort: 'high',
    })
    expect(initialLaunchSelection([plain], null, null, 'balanced')).toEqual({
      profileId: 'plain',
      workProfile: 'balanced',
      effort: '',
    })
    expect(initialLaunchSelection([], null, null, 'balanced').profileId).toBe('')
  })

  it('an empty last-used effort falls back to the profile default', () => {
    const last = { profileId: 'work', workProfile: 'balanced', effort: null }
    expect(initialLaunchSelection(all, last, 'work', 'balanced').effort).toBe('high')
  })
})

describe('profileDefaults', () => {
  it('maps a profile to its defaults', () => {
    expect(profileDefaults(work, 'balanced')).toEqual({ workProfile: 'professional', effort: 'high' })
    expect(profileDefaults(null, 'balanced')).toEqual({ workProfile: 'balanced', effort: '' })
  })
})
