import { useEffect, useState } from "react";
import { GH, HUE, NAV, SHOWCASE, WEB, asset } from "../content";
import { Code } from "./code";
import { Button, Rail, Square, cn } from "./frame";
import { PartViewer } from "./part-viewer";

export function Nav() {
  return (
    <header className="fixed inset-x-0 top-0 z-50 px-3 pt-3 md:pt-4">
      <nav className="mx-auto flex h-12 max-w-[1100px] items-center gap-2 border border-line bg-black/80 pr-1.5 pl-1.5 backdrop-blur-md">
        <a href="#top" className="flex items-center gap-2.5 pr-3">
          <img src={asset("favicon.svg")} alt="" className="size-8" />
          <span className="text-[15px] font-medium tracking-tight">ParCAD</span>
        </a>
        <div className="hidden flex-1 items-center gap-1 lg:flex">
          {NAV.map((item) => (
            <a
              key={item.href}
              href={item.href}
              className="flex items-center gap-2 px-2.5 py-1.5 font-mono text-[11px] tracking-wider text-ink-dim uppercase transition-colors hover:bg-white/5 hover:text-ink"
            >
              <Square hue={item.hue} className="size-2" />
              {item.label}
            </a>
          ))}
          <a
            href={GH}
            target="_blank"
            rel="noreferrer"
            className="flex items-center gap-2 px-2.5 py-1.5 font-mono text-[11px] tracking-wider text-ink-dim uppercase transition-colors hover:bg-white/5 hover:text-ink"
          >
            <span className="inline-block size-2 bg-ink" />
            GitHub
          </a>
        </div>
        <div className="ml-auto">
          <Button variant="primary" href={WEB} layout="h-8">
            Try in browser
          </Button>
        </div>
      </nav>
    </header>
  );
}

export function Hero() {
  return (
    <section id="top" className="relative pt-32 md:pt-44">
      <Rail className="text-center">
        <h1 className="mx-auto max-w-4xl text-[44px] leading-[1.02] font-light tracking-tight text-balance md:text-[76px]">
          Parametric CAD you write as code
        </h1>
        <p className="mx-auto mt-6 max-w-xl text-[15px] leading-relaxed text-ink-dim md:text-[17px]">
          For people and their AI agents. Select edges with a description like “hole rims on the top face”, get every
          part measured on an OpenCASCADE B‑rep, and give your agent the same tools over MCP.
        </p>
        <div className="mt-9 flex flex-wrap items-center justify-center gap-3">
          <Button variant="primary" copy="brew tap ierehon1905/parcad && brew install parcad" icon={<Prompt />}>
            brew install parcad
          </Button>
          <Button variant="outline" href={WEB}>
            Open ParCAD web
          </Button>
        </div>
        <p className="mt-5 font-mono text-[11px] tracking-wider text-ink-faint uppercase">
          Free · MIT or Apache-2.0 · Experimental
        </p>
      </Rail>
      <Showcase />
    </section>
  );
}

function Prompt() {
  return <span className="font-mono text-black/50">$</span>;
}

function Showcase() {
  const [active, setActive] = useState(0);
  const [auto, setAuto] = useState(true);
  const item = SHOWCASE[active];
  const hue = HUE[item.hue];
  // The viewer stays mounted under image tabs, so its WebGL context and mesh cache survive a tab change.
  const [lastMesh, setLastMesh] = useState(SHOWCASE.find((s) => s.visual.kind === "stl")!.visual.src);
  useEffect(() => {
    if (item.visual.kind === "stl") setLastMesh(item.visual.src);
  }, [item]);

  useEffect(() => {
    if (!auto) return;
    const t = setTimeout(() => setActive((i) => (i + 1) % SHOWCASE.length), 9000);
    return () => clearTimeout(t);
  }, [active, auto]);

  return (
    <div id="features" className="mt-16 scroll-mt-24 md:mt-20">
      <Rail className="px-0 md:px-0">
        <div role="tablist" className="flex overflow-x-auto border-y border-line md:grid md:grid-cols-5">
          {SHOWCASE.map((s, i) => {
            const on = i === active;
            return (
              <button
                key={s.id}
                role="tab"
                aria-selected={on}
                onClick={() => {
                  setActive(i);
                  setAuto(false);
                }}
                className={cn(
                  "relative flex h-11 min-w-36 shrink-0 items-center gap-2.5 border-r border-line px-4 font-mono text-[11px] tracking-wider uppercase transition-colors last:border-r-0",
                  on ? cn(HUE[s.hue].tint, HUE[s.hue].text) : "text-ink-dim hover:bg-white/[0.03] hover:text-ink",
                )}
              >
                <Square hue={s.hue} className="size-2" />
                {s.label}
                {on && auto && (
                  <span key={active} className={cn("absolute inset-x-0 bottom-0 h-px origin-left animate-tab-progress", HUE[s.hue].bg)} />
                )}
              </button>
            );
          })}
        </div>

        <div className="grid border-b border-line md:h-[600px] md:grid-cols-[11fr_13fr]">
          <div className="flex min-w-0 flex-col border-line md:border-r">
            <div className="flex h-10 items-center justify-between border-b border-line px-4 font-mono text-[11px] text-ink-faint">
              <span>{item.file}</span>
              <span className={hue.text}>{item.lang === "js" ? "part.js" : "MCP"}</span>
            </div>
            <Code key={item.id} source={item.code} lang={item.lang} className="min-h-0 flex-1 bg-panel py-4 pr-4" />
            <p className="border-t border-line px-5 py-4 text-[13.5px] leading-relaxed text-ink-dim">{item.caption}</p>
          </div>

          <div className="relative h-[380px] overflow-hidden bg-tile md:h-auto">
            <PartViewer src={lastMesh} paused={item.visual.kind !== "stl"} />
            {item.visual.kind === "image" && (
              <img
                key={item.visual.src}
                src={asset(item.visual.src)}
                alt={item.visual.alt}
                className="absolute inset-0 h-full w-full bg-tile object-contain p-6"
              />
            )}
            <span className="pointer-events-none absolute top-3 right-4 font-mono text-[10.5px] tracking-wider text-ink-faint uppercase">
              {item.visual.kind === "stl" ? "Exported mesh · drag to orbit" : "Kernel render"}
            </span>
            <dl className="pointer-events-none absolute bottom-3 left-3 grid grid-cols-[auto_auto] gap-x-4 gap-y-1 border border-line bg-black/70 px-3 py-2.5 font-mono text-[11px] backdrop-blur-sm">
              {item.readout.map(([k, v]) => (
                <div key={k} className="contents">
                  <dt className="tracking-wider text-ink-faint uppercase">{k}</dt>
                  <dd className="text-ink">{v}</dd>
                </div>
              ))}
            </dl>
          </div>
        </div>
      </Rail>
    </div>
  );
}
