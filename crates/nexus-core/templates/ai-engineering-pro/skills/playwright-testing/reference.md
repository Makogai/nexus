# Playwright reference

TypeScript examples for `@playwright/test`. Adapt names to the project; keep its existing conventions when they
differ.

## Locators

```ts
await page.getByRole('button', { name: 'Save' }).click();
await page.getByLabel('Email').fill('user@example.test');
await page.getByRole('link', { name: 'Invoices' }).click();

// Scope to a container instead of relying on position.
const row = page.getByRole('row').filter({ hasText: 'Invoice 42' });
await row.getByRole('button', { name: 'Delete' }).click();

// Test id as a fallback; the attribute name is configurable with use.testIdAttribute.
await page.getByTestId('chart-legend').click();
```

Avoid `nth()`, `first()` and long CSS chains unless order is the behaviour under test: they break when the layout
changes and hide ambiguous locators that strict mode would otherwise report.

## Web-first assertions

```ts
await expect(page.getByRole('heading', { level: 1 })).toHaveText('Invoices');
await expect(page).toHaveURL(/\/invoices\/\d+$/);
await expect(page.getByRole('alert')).toContainText('Saved');
await expect(page.getByRole('listitem')).toHaveCount(3);
await expect(page.getByRole('button', { name: 'Submit' })).toBeEnabled();

// Conditions outside the DOM.
await expect.poll(async () => (await request.get('/api/jobs/7')).status()).toBe(200);
await expect(async () => {
  const res = await request.get('/api/orders/7');
  expect((await res.json()).state).toBe('shipped');
}).toPass({ timeout: 10_000 });
```

## Data through fixtures

```ts
// fixtures.ts
import { test as base, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

type Project = { id: string; name: string };

export const test = base.extend<{ project: Project }>({
  project: async ({ request }, use) => {
    const res = await request.post('/api/test/projects', { data: { name: `e2e-${randomUUID()}` } });
    await expect(res).toBeOK();
    const project: Project = await res.json();
    await use(project);
    await request.delete(`/api/test/projects/${project.id}`);
  },
});
export { expect };
```

```ts
import { test, expect } from './fixtures';

test('owner can rename a project', async ({ page, project }) => {
  await page.goto(`/projects/${project.id}`);
  await page.getByRole('button', { name: 'Rename' }).click();
  await page.getByLabel('Project name').fill('Renamed');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Renamed');
});
```

Unique names (UUID or `test.info().parallelIndex`) keep parallel workers from colliding.

## Authentication with a setup project

```ts
// auth.setup.ts
import { test as setup, expect } from '@playwright/test';

const authFile = 'playwright/.auth/user.json'; // directory listed in .gitignore

setup('sign in', async ({ page }) => {
  await page.goto('/login');
  await page.getByLabel('Email').fill(process.env.E2E_USER ?? '');
  await page.getByLabel('Password').fill(process.env.E2E_PASSWORD ?? '');
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page.getByRole('navigation', { name: 'Main' })).toBeVisible();
  await page.context().storageState({ path: authFile });
});
```

```ts
// playwright.config.ts (projects excerpt)
projects: [
  { name: 'setup', testMatch: /.*\.setup\.ts/ },
  {
    name: 'chromium',
    use: { ...devices['Desktop Chrome'], storageState: 'playwright/.auth/user.json' },
    dependencies: ['setup'],
  },
],
```

When tests change account state (settings, roles), give each worker its own account, selected by
`test.info().parallelIndex`.

## Network and time

```ts
// Stub a third-party service only.
await page.route('**/payments.example.test/**', (route) =>
  route.fulfill({ status: 200, json: { status: 'approved' } }),
);

// Freeze the clock before the page reads the date.
await page.clock.setFixedTime(new Date('2026-01-15T10:00:00Z'));
await page.goto('/reports');
```

## Popups and downloads

```ts
const popupPromise = page.waitForEvent('popup');
await page.getByRole('link', { name: 'Open preview' }).click();
const popup = await popupPromise;
await expect(popup).toHaveTitle(/Preview/);

const downloadPromise = page.waitForEvent('download');
await page.getByRole('button', { name: 'Export CSV' }).click();
const download = await downloadPromise;
expect(download.suggestedFilename()).toMatch(/\.csv$/);
```

Start waiting before the action that triggers the event; awaiting afterwards can miss it.

## CI-oriented configuration

```ts
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 2 : undefined,
  reporter: process.env.CI ? [['html', { open: 'never' }], ['github']] : 'html',
  use: {
    baseURL: process.env.BASE_URL ?? 'http://localhost:3000',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    locale: 'en-GB',
    timezoneId: 'UTC',
  },
  webServer: {
    command: 'npm run start',
    url: 'http://localhost:3000',
    reuseExistingServer: !process.env.CI,
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
```

Retries in CI keep the pipeline moving but a test that passes on retry is still flaky: the report marks it, and it
needs a fix or a tracked quarantine.

## Flake diagnosis checklist

1. **Reproduce.** `npx playwright test <file> -g "<title>" --repeat-each=20`, then again with the CI worker count
   (`--workers=<n>`) and the CI project. Note the failure rate.
2. **Read the trace** of a failing run: the action log, DOM snapshots before and after the failing step, network
   requests and console. Ask: what did the page show at the moment the step ran?
3. **Check the usual causes:**
   - Acting before the page is ready: assert the loaded state first (a heading, a row count), then act.
   - Non-retrying assertions: `expect(await ...)` on a value read once; switch to a locator assertion.
   - Ambiguous locator: matches several elements or a different one after re-render; add role, name, `filter()`
     or a container scope.
   - Navigation races: after a click that navigates, assert with `toHaveURL` before interacting further.
   - Shared data: two tests or workers using the same record or account; make data unique per test.
   - Order dependence: the test passes only after another test; run it alone (`-g`) and with `--workers=1`.
   - Time: dates, timers, animations or debounce; control with `page.clock` or assert the end state.
   - External services: real third-party calls; stub them with `page.route`.
   - Eventual consistency in the backend: poll with `expect.poll` or `toPass` with a bounded timeout.
   - Environment differences: viewport, locale, timezone, fonts; set them explicitly in the config.
   - Resource starvation on CI: too many workers for the runner; lower the worker count and compare.
   - Visual comparisons (`toHaveScreenshot`): snapshots differ across operating systems and browsers; generate
     them in the same environment that runs the assertions.
4. **Fix the cause**, then confirm with `--repeat-each=20` again. Never fix with `waitForTimeout`, larger timeouts,
   `force: true` clicks or more retries.
5. **Serial mode** (`test.describe.configure({ mode: 'serial' })`) hides isolation problems; use it only for a
   journey that is genuinely one sequence, and say so in a comment.
