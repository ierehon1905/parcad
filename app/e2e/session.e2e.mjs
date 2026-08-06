import assert from "node:assert/strict";

/**
 * The live session, on the transport a browser cannot exercise.
 *
 * The SSE half is easy to watch from any browser tab; the desktop webview is
 * the viewer that receives broadcasts as a Tauri event instead, and this suite
 * is the only instrument that can drive it. Three promises are checked, each
 * the desktop half of a rule in `session.rs`:
 *
 * 1. the webview pushes its own document, so `get_session` does not lie;
 * 2. a change pushed by another caller arrives over the Tauri event and is
 *    applied to the editor;
 * 3. it is applied as an *ordinary edit* — one Cmd-Z reverses it, exactly like
 *    the user's own typing, and the reversal propagates back out.
 *
 * The test pushes over `/api/session` rather than MCP `set_script` — both are
 * adapters over the same `session::push`, and HTTP needs no handshake — with
 * `origin: "agent"`, which no window owns, so the webview must apply it.
 */

const port = process.env.PARCAD_HTTP_PORT ?? "4242";
const api = `http://127.0.0.1:${port}/api/session`;

const readSession = async () => (await fetch(api)).json();
const editorDoc = () => browser.execute(() => window.__editor.state.doc.toString());

describe("the live session over the Tauri event transport", () => {
  it("mirrors the webview both ways, and an agent edit is one Cmd-Z from gone", async () => {
    // The suite shares one app with bracket.e2e.mjs, so a part is already
    // open and evaluated; wait only for the debounce push to have landed.
    let session;
    await browser.waitUntil(
      async () => {
        session = await readSession();
        return session.name !== null;
      },
      { timeoutMsg: "the desktop webview never pushed its document" },
    );
    assert.equal(session.script, await editorDoc(), "get_session must describe the screen");

    // An edit made in the webview reaches the session on the debounce. The
    // dispatch goes through the editor's normal update path, so it is the
    // same road a keystroke takes.
    await browser.execute(() => {
      window.__editor.dispatch({
        changes: { from: 0, insert: "// typed in the desktop webview\n" },
      });
    });
    await browser.waitUntil(
      async () => {
        session = await readSession();
        return session.script.startsWith("// typed in the desktop webview");
      },
      { timeoutMsg: "a webview edit never reached get_session — the session lies" },
    );

    // An agent's change arrives over the Tauri event and lands in the editor.
    const edited = session.script.replace(
      "// typed in the desktop webview",
      "// EDITED BY AGENT",
    );
    const pushed = await (
      await fetch(api, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name: session.name, script: edited, origin: "agent" }),
      })
    ).json();
    assert.equal(pushed.origin, "agent");
    await browser.waitUntil(
      async () => (await editorDoc()).includes("// EDITED BY AGENT"),
      { timeoutMsg: "an agent edit never reached the desktop webview" },
    );

    // And it is an ordinary edit: one undo takes it back. What is asserted is
    // history, not key delivery — the agent's change must sit in the same undo
    // history the user's typing goes into — so this invokes that history's own
    // command through the `__undo` handle. The native WKWebView driver cannot
    // reliably deliver a Cmd-Z chord, the same limitation bracket.e2e.mjs
    // records for Cmd-F; the physical shortcut is exercised against the
    // identical bundle by the browser-tab checks.
    await browser.execute(() => window.__editor.focus());
    await browser.execute(() => window.__undo());
    await browser.waitUntil(
      async () => {
        const doc = await editorDoc();
        return !doc.includes("// EDITED BY AGENT") && doc.includes("// typed in the desktop webview");
      },
      { timeoutMsg: "Cmd-Z did not reverse the agent edit — it is not in the undo history" },
    );

    // The reversal is itself a change, and it propagates like any other.
    await browser.waitUntil(
      async () => {
        session = await readSession();
        return !session.script.includes("// EDITED BY AGENT");
      },
      { timeoutMsg: "the undo never propagated back to the session" },
    );
  });
});
