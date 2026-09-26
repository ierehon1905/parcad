import { useState } from "react";
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import { HUE, type Hue } from "../content";

export const cn = (...classes: ClassValue[]) => twMerge(clsx(classes));

/** The content rail. Its edges line up with the page-long rails drawn by <Rails />. */
export function Rail({ className, children }: { className?: string; children: React.ReactNode }) {
  return <div className={cn("mx-auto w-full max-w-[1200px] px-5 md:px-12", className)}>{children}</div>;
}

export function Rails() {
  return (
    <div aria-hidden className="pointer-events-none absolute inset-0 -z-0">
      <div className="mx-auto h-full max-w-[1200px] border-x border-line" />
    </div>
  );
}

/** A full-bleed hairline with a square where it crosses each rail. */
export function Divider({ hue, className }: { hue?: Hue; className?: string }) {
  const dot = cn("absolute top-1/2 size-[7px] -translate-y-1/2", hue ? HUE[hue].bg : "bg-ink-faint");
  return (
    <div aria-hidden className={cn("relative h-px w-full bg-line", className)}>
      <div className="relative mx-auto h-px max-w-[1200px]">
        <span className={cn(dot, "-left-[3.5px]")} />
        <span className={cn(dot, "-right-[3.5px]")} />
      </div>
    </div>
  );
}

export function Square({ hue, className }: { hue: Hue; className?: string }) {
  return <span aria-hidden className={cn("inline-block size-2.5 shrink-0", HUE[hue].bg, className)} />;
}

export function Label({ hue, children }: { hue: Hue; children: React.ReactNode }) {
  return (
    <span className={cn("inline-flex border px-2.5 py-1 font-mono text-[11px] tracking-wider uppercase", HUE[hue].border, HUE[hue].text, HUE[hue].tint)}>
      {children}
    </span>
  );
}

const BUTTON = {
  primary: "bg-accent text-black hover:bg-[#8dbbff] border-accent",
  outline: "border-line-strong text-ink hover:bg-white/5",
  quiet: "border-line text-ink-dim hover:text-ink hover:border-line-strong",
};

type ButtonProps = {
  variant?: keyof typeof BUTTON;
  href?: string;
  copy?: string;
  icon?: React.ReactNode;
  layout?: string;
  children: React.ReactNode;
};

/** A link, or — given `copy` — a button that puts that text on the clipboard. */
export function Button({ variant = "outline", href, copy, icon, layout, children }: ButtonProps) {
  const [copied, setCopied] = useState(false);
  const className = cn(
    "group inline-flex h-9 items-center gap-2.5 border px-3.5 font-mono text-[11.5px] tracking-wider uppercase transition-colors",
    BUTTON[variant],
    layout,
  );
  const tail = copy ? (
    <span className="ml-1 opacity-60">{copied ? "copied" : <CopyGlyph />}</span>
  ) : (
    <span className="ml-1 opacity-60 transition-transform group-hover:translate-x-0.5">›</span>
  );
  if (copy) {
    return (
      <button
        type="button"
        className={cn(className, "normal-case tracking-normal")}
        onClick={() => {
          navigator.clipboard?.writeText(copy).then(() => {
            setCopied(true);
            setTimeout(() => setCopied(false), 1400);
          });
        }}
      >
        {icon}
        {children}
        {tail}
      </button>
    );
  }
  const external = href?.startsWith("http");
  return (
    <a className={className} href={href} {...(external ? { target: "_blank", rel: "noreferrer" } : {})}>
      {icon}
      {children}
      {tail}
    </a>
  );
}

function CopyGlyph() {
  return (
    <svg viewBox="0 0 16 16" className="inline size-3.5" fill="none" stroke="currentColor" strokeWidth="1.3">
      <rect x="5.5" y="5.5" width="8" height="8" />
      <path d="M10.5 5.5v-3h-8v8h3" />
    </svg>
  );
}

export function SectionHead({ label, hue, title, children, className }: { label?: string; hue: Hue; title: string; children?: React.ReactNode; className?: string }) {
  return (
    <div className={cn("max-w-2xl space-y-5", className)}>
      {label && <Label hue={hue}>{label}</Label>}
      <h2 className="text-[34px] leading-[1.08] font-light tracking-tight text-balance md:text-[44px]">{title}</h2>
      {children && <div className="space-y-4 text-[15px] leading-relaxed text-ink-dim md:text-base">{children}</div>}
    </div>
  );
}
