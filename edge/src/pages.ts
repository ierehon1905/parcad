/** The few pages a person sees while signing in. Every client-given string is escaped. */

export const escape = (text: string): string =>
  text.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);

function page(title: string, body: string, wide = false): string {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="dark">
<title>${escape(title)}</title>
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<style>
@font-face{font-family:"Geist";src:url(/fonts/geist.woff2) format("woff2");font-weight:100 900;font-display:swap}
@font-face{font-family:"Geist Mono";src:url(/fonts/geist-mono.woff2) format("woff2");font-weight:100 900;font-display:swap}
/* parcad.dev's tokens and button (site/app/app.css, site/app/components/frame.tsx); the host's front page in http.rs wears the same. */
:root{--bg:#000;--panel:#0a0b0d;--field:#16181c;--line:rgb(255 255 255/.1);--line-strong:rgb(255 255 255/.18);--ink:#eef1f5;--dim:#9aa2af;--accent:#6ea8fe;--bad:#ff6b6b;
--sans:"Geist",ui-sans-serif,system-ui,sans-serif;--mono:"Geist Mono",ui-monospace,"SF Mono",Menlo,monospace}
*{box-sizing:border-box}
body{margin:0;min-height:100vh;display:grid;place-items:center;background:var(--bg);color:var(--ink);font:15px/1.55 var(--sans);-webkit-font-smoothing:antialiased;padding:16px}
main{width:100%;max-width:380px;background:var(--panel);border:1px solid var(--line);padding:28px}
main.wide{max-width:640px}
h2{font-size:15px;font-weight:500;margin:24px 0 6px}
ul{margin:0 0 16px;padding-left:18px;color:var(--dim)}
li{margin:0 0 6px}
strong{color:var(--ink);font-weight:500}
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
<body><main${wide ? ' class="wide"' : ""}>
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
  chatgpt: boolean;
  /** A code by email: only when the server can send one. */
  email: boolean;
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
  const providers = [
    c.google ? provider("google", "Google") : "",
    c.github ? provider("github", "GitHub") : "",
    c.chatgpt ? provider("chatgpt", "ChatGPT") : "",
  ].join("");
  return page(
    "Sign in to ParCAD",
    `<h1>Sign in</h1>
<p>${who} wants to build parts in your ParCAD account.</p>
${c.error ? `<p class="bad">${escape(c.error)}</p>` : ""}
${providers ? `<span class="lead">Continue with</span><div class="row">${providers}</div>` : ""}
${providers && c.email ? `<div class="or">or by email</div>` : ""}
${c.email ? `<form method="post" action="/authorize">${hidden("email")}
<label for="email">Email address</label>
<input id="email" type="email" name="email" required autocomplete="email">
<button type="submit">Email me a code</button></form>` : ""}
${providers || c.email || c.test ? "" : `<p class="bad">No way in is set up on this server yet. Ask its operator.</p>`}
${c.test ? `<div class="or">test sign-in, local only</div><form method="post" action="/authorize">${hidden("test")}<label for="test-email">Any address</label><input id="test-email" type="email" name="email" required><button type="submit">Sign in as this address</button></form>` : ""}
<form method="post" action="/authorize">${hidden("deny")}<button class="quiet" type="submit">Cancel</button></form>
<p class="small">By continuing you agree to the <a href="/terms">terms</a> and have read the <a href="/privacy">privacy notice</a>.</p>`,
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

/** Who runs the hosted service, as the privacy notice and the terms both name it. */
const OPERATOR = "Leon Minasian, an individual in Spain";
const PRIVACY_MAIL = "privacy@parcad.dev";
const CONTACT_MAIL = "contact@parcad.dev";
const EFFECTIVE = "10 November 2026";

const mail = (address: string) => `<a href="mailto:${address}">${address}</a>`;

/** What the hosted service keeps about a person. Every sentence is a claim about the code and the box: change them together. */
export function privacy(): string {
  return page(
    "Privacy · ParCAD",
    `<h1>Privacy</h1>
<p>Effective ${EFFECTIVE}. The hosted ParCAD at mcp.parcad.dev is run by ${OPERATOR}, who decides how the data below is used (the controller). Write to ${mail(PRIVACY_MAIL)}. The ParCAD app and <code>parcad serve</code> on your own computer send us nothing.</p>
<h2>What we keep</h2>
<ul>
<li><strong>Your email address</strong>, once Google, GitHub or a code we email you has verified it, and the random account id it opens. GitHub shows us every address on your account so that we can pick the verified primary one; we keep only that one. We keep nothing else from these services and none of their tokens. Kept until your account is deleted.</li>
<li><strong>Sign-in records</strong>: which AI client you connected, where it asked to be sent back to, and hashes of its tokens. An access token lasts 7 days and a refresh token 30. While you sign in by email, the address you typed is held for up to an hour after the last code, with a hash of the code for 10 minutes.</li>
<li><strong>Your parts</strong>: their scripts, up to 50 earlier versions of each, the pictures and files made from them, and the parts you delete, which wait in a trash for 30 days. They are stored on our server in Germany under your account id, never under your address. A part nobody opens, builds or saves for 30 days is deleted. An exported file is deleted about a day after its link was last given out, and until then the link works for anyone who has it. The server is backed up every night and each backup is kept for 14 days, so a deleted part can outlive its deletion by that long.</li>
<li><strong>Usage against your limits</strong>: build time and exports per day and month, while your account exists.</li>
<li><strong>Network data</strong>: Cloudflare processes your IP address to deliver traffic and stop attacks. Our server does not log it. Its logs record start-up, errors and maintenance, with an account id at most, and are rotated out as they grow.</li>
<li><strong>Statistics</strong>: we may count daily totals of tool calls, builds, failures and active accounts. They keep no IP address, account id or per-person record, and read nothing from your device.</li>
</ul>
<p>Your parts hold whatever you or your AI client put in them, so keep other people's personal data and your own secrets out of them. We look inside a part only to run the service, to deal with abuse or a legal demand, or when you ask us to.</p>
<h2>Why</h2>
<p>To provide the service you signed up for: your account, your parts and their limits (GDPR Art. 6(1)(b)). And our legitimate interest in keeping it secure and in knowing whether it works: logs, abuse limits and statistics (Art. 6(1)(f)). An account needs an email address.</p>
<h2>Cookies</h2>
<p>Signing in sets two short cookies, one after the other, each gone within 10 minutes, that hold one sign-in together. They are strictly necessary, so they need no consent. There are no other cookies, and these pages load nothing from other sites.</p>
<h2>Who else handles it</h2>
<ul>
<li><strong>Cloudflare</strong> (USA) runs the sign-in, stores the address table and the sign-in records, and carries all traffic.</li>
<li><strong>Contabo</strong> (Germany) hosts the server.</li>
<li><strong>Resend</strong> (USA) delivers sign-in codes, when you sign in by email, and keeps a delivery log.</li>
</ul>
<p>They act on our instructions under data processing terms. Transfers outside the EEA rely on the EU-US Data Privacy Framework or the Standard Contractual Clauses. Google and GitHub handle their side of a sign-in as separate controllers.</p>
<p>Your AI client (Claude, ChatGPT or another) sends us your requests and receives our replies, scripts and pictures included. Its maker is a separate controller under its own privacy policy, and never gets your email address from us.</p>
<p>We do not sell personal data or share it for advertising. We disclose it otherwise only when the law requires it, or to protect people or the service.</p>
<h2>Google user data</h2>
<p>ParCAD's use and transfer to any other app of information received from Google APIs will adhere to the <a href="https://developers.google.com/terms/api-services-user-data-policy">Google API Services User Data Policy</a>, including the Limited Use requirements. We ask Google only for <code>openid</code> and <code>email</code>, use the address only to sign you in, never use it for advertising or to train AI models, and never pass it to an AI client.</p>
<h2>Your rights</h2>
<p>You can ask for a copy of your data, or ask us to correct, export, delete or restrict it, or object to how we use it: write to ${mail(PRIVACY_MAIL)}. We confirm the request through your account's address and answer within one month. You can also complain to the Spanish Data Protection Agency (<a href="https://www.aepd.es">aepd.es</a>) or to the authority where you live.</p>
<h2>Children</h2>
<p>ParCAD hosted is not for anyone under 16. We delete an account we learn belongs to someone younger.</p>
<h2>Security</h2>
<p>Tokens and codes are stored only as hashes, and all traffic is encrypted. If a breach puts your data at risk, we tell you and the authority as the law requires.</p>
<h2>Changes</h2>
<p>We post changes here with a new date, and email account holders before a change to how we use their data takes effect. Earlier versions are in ParCAD's public source, <code>edge/src/pages.ts</code>. The <a href="/terms">terms</a> cover the rest of using the service.</p>`,
    true,
  );
}

/** The terms of the hosted service. The liability clause keeps its exceptions: without them Spanish and German courts void it whole. */
export function terms(): string {
  return page(
    "Terms · ParCAD",
    `<h1>Terms</h1>
<p>Effective ${EFFECTIVE}. These terms are a contract between you and ${OPERATOR}, who runs the hosted ParCAD at mcp.parcad.dev. By signing in you accept them. You must be 16 or older, and if you are under 18 a parent or guardian must agree.</p>
<h2>The service</h2>
<p>ParCAD hosted is free and experimental. We may change, limit or stop it at any time. Limits apply, and a part nobody uses for 30 days is deleted. Keep your own copy of anything you need: the hosted service is not a place to store work.</p>
<h2>No warranty</h2>
<p>The service is provided as is and as available. Its geometry, measurements, fit checks and pictures can be wrong, and the AI clients that write most parts make mistakes.</p>
<h2>Check a part before you make it</h2>
<p>A part from ParCAD is a design, not a certified product. Before you print, machine, sell or use one, check its dimensions, fit, strength, material and safety yourself. Do not rely on ParCAD for a part whose failure could hurt someone or cause serious damage, such as anything load-bearing, under pressure, in a vehicle, medical or for children, without an independent engineering review.</p>
<h2>Your parts</h2>
<p>Your parts are yours. You give us only the licence we need to store, build, draw and return them to you. You confirm that you have the rights to what you put in them and that it is lawful.</p>
<h2>Acceptable use</h2>
<p>Do not use the service for anything unlawful or to design weapons or firearm parts. Do not get around its limits, for example with extra accounts, probe or escape its sandbox, overload it, or resell access to it. Report a security problem to ${mail(CONTACT_MAIL)} rather than exploiting it.</p>
<h2>Suspension and closing</h2>
<p>We may limit or close an account that breaks these terms or endangers the service, and we tell you why unless the law or security prevents it. You can stop using the service at any time and ask us to delete your account.</p>
<h2>Liability</h2>
<p>To the extent the law allows, we are not liable for indirect loss, lost data or profits, or damage from parts you make, and our total liability to you is 50 euros. Nothing here limits liability for intent or gross negligence, for death or personal injury, or anything else the law does not allow to be limited.</p>
<h2>Other services</h2>
<p>AI clients and sign-in providers are under their own terms. We are not responsible for what they do or say.</p>
<h2>Open source</h2>
<p>ParCAD is free software under the GNU AGPL 3.0. The source of the version running here is at <a href="https://github.com/ierehon1905/parcad">github.com/ierehon1905/parcad</a>.</p>
<h2>Contact</h2>
<p>Questions, notices and reports of illegal content: ${mail(CONTACT_MAIL)}. Privacy: ${mail(PRIVACY_MAIL)}, and the <a href="/privacy">privacy notice</a>.</p>
<h2>Changes and law</h2>
<p>We email account holders at least 30 days before a material change takes effect. Using the service after that means you accept it. Spanish law applies. If you are a consumer, you keep the mandatory protections and the courts of the country where you live.</p>`,
    true,
  );
}
