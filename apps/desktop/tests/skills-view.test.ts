// Skills page (ADR-0046): every skill in one list; workspace and project skills are created and edited here (with
// the version they replace), built-in and kit skills are read-only.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { SkillEntry, SkillFile, Workspace } from '../src/lib/types'

const api = vi.hoisted(() => ({
  skillsList: vi.fn(),
  skillsRead: vi.fn(),
  skillsCreate: vi.fn(),
  skillsSave: vi.fn(),
  skillsDelete: vi.fn(),
  projectList: vi.fn(),
}))
vi.mock('../src/lib/ipc', () => ({ api, toIpcError: (e: unknown) => (typeof e === 'object' && e && 'code' in e ? e : { code: 'invalid', message: String(e) }) }))
vi.mock('vue-router', () => ({ useRouter: () => ({ push: vi.fn() }), RouterLink: { template: '<a><slot /></a>' } }))

import SkillsView from '../src/views/SkillsView.vue'
import { useAppStore } from '../src/stores/app'
import { useWorkspaceStore } from '../src/stores/workspaces'

const entry = (over: Partial<SkillEntry>): SkillEntry => ({
  scope: 'workspace',
  name: 'release-notes',
  description: 'Draft release notes.',
  invokeAs: 'workspace:release-notes',
  projectId: null,
  projectName: null,
  kitId: null,
  kitName: null,
  path: 'C:\\ws\\skills\\release-notes\\SKILL.md',
  editable: true,
  status: 'approved',
  reaches: 'every session of this workspace',
  ...over,
})
const list: SkillEntry[] = [
  entry({}),
  entry({ scope: 'project', name: 'checkout', invokeAs: 'checkout', projectId: 'p1', projectName: 'shop', status: 'native', path: 'C:\\shop\\.claude\\skills\\checkout\\SKILL.md' }),
  entry({ scope: 'kit', name: 'review', invokeAs: 'kit:review', kitId: 'k1', kitName: 'kit', editable: false, status: 'approved' }),
  entry({ scope: 'builtin', name: 'nexus:learn-project', invokeAs: 'nexus:learn-project', editable: false, status: 'built-in', path: null }),
]
const fileOf = (e: SkillEntry, text = '---\nname: x\ndescription: d\n---\nBody\n'): SkillFile => ({ entry: e, text, hash: e.editable ? 'h1' : '' })

const EditorStub = {
  props: ['modelValue', 'readonly'],
  emits: ['update:modelValue'],
  template: `<textarea data-editor-stub :value="modelValue" :readonly="readonly" @input="$emit('update:modelValue', $event.target.value)" />`,
}
const ModalStub = { props: ['open'], template: '<div v-if="open"><slot /><slot name="footer" /></div>' }
const mountView = () => mount(SkillsView, { global: { stubs: { CodeEditor: EditorStub, BaseModal: ModalStub, RouterLink: { template: '<a><slot /></a>' } } } })

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  useWorkspaceStore().active = { id: 'w1', name: 'W', rootPath: 'C:\\ws' } as Workspace
  api.skillsList.mockResolvedValue(list)
  api.projectList.mockResolvedValue([{ id: 'p1', name: 'shop', available: true }])
  api.skillsRead.mockImplementation(async (ws: string, r: { scope: string; name: string }) => {
    const current: SkillEntry[] = await api.skillsList(ws)
    return fileOf(current.find((s) => s.scope === r.scope && s.name === r.name)!)
  })
})

describe('SkillsView', () => {
  it('lists every kind of skill, grouped, and opens the first editable one', async () => {
    const w = mountView()
    await flushPromises()
    const names = w.findAll('[data-skill]').map((b) => b.attributes('data-skill'))
    expect(names).toEqual(['workspace:release-notes', 'checkout', 'kit:review', 'nexus:learn-project'])
    expect(w.find('[data-skill="checkout"]').text()).toContain('shop')
    expect(w.find('[data-skill-detail]').text()).toContain('workspace:release-notes')
    await w.find('[data-skill-filter="kit"]').trigger('click')
    expect(w.findAll('[data-skill]').map((b) => b.attributes('data-skill'))).toEqual(['kit:review'])
  })

  it('saves an edit with the version it replaces and shows what the save did', async () => {
    const toast = vi.spyOn(useAppStore(), 'toast')
    const w = mountView()
    await flushPromises()
    expect(w.find('[data-skill-save]').attributes('disabled')).toBeDefined()
    await w.find('[data-editor-stub]').setValue('---\nname: release-notes\ndescription: d\n---\nNew body\n')
    api.skillsSave.mockResolvedValue({ file: fileOf(list[0]!, 'saved'), outcome: 'approved', message: 'Saved — sessions get it.' })
    await w.find('[data-skill-save]').trigger('click')
    await flushPromises()
    expect(api.skillsSave).toHaveBeenCalledWith('w1', 'workspace', null, 'release-notes', '---\nname: release-notes\ndescription: d\n---\nNew body\n', 'h1')
    expect(toast).toHaveBeenCalledWith('success', 'Saved — sessions get it.', 7000)

    await w.find('[data-editor-stub]').setValue('changed again')
    api.skillsSave.mockRejectedValue({ code: 'conflict', message: 'changed' })
    await w.find('[data-skill-save]').trigger('click')
    await flushPromises()
    expect(toast).toHaveBeenLastCalledWith('error', expect.stringContaining('changed on disk'))
  })

  it('shows built-in and kit skills read-only, and a review notice for workspace skills not approved here', async () => {
    api.skillsList.mockResolvedValue([entry({ status: 'changed' }), ...list.slice(1)])
    const w = mountView()
    await flushPromises()
    expect(w.find('[data-skill-review]').exists()).toBe(true)
    await w.find('[data-skill="nexus:learn-project"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-editor-stub]').attributes('readonly')).toBeDefined()
    expect(w.find('[data-skill-save]').exists()).toBe(false)
    expect(w.find('[data-skill-delete]').exists()).toBe(false)
  })

  it('creates a skill in a project from a valid name and description', async () => {
    const w = mountView()
    await flushPromises()
    await w.find('[data-skill-new]').trigger('click')
    await w.find('[data-new-scope="project"]').trigger('click')
    await w.find('[data-new-name]').setValue('Checkout Flow!')
    expect((w.find('[data-new-name]').element as HTMLInputElement).value).toBe('checkout-flow')
    expect(w.find('[data-new-create]').attributes('disabled')).toBeDefined()
    await w.find('[data-new-description]').setValue('How checkout works. Use when changing payments.')
    const created = entry({ scope: 'project', name: 'checkout-flow', invokeAs: 'checkout-flow', projectId: 'p1', projectName: 'shop', status: 'native' })
    api.skillsCreate.mockResolvedValue({ file: fileOf(created), outcome: 'native', message: 'Saved in the project.' })
    api.skillsList.mockResolvedValue([...list, created])
    await w.find('[data-new-create]').trigger('click')
    await flushPromises()
    expect(api.skillsCreate).toHaveBeenCalledWith('w1', 'project', 'p1', 'checkout-flow', 'How checkout works. Use when changing payments.')
    expect(w.find('[data-skill-detail]').text()).toContain('checkout-flow')
  })
})
