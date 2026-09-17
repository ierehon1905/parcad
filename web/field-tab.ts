// A field case against a ParCAD web tab: headless Chromium is the tab, each
// trial starts from a fresh project folder, and the transcripts land in
// target/field-web/<label>/groups, one directory per arm, for field/score.py.
// Needs `vite --mode web` on 1420 built with VITE_PARCAD_RELAY, and a
// Chromium (CHROME, or Playwright's).
//
//     bun web/field-tab.ts <label> eval/field/<case>.md
const CHROME = process.env.CHROME ??
  `${process.env.HOME}/Library/Caches/ms-playwright/chromium-1243/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing`;
const REPO = new URL("..", import.meta.url).pathname.replace(/\/$/, "");
const PAGE = "http://localhost:1420/parcad/";
const SAME_ORIGIN_BLANK = "http://localhost:1420/parcad/favicon.svg";
const [label, casePath] = process.argv.slice(2);
const OUT = `${REPO}/target/field-web/${label}`;
const PORT = 9400 + Math.floor(Math.random() * 100);

const PLAN = [
  { group: "haiku-think0", env: { MODEL: "claude-haiku-4-5-20251001", THINK: "0" }, n: 4 },
  { group: "haiku-think8", env: { MODEL: "claude-haiku-4-5-20251001", THINK: "8000" }, n: 4 },
  { group: "sonnet-low", env: { MODEL: "claude-sonnet-5", THINK: "default", EFFORT: "low" }, n: 2 },
  { group: "sonnet-high", env: { MODEL: "claude-sonnet-5", THINK: "default", EFFORT: "high" }, n: 2 },
];

const chrome = Bun.spawn(
  [CHROME, "--headless=new", `--remote-debugging-port=${PORT}`, `--user-data-dir=${OUT}/profile`,
    "--window-size=1440,900", "--use-angle=swiftshader", "--enable-unsafe-swiftshader", "about:blank"],
  { stdout: "ignore", stderr: "ignore" },
);

async function json(path: string, method = "GET") {
  for (let i = 0; i < 100; i++) {
    try { return await (await fetch(`http://127.0.0.1:${PORT}${path}`, { method })).json(); } catch { await Bun.sleep(100); }
  }
  throw new Error("chrome did not start");
}
const target = await json(`/json/new?about:blank`, "PUT");
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0;
const pending = new Map<number, (v: any) => void>();
ws.onmessage = (e) => { const m = JSON.parse(String(e.data)); if (m.id && pending.has(m.id)) { pending.get(m.id)!(m); pending.delete(m.id); } };
const send = (method: string, params: any = {}) => new Promise<any>((r) => { const n = ++id; pending.set(n, r); ws.send(JSON.stringify({ id: n, method, params })); });
const evaluate = async (expression: string) => (await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true })).result?.result?.value;
const waitFor = async (expression: string, ms = 120000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) { if (await evaluate(expression)) return; await Bun.sleep(300); }
  throw new Error(`timed out waiting for ${expression}`);
};
const click = (text: string) => evaluate(`(() => { const el = [...document.querySelectorAll('button')].find(e => e.textContent.trim() === ${JSON.stringify(text)}); el?.click(); return !!el; })()`);
const built = `/\\d+ ms/.test(document.querySelector('#status')?.textContent ?? '')`;

await send("Page.enable");
await send("Page.navigate", { url: PAGE });
await waitFor(built);
await click("Connect your AI");
await Bun.sleep(300);
await click("Create my link");
await waitFor(`[...document.querySelectorAll('code')].some(c => c.textContent.includes('/mcp'))`);
const link: string = await evaluate(`[...document.querySelectorAll('code')].map(c => c.textContent).find(t => t.includes('/mcp')).split(' ').pop()`);
console.log("link", link);

const config = (await Bun.file(`${REPO}/field/field.toml`).text())
  .replace(/^url = .*$/m, `url = "${link}"`)
  .replace(/^health = .*$/m, `health = "${new URL(link).origin}/"`)
  .replace(/^cases = .*$/m, `cases = "${REPO}/eval/field"`);
await Bun.write(`${OUT}/field.toml`, config);

async function reset() {
  await send("Page.navigate", { url: SAME_ORIGIN_BLANK });
  await Bun.sleep(800);
  const deleted = await evaluate(`new Promise((resolve) => { const r = indexedDB.deleteDatabase('/parcad'); r.onsuccess = () => resolve('deleted'); r.onerror = () => resolve('error'); r.onblocked = () => resolve('blocked'); })`);
  if (deleted !== "deleted") throw new Error(`the project folder was not reset: ${deleted}`);
  await send("Page.navigate", { url: PAGE });
  await waitFor(built);
  await waitFor(`/waiting for your AI|connected|working/.test(document.querySelector('#agent-link')?.textContent ?? '')`);
}

for (const step of PLAN) {
  for (let i = 1; i <= step.n; i++) {
    await reset();
    const run = `${OUT}/runs/${step.group}-${i}`;
    const started = Date.now();
    const proc = Bun.spawn(["field/run-case.sh", casePath, "1"], {
      cwd: REPO,
      env: { ...process.env, ...step.env, FIELD_CONFIG: `${OUT}/field.toml`, RUN: run, QUIET: "1" },
      stdout: "inherit",
      stderr: "inherit",
    });
    await proc.exited;
    const session = await evaluate(`document.querySelector('#project')?.textContent`);
    console.log(`${step.group} #${i}: ${Math.round((Date.now() - started) / 1000)} s, page shows ${session}`);
    // Grouped for the scorer: one directory per arm, trials numbered in it.
    const group = `${OUT}/groups/${step.group}`;
    await Bun.$`mkdir -p ${group}`;
    await Bun.$`cp ${run}/trial1.jsonl ${group}/trial${i}.jsonl`;
    await Bun.$`cp ${run}/case.md ${group}/case.md`;
    await Bun.$`cp ${run}/mcp.json ${group}/mcp.json`;
  }
}

for (const step of PLAN) {
  console.log(`\n== ${step.group}`);
  const out = await Bun.$`${REPO}/field/score.py ${OUT}/groups/${step.group}`.env({ ...process.env, FIELD_CONFIG: `${OUT}/field.toml` }).nothrow().text();
  console.log(out.split("\n").filter((l) => /^trial|SOUND|LUCKY|WRONG/.test(l)).join("\n"));
}
ws.close();
chrome.kill();
