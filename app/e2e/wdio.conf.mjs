import { fileURLToPath } from "node:url";

// `parcad` is the headless CLI binary; the Tauri package produces
// `parcad-app`, which is the process this suite must exercise.
const appBinaryPath = fileURLToPath(new URL("../../target/debug/parcad-app", import.meta.url));

/**
 * Run the compiled Tauri application, not Vite's browser-only geometry fixture.
 * The embedded provider is the portable option and is the only supported way
 * to drive a WKWebView on macOS.
 */
export const config = {
  runner: "local",
  specs: ["./bracket.e2e.mjs"],
  maxInstances: 1,
  capabilities: [{
    browserName: "tauri",
    "tauri:options": { application: appBinaryPath },
  }],
  services: [["@wdio/tauri-service", {
    appBinaryPath,
    driverProvider: "embedded",
    embeddedPort: 4445,
    startTimeout: 60_000,
    commandTimeout: 30_000,
  }]],
  framework: "mocha",
  mochaOpts: { ui: "bdd", timeout: 60_000 },
  reporters: ["spec"],
  logLevel: "warn",
  waitforTimeout: 30_000,
  connectionRetryTimeout: 90_000,
  connectionRetryCount: 1,
  // This app has one window. Mark that window as explicitly selected once so
  // the service does not attempt its optional IPC-based focus recovery before
  // every ordinary DOM/WebDriver command.
  before: async () => {
    await browser.switchToWindow(await browser.getWindowHandle());
  },
};
