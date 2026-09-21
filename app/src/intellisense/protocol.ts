/**
 * What the editor asks the language service, and what comes back.
 *
 * Every position here is a document offset. The wrapper the compiler needs is
 * `part-file.ts`'s business and never crosses this boundary.
 */

import type { CompletionItem, Info, Problem, SignatureInfo } from "./analyzer";

export type Request =
  /** The document changed. Answered by nothing; ordered before what follows. */
  | { id: number; kind: "part"; text: string }
  | { id: number; kind: "quickInfo"; pos: number }
  | { id: number; kind: "completions"; pos: number }
  | { id: number; kind: "completionDetail"; pos: number; label: string }
  | { id: number; kind: "signatureHelp"; pos: number }
  | { id: number; kind: "diagnostics" };

export interface Replies {
  part: void;
  quickInfo: (Info & { from: number; to: number }) | undefined;
  completions: CompletionItem[];
  completionDetail: Info | undefined;
  signatureHelp: SignatureInfo | undefined;
  diagnostics: Problem[];
}

export type Reply = { id: number; value: unknown } | { id: number; error: string };
