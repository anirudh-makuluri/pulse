import {
  downloadUrl,
  releasesUrl,
  repositoryUrl,
  siteDescription,
  siteName,
} from "../seo";

export const dynamic = "force-static";

export function GET() {
  const body = `# ${siteName}

> ${siteDescription}

Pulse is a Windows desktop app that turns work activity from tools like Codex and Claude into a focused activity layer. It helps people see what is in progress, what needs attention, and what to do next while keeping session tracking private and local by default.

## Primary Pages

- [Home](/): Product overview and Windows download.

## Useful Links

- [Download Pulse for Windows](${downloadUrl})
- [Latest release](${releasesUrl})
- [Source repository](${repositoryUrl})

## Product Details

- Works with Codex and Claude today.
- Tracks source health and unfinished work across sessions.
- Presents focus, inbox, and continuation views for active work.
- More sources are planned.
`;

  return new Response(body, {
    headers: {
      "Content-Type": "text/plain; charset=utf-8",
      "Cache-Control": "public, max-age=3600, s-maxage=86400",
    },
  });
}
