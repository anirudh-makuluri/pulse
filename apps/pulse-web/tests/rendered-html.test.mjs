import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

const projectRoot = new URL("../", import.meta.url);

async function render() {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("test", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);

  return worker.fetch(
    new Request("http://localhost/", {
      headers: { accept: "text/html" },
    }),
    {
      ASSETS: {
        fetch: async () => new Response("Not found", { status: 404 }),
      },
    },
    {
      waitUntil() {},
      passThroughOnException() {},
    },
  );
}

test("server-renders the Pulse landing page", async () => {
  const response = await render();
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type") ?? "", /^text\/html\b/i);

  const html = await response.text();
  assert.match(html, /Pulse — The activity layer for your work/);
  assert.match(html, /The activity layer/);
  assert.match(html, /for your work\./);
  assert.match(html, /Download Pulse for Windows/);
  assert.match(html, /Works with Codex and Claude/);
  assert.match(html, /Pulse-Setup-x64\.exe/);
  assert.match(html, /Pulse activity dashboard preview/);
  assert.match(html, /Polish the onboarding experience/);
  assert.match(html, /lucide-house/);
  assert.match(html, /icon\.png/);
  assert.match(html, /aria-label="Footer"/);
  assert.match(html, /Built for work in motion/);
  assert.doesNotMatch(html, /Your site is taking shape|codex-preview/);
});

test("ships the required branded assets and a component mockup without starter preview code", async () => {
  const [packageJson, page, logo, forest, socialCard, favicon] = await Promise.all([
    readFile(new URL("../package.json", import.meta.url), "utf8"),
    readFile(new URL("../app/page.tsx", import.meta.url), "utf8"),
    access(new URL("../public/pulse-logo.png", import.meta.url)),
    access(new URL("../public/forest-night.png", import.meta.url)),
    access(new URL("../public/og.png", import.meta.url)),
    access(new URL("../app/icon.png", import.meta.url)),
  ]);

  assert.match(packageJson, /"name": "pulse-web"/);
  assert.doesNotMatch(packageJson, /react-loading-skeleton/);
  await assert.rejects(access(new URL("app/_sites-preview", projectRoot)));
  assert.equal(logo, undefined);
  assert.equal(forest, undefined);
  assert.equal(socialCard, undefined);
  assert.equal(favicon, undefined);
  assert.match(packageJson, /"lucide-react"/);
  assert.doesNotMatch(page, /pulse-dashboard\.png/);
});
