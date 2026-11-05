// Markdown for knowledge documents, rules and skills (W2). Workspace text is untrusted (it can arrive through Git sync,
// ADR-0020): raw HTML is never rendered — markdown-it with `html: false` escapes it — and markdown-it's link validation
// drops `javascript:`, `vbscript:`, `file:` and non-image `data:` URLs. Links never navigate the app window: the view
// opens http(s) links in the system browser, nothing else.
import MarkdownIt from 'markdown-it'

const md = new MarkdownIt({ html: false, linkify: true, typographer: false, breaks: false })

// Every link is marked so the view can intercept it; images are not loaded from the network (shown as their alt text).
const defaultLink = md.renderer.rules.link_open ?? ((tokens, idx, options, _env, self) => self.renderToken(tokens, idx, options))
md.renderer.rules.link_open = (tokens, idx, options, env, self) => {
  tokens[idx]?.attrSet('data-md-link', '1')
  return defaultLink(tokens, idx, options, env, self)
}
md.renderer.rules.image = (tokens, idx) => {
  const alt = tokens[idx]?.content || 'image'
  return `<span class="md-image" title="Images are not loaded">[${md.utils.escapeHtml(alt)}]</span>`
}

/** NEXUS rule markers and other HTML comments are not prose. */
export function stripComments(source: string): string {
  return source.replace(/<!--[\s\S]*?-->/g, '')
}

/** Safe HTML for a Markdown source (no raw HTML, no scripts, no remote images). */
export function renderMarkdown(source: string): string {
  return md.render(stripComments(source))
}

/** Inline Markdown (a rule's one-paragraph text): no wrapping <p>. */
export function renderInline(source: string): string {
  return md.renderInline(stripComments(source))
}

/** Whether a link may open (in the system browser). */
export function externalLink(href: string | null): string | null {
  if (!href) return null
  return /^https?:\/\//i.test(href) ? href : null
}
