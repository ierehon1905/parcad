/** The sign-in page offers only the ways in this server can complete. */

import { expect, test } from "bun:test";
import { privacy, signIn, terms } from "../src/pages";

const choices = { handle: "h", back: "/authorize?x=1", client: "a client", google: false, github: false, chatgpt: false, email: false, test: false };

test("a way in whose secrets are missing is not offered", () => {
  const page = signIn({ ...choices, github: true });
  expect(page).toContain(">GitHub</button>");
  expect(page).not.toContain("Email me a code");
  expect(page).not.toContain("or by email");
  expect(page).not.toContain("Google");
});

test("email is offered beside a provider once the server can send a code", () => {
  const page = signIn({ ...choices, github: true, email: true });
  expect(page).toContain("or by email");
  expect(page).toContain("Email me a code");
});

test("a server with no way in says so rather than showing an empty form", () => {
  const page = signIn(choices);
  expect(page).toContain("No way in is set up on this server yet");
  expect(page).not.toContain("Email me a code");
});

test("the privacy page names what is kept and carries Google's Limited Use statement", () => {
  const page = privacy();
  expect(page).toContain("Your email address");
  expect(page).toContain("Limited Use requirements");
  expect(page).toContain('class="wide"');
  expect(signIn({ ...choices, github: true })).toContain('href="/privacy"');
});

test("the terms keep the liability clause's exceptions and the sign-in page links both pages", () => {
  const page = terms();
  expect(page).toContain("Nothing here limits liability for intent or gross negligence");
  expect(page).toContain("Spanish law applies");
  expect(signIn({ ...choices, github: true })).toContain('href="/terms"');
});

test("no page sends its visitor to another site for a font", () => {
  for (const page of [privacy(), terms(), signIn({ ...choices, github: true })]) {
    expect(page).not.toContain("googleapis");
    expect(page).toContain("url(/fonts/geist.woff2)");
  }
});
