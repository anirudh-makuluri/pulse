import type { CSSProperties } from "react";
import {
  Activity,
  ArrowRight,
  ArrowUp,
  Brain,
  Cloud,
  Database,
  Eye,
  HardDrive,
  History,
  House,
  Inbox,
  Layers3,
  LockKeyhole,
  Minus,
  Search,
  Settings,
  ShieldCheck,
  Sparkles,
  Square,
  Workflow,
  X,
} from "lucide-react";
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
                <div className="mock-nav-item mock-nav-home"><House aria-hidden="true" />Home</div>
                <div className="mock-nav-item"><Inbox aria-hidden="true" />Inbox</div>
                <div className="mock-nav-item mock-nav-copilot"><Sparkles aria-hidden="true" />Task Copilot</div>
                <div className="mock-nav-item"><Database aria-hidden="true" />Sources</div>
                <p className="mock-nav-label">System</p>
                <div className="mock-nav-item"><Settings aria-hidden="true" />Settings</div>
                <div className="mock-sidebar-actions">
                  <button type="button">Capture task</button>
                  <button className="mock-session-sync" type="button">
                    <span className="mock-syncing"><i aria-hidden="true" />Syncing sessions...</span>
                    <span className="mock-sync-ready">Sync latest sessions</span>
                  </button>
                </div>
              </aside>

              <div className="mock-demo-main">
              <div className="mock-main mock-home-screen">
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
                  <div className="mock-task mock-demo-task mock-demo-task-one is-featured">
                    <strong>Polish the onboarding experience</strong>
                    <div className="mock-pills"><span className="mock-status"><b>Today</b><b>Done</b></span><span className="source-codex">codex</span><span className="outcome-progress mock-outcome"><b>In progress</b><b>Completed</b></span><span>pulse</span></div>
                    <small>Review the welcome screen and confirm every empty state.</small>
                  </div>
                  <div className="mock-task mock-demo-task mock-demo-task-two mock-task-secondary">
                    <strong>Prepare the Windows beta release</strong>
                    <div className="mock-pills"><span className="mock-status"><b>Today</b><b>Done</b></span><span className="source-claude">claude</span><span className="outcome-progress mock-outcome"><b>In progress</b><b>Completed</b></span><span>launch</span></div>
                    <small>Run the installer once on a clean Windows machine.</small>
                  </div>
                </section>

                <div className="mock-lower-grid">
                  <section className="mock-small-card">
                    <div className="mock-card-heading">
                      <div><h2>Task Copilot</h2><p>Ask about priorities and blockers, or add and update tasks.</p></div>
                      <Sparkles className="mock-copilot-icon" aria-hidden="true" />
                    </div>
                    <div className="mock-copilot-prompt"><span>What should I work on today?</span><button type="button">Ask</button></div>
                    <p className="mock-copilot-note">Uses your local tasks to help you move work forward.</p>
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
                    <div><h2>Source health</h2><p>Source tracking is private and local by default.</p></div>
                    <button type="button">Manage</button>
                  </div>
                  <div className="mock-source-statuses"><span><i />Claude <b>Watching</b></span><span><i />Codex <b>Watching</b></span></div>
                </section>
              </div>
              <section className="mock-copilot-screen" aria-label="Task Copilot preview">
                <div className="mock-copilot-top"><button type="button"><History aria-hidden="true" />History</button></div>
                <div className="mock-copilot-welcome">
                  <div className="mock-copilot-welcome-copy">
                    <p>Task Copilot</p>
                    <h2>What can I help you move forward?</h2>
                    <span>Ask about your tasks, priorities, or blockers.</span>
                  </div>
                  <div className="mock-copilot-composer">
                    <div><span className="mock-typed-question">What should I focus on next?</span><i aria-hidden="true" /></div>
                    <button type="button"><ArrowUp aria-hidden="true" /></button>
                  </div>
                  <div className="mock-copilot-answer">
                    <div className="mock-copilot-user-message">What should I focus on next?</div>
                    <div className="mock-copilot-answer-card">
                      <div><strong>Task Copilot</strong><span>Powered locally</span></div>
                      <p>Finish the Windows beta release next. It&apos;s already in progress and has a clear next action: run the installer on a clean Windows machine.</p>
                      <small>Supporting task&nbsp; <b>Prepare the Windows beta release</b></small>
                    </div>
                  </div>
                </div>
              </section>
              </div>
            </div>
            <div className="mock-demo-cursor" aria-hidden="true"><span>➤</span><i /></div>
          </div>
        </div>
      </section>

      <div className="content-sections">
        <section className="product-story section-shell" aria-labelledby="product-story-title">
          <div className="section-heading">
            <p className="section-kicker">Work with continuity</p>
            <h2 id="product-story-title">
              Stop reconstructing your day.
              <span>Let Pulse keep the thread.</span>
            </h2>
            <p>
              Pulse brings the useful signals from your work into one calm,
              structured workspace—so context survives the tab, chat, and tool
              where it started.
            </p>
          </div>

          <div className="feature-grid">
            <article className="feature-card">
              <div className="feature-icon"><Eye aria-hidden="true" /></div>
              <p className="feature-number">01</p>
              <h3>See work as it happens</h3>
              <p>
                Enable the sources you want—Codex, Claude, and Brave—and bring
                recent activity into a single view with its supporting evidence.
              </p>
              <span className="feature-detail"><Activity aria-hidden="true" />Source-aware activity</span>
            </article>

            <article className="feature-card feature-card-raised">
              <div className="feature-icon"><Layers3 aria-hidden="true" /></div>
              <p className="feature-number">02</p>
              <h3>Turn motion into next actions</h3>
              <p>
                Move captured work through Inbox, Today, Next, Waiting, and
                Done while keeping outcomes, reminders, and provenance attached.
              </p>
              <span className="feature-detail"><Workflow aria-hidden="true" />A workflow that stays legible</span>
            </article>

            <article className="feature-card">
              <div className="feature-icon"><Brain aria-hidden="true" /></div>
              <p className="feature-number">03</p>
              <h3>Ask what matters now</h3>
              <p>
                Task Copilot answers from your actual work, cites the tasks
                behind its response, and can make bounded updates when you ask.
              </p>
              <span className="feature-detail"><Search aria-hidden="true" />Grounded in your context</span>
            </article>
          </div>
        </section>

        <section className="continuity-section section-shell" aria-labelledby="continuity-title">
          <div className="continuity-copy">
            <p className="section-kicker">A quieter way to resume</p>
            <h2 id="continuity-title">Pick up where the work left off.</h2>
            <p>
              Pulse keeps the trail between an activity and its next action.
              Open the app after a meeting, a coding session, or a long break
              and see what changed, what is unfinished, and why it matters.
            </p>
            <ul className="continuity-list">
              <li><ShieldCheck aria-hidden="true" /><span><strong>Evidence stays attached.</strong> Know which source and session a task came from.</span></li>
              <li><ShieldCheck aria-hidden="true" /><span><strong>Progress stays visible.</strong> Separate workflow status from the outcome you observed.</span></li>
              <li><ShieldCheck aria-hidden="true" /><span><strong>History stays useful.</strong> Search the timeline instead of replaying every conversation.</span></li>
            </ul>
          </div>

          <div className="continuity-visual" aria-label="Activity becoming an actionable task">
            <div className="signal-card signal-source">
              <span className="signal-label">Observed activity</span>
              <div><i className="signal-dot" /><strong>Codex session updated</strong><small>Release validation · 8 minutes ago</small></div>
            </div>
            <div className="signal-connector"><span /><ArrowRight aria-hidden="true" /></div>
            <div className="signal-card signal-task">
              <span className="signal-label">Next action</span>
              <div className="signal-task-top"><span>Today</span><span>codex</span></div>
              <strong>Verify the Windows installer</strong>
              <p>Run the latest build once on a clean machine.</p>
              <div className="signal-progress"><i /><span>In progress</span></div>
            </div>
          </div>
        </section>

        <section className="cloud-section section-shell" aria-labelledby="cloud-title">
          <div className="cloud-heading">
            <div>
              <p className="section-kicker">Local first. Durable when you choose.</p>
              <h2 id="cloud-title">Built with CockroachDB and AWS for memory that lasts.</h2>
            </div>
            <p>
              Pulse works from local SQLite without the cloud. When you enable
              sync, approved structured memory can travel through an authenticated
              AWS path into CockroachDB for durable, semantic retrieval.
            </p>
          </div>

          <div className="architecture-flow">
            <article className="architecture-card architecture-local">
              <div className="architecture-icon"><HardDrive aria-hidden="true" /></div>
              <span className="architecture-tag">On your Windows PC</span>
              <h3>Private local workspace</h3>
              <p>Tasks, sources, reminders, conversations, and evidence live in SQLite by default.</p>
              <div className="architecture-meta"><LockKeyhole aria-hidden="true" />Useful without cloud sync</div>
            </article>

            <div className="architecture-arrow" aria-hidden="true"><span>Opt-in sync</span><ArrowRight /></div>

            <article className="architecture-card architecture-aws">
              <div className="architecture-icon"><Cloud aria-hidden="true" /></div>
              <span className="architecture-tag">AWS</span>
              <h3>Authenticated durability path</h3>
              <p>API Gateway and Lambda validate approved memory, while private versioned S3 archives approved payloads.</p>
              <div className="architecture-meta"><ShieldCheck aria-hidden="true" />Credentials stay out of the UI</div>
            </article>

            <div className="architecture-arrow" aria-hidden="true"><span>Structured memory</span><ArrowRight /></div>

            <article className="architecture-card architecture-cockroach">
              <div className="architecture-icon"><Database aria-hidden="true" /></div>
              <span className="architecture-tag">CockroachDB</span>
              <h3>Searchable agentic memory</h3>
              <p>Distributed vector indexing makes approved activities and decisions retrievable across sessions.</p>
              <div className="architecture-meta"><Search aria-hidden="true" />Semantic search with VECTOR(384)</div>
            </article>
          </div>

          <div className="cloud-proof">
            <span><Database aria-hidden="true" /><strong>CockroachDB</strong> durable structured memory + vector search</span>
            <i aria-hidden="true" />
            <span><Cloud aria-hidden="true" /><strong>AWS</strong> secure sync, validation, and private archives</span>
          </div>
        </section>

        <section className="privacy-section section-shell" aria-labelledby="privacy-title">
          <div className="privacy-mark"><LockKeyhole aria-hidden="true" /></div>
          <div className="privacy-copy">
            <p className="section-kicker">Private by default</p>
            <h2 id="privacy-title">Your work history is not the price of admission.</h2>
            <p>
              Sources start off, cloud sync is optional, and raw transcripts or
              local files are never uploaded automatically. Pulse stays useful
              as a local Windows app before you connect anything else.
            </p>
          </div>
          <div className="privacy-points">
            <span><i />Local SQLite core</span>
            <span><i />Opt-in sources</span>
            <span><i />Approved cloud memory only</span>
          </div>
        </section>

        <section className="final-cta section-shell" aria-labelledby="cta-title">
          <div className="cta-glow" aria-hidden="true" />
          <img src="/pulse-logo.png" alt="" width={56} height={56} />
          <p className="section-kicker">Keep the thread</p>
          <h2 id="cta-title">Give your work a memory.</h2>
          <p>Start with a private local workspace. Add cloud continuity only when you want it.</p>
          <a className="download-button" href={downloadUrl}>
            <span className="windows-mark" aria-hidden="true"><i /><i /><i /><i /></span>
            Download Pulse for Windows
          </a>
          <a className="text-link" href={repositoryUrl}>Explore the open-source project <ArrowRight aria-hidden="true" /></a>
        </section>
      </div>

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
