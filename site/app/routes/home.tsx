import type { Route } from "./+types/home";
import { Divider, Rails } from "../components/frame";
import { Hero, Nav } from "../components/hero";
import { Agents, Case, Examples, FAQ, Footer, GetIt, Operations, Plans } from "../components/sections";
import { DESCRIPTION, FAQS, GH, HOSTED, LICENCE, RELEASES, SITE, TITLE, WEB } from "../content";

// Crawlers fetch the card from an absolute URL, so it names the deployed site rather than BASE_URL.
const CARD = `${SITE}og.png`;
const CARD_ALT =
  "ParCAD: open-source CAD your AI can check. The example bracket, rendered by ParCAD's kernel with each face coloured by the feature that built it.";

export const meta: Route.MetaFunction = () => [
  { title: TITLE },
  { name: "description", content: DESCRIPTION },
  { property: "og:type", content: "website" },
  { property: "og:site_name", content: "ParCAD" },
  { property: "og:locale", content: "en_US" },
  { property: "og:url", content: SITE },
  { property: "og:title", content: TITLE },
  { property: "og:description", content: DESCRIPTION },
  { property: "og:image", content: CARD },
  { property: "og:image:type", content: "image/png" },
  { property: "og:image:width", content: "1200" },
  { property: "og:image:height", content: "630" },
  { property: "og:image:alt", content: CARD_ALT },
  { name: "twitter:card", content: "summary_large_image" },
  { name: "twitter:image", content: CARD },
  { name: "twitter:image:alt", content: CARD_ALT },
  { tagName: "link", rel: "canonical", href: SITE },
  { tagName: "link", rel: "alternate", type: "text/markdown", href: `${SITE}index.html.md` },
  {
    "script:ld+json": {
      "@context": "https://schema.org",
      // Google takes the site's name in a search result from WebSite.
      "@graph": [
        { "@type": "WebSite", name: "ParCAD", url: SITE },
        {
          "@type": "SoftwareApplication",
          "@id": `${SITE}#software`,
          name: "ParCAD",
          alternateName: "parcad",
          description: DESCRIPTION,
          url: SITE,
          image: CARD,
          applicationCategory: "DesignApplication",
          applicationSubCategory: "CAD",
          operatingSystem: "macOS, Linux, Windows, Web",
          downloadUrl: RELEASES,
          installUrl: WEB,
          license: `https://spdx.org/licenses/${LICENCE}-or-later.html`,
          isAccessibleForFree: true,
          sameAs: GH,
          keywords: "CAD MCP server, AI CAD, text to CAD, parametric CAD, JavaScript CAD, OpenCASCADE, 3D printing, STEP, 3MF",
          featureList: [
            `A hosted MCP server at ${HOSTED} with GitHub or Google sign-in`,
            "Works with Claude, ChatGPT, Cursor, Codex and other MCP clients",
            "Parts written in JavaScript and built on an exact OpenCASCADE B-rep kernel",
            "Measured reports: size, volume, wall thickness, fit, and clearance of moving parts",
            "STEP, 3MF and STL export",
            "Runs in the browser, as a desktop app, or headless",
          ],
          publisher: { "@type": "Organization", name: "Maging Studio", url: SITE },
          offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
        },
        {
          "@type": "FAQPage",
          mainEntity: FAQS.map((f) => ({
            "@type": "Question",
            name: f.q,
            acceptedAnswer: { "@type": "Answer", text: f.a.join(" ") },
          })),
        },
      ],
    },
  },
];

export default function Home() {
  return (
    <div className="relative min-h-svh">
      <Rails />
      <Nav />
      <main className="relative">
        <Hero />
        <GetIt />
        <Divider hue="gold" />
        <Case />
        <Divider hue="tag" />
        <Agents />
        <Divider hue="cut" />
        <Plans />
        <Divider hue="accent" />
        <Operations />
        <Divider hue="good" />
        <Examples />
        <Divider hue="accent" />
        <FAQ />
        <Divider hue="tag" />
      </main>
      <Footer />
    </div>
  );
}
