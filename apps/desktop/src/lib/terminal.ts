// Type a command into a new NEXUS terminal *without* pressing Enter (Portable Workspace P5): the person sees exactly what
// will run, can edit it, and runs it themselves — NEXUS never installs or executes setup commands on its own.
import { api } from './ipc'
import { useSessionStore } from '../stores/sessions'
import { useAppStore } from '../stores/app'

/** Pick an installed shell that matches the command's shell (`powershell`, `cmd`, `bash`), else the default. */
async function shellFor(kind: string): Promise<string | null> {
  if (kind === 'any') return null
  const shells = await api.shellList().catch(() => [])
  const match = shells.find((s) => `${s.id} ${s.name}`.toLowerCase().includes(kind === 'bash' ? 'bash' : kind))
  return match?.id ?? null
}

export async function typeIntoTerminal(command: string, opts: { shell?: string; projectId?: string | null } = {}) {
  const sessions = useSessionStore()
  const app = useAppStore()
  const shellId = await shellFor(opts.shell ?? 'any')
  const s = await sessions.openShell({ shellId, projectId: opts.projectId ?? null })
  // Give the shell a moment to show its prompt; the text is typed, not submitted.
  await new Promise((r) => setTimeout(r, 700))
  await api.sessionWrite(s.id, command)
  app.toast('info', 'The command is typed into a new terminal — review it and press Enter to run it.', 7000)
  return s
}
