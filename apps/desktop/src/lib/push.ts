// Push request outcomes (ADR-0047): the words the dialog, the project banner and "Tell Claude" share, by failure kind.
import type { PushFailureKind, PushRequest, PushReview } from './types'

/** One line: what happened. */
export function failureTitle(kind: PushFailureKind | null | undefined): string {
  switch (kind) {
    case 'rejected':
      return 'Rejected — the remote has new commits'
    case 'auth':
      return 'The remote refused the credentials'
    case 'offline':
      return 'The remote cannot be reached'
    case 'timeout':
      return 'The remote did not answer in time'
    case 'remote-missing':
      return 'The remote repository was not found'
    default:
      return 'Git reported an error'
  }
}

/** What to do next — the same guidance Claude gets from `request_push`. */
export function failureGuidance(kind: PushFailureKind | null | undefined): string {
  switch (kind) {
    case 'rejected':
      return 'Integrate the new commits first (pull --rebase or merge), resolve any conflicts, run the tests, then push. NEXUS never force-pushes.'
    case 'auth':
      return 'Sign in with your Git tool or credential manager on this computer (NEXUS never asks for or stores credentials), then review the push again.'
    case 'offline':
    case 'timeout':
      return 'Check the network, then review the push again — nothing was sent.'
    case 'remote-missing':
      return 'Check the remote URL and your access to the repository.'
    default:
      return 'Check the repository in your Git tool before trying again.'
  }
}

/** "origin/main has 3 commits you don't have. …" — why Push is disabled while the remote is ahead. */
export function behindExplanation(r: Pick<PushReview, 'remote' | 'remoteBranch' | 'behind'>): string {
  const n = r.behind
  return `${r.remote}/${r.remoteBranch} has ${n} commit${n === 1 ? '' : 's'} you don't have. Integrate ${n === 1 ? 'it' : 'them'} first (pull --rebase or merge), resolve any conflicts, run the tests, then push. NEXUS never force-pushes.`
}

/** Branch and remote names come from Git; keep them to one plain line. */
const plain = (s: string) => s.replace(/[\u0000-\u001f\u007f]/g, '').slice(0, 200)

/**
 * The fixed, NEXUS-authored message *Tell Claude* types into the requesting session (never with Enter — the person
 * sends it). Only Git names and a count are filled in.
 */
export function tellClaudeText(request: Pick<PushRequest, 'branch'>, review: Pick<PushReview, 'remote' | 'remoteBranch' | 'behind'> | null): string {
  const target = review ? `${plain(review.remote)}/${plain(review.remoteBranch)}` : 'the remote branch'
  const what = review && review.behind > 0 ? `${review.behind} new commit${review.behind === 1 ? '' : 's'}` : 'new commits'
  return `NEXUS: the push of ${plain(request.branch)} was rejected — ${target} has ${what}. Integrate them (git pull --rebase or merge), resolve conflicts, run the tests, then call request_push again.`
}
