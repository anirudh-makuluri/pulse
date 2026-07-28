import type { MetadataRoute } from "next";
import { headers } from "next/headers";
import { getConfiguredSiteUrl, getSiteUrlFromHost } from "./seo";

export default async function robots(): Promise<MetadataRoute.Robots> {
  const requestHeaders = await headers();
  const host =
    requestHeaders.get("x-forwarded-host") ?? requestHeaders.get("host");
  const protocol = requestHeaders.get("x-forwarded-proto");
  const siteUrl =
    getConfiguredSiteUrl() ??
    (host ? getSiteUrlFromHost(host, protocol) : "http://localhost:3000");

  return {
    rules: {
      userAgent: "*",
      allow: "/",
      disallow: ["/api/"],
    },
    sitemap: `${siteUrl}/sitemap.xml`,
    host: siteUrl,
  };
}
