import type { Metadata } from "next";
import { Geist } from "next/font/google";
import { headers } from "next/headers";
import "./globals.css";
import {
  getConfiguredSiteUrl,
  getSiteUrlFromHost,
  siteDescription,
  siteName,
  siteTitle,
} from "./seo";

const geist = Geist({
  variable: "--font-geist",
  subsets: ["latin"],
});

export async function generateMetadata(): Promise<Metadata> {
  const requestHeaders = await headers();
  const host =
    requestHeaders.get("x-forwarded-host") ?? requestHeaders.get("host");
  const protocol =
    requestHeaders.get("x-forwarded-proto") ??
    (host?.startsWith("localhost") ? "http" : "https");
  const origin = getConfiguredSiteUrl() ?? (host ? getSiteUrlFromHost(host, protocol) : null);
  const socialImage = origin ? `${origin}/og.png` : undefined;

  return {
    metadataBase: origin ? new URL(origin) : undefined,
    applicationName: siteName,
    title: siteTitle,
    description: siteDescription,
    keywords: [
      "Pulse",
      "AI work tracker",
      "activity layer",
      "Codex",
      "Claude",
      "task inbox",
      "desktop productivity",
    ],
    authors: [{ name: "Pulse" }],
    creator: "Pulse",
    publisher: "Pulse",
    category: "productivity",
    alternates: origin ? { canonical: origin } : undefined,
    icons: {
      icon: [{ url: "/icon.png", type: "image/png", sizes: "512x512" }],
      shortcut: "/icon.png",
      apple: "/icon.png",
    },
    openGraph: {
      type: "website",
      siteName,
      title: siteTitle,
      description: siteDescription,
      url: origin ?? undefined,
      images: socialImage
        ? [
            {
              url: socialImage,
              width: 1200,
              height: 630,
              alt: siteTitle,
            },
          ]
        : undefined,
    },
    twitter: {
      card: "summary_large_image",
      title: siteTitle,
      description: siteDescription,
      images: socialImage ? [socialImage] : undefined,
    },
    robots: {
      index: true,
      follow: true,
      googleBot: {
        index: true,
        follow: true,
        "max-image-preview": "large",
        "max-snippet": -1,
        "max-video-preview": -1,
      },
    },
  };
}

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body className={geist.variable}>{children}</body>
    </html>
  );
}
