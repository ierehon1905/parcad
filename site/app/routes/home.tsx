import type { Route } from "./+types/home";
import { Divider, Rails } from "../components/frame";
import { Hero, Nav } from "../components/hero";
import { Agents, Case, Examples, FAQ, Footer, GetIt, Operations, Plans } from "../components/sections";

const TITLE = "ParCAD — parametric CAD you write as code";
const DESCRIPTION =
  "Parametric CAD you write as code, for people and their AI agents: selectors that describe edges and check their count, an OpenCASCADE B-rep kernel, measured reports, and the same tools over MCP.";

export const meta: Route.MetaFunction = () => [
  { title: TITLE },
  { name: "description", content: DESCRIPTION },
  { property: "og:type", content: "website" },
  { property: "og:title", content: TITLE },
  { property: "og:description", content: DESCRIPTION },
  { name: "twitter:card", content: "summary_large_image" },
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
