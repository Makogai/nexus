---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Accessibility
status: active
applies: on-demand
prefix: A11Y
stacks: [angular, vue, react, svelte, nextjs, nuxt]
created: {{now}}
updated: {{now}}
source: import
---

Accessibility rules for web user interfaces, with WCAG 2.2 AA as the baseline. Apply them whenever you build or change pages, components, forms, dialogs, navigation or styles.

## Baseline and semantics

Native HTML semantics give assistive technology most of what it needs for free.

#### Meet WCAG 2.2 Level AA
<!-- rule: id=A11Y-1 enforcement=rule -->
All user-facing UI meets WCAG 2.2 Level AA, including the newer criteria on focus not obscured, consistent help, accessible authentication and redundant entry (target size and dragging: A11Y-6). Treat a violation as a defect with the same priority as a functional bug of similar impact.

#### Use semantic HTML first
<!-- rule: id=A11Y-2 enforcement=rule -->
Use native elements for their purpose: `<button>` for actions, `<a href>` for navigation, `<table>` for tabular data, landmarks (`<header>`, `<nav>`, `<main>`, `<footer>`) and one logical heading hierarchy per page. Never attach click handlers to a `<div>` or `<span>` to make it act as a control.

#### Use ARIA only when native semantics are insufficient
<!-- rule: id=A11Y-3 enforcement=rule -->
Add ARIA roles and attributes only for patterns HTML cannot express (tabs, combobox, tree, live regions), and implement that pattern's full keyboard behaviour from the ARIA Authoring Practices Guide. Never override a native element's role, and keep `aria-*` states in sync with the visual state.

## Keyboard, focus and pointer

Many people navigate with a keyboard, switch device or imprecise pointer, not a mouse.

#### Make everything operable by keyboard
<!-- rule: id=A11Y-4 enforcement=rule -->
Every interactive element is reachable with Tab in a logical order and operable with Enter, Space or arrow keys as its pattern expects, with no keyboard traps. Never use a positive `tabindex`; use `tabindex="0"` only for custom widgets and `-1` for programmatic focus targets.

#### Keep focus visible and manage it on route and dialog changes
<!-- rule: id=A11Y-5 enforcement=rule -->
Never remove focus outlines without an equally visible replacement (use `:focus-visible`). After a client-side route change, update the document title and move focus to the main heading. Dialogs (prefer native `<dialog>` with `showModal()`) contain focus, close on Escape and return focus to the trigger.

#### Make pointer targets at least 24 x 24 CSS pixels
<!-- rule: id=A11Y-6 enforcement=rule -->
Give pointer targets at least 24 x 24 CSS pixels, or enough spacing that a 24 px circle centred on each target does not overlap another one (WCAG 2.5.8). Aim for 44 x 44 for primary touch actions, and offer a single-pointer alternative to every drag interaction.

## Content and forms

Information and errors must reach users regardless of how they perceive the page.

#### Label every form control
<!-- rule: id=A11Y-7 enforcement=rule -->
Every input, select and textarea has a visible label programmatically associated with it (`<label for>` or a wrapping label); a placeholder is not a label. Group related controls with `<fieldset>` and `<legend>`, state required fields in text, and set `autocomplete` on personal data fields.

#### Make errors perceivable and announced
<!-- rule: id=A11Y-8 enforcement=rule -->
Show validation errors as text next to the field, link them with `aria-describedby` and set `aria-invalid="true"`. On submit, move focus to the first invalid field or an error summary; announce asynchronous status and errors through a live region. Error text says how to fix the problem.

#### Write meaningful text alternatives
<!-- rule: id=A11Y-9 enforcement=rule -->
Give informative images `alt` text that conveys their purpose in context, never the file name or "image of". Use `alt=""` for decorative images, give icon-only buttons an accessible name (visible text, visually hidden text or `aria-label`), and provide captions for video and transcripts for audio.

#### Meet contrast minimums and never rely on colour alone
<!-- rule: id=A11Y-10 enforcement=rule -->
Text has a contrast ratio of at least 4.5:1 (3:1 for large text: 24 px, or 18.66 px bold); UI component boundaries, focus indicators and meaningful graphics at least 3:1. Pair colour with text, icons or patterns for status, errors, required fields, links and chart series.

#### Respect reduced-motion preferences
<!-- rule: id=A11Y-11 enforcement=rule -->
Disable or reduce non-essential animation, parallax, smooth scrolling and autoplay when `prefers-reduced-motion: reduce` matches. Give anything that moves for more than five seconds a pause control, and never flash content more than three times per second.

## Verification

Automated checks catch only part of the issues, so they are combined with manual testing.

#### Test with axe automatically and list the manual checks
<!-- rule: id=A11Y-12 enforcement=rule check=test -->
Run axe-core checks in component or end-to-end tests (e.g. `@axe-core/playwright`, `vitest-axe`, `jest-axe`) for each page and key state, failing on violations. Key flows also need keyboard-only and screen-reader passes (NVDA or JAWS, VoiceOver); list them as open unless a person did them.
