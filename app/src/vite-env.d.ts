/// <reference types="vite/client" />

/** The app's version, defined by vite.viewer.config.ts for the in-chat viewer. */
declare const __PARCAD_VERSION__: string;

interface ImportMetaEnv {
  /** The relay ParCAD web opens itself to agents through, e.g. `https://relay.example`. */
  readonly VITE_PARCAD_RELAY?: string;
  readonly VITE_PARCAD_PAGE?: string;
}
