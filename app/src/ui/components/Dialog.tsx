/**
 * Asking the user something, and telling them something, over a dimmed page.
 *
 * Not `window.prompt` or `window.confirm`: the desktop webview has neither, and
 * a browser's cannot mark a name invalid while it is being typed.
 *
 * Each is a promise the panel settles, so a caller reads as a sentence —
 * `const answer = await ask({...})` — rather than as a state machine. The
 * signals holding the pending request live in `useDialogs`, and the caller
 * renders `<Dialogs>` wherever the scrim should sit.
 */

import { useSignal, type Signal } from "@preact/signals";
import { type ComponentChildren } from "preact";
import { useEffect, useRef } from "preact/hooks";

import { nameProblem, type ProjectPart } from "../../projects";
import { Button } from "./Button";
import { Caption, Field } from "./Field";

/** A question waiting for an answer, and the promise it will settle. */
interface AskRequest {
  title: string;
  label: string;
  value: string;
  confirm: string;
  /** Offer to start from an existing part rather than the starter script. */
  from?: ProjectPart[];
  /** Skip path validation — for a title, which is prose rather than a path. */
  free?: boolean;
  settle: (answer: { value: string; from?: string } | null) => void;
}

/** A message, and optionally a confirmation. */
interface TellRequest {
  text: string;
  confirm?: string;
  settle: (ok: boolean) => void;
}

export type Ask = (request: Omit<AskRequest, "settle">) => Promise<{ value: string; from?: string } | null>;
export type Tell = (text: string, confirm?: string) => Promise<boolean>;
export type Failed = (error: unknown) => Promise<boolean>;

/** The card a question is asked on, over the dimmed page. */
const CARD =
  "w-[min(420px,84%)] p-[18px] rounded-xl border border-line bg-panel " +
  "shadow-[0_18px_44px_rgb(0_0_0/0.55)]";

export function useDialogs() {
  const asking = useSignal<AskRequest | undefined>(undefined);
  const telling = useSignal<TellRequest | undefined>(undefined);

  const ask: Ask = (request) =>
    new Promise((settle) => {
      asking.value = { ...request, settle };
    });
  const tell: Tell = (text, confirm) =>
    new Promise((settle) => {
      telling.value = { text, confirm, settle };
    });
  /** A host refusal, whole. Its wording names the fix; do not summarise it. */
  const failed: Failed = (error) =>
    tell(error instanceof Error ? error.message : String(error));

  return { ask, tell, failed, asking, telling };
}

export function Dialogs({
  asking,
  telling,
}: {
  asking: Signal<AskRequest | undefined>;
  telling: Signal<TellRequest | undefined>;
}) {
  return (
    <>
      {asking.value && <AskPanel request={asking} />}
      {telling.value && <TellPanel request={telling} />}
    </>
  );
}

function AskPanel({ request }: { request: Signal<AskRequest | undefined> }) {
  const value = useSignal(request.value!.value);
  const from = useSignal("");
  const input = useRef<HTMLInputElement>(null);
  const ask = request.value!;

  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);

  const problem = ask.free ? null : nameProblem(value.value);
  const done = (answer: { value: string; from?: string } | null) => {
    request.value = undefined;
    ask.settle(answer);
  };
  const submit = () => {
    if (problem === null) done({ value: value.value.trim(), from: from.value || undefined });
  };

  return (
    <Scrim>
      <div class={CARD}>
        <h3 class="m-0 mb-2.5 text-sm">{ask.title}</h3>
        <Caption>{ask.label}</Caption>
        <Field
          ref={input}
          type="text"
          spellcheck={false}
          value={value.value}
          onInput={(e) => (value.value = e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              submit();
            }
            // Escape closes the question, not the whole dialog behind it.
            if (e.key === "Escape") {
              e.preventDefault();
              e.stopPropagation();
              done(null);
            }
          }}
        />
        <p class="min-h-4 mt-1.5 mb-1 text-bad text-[11.5px]">{problem ?? ""}</p>
        {ask.from && (
          <Caption>
            Start from
            <select
              class="w-full mt-1"
              value={from.value}
              onChange={(e) => (from.value = e.currentTarget.value)}
            >
              <option value="">an empty part</option>
              {ask.from.map((part) => (
                <option key={part.path} value={part.path}>
                  {part.path}
                </option>
              ))}
            </select>
          </Caption>
        )}
        <div class="flex justify-end gap-2 mt-2.5">
          <Button onClick={() => done(null)}>Cancel</Button>
          <Button variant="primary" disabled={problem !== null} onClick={submit}>
            {ask.confirm}
          </Button>
        </div>
      </div>
    </Scrim>
  );
}

function TellPanel({ request }: { request: Signal<TellRequest | undefined> }) {
  const ask = request.value!;
  const focus = useRef<HTMLButtonElement>(null);
  useEffect(() => focus.current?.focus(), []);

  const done = (ok: boolean) => {
    request.value = undefined;
    ask.settle(ok);
  };

  return (
    <Scrim>
      <div class={CARD}>
        <p class="m-0 mb-3.5 whitespace-pre-wrap">{ask.text}</p>
        <div class="flex justify-end gap-2 mt-2.5">
          <Button ref={ask.confirm ? undefined : focus} onClick={() => done(false)}>
            {ask.confirm ? "Cancel" : "OK"}
          </Button>
          {ask.confirm && (
            <Button variant="danger" ref={focus} onClick={() => done(true)}>
              {ask.confirm}
            </Button>
          )}
        </div>
      </div>
    </Scrim>
  );
}



const Scrim = ({ children }: { children: ComponentChildren }) => (
  <div class="absolute inset-0 flex items-center justify-center bg-[rgb(8_9_12/0.66)]">
    {children}
  </div>
);
