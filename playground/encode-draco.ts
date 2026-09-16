#!/usr/bin/env bun
/**
 * Squeeze the mesh out of a recorded evaluation and into a Draco file.
 *
 *     bun playground/encode-draco.ts target/playground/first-part.json
 *
 * `playground/prebuild.sh` runs this. The evaluation keeps everything measured
 * — the snapshot, the edges, the faces — and loses only `positions`, `normals`
 * and `indices`, which are 97% of its bytes and are the one part a Draco file
 * carries better: about 0.3 MB in place of 3 MB gzipped.
 *
 * Draco quantises. Positions are held to 14 bits over the part's own bounding
 * box — 0.006 mm on a 104 mm part, finer than the 0.01 mm the mesher itself is
 * allowed — and this mesh is a stand-in that the kernel in the tab replaces
 * within seconds. Nothing measured travels through it: every number the page
 * reports comes from the snapshot, which is not touched here.
 */

import { readFileSync, writeFileSync } from "node:fs";
import draco3d from "draco3d";

const file = process.argv[2] ?? "target/playground/first-part.json";
const drc = file.replace(/\.json$/, ".drc");

const evaluated = JSON.parse(readFileSync(file, "utf8")) as {
  positions: number[];
  normals: number[];
  indices: number[];
  mesh?: string;
};

const encoderModule = await draco3d.createEncoderModule({});
const builder = new encoderModule.MeshBuilder();
const mesh = new encoderModule.Mesh();
const vertices = evaluated.positions.length / 3;

builder.AddFloatAttributeToMesh(mesh, encoderModule.POSITION, vertices, 3, new Float32Array(evaluated.positions));
builder.AddFloatAttributeToMesh(mesh, encoderModule.NORMAL, vertices, 3, new Float32Array(evaluated.normals));
builder.AddFacesToMesh(mesh, evaluated.indices.length / 3, new Uint32Array(evaluated.indices));

const encoder = new encoderModule.Encoder();
encoder.SetAttributeQuantization(encoderModule.POSITION, 14);
encoder.SetAttributeQuantization(encoderModule.NORMAL, 10);
// 10 is the slowest and smallest of Draco's settings, and this runs once, here.
encoder.SetSpeedOptions(0, 0);
encoder.SetEncodingMethod(encoderModule.MESH_EDGEBREAKER_ENCODING);

const buffer = new encoderModule.DracoInt8Array();
const length = encoder.EncodeMeshToDracoBuffer(mesh, buffer);
if (length <= 0) throw new Error("draco encoded nothing; the mesh did not survive the encoder");
const bytes = new Uint8Array(length);
for (let i = 0; i < length; i++) bytes[i] = buffer.GetValue(i);

encoderModule.destroy(buffer);
encoderModule.destroy(mesh);
encoderModule.destroy(builder);
encoderModule.destroy(encoder);

writeFileSync(drc, bytes);
const { positions, normals, indices, ...rest } = evaluated;
writeFileSync(file, JSON.stringify({ ...rest, mesh: "draco" }));

const mb = (n: number) => `${(n / 1e6).toFixed(2)} MB`;
console.log(
  `draco: ${vertices} vertices, ${indices.length / 3} triangles -> ${mb(bytes.length)} ` +
    `(the arrays were ${mb(JSON.stringify({ positions, normals, indices }).length)} of JSON)`,
);
