import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

const projectRoot = new URL("../", import.meta.url);

test("builds as a Next app for Vercel", async () => {
  await access(new URL("../.next/BUILD_ID", import.meta.url));
  await access(new URL("../.next/server/app/page.js", import.meta.url));
  await access(new URL("../.next/static", import.meta.url));
});

test("ships the branded landing page without Cloudflare deployment code", async () => {
  const [packageJson, page, layout, logo, forest, socialCard, favicon] =
    await Promise.all([
      readFile(new URL("../package.json", import.meta.url), "utf8"),
      readFile(new URL("../app/page.tsx", import.meta.url), "utf8"),
      readFile(new URL("../app/layout.tsx", import.meta.url), "utf8"),
      access(new URL("../public/pulse-logo.png", import.meta.url)),
      access(new URL("../public/forest-night.png", import.meta.url)),
      access(new URL("../public/og.png", import.meta.url)),
      access(new URL("../app/icon.png", import.meta.url)),
    ]);

  assert.match(packageJson, /"name": "pulse-web"/);
  assert.match(packageJson, /"build": "next build"/);
  assert.doesNotMatch(packageJson, /cloudflare|wrangler|vinext|vite/i);
  assert.doesNotMatch(packageJson, /react-loading-skeleton/);
  await assert.rejects(access(new URL("app/_sites-preview", projectRoot)));
  await assert.rejects(access(new URL("worker/index.ts", projectRoot)));
  await assert.rejects(access(new URL("vite.config.ts", projectRoot)));
  await assert.rejects(access(new URL(".openai/hosting.json", projectRoot)));
  assert.equal(logo, undefined);
  assert.equal(forest, undefined);
  assert.equal(socialCard, undefined);
  assert.equal(favicon, undefined);
  assert.match(packageJson, /"lucide-react"/);
  assert.match(page, /Download Pulse for Windows/);
  assert.match(page, /Pulse activity dashboard preview/);
  assert.doesNotMatch(page, /pulse-dashboard\.png/);
  assert.match(layout, /Pulse - The activity layer for your work/);
});
