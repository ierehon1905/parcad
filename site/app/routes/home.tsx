import type { Route } from "./+types/home";
import { Divider, Rails } from "../components/frame";
import { Hero, Nav } from "../components/hero";
import { Agents, Case, Examples, FAQ, Footer, GetIt, Operations, Plans } from "../components/sections";
import { DESCRIPTION, GH, LICENCE, RELEASES, SITE, TITLE, WEB } from "../content";

// Crawlers fetch the card from an absolute URL, so it names the deployed site rather than BASE_URL.
const CARD = `${SITE}og.png`;
const CARD_ALT =
  "ParCAD: open-source CAD your AI can check. The example bracket, rendered by ParCAD's kernel with each face coloured by the feature that built it.";

export const meta: Route.MetaFunction = () => [
  { title: TITLE },
  { name: "description", content: DESCRIPTION },
  { property: "og:type", content: "website" },
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
      "@type": "SoftwareApplication",
      name: "ParCAD",
      description: DESCRIPTION,
      url: SITE,
      image: CARD,
      applicationCategory: "DesignApplication",
      operatingSystem: "macOS, Linux, Windows, Web",
      downloadUrl: RELEASES,
      installUrl: WEB,
      license: `https://spdx.org/licenses/${LICENCE}-or-later.html`,
      sameAs: GH,
      offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
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
