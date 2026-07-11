import { test, expect } from '@playwright/test';

const SLUG = /[A-Za-z0-9]{7}$/;

test('create a short link through the form and follow it', async ({ page, request, baseURL }) => {
  await page.goto('/');
  await page.fill('#url', 'https://example.com/e2e/target');
  await page.click('button[type=submit]');

  const link = page.locator('#short');
  await expect(link).toBeVisible();
  await expect(link).toHaveText(new RegExp(`^${baseURL}/[A-Za-z0-9]{7}$`));

  const href = await link.getAttribute('href');
  expect(href).toMatch(SLUG);
  const res = await request.get(href!, { maxRedirects: 0 });
  expect(res.status()).toBe(303);
  expect(res.headers()['location']).toBe('https://example.com/e2e/target');
});

test('QR code renders for a created link', async ({ page, request }) => {
  await page.goto('/');
  await page.fill('#url', 'https://example.com/e2e/qr');
  await page.click('button[type=submit]');

  const qr = page.locator('#qr');
  await expect(qr).toBeVisible();
  const src = await qr.getAttribute('src');
  expect(src).toMatch(/\/qr$/);
  const res = await request.get(src!);
  expect(res.status()).toBe(200);
  expect(res.headers()['content-type']).toContain('image/svg+xml');
  expect(await res.text()).toContain('<svg');
});

test('rejected URL shows the error message in the UI', async ({ page }) => {
  await page.goto('/');
  // passes the browser's type=url check but fails the server's scheme rule
  await page.fill('#url', 'ftp://example.com/file');
  await page.click('button[type=submit]');

  const error = page.locator('#error');
  await expect(error).toBeVisible();
  await expect(error).toContainText('scheme');
});

test('unknown slug returns a JSON 404', async ({ request }) => {
  const res = await request.get('/zzzzzzz', { maxRedirects: 0 });
  expect(res.status()).toBe(404);
  const body = await res.json();
  expect(body.error).toBeTruthy();
});

test('POST /api/valid contract', async ({ request }) => {
  const good = await request.post('/api/valid', { data: { url: 'https://ok.example/x' } });
  expect(good.status()).toBe(200);
  expect(await good.json()).toEqual({ valid: true });

  const bad = await request.post('/api/valid', { data: { url: 'not a url' } });
  expect(bad.status()).toBe(200);
  const body = await bad.json();
  expect(body.valid).toBe(false);
  expect(body.error).toBeTruthy();
});
