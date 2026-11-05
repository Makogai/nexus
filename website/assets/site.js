// NEXUS website: release data (release.json, written by the Pages workflow from the latest GitHub release), the
// visitor's platform, the screenshot tour and gentle reveal-on-scroll. No tracking, no third-party scripts.
;(function () {
  const REPO = 'https://github.com/Makogai/nexus'
  const FALLBACK = {
    version: '0.1.0',
    tag: 'v0.1.0',
    url: `${REPO}/releases/tag/v0.1.0`,
    assets: ['NEXUS_0.1.0_x64-setup.exe', 'NEXUS_0.1.0_aarch64.dmg', 'NEXUS_0.1.0_x64.dmg', 'NEXUS_0.1.0_amd64.AppImage', 'NEXUS_0.1.0_amd64.deb', 'SHA256SUMS.txt'].map((name) => ({
      name,
      size: null,
      url: `${REPO}/releases/download/v0.1.0/${name}`,
    })),
  }
  const PATTERNS = {
    windows: /-setup\.exe$/i,
    'mac-arm': /aarch64\.dmg$/i,
    'mac-intel': /x64\.dmg$/i,
    appimage: /\.AppImage$/i,
    deb: /\.deb$/i,
    sums: /^SHA256SUMS\.txt$/i,
  }

  const $$ = (sel) => Array.from(document.querySelectorAll(sel))
  const mb = (n) => (n ? `${(n / 1048576).toFixed(1)} MB` : '')

  function platform() {
    const p = (navigator.userAgentData?.platform || navigator.platform || navigator.userAgent || '').toLowerCase()
    if (p.includes('win')) return 'windows'
    if (p.includes('mac') || /iphone|ipad/.test(p)) return 'macos'
    if (p.includes('linux') || p.includes('x11')) return 'linux'
    return null
  }
  const LABEL = { windows: 'for Windows', macos: 'for macOS', linux: 'for Linux' }

  function apply(rel) {
    $$('[data-version]').forEach((el) => (el.textContent = `v${rel.version}`))
    for (const a of $$('[data-asset]')) {
      const asset = rel.assets.find((x) => PATTERNS[a.dataset.asset]?.test(x.name))
      if (!asset) {
        a.removeAttribute('href')
        a.setAttribute('aria-disabled', 'true')
        a.style.opacity = '0.45'
        continue
      }
      a.href = asset.url
      a.title = asset.name
      const size = a.querySelector('[data-size]')
      if (size) size.textContent = mb(asset.size)
    }
  }

  // Platform: highlight the visitor's card and label the hero button.
  const os = platform()
  if (os) {
    $$('[data-os-label]').forEach((el) => (el.textContent = LABEL[os]))
    const card = document.querySelector(`[data-platform="${os}"]`)
    if (card) {
      card.classList.add('current')
      card.querySelector('.you')?.removeAttribute('hidden')
      card.parentElement?.prepend(card)
    }
  }

  apply(FALLBACK)
  fetch('release.json', { cache: 'no-cache' })
    .then((r) => (r.ok ? r.json() : null))
    .then((rel) => rel && rel.assets?.length && apply(rel))
    .catch(() => undefined)

  // Screenshot tour.
  const SHOTS = {
    learn: ['Learn', 'NEXUS reads the repository itself — files, languages, docs, history — then a guided Claude session drafts the overview and architecture for you to accept.'],
    files: ['Files', 'A read-only explorer with syntax highlighting. NEXUS never changes your project, and files that may hold secrets are never shown.'],
    commits: ['Commits', 'The history of what was committed: changed files with renames and line counts, and every diff — without running anything from the repository.'],
    knowledge: ['Rules', 'Constitutions read like documentation: rule ids, mandatory policies and the gate that verifies each rule. Edit in place when something changes.'],
    constitutions: ['Constitutions', 'Workspace, stack, project and task rules with stable ids — only the small core is always on; the rest Claude reads when relevant.'],
  }
  const img = document.querySelector('[data-shot-img]')
  const title = document.querySelector('[data-shot-title]')
  const caption = document.querySelector('[data-shot-caption]')
  for (const b of $$('[data-shot]')) {
    const preload = new Image()
    preload.src = `assets/img/screen-${b.dataset.shot}.webp`
    b.addEventListener('click', () => {
      $$('[data-shot]').forEach((x) => x.setAttribute('aria-selected', String(x === b)))
      const [name, text] = SHOTS[b.dataset.shot]
      if (img) {
        img.src = `assets/img/screen-${b.dataset.shot}.webp`
        img.alt = `${name} in NEXUS`
      }
      if (title) title.textContent = `NEXUS — Storefront · ${name}`
      if (caption) caption.textContent = text
    })
  }

  // Reveal on scroll (CSS keeps everything visible when motion is reduced).
  const io = 'IntersectionObserver' in window ? new IntersectionObserver((entries) => entries.forEach((e) => e.isIntersecting && (e.target.classList.add('in'), io.unobserve(e.target))), { threshold: 0.12 }) : null
  $$('.reveal').forEach((el) => (io ? io.observe(el) : el.classList.add('in')))
})()
