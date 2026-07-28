import type { MetadataRoute } from "next";
import { headers } from "next/headers";
import { getConfiguredSiteUrl, getSiteUrlFromHost } from "./seo";

export default async function sitemap(): Promise<MetadataRoute.Sitemap> {
  const requestHeaders = await headers();
  const host =
    requestHeaders.get("x-forwarded-host") ?? requestHeaders.get("host");
  const protocol = requestHeaders.get("x-forwarded-proto");
  const siteUrl =
    getConfiguredSiteUrl() ??
    (host ? getSiteUrlFromHost(host, protocol) : "http://localhost:3000");

  return [
    {
      url: siteUrl,
      lastModified: new Date("2026-07-28"),
      changeFrequency: "weekly",
      priority: 1,
    },
  ];
}
