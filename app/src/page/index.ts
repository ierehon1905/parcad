/**
 * The playground's host, which is the page itself: a kernel in a Web Worker
 * and a project folder in IndexedDB. Only `backend.ts` imports this, and only
 * in the playground build.
 */

export * as kernel from "./kernel";
export * as store from "./store";
