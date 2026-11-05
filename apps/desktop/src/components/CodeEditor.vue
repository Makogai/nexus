<script setup lang="ts">
// CodeMirror 6 (W2/W3): a Markdown editor for knowledge documents and a read-only viewer for project files. Colours
// come from the design tokens, so it follows the dark/light theme. Languages load on demand (one chunk each).
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Compartment, EditorState, type Extension } from '@codemirror/state'
import { EditorView, drawSelection, highlightActiveLine, highlightActiveLineGutter, keymap, lineNumbers } from '@codemirror/view'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { highlightSelectionMatches, search, searchKeymap } from '@codemirror/search'
import { HighlightStyle, LanguageDescription, bracketMatching, foldGutter, foldKeymap, indentOnInput, syntaxHighlighting } from '@codemirror/language'
import { languages } from '@codemirror/language-data'
import { markdown } from '@codemirror/lang-markdown'
import { tags as t } from '@lezer/highlight'

const props = withDefaults(
  defineProps<{
    modelValue: string
    /** File name for language detection (viewer). */
    filename?: string | null
    /** `markdown` for knowledge documents. */
    language?: 'markdown' | null
    readonly?: boolean
    wrap?: boolean
    /** CSS height of the editor (it scrolls inside). */
    height?: string
  }>(),
  { filename: null, language: null, readonly: false, wrap: false, height: '360px' },
)
const emit = defineEmits<{ 'update:modelValue': [string] }>()

const host = ref<HTMLDivElement | null>(null)
let view: EditorView | null = null
const lang = new Compartment()
const editable = new Compartment()

const highlight = HighlightStyle.define([
  { tag: [t.keyword, t.modifier, t.operatorKeyword, t.controlKeyword], color: 'var(--syn-keyword)' },
  { tag: [t.string, t.special(t.string), t.regexp], color: 'var(--syn-string)' },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], color: 'var(--syn-comment)', fontStyle: 'italic' },
  { tag: [t.number, t.bool, t.null, t.atom], color: 'var(--syn-number)' },
  { tag: [t.function(t.variableName), t.function(t.propertyName), t.macroName], color: 'var(--syn-function)' },
  { tag: [t.typeName, t.className, t.namespace, t.tagName], color: 'var(--syn-type)' },
  { tag: [t.propertyName, t.attributeName], color: 'var(--syn-property)' },
  { tag: [t.heading], color: 'var(--syn-heading)', fontWeight: '600' },
  { tag: [t.strong], fontWeight: '600' },
  { tag: [t.emphasis], fontStyle: 'italic' },
  { tag: [t.link, t.url], color: 'var(--syn-string)', textDecoration: 'underline' },
  { tag: [t.meta, t.processingInstruction], color: 'var(--syn-comment)' },
  { tag: [t.invalid], color: 'var(--red)' },
])

const theme = EditorView.theme({
  '&': { backgroundColor: 'var(--bg-input)', color: 'var(--text)', fontSize: '12.5px', height: '100%' },
  '.cm-scroller': { fontFamily: 'var(--font-mono, ui-monospace, monospace)', lineHeight: '1.6' },
  '.cm-content': { padding: '8px 0', caretColor: 'var(--text)' },
  '.cm-gutters': { backgroundColor: 'var(--bg-panel)', color: 'var(--text-faint)', border: 'none', borderRight: '1px solid var(--border)' },
  '.cm-activeLine': { backgroundColor: 'var(--editor-active-line)' },
  '.cm-activeLineGutter': { backgroundColor: 'var(--editor-active-line)', color: 'var(--text-muted)' },
  '&.cm-focused': { outline: 'none' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': { backgroundColor: 'var(--editor-selection) !important' },
  '.cm-cursor': { borderLeftColor: 'var(--text)' },
  '.cm-panels': { backgroundColor: 'var(--bg-panel)', color: 'var(--text)', borderTop: '1px solid var(--border)' },
  '.cm-searchMatch': { backgroundColor: 'var(--editor-match)', outline: '1px solid var(--border-strong)' },
  '.cm-foldPlaceholder': { backgroundColor: 'var(--bg-raised)', border: '1px solid var(--border)', color: 'var(--text-muted)' },
})

function editableExt(): Extension {
  return [EditorState.readOnly.of(props.readonly)]
}

async function languageFor(): Promise<Extension> {
  if (props.language === 'markdown') return markdown({ codeLanguages: languages })
  if (!props.filename) return []
  const desc = LanguageDescription.matchFilename(languages, props.filename)
  return desc ? await desc.load().catch(() => []) : []
}

onMounted(async () => {
  if (!host.value) return
  const state = EditorState.create({
    doc: props.modelValue,
    extensions: [
      lineNumbers(),
      highlightActiveLineGutter(),
      foldGutter(),
      history(),
      drawSelection(),
      indentOnInput(),
      bracketMatching(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      search({ top: true }),
      syntaxHighlighting(highlight),
      keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap, ...foldKeymap, indentWithTab]),
      props.wrap ? EditorView.lineWrapping : [],
      theme,
      editable.of(editableExt()),
      lang.of([]),
      EditorView.updateListener.of((u) => {
        if (u.docChanged) emit('update:modelValue', u.state.doc.toString())
      }),
    ],
  })
  view = new EditorView({ state, parent: host.value })
  view.dispatch({ effects: lang.reconfigure(await languageFor()) })
})

watch(
  () => props.modelValue,
  (v) => {
    if (view && v !== view.state.doc.toString()) view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: v } })
  },
)
watch(
  () => [props.filename, props.language],
  async () => view?.dispatch({ effects: lang.reconfigure(await languageFor()) }),
)
watch(
  () => props.readonly,
  () => view?.dispatch({ effects: editable.reconfigure(editableExt()) }),
)
onBeforeUnmount(() => view?.destroy())
</script>

<template>
  <div ref="host" class="code-editor" :style="{ height }" data-code-editor />
</template>

<style scoped>
.code-editor {
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  overflow: hidden;
}
.code-editor :deep(.cm-editor) {
  height: 100%;
}
</style>
