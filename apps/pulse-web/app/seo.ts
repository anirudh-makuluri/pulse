export const siteName = "Pulse";
export const siteTitle = "Pulse - The activity layer for your work";
export const siteDescription =
  "Pulse turns activity across your tools into a clear view of what is in progress, what needs attention, and what to do next.";

export const repositoryUrl = "https://github.com/anirudh-makuluri/pulse";
export const releasesUrl = `${repositoryUrl}/releases/latest`;
export const downloadUrl =
  "https://github.com/anirudh-makuluri/pulse/releases/latest/download/Pulse-Setup-x64.exe";
export const productionSiteUrl = "https://pulse.makuluri.com";

export function normalizeSiteUrl(siteUrl: string) {
  return siteUrl.replace(/\/$/, "");
}

export function getConfiguredSiteUrl() {
  const siteUrl =
    process.env.NEXT_PUBLIC_SITE_URL ??
    process.env.SITE_URL ??
    process.env.VERCEL_PROJECT_PRODUCTION_URL ??
    process.env.VERCEL_URL ??
    productionSiteUrl;

  const protocol = siteUrl.startsWith("http") ? "" : "https://";
  return normalizeSiteUrl(`${protocol}${siteUrl}`);
}

export function getSiteUrlFromHost(host: string, protocol?: string | null) {
  const resolvedProtocol =
    protocol ?? (host.startsWith("localhost") ? "http" : "https");

  return normalizeSiteUrl(`${resolvedProtocol}://${host}`);
}
