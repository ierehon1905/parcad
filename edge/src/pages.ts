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
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Geist:wght@400;500&family=Geist+Mono:wght@400;500&display=swap">
<style>
/* parcad.dev's tokens (site/app/app.css), whose hues are the app's own. */
:root{--bg:#000;--panel:#0a0b0d;--line:rgb(255 255 255/.1);--line-strong:rgb(255 255 255/.18);--ink:#eef1f5;--dim:#9aa2af;--faint:#5f6672;--accent:#6ea8fe;--bad:#ff6b6b;
--sans:"Geist",ui-sans-serif,system-ui,sans-serif;--mono:"Geist Mono",ui-monospace,"SF Mono",Menlo,monospace}
*{box-sizing:border-box}
body{margin:0;min-height:100vh;display:grid;place-items:center;background:var(--bg);color:var(--ink);font:15px/1.55 var(--sans);-webkit-font-smoothing:antialiased;padding:16px}
main{width:100%;max-width:380px;background:var(--panel);border:1px solid var(--line);padding:28px}
h1{font-size:19px;font-weight:500;margin:14px 0 6px}
p{margin:0 0 16px;color:var(--dim)}
img{width:36px;height:36px;display:block}
form{margin:0}
button,input{width:100%;height:40px;margin-top:8px}
button{font:500 11.5px/1 var(--mono);letter-spacing:.06em;text-transform:uppercase;border:1px solid var(--line-strong);background:transparent;color:var(--ink);cursor:pointer}
button:hover{background:rgb(255 255 255/.05)}
button.primary{background:var(--accent);border-color:var(--accent);color:#000}
button.primary:hover{background:#8dbbff}
button.quiet{border-color:var(--line);color:var(--dim)}
input{font:15px var(--sans);border:1px solid var(--line-strong);background:var(--bg);color:var(--ink);padding:0 12px}
input:focus,button:focus-visible{outline:1px solid var(--accent);outline-offset:1px}
.or{display:flex;align-items:center;gap:10px;color:var(--faint);font:11px var(--mono);letter-spacing:.06em;text-transform:uppercase;margin:20px 0 6px}
.or::before,.or::after{content:"";flex:1;border-top:1px solid var(--line)}
.bad{color:var(--bad)}
.small{font-size:13px;margin-top:18px}
a{color:var(--accent);text-underline-offset:3px}
</style>
</head>
<body><main>
<img src="/favicon.svg" alt="">
${body}
</main></body>
</html>`;
}

export interface SignInChoices {
  handle: string;
  client: string;
  domain?: string;
  google: boolean;
  github: boolean;
  test: boolean;
  error?: string;
}

export function signIn(c: SignInChoices): string {
  const hidden = (method: string) =>
    `<input type="hidden" name="handle" value="${escape(c.handle)}"><input type="hidden" name="method" value="${method}">`;
  const provider = (method: string, label: string) =>
    `<form method="post" action="/authorize">${hidden(method)}<button class="primary" type="submit">Continue with ${label}</button></form>`;
  const who = c.domain ? `${escape(c.client)} (${escape(c.domain)})` : escape(c.client);
  return page(
    "Sign in to ParCAD",
    `<h1>Sign in to ParCAD</h1>
<p>${who} wants to build parts in your ParCAD account.</p>
${c.error ? `<p class="bad">${escape(c.error)}</p>` : ""}
${c.google ? provider("google", "Google") : ""}
${c.github ? provider("github", "GitHub") : ""}
<div class="or">or by email</div>
<form method="post" action="/authorize">${hidden("email")}
<input type="email" name="email" placeholder="you@example.com" required autocomplete="email">
<button type="submit">Email me a code</button></form>
${c.test ? `<div class="or">test sign-in, local only</div><form method="post" action="/authorize">${hidden("test")}<input type="email" name="email" placeholder="any address" required><button type="submit">Sign in as this address</button></form>` : ""}
<form method="post" action="/authorize">${hidden("deny")}<button class="quiet" type="submit">Cancel</button></form>`,
  );
}

export function enterCode(state: string, email: string, error?: string): string {
  return page(
    "Enter your code",
    `<h1>Check your email</h1>
<p>We sent a six-digit code to ${escape(email)}. It works for 10 minutes.</p>
${error ? `<p class="bad">${escape(error)}</p>` : ""}
<form method="get" action="/callback/email">
<input type="hidden" name="state" value="${escape(state)}">
<input name="code" inputmode="numeric" pattern="[0-9]{6}" maxlength="6" placeholder="123456" required autocomplete="one-time-code">
<button class="primary" type="submit">Sign in</button></form>`,
  );
}

export function inviteOnly(email: string): string {
  return page(
    "Invite only",
    `<h1>Not on the list yet</h1>
<p>The hosted ParCAD is letting in invited addresses only for a while, and ${escape(email)} is not one of them.</p>
<p>ParCAD also runs free on your own computer, with nothing to sign in to: <a href="https://parcad.dev">parcad.dev</a>.</p>`,
  );
}

export function problem(title: string, detail: string): string {
  return page(title, `<h1>${escape(title)}</h1><p>${escape(detail)}</p><p class="small">Start again from your app.</p>`);
}
