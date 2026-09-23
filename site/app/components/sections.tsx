import { useEffect, useRef, useState } from "react";
import { AGENT_CARDS, BLOB, CASE, EXAMPLES, FAQS, GH, HUE, PLANS, REFUSAL, RELEASES, WEB, asset, type CaseItem, type Hue } from "../content";
import { Button, Rail, SectionHead, Square, cn } from "./frame";

export function GetIt() {
  const options = [
    { os: "macOS · Linux", label: "brew install parcad", copy: "brew tap ierehon1905/parcad && brew install parcad" },
    { os: "Windows", label: "winget install ParCAD.ParCAD", copy: "winget install ParCAD.ParCAD" },
    { os: "Desktop app", label: "Download from Releases", href: RELEASES },
    { os: "No install", label: "Open ParCAD web", href: WEB },
  ];
  return (
    <section className="py-16 md:py-20">
      <Rail className="max-w-[720px] space-y-6">
        <p className="text-center font-mono text-[11px] tracking-wider text-ink-dim uppercase">
          Free and open source · macOS, Linux, Windows, browser
        </p>
        <div className="grid gap-2 sm:grid-cols-2">
          {options.map((o) => (
            <GetButton key={o.label} {...o} />
          ))}
        </div>
      </Rail>
    </section>
  );
}

function GetButton({ os, label, copy, href }: { os: string; label: string; copy?: string; href?: string }) {
  const [copied, setCopied] = useState(false);
  const body = (
    <>
      <span className="font-mono text-[10.5px] tracking-wider text-ink-faint uppercase">{os}</span>
      <span className="flex items-center justify-between gap-3 font-mono text-[12.5px] text-ink">
        {label}
        <span className="text-ink-faint transition-transform group-hover:translate-x-0.5">{copy ? (copied ? "copied" : "copy") : "›"}</span>
      </span>
    </>
  );
  const className = "group flex flex-col gap-1.5 border border-line bg-panel px-4 py-3 text-left transition-colors hover:border-line-strong hover:bg-white/[0.03]";
  if (copy)
    return (
      <button
        type="button"
        className={className}
        onClick={() =>
          navigator.clipboard?.writeText(copy).then(() => {
            setCopied(true);
            setTimeout(() => setCopied(false), 1400);
          })
        }
      >
        {body}
      </button>
    );
  return (
    <a className={className} href={href} target="_blank" rel="noreferrer">
      {body}
    </a>
  );
}

export function Case() {
  const [active, setActive] = useState(0);
  const refs = useRef<(HTMLDivElement | null)[]>([]);

  useEffect(() => {
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) if (e.isIntersecting) setActive(Number((e.target as HTMLElement).dataset.index));
      },
      { rootMargin: "-45% 0px -45% 0px" },
    );
    refs.current.forEach((el) => el && io.observe(el));
    return () => io.disconnect();
  }, []);

  return (
    <section className="py-20 md:py-28">
      <Rail className="grid gap-14 md:grid-cols-2 md:gap-16">
        <div className="md:sticky md:top-28 md:self-start">
          <SectionHead label="Why ParCAD" hue="tag" title="Code-CAD that stops when an edit breaks the part">
            <p>
              In a CAD script, moving one hole can shift a fillet onto a different edge while the build still succeeds.
              ParCAD describes geometry, checks each description on every build and measures the solid it made.
            </p>
          </SectionHead>
          <div className="relative mt-10 hidden aspect-[4/3] border border-line bg-tile md:block">
            {CASE.map((item, i) => (
              <div key={item.title} className={cn("absolute inset-0 transition-opacity duration-500", i === active ? "opacity-100" : "opacity-0")}>
                <CaseMedia item={item} />
              </div>
            ))}
          </div>
        </div>
        <div className="space-y-16 md:space-y-28 md:py-24">
          {CASE.map((item, i) => (
            <div key={item.title} ref={(el) => void (refs.current[i] = el)} data-index={i} className="space-y-4">
              <Square hue={item.hue} className="size-4" />
              <h3 className="text-[26px] leading-tight font-light tracking-tight md:text-[30px]">{item.title}</h3>
              <p className="max-w-md text-[15px] leading-relaxed text-ink-dim">{item.body}</p>
              <div className="relative aspect-[4/3] border border-line bg-tile md:hidden">
                <CaseMedia item={item} />
              </div>
            </div>
          ))}
        </div>
      </Rail>
    </section>
  );
}

function CaseMedia({ item }: { item: CaseItem }) {
  if (item.media.kind === "refusal")
    return (
      <div className="absolute inset-0 flex flex-col bg-panel p-5 font-mono text-[11.5px] leading-relaxed md:p-7 md:text-[12.5px]">
        <span className="mb-4 text-ink-faint">evaluate_part · refused</span>
        <pre className="overflow-x-auto whitespace-pre text-ink-dim">
          <span className="text-bad">error </span>
          {REFUSAL}
        </pre>
      </div>
    );
  return <img src={asset(item.media.src)} alt={item.media.alt} loading="lazy" className="absolute inset-0 h-full w-full object-contain p-4" />;
}

const CARD_ART: Record<Hue, string> = {
  accent: "from-accent/25 via-accent/5",
  tag: "from-tag/25 via-tag/5",
  gold: "from-gold/20 via-gold/5",
  good: "from-good/20 via-good/5",
  cut: "from-cut/25 via-cut/5",
  bad: "from-bad/20 via-bad/5",
};

export function Agents() {
  return (
    <section id="agents" className="scroll-mt-20 py-20 md:py-28">
      <Rail className="space-y-12">
        <SectionHead hue="tag" title="Built so models can make models">
          <p>
            The running app serves MCP at 127.0.0.1:4242/mcp from the same service the window uses. An agent reads the
            language, builds, measures and exports parts, and works in the session you have open.
          </p>
        </SectionHead>
        <figure className="border border-line bg-black">
          <img src={asset("renders/agent-view.webp")} alt="What an agent is shown: four views at one scale, two sections, and faces coloured by tag" loading="lazy" className="w-full" />
        </figure>
        <div className="grid gap-4 md:grid-cols-2">
          {AGENT_CARDS.map((card) => (
            <article
              key={card.title}
              className={cn("relative flex min-h-64 flex-col overflow-hidden border bg-gradient-to-br to-transparent p-6 md:p-7", HUE[card.hue].border, CARD_ART[card.hue])}
            >
              <GridArt hue={card.hue} />
              <h3 className="relative text-[24px] leading-tight font-light tracking-tight">{card.title}</h3>
              <p className="relative mt-3 max-w-sm text-[14.5px] leading-relaxed text-ink-dim">{card.body}</p>
              {card.code && (
                <code className="relative mt-4 self-start border border-line bg-black/60 px-3 py-1.5 font-mono text-[12px] text-ink">{card.code}</code>
              )}
              <div className="relative mt-auto pt-6">
                <a
                  href={card.href}
                  target="_blank"
                  rel="noreferrer"
                  className={cn(
                    "inline-flex border px-3 py-1.5 font-mono text-[11px] tracking-wider uppercase transition-colors hover:bg-white/5",
                    HUE[card.hue].border,
                    HUE[card.hue].text,
                  )}
                >
                  {card.cta} <span className="ml-2 opacity-70">›</span>
                </a>
              </div>
            </article>
          ))}
        </div>
      </Rail>
    </section>
  );
}

/** An isometric wireframe box in the corner of a card, in the card's hue. */
function GridArt({ hue }: { hue: Hue }) {
  const c = HUE[hue].hex;
  return (
    <svg aria-hidden viewBox="0 0 200 200" className="pointer-events-none absolute -right-6 -bottom-6 size-52 opacity-40">
      <g fill="none" stroke={c} strokeWidth="0.8">
        <path d="M100 30 L170 70 L170 150 L100 190 L30 150 L30 70 Z" />
        <path d="M30 70 L100 110 L170 70 M100 110 L100 190" />
        <path d="M65 50 L135 90 L135 170 M135 50 L65 90 L65 170" strokeDasharray="2 3" />
      </g>
      <g fill={c}>
        {[
          [100, 30],
          [170, 70],
          [30, 70],
          [100, 110],
          [100, 190],
        ].map(([x, y]) => (
          <rect key={`${x}-${y}`} x={x - 2.5} y={y - 2.5} width="5" height="5" />
        ))}
      </g>
    </svg>
  );
}

export function Plans() {
  return (
    <section id="install" className="scroll-mt-20 py-20 md:py-28">
      <Rail className="space-y-14">
        <SectionHead hue="cut" title="Free. Three ways to run it">
          <p>Each one runs the same kernel, with the same tools and reports.</p>
        </SectionHead>
        <div className="grid border border-line md:grid-cols-3">
          {PLANS.map((plan) => (
            <div key={plan.name} className="flex flex-col border-line p-6 not-last:border-b md:p-7 md:not-last:border-r md:not-last:border-b-0">
              <div className="flex items-center gap-2.5 font-mono text-[11px] tracking-wider uppercase">
                <Square hue={plan.hue} className="size-2" />
                <span className={HUE[plan.hue].text}>{plan.name}</span>
              </div>
              <p className="mt-6 text-[44px] leading-none font-light tracking-tight">{plan.price}</p>
              <p className="mt-2 font-mono text-[11px] tracking-wider text-ink-faint uppercase">{plan.note}</p>
              <p className="mt-6 min-h-12 text-[14.5px] leading-relaxed text-ink-dim">{plan.blurb}</p>
              <div className="mt-6">
                {plan.cta.copy ? (
                  <Button variant="primary" copy={plan.cta.copy} layout="w-full justify-between">
                    {plan.cta.label}
                  </Button>
                ) : (
                  <Button variant="outline" href={plan.cta.href} layout="w-full justify-between">
                    {plan.cta.label}
                  </Button>
                )}
              </div>
              <p className="mt-8 mb-4 font-mono text-[10.5px] tracking-wider text-ink-faint uppercase">What you get</p>
              <ul className="space-y-3 text-[13.5px] leading-snug text-ink-dim">
                {plan.features.map((f) => (
                  <li key={f} className="flex gap-3">
                    <span className={cn("mt-[7px] size-1.5 shrink-0", HUE[plan.hue].bg)} />
                    {f}
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
        <p className="max-w-2xl text-[13px] leading-relaxed text-ink-faint">
          MIT or Apache-2.0, at your option. OpenCASCADE and its bindings are LGPL-2.1, so redistributing a binary carries
          obligations — <a className="underline decoration-line-strong underline-offset-4 hover:text-ink" href={`${BLOB}/NOTICE.md`}>NOTICE.md</a> spells them out.
          Builds are unsigned for now; the <a className="underline decoration-line-strong underline-offset-4 hover:text-ink" href={`${GH}#install-it`}>README</a> says how to open them.
        </p>
      </Rail>
    </section>
  );
}

export function Operations() {
  const extras = [
    { file: "screw-top-jar", title: "Modelled threads", body: "threadedRod on the neck, threadedHole in the cap, 0.25 mm clearance each. between_bodies reads the gap back off the built solids." },
    { file: "spur-gears", title: "Curves from a formula", body: "Every flank an involute the script certifies to a few millionths of a millimetre." },
    { file: "twisted-planter", title: "Lofts with a measured wall", body: "A ruled loft through nine star sections, hollowed along the same twist to a 2 mm wall." },
  ];
  return (
    <section className="py-20 md:py-28">
      <Rail className="space-y-12">
        <div className="grid gap-8 md:grid-cols-2 md:gap-16">
          <SectionHead hue="accent" title="Operations for small mechanical parts" />
          <div className="space-y-4 self-end text-[15px] leading-relaxed text-ink-dim">
            <p>
              Fillet, chamfer, blended union, intersect, loft, sweep, helix, revolve, shell, draft, patterns and mirror,
              plus threads, thickened surfaces and fitted curves.
            </p>
            <p>Each operation lands with a case in eval/cases/ that records what it measured. When a change moves one of those numbers, a test fails before release.</p>
          </div>
        </div>
        <figure className="border border-line bg-black">
          <img src={asset("renders/features.webp")} alt="Fillet, chamfer, blended union, intersect, loft, sweep, helix, revolve, shell, draft, polar pattern and mirror, each built by the exact kernel" loading="lazy" className="w-full" />
        </figure>
        <div className="grid gap-4 md:grid-cols-3">
          {extras.map((x) => (
            <article key={x.file} className="flex flex-col border border-line bg-panel">
              <img src={asset(`renders/${x.file}.webp`)} alt="" loading="lazy" className="aspect-[4/3] w-full bg-tile object-contain p-4" />
              <div className="border-t border-line p-5">
                <h3 className="text-[19px] font-light tracking-tight">{x.title}</h3>
                <p className="mt-2 text-[13.5px] leading-relaxed text-ink-dim">{x.body}</p>
              </div>
            </article>
          ))}
        </div>
      </Rail>
    </section>
  );
}

export function Examples() {
  return (
    <section id="examples" className="scroll-mt-20 py-20 md:py-28">
      <Rail className="space-y-12">
        <div className="flex flex-wrap items-end justify-between gap-6">
          <SectionHead hue="good" title="Twenty-eight example parts">
            <p>
              Brackets, flanges, manifolds and heat sinks — parts you print or machine one of. Each opens as a seed part
              in the app, and most have a case in the eval corpus, so an example that stops building fails a test.
            </p>
          </SectionHead>
          <Button variant="quiet" href={`${GH}/tree/main/examples`}>
            All examples
          </Button>
        </div>
        <div className="grid border-t border-l border-line sm:grid-cols-2 lg:grid-cols-3">
          {EXAMPLES.map((ex) => (
            <a
              key={ex.file}
              href={`${BLOB}/examples/${ex.file}.js`}
              target="_blank"
              rel="noreferrer"
              className="group relative flex flex-col border-r border-b border-line transition-colors hover:bg-white/[0.02]"
            >
              <Corner />
              <div className="relative aspect-square overflow-hidden bg-tile">
                <img
                  src={asset(`renders/${ex.file}.webp`)}
                  alt={ex.name}
                  loading="lazy"
                  className="h-full w-full object-contain p-6 transition-transform duration-500 group-hover:scale-[1.04]"
                />
              </div>
              <div className="flex flex-1 flex-col gap-2 border-t border-line p-5">
                <h3 className="text-[16px] tracking-tight">{ex.name}</h3>
                <p className="text-[13.5px] leading-relaxed text-ink-dim">{ex.why}</p>
                <div className="mt-auto flex items-center justify-between pt-4 font-mono text-[11px]">
                  <span className="text-ink-faint">{ex.file}.js</span>
                  <span className="text-good">{ex.stat}</span>
                </div>
              </div>
            </a>
          ))}
        </div>
      </Rail>
    </section>
  );
}

function Corner() {
  return <span aria-hidden className="absolute -right-[3px] -bottom-[3px] z-10 size-[5px] bg-ink-faint" />;
}

export function FAQ() {
  return (
    <section id="faq" className="scroll-mt-20 py-20 md:py-28">
      <Rail className="space-y-12">
        <div className="flex flex-wrap items-end justify-between gap-6">
          <SectionHead hue="accent" title="Frequently asked questions">
            <p>For anything else, open an issue on GitHub.</p>
          </SectionHead>
          <Button variant="quiet" href={`${GH}/issues`}>
            Open an issue
          </Button>
        </div>
        <div className="grid border-t border-l border-line md:grid-cols-2 lg:grid-cols-3">
          {FAQS.map((f) => (
            <div key={f.q} className="relative flex flex-col gap-4 border-r border-b border-line p-6 md:p-7">
              <Corner />
              <h3 className="text-[21px] leading-snug font-light tracking-tight">{f.q}</h3>
              {f.a.map((p) => (
                <p key={p} className="text-[14px] leading-relaxed text-ink-dim">
                  {p}
                </p>
              ))}
            </div>
          ))}
        </div>
      </Rail>
    </section>
  );
}

export function Footer() {
  const tree: { root: string; hue: Hue; links: [string, string][] }[] = [
    {
      root: "parcad",
      hue: "accent",
      links: [
        ["GitHub", GH],
        ["Releases", RELEASES],
        ["Examples", `${GH}/tree/main/examples`],
        ["Architecture", `${BLOB}/docs/ARCHITECTURE.md`],
        ["What's next", `${BLOB}/docs/NEXT.md`],
      ],
    },
    {
      root: "agents",
      hue: "tag",
      links: [
        ["MCP setup", `${GH}#use-it-from-an-agent`],
        ["Perception", `${BLOB}/docs/PERCEPTION.md`],
        ["Writing for models", `${BLOB}/docs/WRITING_FOR_MODELS.md`],
        ["Field suite", `${GH}/tree/main/field`],
      ],
    },
    {
      root: "try it",
      hue: "good",
      links: [
        ["ParCAD web", WEB],
        ["Homebrew", `${GH}#install-it`],
        ["winget", `${GH}#install-it`],
        ["Build from source", `${GH}#build-it`],
        ["Contributing", `${BLOB}/CONTRIBUTING.md`],
      ],
    },
  ];
  return (
    <footer className="pt-20 pb-16 md:pt-28">
      <Rail className="space-y-20">
        <div className="relative overflow-hidden border border-tag/40 bg-gradient-to-br from-tag/25 via-accent/10 to-transparent p-8 md:p-12">
          <FooterArt />
          <h2 className="relative max-w-md text-[34px] leading-[1.05] font-light tracking-tight md:text-[48px]">Get started with ParCAD</h2>
          <p className="relative mt-4 max-w-md text-[15px] text-ink-dim">The editor and the OpenCASCADE kernel in a browser tab. No install, no account.</p>
          <div className="relative mt-8">
            <Button variant="outline" href={WEB} layout="bg-black/40">
              Open ParCAD web
            </Button>
          </div>
        </div>
        <nav className="grid gap-12 sm:grid-cols-3">
          {tree.map((t) => (
            <div key={t.root} className="font-mono text-[11px] tracking-wider uppercase">
              <span className={cn("inline-block border px-2 py-1", HUE[t.hue].border, HUE[t.hue].text)}>{t.root}</span>
              <ul className={cn("ml-3 border-l pt-2", HUE[t.hue].border)}>
                {t.links.map(([label, href]) => (
                  <li key={label} className="relative flex items-center pt-2.5">
                    <span className={cn("h-px w-5", HUE[t.hue].bg, "opacity-50")} />
                    <a href={href} target="_blank" rel="noreferrer" className="border border-line px-2 py-1 text-ink-dim transition-colors hover:border-line-strong hover:text-ink">
                      {label}
                    </a>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </nav>
        <div className="flex flex-wrap items-center justify-between gap-4 border-t border-line pt-6 font-mono text-[10.5px] tracking-wider text-ink-faint uppercase">
          <span>ParCAD · MIT or Apache-2.0</span>
          <span>Uses Open CASCADE Technology</span>
        </div>
      </Rail>
    </footer>
  );
}

function FooterArt() {
  const nodes = [
    [360, 40],
    [460, 110],
    [560, 40],
    [460, 190],
    [620, 150],
    [380, 170],
  ];
  const edges = [
    [0, 1],
    [1, 2],
    [1, 3],
    [2, 4],
    [3, 4],
    [0, 5],
    [5, 3],
  ];
  return (
    <svg aria-hidden viewBox="0 0 640 230" preserveAspectRatio="xMaxYMid slice" className="pointer-events-none absolute inset-0 hidden h-full w-full md:block">
      <g stroke="#b79cff" strokeOpacity="0.45" fill="none">
        <circle cx="560" cy="40" r="70" />
        <circle cx="380" cy="170" r="46" />
        {edges.map(([a, b]) => (
          <line key={`${a}-${b}`} x1={nodes[a][0]} y1={nodes[a][1]} x2={nodes[b][0]} y2={nodes[b][1]} />
        ))}
      </g>
      {nodes.map(([x, y], i) => (
        <g key={i}>
          <rect x={x - 11} y={y - 11} width="22" height="22" fill="#000" stroke="#f5b942" strokeOpacity="0.8" />
          <rect x={x - 4} y={y - 4} width="8" height="8" fill="#f5b942" />
        </g>
      ))}
    </svg>
  );
}

