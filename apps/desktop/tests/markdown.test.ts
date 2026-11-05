import { describe, expect, it } from 'vitest'
import { externalLink, renderInline, renderMarkdown, stripComments } from '../src/lib/markdown'

// Workspace documents are untrusted (they can arrive through Git sync): rendering must never produce live HTML.
describe('markdown rendering of untrusted documents', () => {
  it('formats ordinary Markdown', () => {
    const html = renderMarkdown('## Why\n\nTests **first**, then `code`.\n\n- one\n- two')
    expect(html).toContain('<h2>Why</h2>')
    expect(html).toContain('<strong>first</strong>')
    expect(html).toContain('<code>code</code>')
    expect(html).toContain('<li>one</li>')
  })

  it('escapes raw HTML instead of rendering it', () => {
    const html = renderMarkdown('<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n<a href="javascript:alert(1)">x</a>')
    expect(html).not.toMatch(/<script|<img|<a href="javascript/i)
    expect(html).toContain('&lt;script&gt;')
  })

  it('drops dangerous link targets and never loads remote images', () => {
    expect(renderMarkdown('[x](javascript:alert(1))')).not.toContain('href="javascript')
    expect(renderMarkdown('[x](file:///C:/Windows/system.ini)')).not.toContain('href="file:')
    const img = renderMarkdown('![tracker](https://example.com/pixel.png)')
    expect(img).not.toContain('<img')
    expect(img).toContain('[tracker]')
  })

  it('marks links so the view opens only http(s) in the system browser', () => {
    expect(renderMarkdown('[docs](https://example.com)')).toContain('data-md-link="1"')
    expect(externalLink('https://example.com/a')).toBe('https://example.com/a')
    expect(externalLink('#top')).toBeNull()
    expect(externalLink('mailto:a@example.invalid')).toBeNull()
  })

  it('hides NEXUS rule markers and renders a rule text inline', () => {
    expect(stripComments('a <!-- rule: id=X-1 enforcement=rule --> b')).toBe('a  b')
    expect(renderInline('Keep **small**.')).toBe('Keep <strong>small</strong>.')
  })
})
