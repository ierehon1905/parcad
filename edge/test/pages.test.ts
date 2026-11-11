/** The sign-in page offers only the ways in this server can complete. */

import { expect, test } from "bun:test";
import { accessCode, privacy, signIn, terms } from "../src/pages";

const choices = { handle: "h", back: "/authorize?x=1", client: "a client", google: false, github: false, chatgpt: false, email: false, access: false, test: false };

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

test("the access code is a quiet link only while the server has one, and its form leads back to the other ways in", () => {
  expect(signIn({ ...choices, github: true })).not.toContain("access code");
  const page = signIn({ ...choices, github: true, access: true });
  expect(page).toContain('<button class="link" type="submit">Have an access code?</button>');
  expect(page).toContain('value="access-code"');
  const form = accessCode("h", "/authorize?x=1", "That code is not right.");
  expect(form).toContain('name="code"');
  expect(form).toContain("That code is not right.");
  expect(form).toContain('<a href="/authorize?x=1">Other ways to sign in</a>');
  expect(accessCode("h", "https://evil.test/")).not.toContain("evil.test");
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
