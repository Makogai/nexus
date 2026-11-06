// Open modal dialogs, oldest first. The newest is on top and is the only one Escape closes, so a confirmation opened
// from a dialog (commit, push) always covers that dialog — whatever order the dialogs were mounted in.
const open: number[] = []
let seq = 0
/** The first dialog's layer; each dialog opened on top of it gets the next one (below toasts, which stay readable). */
const BASE = 50

export function openModal(): { id: number; z: number } {
  const id = ++seq
  open.push(id)
  return { id, z: BASE + 2 * (open.length - 1) }
}

export function closeModal(id: number): void {
  const i = open.indexOf(id)
  if (i >= 0) open.splice(i, 1)
}

export function isTopModal(id: number): boolean {
  return open.length > 0 && open[open.length - 1] === id
}
