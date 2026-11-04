// Pure helpers for the AI Kit manager (unit-tested).
import type { InvEntry, KitInventory, KitSelection } from './types'

/** Estimated tokens of listings Claude keeps in context at session start (≈ chars / 4). */
export function listingTokens(entries: InvEntry[]): number {
  return Math.ceil(entries.reduce((n, e) => n + e.listingChars, 0) / 4)
}

/** What a profile's sessions get from a kit with this selection. */
export function effectiveTokens(inv: KitInventory, sel: KitSelection): number {
  const skills = sel.skills === null ? inv.skills : inv.skills.filter((s) => sel.skills!.includes(s.name))
  return listingTokens([...skills, ...(sel.agents ? inv.agents : []), ...(sel.commands ? inv.commands : [])])
}

/** NEXUS's default for a newly enabled kit: all skills of a small kit, none of a large one. */
export function defaultSelection(inv: KitInventory): KitSelection {
  const small = inv.skills.length <= 10 && listingTokens([...inv.skills, ...inv.agents, ...inv.commands]) <= 1500
  return { skills: small ? null : [], agents: true, commands: true, hooks: false, mcp: false }
}

export function selectedSkillCount(inv: KitInventory, sel: KitSelection): number {
  return sel.skills === null ? inv.skills.length : sel.skills.length
}

export const KIT_STATUS: Record<string, { label: string; tone: string }> = {
  approved: { label: 'Approved', tone: 'badge-ok' },
  'needs-review': { label: 'Needs review', tone: 'badge-warn' },
  changed: { label: 'Changed — review again', tone: 'badge-warn' },
  missing: { label: 'Files missing', tone: 'badge-error' },
}
