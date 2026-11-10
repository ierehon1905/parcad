/** The sign-in page offers only the ways in this server can complete. */

import { expect, test } from "bun:test";
import { signIn } from "../src/pages";

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
