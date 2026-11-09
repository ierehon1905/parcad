/** The few pages a person sees while signing in. Every client-given string is escaped. */

export const escape = (text: string): string =>
  text.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);

function page(title: string, body: string): string {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="dark">
<title>${escape(title)}</title>
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Geist:wght@400;500&family=Geist+Mono:wght@400&display=swap">
<style>
/* parcad.dev's tokens and button (site/app/app.css, site/app/components/frame.tsx); the host's front page in http.rs wears the same. */
:root{--bg:#000;--panel:#0a0b0d;--field:#16181c;--line:rgb(255 255 255/.1);--line-strong:rgb(255 255 255/.18);--ink:#eef1f5;--dim:#9aa2af;--accent:#6ea8fe;--bad:#ff6b6b;
--sans:"Geist",ui-sans-serif,system-ui,sans-serif;--mono:"Geist Mono",ui-monospace,"SF Mono",Menlo,monospace}
*{box-sizing:border-box}
body{margin:0;min-height:100vh;display:grid;place-items:center;background:var(--bg);color:var(--ink);font:15px/1.55 var(--sans);-webkit-font-smoothing:antialiased;padding:16px}
main{width:100%;max-width:380px;background:var(--panel);border:1px solid var(--line);padding:28px}
.brand{display:flex;align-items:center;gap:10px;font-size:15px;font-weight:500;letter-spacing:-.025em}
.brand img{width:32px;height:32px}
h1{font-size:19px;font-weight:500;margin:20px 0 6px}
p{margin:0 0 16px;color:var(--dim)}
form{margin:0}
label,.lead{display:block;font-size:13px;color:var(--dim);margin:0 0 6px}
.row{display:flex;gap:8px}
.row form{flex:1}
button,input{width:100%;height:36px}
button{font:400 11.5px/1 var(--mono);letter-spacing:.05em;text-transform:uppercase;border:1px solid var(--line-strong);background:transparent;color:var(--ink);cursor:pointer;margin-top:8px}
button:hover{background:rgb(255 255 255/.05)}
button.primary{background:var(--accent);border-color:var(--accent);color:#000}
button.primary:hover{background:#8dbbff}
button.quiet{border-color:var(--line);color:var(--dim)}
input{font:15px var(--sans);border:1px solid var(--line-strong);background:var(--field);color:var(--ink);padding:0 12px;box-shadow:inset 0 1px 3px rgb(0 0 0/.7)}
input:focus,button:focus-visible{outline:1px solid var(--accent);outline-offset:1px}
.or{display:flex;align-items:center;gap:10px;color:var(--dim);font:11.5px var(--mono);letter-spacing:.05em;text-transform:uppercase;margin:22px 0 12px}
.or::before,.or::after{content:"";flex:1;border-top:1px solid var(--line)}
.bad{color:var(--bad)}
.small{font-size:13px;margin:18px 0 0}
a{color:var(--accent);text-underline-offset:3px}
</style>
</head>
<body><main>
<div class="brand"><img src="/favicon.svg" alt="">ParCAD</div>
${body}
</main></body>
</html>`;
}

export interface SignInChoices {
  handle: string;
  /** This page's own path and query: where "use another address" leads back to. */
  back: string;
  client: string;
  domain?: string;
  google: boolean;
  github: boolean;
  test: boolean;
  error?: string;
}

/** A link back to the sign-in page, when `back` is one; anything else is dropped. */
const startOver = (back: string | undefined, label: string): string =>
  back?.startsWith("/authorize?") ? `<p class="small"><a href="${escape(back)}">${label}</a></p>` : "";

export function signIn(c: SignInChoices): string {
  const hidden = (method: string) =>
    `<input type="hidden" name="handle" value="${escape(c.handle)}"><input type="hidden" name="back" value="${escape(c.back)}"><input type="hidden" name="method" value="${method}">`;
  const provider = (method: string, label: string) =>
    `<form method="post" action="/authorize">${hidden(method)}<button class="primary" type="submit">${label}</button></form>`;
  const who = c.domain ? `${escape(c.client)} (${escape(c.domain)})` : escape(c.client);
  const providers = [c.google ? provider("google", "Google") : "", c.github ? provider("github", "GitHub") : ""].join("");
  return page(
    "Sign in to ParCAD",
    `<h1>Sign in</h1>
<p>${who} wants to build parts in your ParCAD account.</p>
${c.error ? `<p class="bad">${escape(c.error)}</p>` : ""}
${providers ? `<span class="lead">Continue with</span><div class="row">${providers}</div><div class="or">or by email</div>` : ""}
<form method="post" action="/authorize">${hidden("email")}
<label for="email">Email address</label>
<input id="email" type="email" name="email" required autocomplete="email">
<button type="submit">Email me a code</button></form>
${c.test ? `<div class="or">test sign-in, local only</div><form method="post" action="/authorize">${hidden("test")}<label for="test-email">Any address</label><input id="test-email" type="email" name="email" required><button type="submit">Sign in as this address</button></form>` : ""}
<form method="post" action="/authorize">${hidden("deny")}<button class="quiet" type="submit">Cancel</button></form>`,
  );
}

export function enterCode(state: string, email: string, back: string | undefined, error?: string): string {
  return page(
    "Enter your code",
    `<h1>Check your email</h1>
<p>We sent a six-digit code to ${escape(email)}. It works for 10 minutes.</p>
${error ? `<p class="bad">${escape(error)}</p>` : ""}
<form method="get" action="/callback/email">
<input type="hidden" name="state" value="${escape(state)}">
<label for="code">Code</label>
<input id="code" name="code" inputmode="numeric" pattern="[0-9]{6}" maxlength="6" required autocomplete="one-time-code">
<button class="primary" type="submit">Sign in</button></form>
${startOver(back, "Use another address")}`,
  );
}

export function inviteOnly(email: string, back: string | undefined): string {
  return page(
    "Invite only",
    `<h1>Not on the list yet</h1>
<p>The hosted ParCAD is letting in invited addresses only for a while, and ${escape(email)} is not one of them.</p>
<p>ParCAD also runs free on your own computer, with nothing to sign in to: <a href="https://parcad.dev">parcad.dev</a>.</p>
${startOver(back, "Sign in with another address")}`,
  );
}

export function problem(title: string, detail: string): string {
  return page(title, `<h1>${escape(title)}</h1><p>${escape(detail)}</p><p class="small">Start again from your app.</p>`);
}
