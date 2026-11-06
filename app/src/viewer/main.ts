/**
 * The part inside a chat: an MCP App view, rendered by the client in a
 * sandboxed iframe beside an `open_project` call.
 *
 * The client hands this page the call's result, which names the project it
 * opened; the page asks the server for that project's mesh through
 * `view_part`, a tool only the page can call, and draws it with the window's
 * own viewport. No network: every byte arrives through the client, which is
 * all the iframe's default policy allows.
 */

import { App } from "@modelcontextprotocol/ext-apps/app-with-deps";
import { decompress } from "fzstd";

import "../style.css";
import { motionsOf } from "../motion";
import type { EvaluationSnapshot } from "../state";
import type { BodySpan, EdgeCurve, FaceMaterial, FaceRun } from "../viewport";
import { VIEWS, Viewport } from "../viewport";
import { mountControls, showPart } from "./controls";

/** Defined by the Draco decoder script vite.viewer.config.ts loads ahead of this one. */
declare const DracoDecoderModule: (options: object) => Promise<Draco>;

// The decoder's own API, as much of it as this page calls.
interface Draco {
  DecoderBuffer: new () => { Init(data: Int8Array, length: number): void };
  Decoder: new () => DracoDecoder;
  Mesh: new () => { num_points(): number; num_faces(): number };
  POSITION: number;
  NORMAL: number;
  GENERIC: number;
  DT_FLOAT32: number;
  DT_UINT32: number;
  HEAPF32: Float32Array;
  HEAPU32: Uint32Array;
  _malloc(bytes: number): number;
  _free(pointer: number): void;
  destroy(object: unknown): void;
}
interface DracoDecoder {
  DecodeBufferToMesh(buffer: unknown, mesh: unknown): { ok(): boolean; error_msg(): string };
  GetAttributeId(mesh: unknown, kind: number): number;
  GetAttribute(mesh: unknown, id: number): unknown;
  GetAttributeDataArrayForAllPoints(mesh: unknown, attribute: unknown, type: number, bytes: number, pointer: number): boolean;
  GetTrianglesUInt32Array(mesh: unknown, bytes: number, pointer: number): boolean;
}

/** What `view_part` packs beside the mesh: the window's reply, less its arrays. */
interface ViewedHeader {
  edges: EdgeCurve[];
  /** By kernel face number; `body` names the face's body in a part of several. */
  faces?: { material?: FaceMaterial; body?: string }[];
  snapshot: EvaluationSnapshot;
}

const status = document.getElementById("status")!;
const loading = document.getElementById("loading")!;
const loadingText = document.getElementById("loading-text")!;
const spinner = document.getElementById("spinner")!;
const viewport = new Viewport(document.getElementById("viewport")!);
mountControls(viewport, document.getElementById("on-view")!, document.getElementById("controls")!);
const draco = DracoDecoderModule({});
/** What `view_part` builds from: the project opened, or a script when the reply carried one. */
type Source = { project: string } | { script: string };
let shown: string | undefined;
/** The project `open_project` was called with, for a result that arrives without its structured half. */
let requested: string | undefined;

function bytesOf(base64: string): Uint8Array {
  return Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
}

/**
 * The mesh `view_part` encoded: positions, normals, and each vertex's kernel
 * face number, which Draco carries while it reorders the triangles. The
 * triangles are put back in runs by face, which is what the viewport picks and
 * colours by, and the faces in runs by `rank` — their body — so each body's
 * triangles are one span.
 */
async function decodeMesh(bytes: Uint8Array, rank: (face: number) => number) {
  const d = await draco;
  const buffer = new d.DecoderBuffer();
  const decoder = new d.Decoder();
  const mesh = new d.Mesh();
  try {
    buffer.Init(new Int8Array(bytes.buffer, bytes.byteOffset, bytes.length), bytes.length);
    const decoded = decoder.DecodeBufferToMesh(buffer, mesh);
    if (!decoded.ok()) throw new Error(`the part's mesh did not decode: ${decoded.error_msg()}`);
    const points = mesh.num_points();
    const triangles = mesh.num_faces();
    const copy = <T extends Float32Array | Uint32Array>(heap: () => T, count: number, fill: (pointer: number, bytes: number) => boolean): T => {
      const bytes = count * 4;
      const pointer = d._malloc(bytes);
      try {
        if (!fill(pointer, bytes)) throw new Error("the part's mesh is missing an attribute");
        return heap().slice(pointer / 4, pointer / 4 + count) as T;
      } finally {
        d._free(pointer);
      }
    };
    const attribute = (kind: number, type: number, heap: () => Float32Array | Uint32Array, components: number) => {
      const found = decoder.GetAttribute(mesh, decoder.GetAttributeId(mesh, kind));
      return copy(heap, points * components, (p, n) => decoder.GetAttributeDataArrayForAllPoints(mesh, found, type, n, p));
    };
    const positions = attribute(d.POSITION, d.DT_FLOAT32, () => d.HEAPF32, 3) as Float32Array;
    const normals = attribute(d.NORMAL, d.DT_FLOAT32, () => d.HEAPF32, 3) as Float32Array;
    const faceOf = attribute(d.GENERIC, d.DT_UINT32, () => d.HEAPU32, 1) as Uint32Array;
    const decodedIndices = copy(() => d.HEAPU32, triangles * 3, (p, n) => decoder.GetTrianglesUInt32Array(mesh, n, p));

    const order = Array.from({ length: triangles }, (_, t) => t).sort((a, b) => {
      const faceA = faceOf[decodedIndices[a * 3]];
      const faceB = faceOf[decodedIndices[b * 3]];
      return rank(faceA) - rank(faceB) || faceA - faceB;
    });
    const indices = new Uint32Array(triangles * 3);
    const faceRuns: FaceRun[] = [];
    order.forEach((t, at) => {
      indices.set(decodedIndices.subarray(t * 3, t * 3 + 3), at * 3);
      const face = faceOf[decodedIndices[t * 3]];
      const run = faceRuns[faceRuns.length - 1];
      if (run?.face === face) run.count++;
      else faceRuns.push({ face, start: at, count: 1 });
    });
    return { positions, normals, indices, faceRuns };
  } finally {
    d.destroy(mesh);
    d.destroy(decoder);
    d.destroy(buffer);
  }
}

function say(message: string, bad = false) {
  status.textContent = message;
  status.dataset.bad = String(bad);
}

/** The overlay over the view: a stage while working, the reason when it failed. */
function progress(message: string) {
  loading.hidden = false;
  spinner.hidden = false;
  loadingText.textContent = message;
  loadingText.dataset.bad = "false";
  say("");
}

function fail(message: string) {
  loading.hidden = false;
  spinner.hidden = true;
  loadingText.textContent = message;
  loadingText.dataset.bad = "true";
  say(message, true);
}

async function show(source: Source) {
  const key = JSON.stringify(source);
  if (key === shown) return;
  shown = key;
  progress("Building the part…");
  const result = await app.callServerTool({ name: "view_part", arguments: source });
  if (key !== shown) return;
  if (result.isError) {
    const text = result.content.find((block) => block.type === "text");
    fail(text && "text" in text ? text.text : "The part did not build.");
    return;
  }
  const reply = result.structuredContent as { format?: string; header?: string; draco?: string } | undefined;
  if (reply?.format !== "parcad-mesh/2" || !reply.header || !reply.draco) {
    fail(`The server sent a part this viewer cannot read (${reply?.format ?? "no format"}); update ParCAD.`);
    return;
  }
  progress("Unpacking the mesh…");
  // Decoding does not yield, so give the message a frame to paint first.
  await new Promise((resolve) => requestAnimationFrame(resolve));
  const header = JSON.parse(new TextDecoder().decode(decompress(bytesOf(reply.header)))) as ViewedHeader;
  const snapshot = header.snapshot;
  const named = snapshot.named_bodies ?? [];
  const bodyOfFace = (header.faces ?? []).map((face) => named.findIndex((body) => body.name === face.body));
  const mesh = await decodeMesh(bytesOf(reply.draco), (face) => bodyOfFace[face] ?? -1);
  if (key !== shown) return;
  const v = ([x, y, z]: [number, number, number]) => ({ x, y, z });
  const bounds = { min: v(snapshot.bounds_min), max: v(snapshot.bounds_max) };
  viewport.setGeometry(
    {
      ...mesh,
      edges: header.edges,
      faceMaterials: header.faces?.map((face) => face.material),
      bodies: bodySpans(mesh.faceRuns, bodyOfFace, named),
    },
    bounds,
  );
  viewport.look(VIEWS.threeQuarter, false);
  showPart(
    named.map((body) => ({ name: body.name, reference: body.reference === true })),
    motionsOf(snapshot.checks?.motions ?? []).filter((motion) => motion.from !== motion.to),
  );
  const size = snapshot.bounds_max.map((hi, i) => (hi - snapshot.bounds_min[i]).toFixed(1)).join(" × ");
  const volume = snapshot.volume_mm3 == null ? "" : ` · ${snapshot.volume_mm3.toFixed(1)} mm³`;
  loading.hidden = true;
  say(`${size} mm${volume}`);
}

/** Each named body's triangles, from the face runs once they are in body order. */
function bodySpans(runs: FaceRun[], bodyOfFace: number[], named: { name: string }[]): BodySpan[] {
  const spans: BodySpan[] = [];
  let current: BodySpan | undefined;
  for (const run of runs) {
    const body = named[bodyOfFace[run.face] ?? -1];
    if (!body) continue;
    if (current?.name !== body.name) {
      current = { name: body.name, start: run.start, count: 0 };
      spans.push(current);
    }
    current.count = run.start + run.count - current.start;
  }
  return spans;
}

const app = new App({ name: "ParCAD viewer", version: __PARCAD_VERSION__ });
app.addEventListener("toolinput", (input) => {
  const name = input.arguments?.name;
  if (typeof name === "string") requested = name;
});
app.addEventListener("toolresult", (result) => {
  if (result.isError) {
    const text = result.content?.find((block) => block.type === "text");
    fail(text && "text" in text ? text.text : "The part did not open.");
    return;
  }
  const reply = (result.structuredContent ?? replyText(result.content)) as { name?: unknown; script?: unknown } | undefined;
  // A reply carries the script only when asked (`script: true`); the project
  // it names is what the server builds from otherwise. A chat reloaded from
  // history may deliver the result without either (claude-ai-mcp #1158); the
  // project the call asked for is then what to build.
  const source: Source | undefined =
    typeof reply?.script === "string" ? { script: reply.script }
    : typeof reply?.name === "string" ? { project: reply.name }
    : requested !== undefined ? { project: requested }
    : undefined;
  if (!source) {
    fail("The server sent neither a project name nor a script with the part; update ParCAD.");
    return;
  }
  show(source).catch((e: unknown) => fail(e instanceof Error ? e.message : String(e)));
});

/** The reply's JSON from its text block, which a client keeps when it drops `structuredContent`. */
function replyText(content: { type: string; text?: string }[] | undefined): unknown {
  for (const block of content ?? []) {
    if (block.type !== "text" || !block.text?.trimStart().startsWith("{")) continue;
    try {
      return JSON.parse(block.text);
    } catch {
      // Not the reply; try the next block.
    }
  }
  return undefined;
}
progress("Waiting for the part…");
app.connect().catch((e: unknown) => fail(`Could not reach the chat client: ${e}`));
