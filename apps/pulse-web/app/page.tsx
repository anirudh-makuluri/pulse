import type { CSSProperties } from "react";
import { Database, House, Inbox, Minus, Settings, Square, X } from "lucide-react";
import Link from "next/link";
import { headers } from "next/headers";
import {
  downloadUrl,
  getConfiguredSiteUrl,
  getSiteUrlFromHost,
  releasesUrl,
  repositoryUrl,
  siteDescription,
  siteName,
} from "./seo";

const fireflies = [
  { x: "6%", y: "25%", size: "3px", duration: "15s", delay: "-4s", dx: "44px", dy: "-22px" },
  { x: "13%", y: "68%", size: "2px", duration: "12s", delay: "-8s", dx: "-28px", dy: "-38px" },
  { x: "19%", y: "43%", size: "4px", duration: "18s", delay: "-11s", dx: "36px", dy: "34px" },
  { x: "25%", y: "79%", size: "3px", duration: "14s", delay: "-2s", dx: "52px", dy: "-28px" },
  { x: "31%", y: "59%", size: "2px", duration: "16s", delay: "-7s", dx: "-34px", dy: "26px" },
  { x: "39%", y: "84%", size: "4px", duration: "19s", delay: "-13s", dx: "25px", dy: "-42px" },
  { x: "48%", y: "72%", size: "2px", duration: "13s", delay: "-6s", dx: "-40px", dy: "-24px" },
  { x: "58%", y: "81%", size: "3px", duration: "17s", delay: "-10s", dx: "38px", dy: "-32px" },
  { x: "68%", y: "61%", size: "2px", duration: "15s", delay: "-5s", dx: "27px", dy: "39px" },
  { x: "75%", y: "77%", size: "4px", duration: "18s", delay: "-14s", dx: "-46px", dy: "-25px" },
  { x: "82%", y: "45%", size: "3px", duration: "14s", delay: "-9s", dx: "34px", dy: "-35px" },
  { x: "91%", y: "66%", size: "2px", duration: "12s", delay: "-3s", dx: "-30px", dy: "24px" },
  { x: "96%", y: "31%", size: "4px", duration: "20s", delay: "-16s", dx: "-42px", dy: "31px" },
] as const;

const heroFireflies = [
  { x: "8%", y: "16%", size: "2px", duration: "18s", delay: "-10s", dx: "32px", dy: "28px" },
  { x: "14%", y: "39%", size: "2px", duration: "17s", delay: "-5s", dx: "26px", dy: "-34px" },
  { x: "24%", y: "28%", size: "2px", duration: "20s", delay: "-16s", dx: "-35px", dy: "24px" },
  { x: "32%", y: "43%", size: "3px", duration: "15s", delay: "-12s", dx: "30px", dy: "-27px" },
  { x: "42%", y: "20%", size: "2px", duration: "16s", delay: "-3s", dx: "-24px", dy: "31px" },
  { x: "55%", y: "44%", size: "2px", duration: "19s", delay: "-15s", dx: "34px", dy: "22px" },
  { x: "62%", y: "27%", size: "3px", duration: "14s", delay: "-7s", dx: "-28px", dy: "-24px" },
  { x: "72%", y: "36%", size: "2px", duration: "21s", delay: "-18s", dx: "38px", dy: "-18px" },
  { x: "80%", y: "22%", size: "2px", duration: "17s", delay: "-9s", dx: "-33px", dy: "30px" },
  { x: "87%", y: "34%", size: "2px", duration: "16s", delay: "-1s", dx: "24px", dy: "36px" },
  { x: "93%", y: "44%", size: "3px", duration: "19s", delay: "-14s", dx: "-36px", dy: "-20px" },
] as const;

type FireflyStyle = CSSProperties & {
  "--x": string;
  "--y": string;
  "--size": string;
  "--duration": string;
  "--delay": string;
  "--dx": string;
  "--dy": string;
};

export default async function Home() {
  const requestHeaders = await headers();
  const host =
    requestHeaders.get("x-forwarded-host") ?? requestHeaders.get("host");
  const protocol = requestHeaders.get("x-forwarded-proto");
  const siteUrl =
    getConfiguredSiteUrl() ??
    (host ? getSiteUrlFromHost(host, protocol) : "http://localhost:3000");
  const structuredData = {
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "WebSite",
        "@id": `${siteUrl}/#website`,
        name: siteName,
        url: siteUrl,
        description: siteDescription,
        inLanguage: "en-US",
      },
      {
        "@type": "SoftwareApplication",
        "@id": `${siteUrl}/#software`,
        name: siteName,
        applicationCategory: "ProductivityApplication",
        operatingSystem: "Windows",
        description: siteDescription,
        url: siteUrl,
        downloadUrl,
        softwareHelp: repositoryUrl,
        sameAs: [repositoryUrl, releasesUrl],
        image: `${siteUrl}/og.png`,
      },
      {
        "@type": "Organization",
        "@id": `${siteUrl}/#organization`,
        name: siteName,
        url: siteUrl,
        logo: `${siteUrl}/pulse-logo.png`,
      },
    ],
  };

  return (
    <main className="landing">
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }}
      />
      <div className="forest" aria-hidden="true" />
      <div className="atmosphere" aria-hidden="true" />
      <div className="fireflies" aria-hidden="true">
        {fireflies.map((firefly, index) => (
          <span
            className="firefly"
            key={index}
            style={
              {
                "--x": firefly.x,
                "--y": firefly.y,
                "--size": firefly.size,
                "--duration": firefly.duration,
                "--delay": firefly.delay,
                "--dx": firefly.dx,
                "--dy": firefly.dy,
              } as FireflyStyle
            }
          />
        ))}
      </div>

      <header className="site-header">
        <Link className="brand" href="/" aria-label="Pulse home">
          <img
            src="/pulse-logo.png"
            alt=""
            width={40}
            height={40}
          />
          <span>Pulse</span>
        </Link>
      </header>

      <section className="hero" aria-labelledby="hero-title">
        <div className="hero-fireflies" aria-hidden="true">
          {heroFireflies.map((firefly, index) => (
            <span
              className="firefly"
              key={index}
              style={
                {
                  "--x": firefly.x,
                  "--y": firefly.y,
                  "--size": firefly.size,
                  "--duration": firefly.duration,
                  "--delay": firefly.delay,
                  "--dx": firefly.dx,
                  "--dy": firefly.dy,
                } as FireflyStyle
              }
            />
          ))}
        </div>

        <div className="hero-copy">
          <p className="eyebrow">Your work, kept in view</p>
          <h1 id="hero-title">
            The activity layer
            <span>for your work.</span>
          </h1>
          <p className="hero-description">
            Pulse turns activity across your tools into a clear view of
            what&apos;s in progress, what needs attention, and what to do next.
          </p>

          <div className="hero-actions">
            <a className="download-button" href={downloadUrl}>
              <span className="windows-mark" aria-hidden="true">
                <i />
                <i />
                <i />
                <i />
              </span>
              Download Pulse for Windows
            </a>
            <p className="source-note">
              <span>Works with Codex and Claude</span>
              <i aria-hidden="true" />
              <span>More sources coming soon</span>
            </p>
          </div>
        </div>

        <div className="product-stage">
          <div className="product-glow" aria-hidden="true" />
          <div className="pulse-mockup" aria-label="Pulse activity dashboard preview">
            <div className="mock-titlebar">
              <div className="mock-app-name">
                <img src="/pulse-logo.png" alt="" width={24} height={24} />
                <span>Pulse</span>
              </div>
              <div className="mock-window-controls" aria-hidden="true">
                <Minus />
                <Square />
                <X />
              </div>
            </div>

            <div className="mock-content">
              <aside className="mock-sidebar" aria-label="Pulse sections">
                <div className="mock-nav-item is-active"><House aria-hidden="true" />Home</div>
                <div className="mock-nav-item"><Inbox aria-hidden="true" />Inbox</div>
                <div className="mock-nav-item"><Database aria-hidden="true" />Sources</div>
                <p className="mock-nav-label">System</p>
                <div className="mock-nav-item"><Settings aria-hidden="true" />Settings</div>
                <div className="mock-sidebar-actions">
                  <button type="button">Capture task</button>
                  <button type="button">Sync latest sessions</button>
                </div>
              </aside>

              <div className="mock-main">
                <div className="mock-home-header">
                  <div>
                    <p className="mock-kicker">Home</p>
                    <p className="mock-subtitle">Stay on top of what needs your attention.</p>
                  </div>
                  <button className="mock-primary" type="button">Open inbox</button>
                </div>

                <section className="mock-focus-card">
                  <div className="mock-card-heading">
                    <div>
                      <h2>Focus now</h2>
                      <p>Today&apos;s work and sessions that are still in progress.</p>
                    </div>
                    <button type="button">View today</button>
                  </div>
                  <div className="mock-task is-featured">
                    <strong>Polish the onboarding experience</strong>
                    <div className="mock-pills"><span>Today</span><span className="source-codex">codex</span><span className="outcome-progress">In progress</span><span>pulse</span></div>
                    <small>Review the welcome screen and confirm every empty state.</small>
                  </div>
                  <div className="mock-task mock-task-secondary">
                    <strong>Prepare the Windows beta release</strong>
                    <div className="mock-pills"><span>Today</span><span className="source-claude">claude</span><span className="outcome-progress">In progress</span><span>launch</span></div>
                    <small>Run the installer once on a clean Windows machine.</small>
                  </div>
                </section>

                <div className="mock-lower-grid">
                  <section className="mock-small-card">
                    <div className="mock-card-heading">
                      <div><h2>Needs triage</h2><p>1 task is waiting in Inbox.</p></div>
                      <button type="button">Review</button>
                    </div>
                    <div className="mock-task"><strong>Review the reminder notification copy</strong><div className="mock-pills"><span>Inbox</span><span className="source-claude">claude</span><span>desktop</span></div></div>
                  </section>
                  <section className="mock-small-card mock-continue-card">
                    <div className="mock-card-heading">
                      <div><h2>Continue working</h2><p>Recently updated unfinished tasks.</p></div>
                      <button type="button">View all</button>
                    </div>
                    <div className="mock-task"><strong>Design the weekly progress view</strong><div className="mock-pills"><span>Next</span><span className="source-claude">claude</span><span>pulse</span></div><small>Turn the approved layout into a first working prototype.</small></div>
                    <div className="mock-task mock-task-secondary"><strong>Add keyboard shortcut hints</strong><div className="mock-pills"><span>Next</span><span className="source-codex">codex</span><span>desktop</span></div></div>
                  </section>
                </div>

                <section className="mock-source-card">
                  <div className="mock-card-heading">
                    <div><h2>Source health</h2><p>Session tracking is private and local by default.</p></div>
                    <button type="button">Manage</button>
                  </div>
                  <div className="mock-source-statuses"><span><i />Claude <b>Watching</b></span><span><i />Codex <b>Watching</b></span></div>
                </section>
              </div>
            </div>
          </div>
        </div>
      </section>

      <footer className="site-footer">
        <div className="footer-inner">
          <div className="footer-identity">
            <div className="footer-brand">
              <img
                src="/pulse-logo.png"
                alt=""
                width={28}
                height={28}
              />
              <span>Pulse</span>
            </div>
            <p>The activity layer for your work.</p>
          </div>

          <nav className="footer-links" aria-label="Footer">
            <a href={repositoryUrl}>GitHub</a>
            <a href={releasesUrl}>Releases</a>
            <a href={downloadUrl}>Download</a>
          </nav>

          <p className="footer-meta">&copy; 2026 Pulse. Built for work in motion.</p>
        </div>
      </footer>
    </main>
  );
}
