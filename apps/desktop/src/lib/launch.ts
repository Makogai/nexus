import type { ClaudeProfile, UiPreferences } from './types'

export interface LaunchSelection {
  profileId: string
  workProfile: string
  effort: string
}

/**
 * Initial launch-dialog selection.
 * Profile: explicitly requested → last used → first registered.
 * Rigor/effort: the last-used values carry over only when launching the *same* profile again; any other
 * profile starts from its own defaults (then NEXUS's default rigor and Claude's default effort).
 */
export function initialLaunchSelection(
  profiles: ClaudeProfile[],
  last: UiPreferences['lastLaunch'],
  requestedProfileId: string | null | undefined,
  defaultWorkProfile: string,
): LaunchSelection {
  const exists = (id: string | null | undefined): id is string => !!id && profiles.some((p) => p.id === id)
  const profileId = exists(requestedProfileId)
    ? requestedProfileId
    : exists(last?.profileId)
      ? last!.profileId!
      : (profiles[0]?.id ?? '')
  const profile = profiles.find((p) => p.id === profileId) ?? null
  const sameAsLast = !!last && last.profileId === profileId
  return {
    profileId,
    workProfile: (sameAsLast && last?.workProfile) || profile?.defaultWorkProfile || defaultWorkProfile,
    effort: (sameAsLast ? last?.effort : null) ?? profile?.defaultEffort ?? '',
  }
}

/** Defaults when the user switches profile inside the dialog. */
export function profileDefaults(profile: ClaudeProfile | null, defaultWorkProfile: string) {
  return {
    workProfile: profile?.defaultWorkProfile ?? defaultWorkProfile,
    effort: profile?.defaultEffort ?? '',
  }
}
