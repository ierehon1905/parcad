import{L as se,F as X,S as C,a as re,B as ie,b as Y,C as le,c as he}from"./index-BnZquGbb.js";var R={script:"kernel/7a4592ab417e/parcad-wasm.js",wasm:"kernel/7a4592ab417e/parcad_wasm.wasm",bytes:21247373};const V=6e4,F=new Set;let S={phase:"downloading",received:0,total:R.bytes};function D(t){S=t;for(const e of F)e(S)}function ce(t){return F.add(t),t(S),()=>F.delete(t)}let B;function Q(){return B??=(async()=>{const t=new URL(R.wasm,document.baseURI).href,e=await fetch(t);if(!e.ok||!e.body)throw new Error(`the geometry kernel did not download (${e.status} from ${t}); reload the page to try again`);const n=R.bytes,a=e.body.getReader(),r=[];let i=0;for(;;){const{done:h,value:d}=await a.read();if(h)break;r.push(d),i+=d.length,D({phase:"downloading",received:i,total:n})}D({phase:"compiling",received:i,total:n});const s=new Uint8Array(i);let l=0;for(const h of r)s.set(h,l),l+=h.length;const o=await WebAssembly.compile(s);return D({phase:"ready",received:i,total:n}),o})().catch(t=>{B=void 0;const e=t instanceof Error?t.message:String(t);throw D({...S,phase:"failed",error:e}),new Error(`${e}
This browser could not start the WebAssembly kernel. It needs WebAssembly exception handling: Chrome 95, Firefox 100 or Safari 15.2, or newer.`)}),B}function de(){Q().catch(()=>{})}let L,ue=0,Z=Promise.resolve();async function pe(){const t=await Q(),e=new Worker(new URL("/parcad/assets/kernel-worker-CrRtYMWW.js",import.meta.url),{type:"module"}),n={worker:e,stage:"starting up",ready:Promise.resolve()};n.ready=new Promise((r,i)=>{e.onmessage=s=>{s.data.kind==="ready"&&r(),s.data.kind==="died"&&i(new Error(H("starting up",s.data.detail)))},e.onerror=s=>i(new Error(H("starting up",s.message)))});const a=new URL(R.script,document.baseURI).href;return e.postMessage({kind:"start",module:t,script:a}),await n.ready,n}function A(){L?.worker.terminate(),L=void 0}const H=(t,e)=>`the geometry kernel crashed while ${t} (${e}). This is usually a dimension the operation cannot satisfy — a fillet larger than the material, a blend across a junction where several members meet or touch face-on, or a boolean between shapes that do not overlap. The next build starts a fresh kernel.`,me=t=>`the geometry kernel was still ${t} after ${V/1e3}s and was stopped. The WebAssembly kernel runs two to three times slower than the installed app, which gives a part 20 s by default and lets you raise it; a part this heavy is one to build there.`;function ee(t){const e=Z.then(()=>ge(t));return Z=e.catch(()=>{}),e}async function ge(t){L??=await pe().catch(a=>{throw A(),a});const e=L,n=ue++;return e.stage="reading the request",new Promise((a,r)=>{const i=window.setTimeout(()=>{A(),r(new Error(me(e.stage)))},V),s=()=>window.clearTimeout(i);e.worker.onmessage=l=>{const o=l.data;o.kind==="stage"?e.stage=o.stage:o.kind==="died"?(s(),A(),r(new Error(H(e.stage,o.detail)))):o.kind==="reply"&&o.id===n&&(s(),o.ok?a(o.bytes?{bytes:o.bytes}:{json:o.json}):r(new Error(o.message)))},e.worker.onerror=l=>{s(),A(),r(new Error(H(e.stage,l.message||"the worker failed")))},e.worker.postMessage({kind:"call",id:n,request:t})})}const ft=Object.freeze(Object.defineProperty({__proto__:null,call:ee,preload:de,watchLoad:ce},Symbol.toStringTag,{value:"Module"})),I=new WeakMap;class be extends se{constructor(e){super(e),this.decoderPath="",this.decoderConfig={},this.decoderBinary=null,this.decoderPending=null,this.workerLimit=4,this.workerPool=[],this.workerNextTaskID=1,this.workerSourceURL="",this.defaultAttributeIDs={position:"POSITION",normal:"NORMAL",color:"COLOR",uv:"TEX_COORD"},this.defaultAttributeTypes={position:"Float32Array",normal:"Float32Array",color:"Float32Array",uv:"Float32Array"}}setDecoderPath(e){return this.decoderPath=e,this}setDecoderConfig(e){return this.decoderConfig=e,this}setWorkerLimit(e){return this.workerLimit=e,this}load(e,n,a,r){const i=new X(this.manager);i.setPath(this.path),i.setResponseType("arraybuffer"),i.setRequestHeader(this.requestHeader),i.setWithCredentials(this.withCredentials),i.load(e,s=>{this.parse(s,n,r)},a,r)}parse(e,n,a=()=>{}){this.decodeDracoFile(e,n,null,null,C,a).catch(a)}decodeDracoFile(e,n,a,r,i=re,s=()=>{}){const l={attributeIDs:a||this.defaultAttributeIDs,attributeTypes:r||this.defaultAttributeTypes,useUniqueIDs:!!a,vertexColorSpace:i};return this.decodeGeometry(e,l).then(n).catch(s)}decodeGeometry(e,n){const a=JSON.stringify(n);if(I.has(e)){const o=I.get(e);if(o.key===a)return o.promise;if(e.byteLength===0)throw new Error("THREE.DRACOLoader: Unable to re-decode a buffer with different settings. Buffer has already been transferred.")}let r;const i=this.workerNextTaskID++,s=e.byteLength,l=this._getWorker(i,s).then(o=>(r=o,new Promise((h,d)=>{r._callbacks[i]={resolve:h,reject:d},r.postMessage({type:"decode",id:i,taskConfig:n,buffer:e},[e])}))).then(o=>this._createGeometry(o.geometry));return l.catch(()=>!0).then(()=>{r&&i&&this._releaseTask(r,i)}),I.set(e,{key:a,promise:l}),l}_createGeometry(e){const n=new ie;e.index&&n.setIndex(new Y(e.index.array,1));for(let a=0;a<e.attributes.length;a++){const r=e.attributes[a],i=r.name,s=r.array,l=r.itemSize,o=new Y(s,l);i==="color"&&(this._assignVertexColorSpace(o,r.vertexColorSpace),o.normalized=!(s instanceof Float32Array)),n.setAttribute(i,o)}return n}_assignVertexColorSpace(e,n){if(n!==C)return;const a=new le;for(let r=0,i=e.count;r<i;r++)a.fromBufferAttribute(e,r),he.colorSpaceToWorking(a,C),e.setXYZ(r,a.r,a.g,a.b)}_loadLibrary(e,n){const a=new X(this.manager);return a.setPath(this.decoderPath),a.setResponseType(n),a.setWithCredentials(this.withCredentials),new Promise((r,i)=>{a.load(e,r,void 0,i)})}preload(){return this._initDecoder(),this}_initDecoder(){if(this.decoderPending)return this.decoderPending;const e=typeof WebAssembly!="object"||this.decoderConfig.type==="js",n=[];return e?n.push(this._loadLibrary("draco_decoder.js","text")):(n.push(this._loadLibrary("draco_wasm_wrapper.js","text")),n.push(this._loadLibrary("draco_decoder.wasm","arraybuffer"))),this.decoderPending=Promise.all(n).then(a=>{const r=a[0];e||(this.decoderConfig.wasmBinary=a[1]);const i=fe.toString(),s=["/* draco decoder */",r,"","/* worker */",i.substring(i.indexOf("{")+1,i.lastIndexOf("}"))].join(`
`);this.workerSourceURL=URL.createObjectURL(new Blob([s]))}),this.decoderPending}_getWorker(e,n){return this._initDecoder().then(()=>{if(this.workerPool.length<this.workerLimit){const r=new Worker(this.workerSourceURL);r._callbacks={},r._taskCosts={},r._taskLoad=0,r.postMessage({type:"init",decoderConfig:this.decoderConfig}),r.onmessage=function(i){const s=i.data;switch(s.type){case"decode":r._callbacks[s.id].resolve(s);break;case"error":r._callbacks[s.id].reject(s);break;default:console.error('THREE.DRACOLoader: Unexpected message, "'+s.type+'"')}},this.workerPool.push(r)}else this.workerPool.sort(function(r,i){return r._taskLoad>i._taskLoad?-1:1});const a=this.workerPool[this.workerPool.length-1];return a._taskCosts[e]=n,a._taskLoad+=n,a})}_releaseTask(e,n){e._taskLoad-=e._taskCosts[n],delete e._callbacks[n],delete e._taskCosts[n]}debug(){console.log("Task load: ",this.workerPool.map(e=>e._taskLoad))}dispose(){for(let e=0;e<this.workerPool.length;++e)this.workerPool[e].terminate();return this.workerPool.length=0,this.workerSourceURL!==""&&URL.revokeObjectURL(this.workerSourceURL),this}}function fe(){let t,e;onmessage=function(s){const l=s.data;switch(l.type){case"init":t=l.decoderConfig,e=new Promise(function(d){t.onModuleLoaded=function(p){d({draco:p})},DracoDecoderModule(t)});break;case"decode":const o=l.buffer,h=l.taskConfig;e.then(d=>{const p=d.draco,c=new p.Decoder;try{const u=n(p,c,new Int8Array(o),h),g=u.attributes.map(b=>b.array.buffer);u.index&&g.push(u.index.array.buffer),self.postMessage({type:"decode",id:l.id,geometry:u},g)}catch(u){console.error(u),self.postMessage({type:"error",id:l.id,error:u.message})}finally{p.destroy(c)}});break}};function n(s,l,o,h){const d=h.attributeIDs,p=h.attributeTypes;let c,u;const g=l.GetEncodedGeometryType(o);if(g===s.TRIANGULAR_MESH)c=new s.Mesh,u=l.DecodeArrayToMesh(o,o.byteLength,c);else if(g===s.POINT_CLOUD)c=new s.PointCloud,u=l.DecodeArrayToPointCloud(o,o.byteLength,c);else throw new Error("THREE.DRACOLoader: Unexpected geometry type.");if(!u.ok()||c.ptr===0)throw new Error("THREE.DRACOLoader: Decoding failed: "+u.error_msg());const b={index:null,attributes:[]};for(const w in d){const x=self[p[w]];let T,k;if(h.useUniqueIDs)k=d[w],T=l.GetAttributeByUniqueId(c,k);else{if(k=l.GetAttributeId(c,s[d[w]]),k===-1)continue;T=l.GetAttribute(c,k)}const q=r(s,l,c,w,x,T);w==="color"&&(q.vertexColorSpace=h.vertexColorSpace),b.attributes.push(q)}return g===s.TRIANGULAR_MESH&&(b.index=a(s,l,c)),s.destroy(c),b}function a(s,l,o){const d=o.num_faces()*3,p=d*4,c=s._malloc(p);l.GetTrianglesUInt32Array(o,p,c);const u=new Uint32Array(s.HEAPF32.buffer,c,d).slice();return s._free(c),{array:u,itemSize:1}}function r(s,l,o,h,d,p){const c=p.num_components(),g=o.num_points()*c,b=g*d.BYTES_PER_ELEMENT,w=i(s,d),x=s._malloc(b);l.GetAttributeDataArrayForAllPoints(o,p,w,b,x);const T=new d(s.HEAPF32.buffer,x,g).slice();return s._free(x),{name:h,array:T,itemSize:c}}function i(s,l){switch(l){case Float32Array:return s.DT_FLOAT32;case Int8Array:return s.DT_INT8;case Int16Array:return s.DT_INT16;case Int32Array:return s.DT_INT32;case Uint8Array:return s.DT_UINT8;case Uint16Array:return s.DT_UINT16;case Uint32Array:return s.DT_UINT32}}}var U={url:"kernel/7a4592ab417e/first-part.json",mesh:"kernel/7a4592ab417e/first-part.drc",decoder:"kernel/7a4592ab417e/draco/",part:"twisted-planter",script:`// Twisted planter and its drip saucer, printed as two parts.
//
// The planter is a six-point star lofted through nine sections, each wider
// and turned further than the last, so its walls come out as twisted facets.
// The saucer is a dish of smooth bumps in rings of 1, 6, 12 and 18, which the
// planter stands on.

const points = 6;
const height = 90;
const twist = 60; // degrees, base to rim
const layers = 8;
const wall = 3; // inset across the star; the facets' own wall measures 2.0 mm
const floor = 3;

// At height z: the star's tip radius, flaring fastest near the base, and its turn.
const radiusAt = (z) => 34 + 18 * Math.sin((z / height) * (Math.PI / 2));
const turnAt = (z) => (z / height) * twist;

function star(z, inset) {
  const outline = Array.from({ length: 2 * points }, (_, i) => {
    const r = (i % 2 === 0 ? 1 : 0.78) * radiusAt(z) - inset;
    const angle = ((i * 180) / points + turnAt(z)) * (Math.PI / 180);
    return [r * Math.cos(angle), r * Math.sin(angle)];
  });
  return { z, outline };
}

// Section heights shared by the outside and the cavity, so their facets stay parallel.
const levels = Array.from({ length: layers + 1 }, (_, i) => (i * height) / layers);

const outside = loft(levels.map((z) => star(z, 0)));
const cavity = loft([floor, ...levels.filter((z) => z > floor), height + 1].map((z) => star(z, wall)));

const drain = cylinder(2.5, 3 * floor);

const planter = outside
  .cut(
    cavity, // 1 mm past the rim, so the top opens cleanly
    ...polar(6, 14, { straddle: true }).map(([x, y]) => drain.at(x, y)), // between the saucer's first ring of bumps
  )
  .tag("planter");

// Wide enough that the 34 mm star base sits inside with room for runoff.
const saucerRadius = 42;
const saucerFloor = 2;
const bumpRadius = 5.4;
const bumpHeight = 2.4;
const ringSpacing = 11;
const ringCount = 3;

// Smootherstep: no slope and no curvature where a bump leaves the floor or at its top.
const ease = (x) => x * x * x * (x * (6 * x - 15) + 10);

function bump() {
  const z = (r) => saucerFloor + bumpHeight * (1 - ease(r / bumpRadius));
  const profile = [0.92, 0.82, 0.7, 0.58, 0.46, 0.34, 0.2].map((t) => [t * bumpRadius, z(t * bumpRadius)]);
  return revolve([
    [0, 0],
    [bumpRadius, 0],
    [bumpRadius, saucerFloor],
    { spline: profile, start: [-1, 0], end: [-1, 0] },
    [0, saucerFloor + bumpHeight],
  ]);
}

const dish = revolve([
  [0, 0],
  { at: [saucerRadius + 2, 0], round: 3 },
  [saucerRadius + 2, 11],
  { through: [saucerRadius + 1, 12] },
  [saucerRadius, 11],
  { at: [saucerRadius, saucerFloor], round: 2 },
  [0, saucerFloor],
]);

// One bump in the middle, then rings of 6, 12, 18: each ring's circumference grows
// by 2π·spacing, so six more keeps every bump the same distance from its neighbours.
// Even rings sit half a step round from the odd ones.
const one = bump();
const bumps = [one];
for (let ring = 1; ring <= ringCount; ring++) {
  const count = 6 * ring;
  const start = ring % 2 === 0 ? 180 / count : 0;
  for (const [x, y] of polar(count, ring * ringSpacing, { start })) bumps.push(one.at(x, y));
}

const saucer = dish.union(...bumps);

return {
  planter: planter.at(0, 0, saucerFloor + bumpHeight),
  saucer,
};
`};const y=typeof U>"u"?void 0:U;let G=!1,N;const te=(t,e)=>!!y&&t===y.part&&e===y.script;async function we(t,e){if(!y||G||!te(t,e))return;G=!0;const n=await fetch(new URL(y.url,document.baseURI)).catch(()=>{});if(!n?.ok)return;const a=await n.json().catch(()=>{});if(!a)return;const r=await ye(y.mesh,y.decoder).catch(()=>{});if(r)return{...a,...r,shipped:!0}}async function ye(t,e){const n=await fetch(new URL(t,document.baseURI));if(!n.ok)throw new Error(`the shipped mesh is not there: ${n.status}`);const a=await n.arrayBuffer(),r=new be().setDecoderPath(new URL(e,document.baseURI).href),i=await new Promise((s,l)=>r.parse(a,s,l));return r.dispose(),{positions:i.getAttribute("position").array,normals:i.getAttribute("normal").array,indices:i.getIndex().array}}function ve(t){return N=ee({op:"evaluate",graph:t}).then(e=>{if(!("json"in e))throw new Error("the kernel answered the rebuild with bytes, not an evaluation");return e.json}),N}function xe(t,e){return te(t,e)?N:void 0}const wt=Object.freeze(Object.defineProperty({__proto__:null,inFlight:xe,rebuild:ve,take:we},Symbol.toStringTag,{value:"Module"})),Te=`// parcad — everything is millimetres, Z is up.
// Primitives are centred on the origin; place them with .at(x, y, z).
// The script must return a shape.

const t = 8;          // plate thickness
const w = 80;         // plate width
const d = 60;         // plate depth
const wallH = 40;     // upright height

const plate = box(w, d, t).tag("plate");

const wall = box(t, d, wallH)
  .at(-(w - t) / 2, 0, (wallH + t) / 2 - t / 2)
  .tag("wall");

// A blended union rounds the seam. The blend is the fillet.
const body = union(plate, wall, { blend: 6 }).tag("body");

const hole = cylinder(3, t * 4);

const drilled = body
  .cut(...grid(2, 2, 36, 40).map(([x, y]) => hole.at(x, y)))
  .tag("mount_holes");

// Every selected edge is a circular hole rim bordering the upward-facing top face.
// The same query keeps selecting just the top rim if a hole moves or more
// holes are added; the lower rims and vertical walls are not selected.
const roundedHoles = drilled
  .edges({
    generatedBy: "mount_holes",
    curve: "circle",
    role: "hole",
    adjacentTo: { faceNormal: "+z" },
  })
  .expect({ count: 4 })
  .fillet(0.8)
  .tag("top_hole_rims");

// This names the leftmost underside edge rather than a kernel edge ID. It is
// deliberately away from the corner treatment below, so each feature remains
// independently inspectable in the viewport and source.
const chamferedBase = roundedHoles
  .edges("<X and <Z and |Y")
  .expect({ count: 1 })
  .chamfer(1)
  .tag("left_base_chamfer");

// This names a corner rather than a kernel vertex ID. The exact backend expands
// it to its three incident edges, then makes one rolling-ball corner fillet.
return chamferedBase
  .vertices(">X and >Y and <Z")
  .expect({ count: 1 })
  .fillet(2)
  .tag("outer_corner_round");
`,ke=`// A cast machine foot: a drafted pedestal on a drafted base, bolted down
// through four holes and tapped on top for the equipment it carries.
//
// This is the first part here that could actually be cast. Every wall leans by
// a couple of degrees so the pattern releases from the sand — before \`draft\`
// existed the same shape had vertical walls, which is a part a foundry sends
// back. The hole diameters are not literals either: \`holeFor("M10", ...)\` knows
// the ISO 273 clearance is 11.0 and \`{ tapped: true }\` knows the M8 coarse tap
// drill is 6.8.

const baseX = 120;
const baseY = 80;
const baseT = 14;
const baseDraft = 2;     // degrees, sand casting

const padX = 60;
const padY = 40;
const rise = 46;         // top of the pedestal above the floor
const padDraft = 3;

const boltX = 90;        // bolt centres in the base
const boltY = 56;
const tapX = 36;         // tapped centres on the pad
const tapY = 20;

const rect = (x, y) => [
  [-x / 2, -y / 2],
  [x / 2, -y / 2],
  [x / 2, y / 2],
  [-x / 2, y / 2],
];

const base = extrude(rect(baseX, baseY), baseT, { draft: baseDraft }).at(0, 0, baseT / 2);

// The pedestal starts inside the base rather than on top of it: a blended union
// of two solids that only touch on a face cannot be blended
// (docs/GOTCHAS.md), and a casting has a generous root radius there anyway.
const buried = 6;
const pedestalH = rise - baseT + buried;
const pedestal = extrude(rect(padX, padY), pedestalH, { draft: padDraft }).at(
  0,
  0,
  baseT - buried + pedestalH / 2,
);

const casting = union(base, pedestal, { blend: 5 }).tag("casting");

// Four M10 through the base, four M8 tapped into the pad. Both cutters enter
// the face they are placed on and run past it — that is what \`holeFor\` does, and
// why no example has to say so in a comment any more.
const bolts = grid(2, 2, boltX, boltY).map(([x, y]) => holeFor("M10", baseT, { through: true }).at(x, y, baseT));
const taps = grid(2, 2, tapX, tapY).map(([x, y]) => holeFor("M8", 16, { tapped: true }).at(x, y, rise));

const machined = casting.cut(...bolts, ...taps).tag("machined");

// The four bolt holes where they break out of the underside, which is the face
// that sits on the floor: a burr there rocks the machine. The tapped holes are
// blind and the pedestal top is a machined pad, so \`z: "min"\` reaches exactly
// the four rims meant here.
return machined
  .edges({ generatedBy: "machined", curve: "circle", role: "hole", at: { z: "min" } })
  .expect({ count: 4 })
  .chamfer(0.8)
  .tag("seat_deburr");
`,_e=`// A threaded rod-end clevis: a shank with spanner flats, and a two-armed fork
// for a 10 mm pin.
//
// This is the part that shows what \`mirror\` is for. The fork is symmetric, so
// one arm is authored and the other is its reflection — before mirror existed
// the second arm was a second copy of the same arithmetic with the signs
// changed by hand, which is how a part ends up with one arm 13 mm thick and the
// other 13.5. \`ngon\` is the other new one: spanner flats are quoted across the
// flats, and that is what the call says.

const shankDia = 20;
const shankLen = 26;
const drill = tapDrill("M12"); // 10.2, for M12 x 1.75
const threadDepth = 20;
const flats = 17;        // across the flats, a 17 mm spanner
const flatsLen = 12;

const crownW = 40;
const crownD = 24;
const crownT = 8;

const armT = 13;         // each arm
const gap = 14;          // the tongue that fits between them
const armH = 26;
const pinDia = 10;
const pinZ = crownT + armH - 9;

// The shank hangs below the crown. It runs up past z = 0 so the crown has
// something to sit *in* rather than *on*: a blended union of two solids that
// only touch on a face cannot be blended (docs/GOTCHAS.md).
const shank = cylinder(shankDia / 2, shankLen + 3).at(0, 0, (-shankLen + 3) / 2);

// Spanner flats: a band of everything outside the hexagon, taken off the round
// shank. The hexagon is the thing being kept, so the cutter is the band with
// the hexagon removed from it — which reads as what a machinist would say.
const band = box(shankDia * 2, shankDia * 2, flatsLen)
  .at(0, 0, -shankLen + flatsLen / 2)
  .cut(ngon(6, flats, flatsLen, { across: "flats" }).at(0, 0, -shankLen + flatsLen / 2));

const crown = box(crownW, crownD, crownT).at(0, 0, crownT / 2);

// One arm, authored on +X, and its reflection. \`union\` is separate from
// \`mirror\` on purpose: the reflection alone is the left-hand part.
const arm = box(armT, crownD, armH).at(gap / 2 + armT / 2, 0, crownT + armH / 2);
const fork = union(arm, arm.mirror("x"));

const body = union(shank, crown, fork).tag("body");

// The tapped hole is drawn as its tap drill — there is no thread op, and a
// stack of tori pretending to be one is the approximation this project refuses.
// It starts below the end face so the cutter crosses it rather than ending on
// it, and reaches the called-out depth.
const thread = cylinder(drill / 2, threadDepth + 4)
  .at(0, 0, -shankLen - 4 + (threadDepth + 4) / 2)
  .tag("tap_drill");

const turned = body.cut(band, thread).tag("turned");

// Across both arms, and out the far side of each.
const pin = cylinder(pinDia / 2, crownW * 2).rotate("y", 90).at(0, 0, pinZ);
const machined = turned.cut(pin).tag("machined");

// Four pin-hole rims: two arms, two faces each, and no others because the bore
// is its own cut. The count is the assertion that the pin hole still goes all
// the way through — an arm moved outboard of it would leave two.
return machined
  .edges({ generatedBy: "machined", curve: "circle", role: "hole" })
  .expect({ count: 4 })
  .chamfer(0.5)
  .tag("pin_lead_in");
`,De=`// A bolted cover plate with a turned spigot and countersunk screws.
//
// This is the part that could not be modelled at all until \`revolve\` existed:
// both the spigot's taper and the screw countersinks are cones, and a cone is a
// revolved triangle. Everything here is still exact — a taper authored as a
// section is the shape a lathe leaves, not an approximation of it.

const plate = 90;      // square
const plateT = 8;
const spigotDia = 40;  // where it enters the housing bore
const spigotTip = 36;  // lead-in taper at the far end
const spigotH = 14;
const bore = 20;
const screw = 5.5;     // clearance for M5
const head = 10.4;     // M5 countersunk head diameter
const boltSquare = 70;

const body = box(plate, plate, plateT).at(0, 0, plateT / 2).tag("plate");

// The spigot hangs below the plate and tapers, so it finds the bore on the way
// in. \`cone\` takes radii, like \`cylinder\` — the diameters above are halved here
// rather than at the top, because a drawing calls out the diameter and it is
// worth having the numbers in the file match it. It is centred on the origin
// like every other primitive, so it is placed by its middle.
//
// It reaches up into the plate rather than butting onto its underside: a
// blended union between solids that only touch on a face is refused
// (docs/GOTCHAS.md), and a cast cover has a root radius there anyway.
const spigot = cone(spigotTip / 2, spigotDia / 2, spigotH)
  .at(0, 0, -spigotH / 2 + 2)
  .tag("spigot");

const casting = union(body, spigot, { blend: 3 }).tag("casting");

// A countersink is a cone too, and this one is sized the way a drawing calls it
// out: by head diameter and included angle. 90° is the metric standard.
const sink = countersink(head, 90);

const screwHoles = grid(2, 2, boltSquare, boltSquare).flatMap(([x, y]) => [
  cylinder(screw / 2, plateT * 4).at(x, y),
  // The countersink sits on the top face, which is where the head lands.
  sink.at(x, y, plateT),
]);

const machined = casting
  .cut(cylinder(bore / 2, (plateT + spigotH) * 4), ...screwHoles)
  .tag("machined");

// The four countersinks meet the top face on a circle of the head diameter, and
// the bore rim is a fifth. Deburring all five in one call is what a machinist
// would do, and the count is the assertion that all four screws are still
// there — a hole lost to an edit fails here rather than in assembly.
//
// No \`role: "hole"\` here, unlike the other examples: that term does not
// recognise a *conical* opening, so adding it drops the four countersink rims
// and leaves only the bore. See docs/DSL_GAPS.md.
return machined
  .edges({
    generatedBy: "machined",
    curve: "circle",
    at: { z: "max" },
  })
  .expect({ count: 5 })
  .chamfer(0.4)
  .tag("top_face_deburr");
`,Ae=`// diamond v19 — recreated from a Fusion 360 export.
//
// A round brilliant cut, and every one of its 57 faces is a plane: a table,
// 8 star facets, 8 crown mains, 16 upper girdle facets, 16 lower girdle
// facets and 8 pavilion mains meeting in a point (no culet). Fusion built it
// with BoundaryFill — planes arranged in a circular pattern, then the
// enclosed region kept. parcad has no BoundaryFill and does not need one
// here: the solid is convex, so it *is* the intersection of its 57
// half-spaces, and an intersection of rotated boxes is exact in both
// backends.
//
//   volume   196778.06 mm^3 against Fusion's 196778.06   (matches)
//   area     19789.2 mm^2 against Fusion's 19789.2       (matches)
//   bounding box  101.96 x 101.96 x 59.4 mm              (exact; axes remapped)
//   faces    57, all planes, the same six rings
//
// Ground truth is reference/fusion/diamond-v19/ (gitignored). The ring data
// below — tilt of each facet ring's normal from vertical, its plane offset
// from the stone's axis point, and its azimuths — was read off the PLANE
// entities in the STEP export, and the polytope they bound was measured
// against Fusion's own numbers before this script was written.
//
// Two things the export contains that this deliberately does not:
// - a second, identical diamond (CopyPasteBody + Move): a copy, not geometry;
// - a zero-volume cylindrical construction surface (r = 50) left over from
//   the BoundaryFill. The STEP body is additionally trimmed by it into a
//   73-face variant; Fusion's measured body — the specification — is the
//   57-plane solid, which is what this builds.
//
// Axes are remapped from Fusion, whose stone axis was +Y: parcad z = Fusion
// y, so the table faces +Z and the girdle lies in the XY plane.

// Each ring: cosine of the normal's angle to +Z (nz), the plane's signed
// offset d along its own normal (x . n = d), how many facets, and the first
// facet's azimuth. Values are the STEP's own, full precision.
const RINGS = [
  { nz: 1.0, d: 16.2, count: 1, az0: 0 }, // table
  { nz: 0.925444781, d: 24.268313074911, count: 8, az0: 22.5 }, // star
  { nz: 0.833927619, d: 28.134283972459, count: 8, az0: 0 }, // crown mains
  { nz: 0.775787064, d: 31.549739723196, count: 16, az0: 11.25 }, // upper girdle
  { nz: -0.749604959, d: 33.094274660592, count: 16, az0: 11.25 }, // lower girdle
  { nz: -0.762917856, d: 32.958051375205, count: 8, az0: 45 }, // pavilion mains
];

// A half-space is a box big enough to swallow the whole stone from any
// direction, rotated so its +Z face lies in the facet's plane. The stone
// spans ~102 mm; 400 leaves no face of the box near the result.
const S = 400;

const facets = [];
for (const { nz, d, count, az0 } of RINGS) {
  const sin = Math.sqrt(Math.max(0, 1 - nz * nz));
  for (let i = 0; i < count; i++) {
    const az = ((az0 + (i * 360) / count) * Math.PI) / 180;
    const n = { x: sin * Math.cos(az), y: sin * Math.sin(az), z: nz };
    let half = box(S, S, S);
    const tilt = (Math.acos(n.z) * 180) / Math.PI;
    if (tilt > 1e-9) {
      // Rotate +Z onto n about the horizontal axis z x n. No facet points
      // straight down, so the axis never degenerates.
      const len = Math.hypot(-n.y, n.x);
      half = half.rotate({ x: -n.y / len, y: n.x / len, z: 0 }, tilt);
    }
    // Push the box back along the normal until its +Z face is the plane.
    facets.push(half.at((d - S / 2) * n.x, (d - S / 2) * n.y, (d - S / 2) * n.z));
  }
}

return intersect(...facets).tag("stone");
`,Re=`// An instrument fascia: a display module drops into a milled seat in the front
// face, and looks out through an aperture cut all the way to the back.
//
// Every other part here cuts *through* something, so only one end of a cutter
// is ever in question — "a tool ending exactly on a face leaves a zero-thickness
// sliver", which is the exit. A recess is the other half of that rule: it
// enters a face and stops inside, and the entry needs the same overshoot. A
// seat cutter whose outer face lands four microns short of the face it enters
// makes no seat at all — it makes a sealed void under a feather edge, and the
// part still measures watertight, right size, right shape. docs/GOTCHAS.md,
// "A cut needs overlength at both ends", has the measurements.
//
// \`over\` is that overshoot, and it appears at the entry of the seat and at both
// ends of the aperture. It is 0.5 mm because that is what \`holeFor\` uses.

const panelW = 120;
const panelD = 80;
const panelT = 6;

const seatW = 70;        // the module's outline, plus assembly clearance
const seatD = 52;
const seatDepth = 1.6;   // the module's bezel, so its face finishes flush

const apertureW = 58;    // the active area, opened out to clear the glass
const apertureD = 44;

const endMill = 3;       // a 6 mm cutter: no milled pocket has sharper corners
const boltX = 104;
const boltY = 64;

const over = 0.5;

const panel = box(panelW, panelD, panelT).at(0, 0, panelT / 2).tag("panel");

// A milled pocket is not a box: its corners carry the cutter's radius. Two
// crossed slabs and four corner cylinders is that shape exactly, which is the
// same reason \`pillow-block.js\` builds its slots out of three primitives.
const pocket = (w, d, h) =>
  union(
    box(w - 2 * endMill, d, h),
    box(w, d - 2 * endMill, h),
    ...grid(2, 2, w - 2 * endMill, d - 2 * endMill).map(([x, y]) =>
      cylinder(endMill, h).at(x, y),
    ),
  );

// The seat stops inside the panel, so the entry is the end that has to
// overshoot: the cutter is \`over\` taller than the seat is deep and stands that
// much proud of the front face. The floor lands where it was going to anyway.
const seat = pocket(seatW, seatD, seatDepth + over)
  .at(0, 0, panelT + over - (seatDepth + over) / 2)
  .tag("seat");

// The aperture goes all the way through, so it overshoots at both ends.
const aperture = pocket(apertureW, apertureD, panelT + 2 * over)
  .at(0, 0, panelT / 2)
  .tag("aperture");

const bolts = grid(2, 2, boltX, boltY).map(([x, y]) =>
  holeFor("M4", panelT, { through: true }).at(x, y, panelT),
);

// The seat is cut in its own tagged step so the lead-in below can name it.
// Rolled into one cut with the bolt holes, \`generatedBy\` would reach their rims
// as well and the count would stop meaning anything — docs/GOTCHAS.md,
// "\`generatedBy\` names the cut, not the tool".
const seated = panel.cut(seat).tag("bezel_seat");
const machined = seated.cut(aperture, ...bolts).tag("machined");

// Break the seat's rim, which is the edge the module drops past. Eight of them:
// four straight sides and the four corner arcs the end mill leaves behind.
//
// This selector is also what fails if \`over\` is ever taken away: with the seat
// cutter stopping short of the front face there is no rim, \`bezel_seat\` tracks
// no edge there, and the build stops by name instead of shipping a void.
return machined
  .edges({ generatedBy: "bezel_seat", at: { z: "max" } })
  .expect({ count: 8 })
  .chamfer(0.4)
  .tag("seat_lead_in");
`,Se=`// Selected edge treatments. The queries are geometric, not \`edge[7]\`:
// >Z = topmost, >Y = positive-Y-most, |X = a straight edge running along X.
// If a later edit makes that query empty, the B-rep evaluator refuses instead
// of treating a different edge by accident.

const body = box(80, 60, 8).tag("body");

const rounded = body
  .edges(">Z and >Y and |X")
  .fillet(2);

// A chamfer uses the same selected-edge contract but makes a planar bevel.
return rounded
  .edges("<Z and >Y and |X")
  .expect({ count: 1 })
  .chamfer(1)
  .tag("edge_treatments");
`,Le=`// A printable enclosure — shows shell() and offset().

const w = 70, d = 45, h = 28;
const wall = 2.0;

// offset() grows the shape and rounds every convex edge as it goes,
// which is the cheapest way to break sharp corners for printing.
const outer = box(w - 6, d - 6, h - 6).offset(3).tag("outer");

const body = outer.shell(wall).tag("walls");

// Open the top by cutting away the top wall and everything above it. The
// cutter starts a wall's thickness below the outer top face and runs proud
// of the sides: placed exactly on that face, it touched the box and removed
// nothing, and the enclosure shipped sealed for months with every check green.
const lid = box(w + 2, d + 2, h).at(0, 0, h - wall).tag("open_top");

const port = cylinder(5, 40).rotate("x", 90).at(0, -d / 2, 0).tag("port");

return body.cut(lid).cut(port);
`,He=`// A 200 mm length of 20x20 T-slot aluminium extrusion.
//
// The profile is the standard one every 3D printer frame is built from: a
// 20 mm square with a 6 mm slot opening on each face, widening to an 11 mm
// channel that the T-nut sits in, and a 4.2 mm core hole for a self-tapping
// end screw.
//
// It is written as one side's cuts, placed four times by rotating the *cutter*
// rather than the body — \`around(slot, 4)\`. The profile is genuinely four-fold
// symmetric, so that is exact rather than four placements that could drift
// apart under an edit.

const size = 20;
const length = 200;
const slotOpening = 6.2;   // the gap at the face
const openingDepth = 2.0;  // how deep before it widens
const channel = 11.0;      // T-nut chamber width
const channelDepth = 6.0;  // from the face to the back of the chamber
const core = 4.2;          // end-screw hole

const bar = box(size, size, length).tag("bar");

// One face's cut, made from the outside in: the visible gap, then the chamber
// behind it. Both are overlength along the extrusion axis so nothing depends
// on a tool ending exactly on the end faces.
const opening = box(slotOpening, openingDepth * 2, length * 1.2)
  .at(0, size / 2 - openingDepth / 2 + openingDepth / 2, 0);

const chamber = box(channel, channelDepth - openingDepth, length * 1.2)
  .at(0, size / 2 - openingDepth - (channelDepth - openingDepth) / 2, 0);

const slot = union(opening, chamber);

// Four identical slots. Rotating the cutter about the extrusion axis is exact:
// the profile is genuinely four-fold symmetric, so nothing here is a placement
// approximation that could drift.
const slots = around(slot, 4).tag("slots");

const coreHole = cylinder(core / 2, length * 1.2).tag("core_hole");

// The webs. Adjacent chambers overlap at each corner, so without them the
// core and the four corner blocks are five separate bars that happen to be
// drawn together — and that is how this part shipped: watertight, the right
// volume, every count in its case green. A real profile has a diagonal web
// from each core corner to its corner block, and so does this one now; the
// corpus records that it is one body and stands on one patch, which is what
// found the missing ones.
// Drawn along X and turned 45°, which carries +X onto the (1, 1) diagonal;
// drawn along Y it would land on (-1, 1) and float in the void, and the
// corpus would report nine bodies — which it did.
const web = 1.6;
const webs = around(
  box(5, web, length).rotate("z", 45).at(4.75, 4.75, 0),
  4,
).tag("webs");
const profile = bar.cut(slots, coreHole).union(webs).tag("profile");

// The four outside corners are broken on a real extrusion — the die has a
// radius there, and a sharp 20x20 corner is a hazard on a frame.
//
// One selector per corner, because "|Z" alone is every edge running along the
// extrusion, and this profile has thirty-seven of them: each slot contributes
// its own. Adding the two extrema per corner narrows it to the one edge meant.
// docs/DSL_GAPS.md notes what is missing — a way to say "the outermost of
// these" without enumerating corners by hand.
const corners = [">X and >Y", ">X and <Y", "<X and >Y", "<X and <Y"];

return corners
  .reduce(
    (shape, corner) => shape.edges(\`\${corner} and |Z\`).expect({ count: 1 }).fillet(1),
    profile,
  )
  .tag("corner_radii");
`,Ee=`// Control box: a moulded enclosure for a small board, in the field-instrument
// manner — one slab, one module, one hero control, detail only where a finger
// or a screw goes. The numbers are the style's, from
// docs/styles/field-instrument.md (read_docs \`style-field-instrument\`):
//
// - an 8 mm module: the plan is 14 x 10 cells and every centre below is on it
//   or on half of it;
// - a moulded body keeps its plan corners near-sharp, r 0.5, and its top
//   perimeter crisp — rounding is for machined metal;
// - a square grille on a 4 mm pitch, hole 0.55–0.75 of the pitch: Ø2.8 here,
//   which leaves 1.2 mm webs a 0.4 mm nozzle lays as three lines;
// - pad pockets 16 mm on a 24 mm pitch (three cells), their caps 15 mm, so a
//   cap stands 0.5 mm inside its pocket all round;
// - one knob: an Ø8 cap in an Ø16.2 recessed ring.
//
// Three bodies: the shell, its base plate, and the caps with the knob, which
// is where a second colour would go. Printed separately; the caps are drawn
// seated in their pockets, 2 mm proud, so \`caps\` is seven pieces on purpose.
const M = 8; // module
const L = 14 * M; // 112
const W = 10 * M; // 80
const H = 2 * M; // 16
const wall = 2;
const top = 2.5; // 1.5 mm left under a 1 mm pocket
const pocketDepth = 1;
const proud = 2;
const plateT = 2;
const plateFit = 0.2;

// --- shell ---------------------------------------------------------------
let shell = box(L, W, H)
  .at(0, 0, H / 2)
  .fillet(0.5, "|Z", { count: 4 })
  // Open underneath: the cutter leaves through the bottom face by 0.5 mm.
  .cut(box(L - 2 * wall, W - 2 * wall, H - top + 0.5).at(0, 0, (H - top - 0.5) / 2))
  .tag("shell");

// Screw bosses from the base plate up to the underside of the top, in the
// corners and clear of every hole through the top: the nearest grille hole
// wall is 1.3 mm from the top-left boss.
const bossAt = grid(2, 2, 12 * M, 8 * M); // (±48, ±32)
const bossH = H - top - plateT;
const boss = cylinder(3.5, bossH + 0.5).at(0, 0, plateT + bossH / 2 + 0.25);
shell = shell.union(repeat(boss, bossAt));
// Tapped from below: the cutter enters at the plate's top face and goes up.
shell = shell.cut(repeat(holeFor("M2", 8, { tapped: true }).rotate("x", 180).at(0, 0, plateT), bossAt));

// Grille: 8 x 8 holes, left half; its top row meets the function keys' top edge.
const grilleX = -4 * M;
const grilleY = 1.5 * M;
const grilleHole = cylinder(1.4, top + 2).at(0, 0, H - top / 2);
shell = shell.cut(repeat(grilleHole, grid(8, 8, M / 2, M / 2).map(([x, y]) => [x + grilleX, y + grilleY])));

// Pads: 2 x 2 at a three-cell pitch, right half, and two function keys as a
// third row on the same pitch. The bottom pads and the knob ring share a row,
// one module in from the edge; the function keys and the grille top share one.
const padsX = 3 * M;
const padPockets = grid(2, 2, 3 * M, 3 * M).map(([x, y]) => [x + padsX, y - 1.5 * M]);
const fnPockets = grid(2, 1, 3 * M, 0).map(([x]) => [x + padsX, 3 * M]);
const pocketZ = H - pocketDepth / 2 + 0.25;
shell = shell.cut(
  repeat(box(16, 16, pocketDepth + 0.5).at(0, 0, pocketZ), padPockets),
  repeat(box(16, 8, pocketDepth + 0.5).at(0, 0, pocketZ), fnPockets),
);

// Knob ring: the one round thing on the deck, below the grille.
const knobX = grilleX;
const knobY = -3 * M;
shell = shell.cut(cylinder(16.2 / 2, pocketDepth + 0.5).at(knobX, knobY, pocketZ));

// Two holes on the short side for pegs that tile boxes together, on the module.
shell = shell.cut(
  repeat(cylinder(2.4, wall + 2).rotate("y", 90).at(-L / 2, 0, H / 2), [[0, -1.5 * M], [0, 1.5 * M]]),
);

// --- base plate ----------------------------------------------------------
const plate = box(L - 2 * wall - 2 * plateFit, W - 2 * wall - 2 * plateFit, plateT)
  .at(0, 0, plateT / 2)
  .cut(
    repeat(holeFor("M2", plateT, { through: true }).at(0, 0, plateT), bossAt),
    // Countersunk from underneath, so the screw heads sit flush with the bed face.
    repeat(countersink("M2").rotate("x", 180), bossAt),
    // Four foot recesses, a module inside the screws so a foot never covers one.
    repeat(cylinder(4, 1).at(0, 0, 0), grid(2, 2, 10 * M, 6 * M)),
  )
  .tag("plate");

// --- caps ----------------------------------------------------------------
const capZ = H - pocketDepth + (pocketDepth + proud) / 2;
const pad = box(15, 15, pocketDepth + proud).fillet(1.5, "|Z", { count: 4 }).at(0, 0, capZ);
const fn = box(15, 7, pocketDepth + proud).fillet(1.75, "|Z", { count: 4 }).at(0, 0, capZ);
const knobH = 11.8;
const knob = cylinder(4, knobH).at(knobX, knobY, H - pocketDepth + knobH / 2);
const caps = union(repeat(pad, padPockets), repeat(fn, fnPockets), knob).tag("caps");

return { shell, plate, caps };
`,ze=`// Desk stand: a phone rest milled from one slab, in the field-instrument
// manner — the machined-metal half of it, where \`control-box.js\` is the
// moulded half. The numbers are the style's, from
// docs/styles/field-instrument.md (read_docs \`style-field-instrument\`):
//
// - plan near √2 (120 x 85), thickness about a sixth of the short side — thicker
//   than the style likes, for 2 mm of floor over the cable;
// - plan corners rounded at about 5 % of the short side, as machined bodies
//   are; top perimeter crisp, broken by 0.4 mm only;
// - a larger chamfer on the bottom perimeter, so the slab reads as floating
//   on its feet;
// - one gesture — a shallow round dish beside the slot — and detail only where
//   something touches: the slot, the cable path, the feet.
const L = 120;
const W = 85;
const T = 14;
const corner = 4; // ≈ 0.05 x 85
const topBreak = 0.4;
const bottomChamfer = 1.2;

// Slot: 12 mm wide for a phone in a case, 9 deep, leaning back 15° from upright.
const slotW = 12;
const slotDepth = 9;
const lean = 15;
const slotY = -W / 2 + 25;

// The dish: Ø40, 2 mm deep, off to the right and behind the slot.
const dishR = 20;
const dishDepth = 2;
const dishX = 30;
const dishY = 17.5;

let stand = box(L, W, T)
  .at(0, 0, T / 2)
  .fillet(corner, "|Z", { count: 4 })
  .chamfer(topBreak, ">Z", { count: 8 })
  .chamfer(bottomChamfer, "<Z", { count: 8 })
  .tag("slab");

// The walls lean back, the floor stays flat: a leaning box, intersected with
// everything above the floor. A leaning floor would drop to its lowest along
// the slot's back edge, which is exactly where the cable channel passes under.
const floorZ = T - slotDepth;
const a = (lean * Math.PI) / 180;
const along = 15; // the leaning box's centre, this far up its own axis from the floor
const slot = box(L + 2, slotW, 40)
  .rotate("x", -lean)
  .at(0, slotY + along * Math.sin(a), floorZ + along * Math.cos(a))
  .intersect(box(L + 4, W, T).at(0, slotY, floorZ + T / 2))
  .tag("slot");

const dish = cylinder(dishR, dishDepth + 1).at(dishX, dishY, T - dishDepth + (dishDepth + 1) / 2).tag("dish");

// Cable path: down through the slot floor at the centre, then out of the back
// edge in a channel under the slab.
// 2 mm of floor stays over the channel along its whole length.
const channelDepth = 3;
const channelW = 6;
const drop = cylinder(4, floorZ + 2).at(0, slotY, floorZ / 2);
const channelLen = W / 2 - slotY + 1;
const channel = box(channelW, channelLen, channelDepth + 1).at(0, slotY + channelLen / 2, (channelDepth - 1) / 2);

// Four recesses for Ø10 rubber bumpers, 10 mm in from each edge.
const foot = cylinder(5.1, 1.6).at(0, 0, 0);

stand = stand.cut(
  slot,
  dish,
  drop.tag("cable"),
  channel.tag("cable"),
  repeat(foot, grid(2, 2, L - 20, W - 20)).tag("feet"),
);

// The leaning walls meet the top at 75° in front and 105° behind; break both so
// the front lip is not a knife edge and a phone finds the slot.
stand = stand.chamfer(1, { between: ["slot", "slab"], at: { z: "max" } }, { count: 2 });

return stand;
`,Me=`// Field planter: a wide pot standing on a vented plinth, and a tray it sits in.
//
// Every number is the style's (docs/styles/field-instrument.md, read_docs
// \`style-field-instrument\`): an 8 mm module, a √2 plan, near-sharp moulded
// corners, a crisp top rim, a grille whose holes are 0.7 of the pitch, and one
// gesture — the dial. The plinth is a module narrower than the body all round
// and stands proud of the tray, so the body reads as floating over a shadow
// gap; its vents are the only holes that go through a wall.
const M = 8;

const L = 14 * M; // 112 along the front
const W = 10 * M; // 80 deep — 1.4, the style's plan ratio
const H = 9 * M; // 72 of body above the plinth
const wall = 3; // 1.8 left behind the deepest thing cut into it, the dial
const floor = 3;

const PLINTH_L = L - 2 * M;
const PLINTH_W = W - 2 * M;
const plinthH = 1.5 * M;

// z = 0 is the tray's pool floor; the pot stands on its pads.
const stand = 2;
const plinthTop = stand + plinthH;
const floorTop = plinthTop + floor;
const deck = plinthTop + H / 2; // the front face's own middle, where the graphics sit

// Ventilation through the plinth walls, a module of plain material at each end.
const ventD = 4;

// Drainage through the floor: 0.63 of the pitch, like the vents.
const drainD = 5;

// The grille: blind dimples, because a hole here would leak soil.
const grillD = 2.8;
const grillPitch = 4;
const grillDepth = 1;
const grillCols = 9;
const grillRows = 5;
const grillX = -3.5 * M;

// The gesture: a Ø32 dial sunk 2, its boss standing half a millimetre proud.
const dialD = 32;
const dialDepth = 1.2;
const bossD = 10;
const bossProud = 0.5;
const dialX = 3.5 * M;

const plinth = box(PLINTH_L, PLINTH_W, plinthH)
  .at(0, 0, stand + plinthH / 2)
  .fillet(0.5, "|Z", { count: 4 })
  .tag("plinth");

const body = box(L, W, H)
  .at(0, 0, plinthTop + H / 2)
  .fillet(0.5, "|Z", { count: 4 })
  .tag("body");

const cavity = box(L - 2 * wall, W - 2 * wall, H).at(0, 0, floorTop + H / 2).tag("cavity");
const hollow = box(PLINTH_L - 2 * wall, PLINTH_W - 2 * wall, plinthH + 1).at(0, 0, stand + (plinthH - 1) / 2);

// Each vent is longer than the wall it crosses and centred on it, so it leaves
// through both faces: a cutter that stops short leaves a membrane, and this one
// did — 0.25 mm over every hole, which measure_wall_thickness named.
const ventZ = stand + plinthH / 2;
const vent = cylinder(ventD / 2, wall + 4).rotate("x", 90).tag("vent");
const along = vent.at(0, -PLINTH_W / 2 + wall / 2, ventZ);
const across = vent.rotate("z", 90).at(-PLINTH_L / 2 + wall / 2, 0, ventZ);
const vents = union(
  repeat(along, grid(9, 1, M, 0)),
  repeat(along.mirror("y"), grid(9, 1, M, 0)),
  repeat(across, grid(1, 5, 0, M)),
  repeat(across.mirror("x"), grid(1, 5, 0, M)),
);

const drain = cylinder(drainD / 2, floor + 2).at(0, 0, floorTop - floor / 2).tag("drain");
const drains = repeat(drain, grid(5, 3, M, M));

const dimple = cylinder(grillD / 2, grillDepth * 2)
  .rotate("x", 90)
  .at(grillX, -W / 2 + grillDepth - grillDepth, deck)
  .tag("grille");
// On a vertical face the field runs in x and z, so the grid's second axis is
// lifted into z rather than left in the plan.
const grille = repeat(
  dimple,
  grid(grillCols, grillRows, grillPitch, grillPitch).map(([x, y]) => [x, 0, y]),
);

// The cutter stands 1 mm proud of the wall and ends \`dialDepth\` inside it; the
// boss then runs from that floor back out to \`bossProud\` beyond the wall.
const dial = cylinder(dialD / 2, dialDepth + 1)
  .rotate("x", 90)
  .at(dialX, -W / 2 + dialDepth - (dialDepth + 1) / 2, deck)
  .tag("dial");
const boss = cylinder(bossD / 2, dialDepth + bossProud)
  .rotate("x", 90)
  .at(dialX, -W / 2 + dialDepth - (dialDepth + bossProud) / 2, deck)
  .tag("boss");

// One accent, on one body: the tray is RAL 2004, the orange this style uses
// where something matters; the pot is the neutral moulded grey around it.
const moulded = { color: "#d6d8d2", roughness: 0.75 };
const accent = { color: "#e25303", roughness: 0.45, clearcoat: 0.3 };

const pot = body
  .union(plinth)
  .cut(cavity, hollow, vents, drains, grille, dial)
  .union(boss)
  .chamfer(1, { on: "cavity", at: { z: "max" } }, { count: 4 })
  .material(moulded)
  .tag("pot");

// Tray: a plate with a pool the plinth stands clear of, so the pot never sits
// in what it drains. Two modules wider than the body, on the same module.
const TRAY_L = L + 2 * M;
const TRAY_W = W + 2 * M;
const trayFloor = 2.5;
const pool = 6;
const padH = stand;

const pad = box(M, M, padH + 1).at(0, 0, (padH - 1) / 2).tag("pads");

const tray = box(TRAY_L, TRAY_W, trayFloor + pool)
  .at(0, 0, (trayFloor + pool) / 2 - trayFloor)
  .fillet(0.5, "|Z", { count: 4 })
  .cut(box(TRAY_L - 2 * wall, TRAY_W - 2 * wall, pool + 1).at(0, 0, (pool + 1) / 2 - 0.5))
  .chamfer(0.4, "<Z", { count: 8 })
  .union(repeat(pad, grid(2, 2, PLINTH_L - 2 * M, PLINTH_W - 2 * M)))
  .cut(repeat(cylinder(5, 1.6).at(0, 0, -trayFloor), grid(2, 2, TRAY_L - 4 * M, TRAY_W - 4 * M)))
  .material(accent)
  .tag("tray");

return { pot, tray };
`,Pe=`// A slip-on pipe flange, dimensioned after ASME B16.5 class 150, NPS 2.
//
// Nominal dimensions, all millimetres: OD 152.4, flange thickness 19.1,
// hub OD 92.1, length through hub 25.4 measured from the back face, bore 60.3,
// four bolt holes of 19.1 on a 120.7 bolt circle. The holes straddle the
// centrelines, which is the part of the standard people get wrong: they sit at
// 45°, not at 0°.
//
// \`polar()\` is the rotational counterpart to \`grid()\`, and \`straddle\` is the
// convention itself: the holes sit half a step off the centrelines. Written as
// arithmetic it was a \`+ 0.5\` nobody could check against a drawing.

const od = 152.4;
const thickness = 19.1;
const bore = 60.3;
const hubOd = 92.1;
const throughHub = 25.4;   // back face to top of hub
const boltCircle = 120.7;
const boltDia = 19.1;
const bolts = 4;

// The flange plate straddles z = 0; the hub grows off its top face, so the
// back face (the one that gets faced flat) stays at z = -thickness / 2.
const plate = cylinder(od / 2, thickness).tag("plate");

// The hub is modelled buried to the plate's mid-plane rather than standing on
// its top face. Two coaxial cylinders that meet exactly on a face cannot be
// blended — the kernel refuses at any radius, including 1 mm — and an
// overlap is both the fix and what a casting actually is. See docs/DSL_GAPS.md.
const hubTop = throughHub - thickness / 2;
const hub = cylinder(hubOd / 2, hubTop)
  .at(0, 0, hubTop / 2)
  .tag("hub");

// A small blend at the hub root: a sharp internal corner there is a stress
// riser, and every real casting has a radius at it.
const body = union(plate, hub, { blend: 3 }).tag("body");

// Overlength on purpose. A cutting tool that ends exactly on a face leaves a
// zero-thickness sliver that booleans handle badly — extend it past both ends.
const throughBore = cylinder(bore / 2, throughHub * 4).tag("bore");

const boltHoles = polar(bolts, boltCircle / 2, { straddle: true });

// One tagged cut, not two. \`generatedBy\` names a single operation, so bore and
// bolt holes have to be drilled by the same node for one selector to reach all
// five rims — splitting the cut in two silently halves what the query matches.
const drilled = body
  .cut(throughBore, repeat(cylinder(boltDia / 2, thickness * 4), boltHoles))
  .tag("drilled");

// Break the bore rim where a gasket seats. \`role: "hole"\` is what keeps the
// hub's outside rim — same curve, same face normal — out of the selection.
return drilled
  .edges({
    generatedBy: "drilled",
    curve: "circle",
    role: "hole",
    adjacentTo: { faceNormal: "-z" },
  })
  .expect({ count: 5 })
  .chamfer(1.5)
  .tag("back_face_breaks");
`,Oe=`// Retainer v1 — recreated from a Fusion 360 export.
//
// The one Fusion part in here that is a faithful recreation rather than a
// target: it builds, it is watertight, and it agrees with the original.
//
//   volume   143829.66 mm^3 against Fusion's 143825.63  (+0.0028%)
//   bounding box  59.49 x 148.13 x 36.66 mm             (exact)
//   faces    23, and the same 23: 14 plane, 6 cylinder, 2 cone, 1 torus
//
// Ground truth is the STEP in reference/fusion/Retainer-v1. Every number below
// was read off the B-rep in it, not scaled off a drawing. The face count is 23
// rather than the 25 Fusion's own API reports, because that is what its STEP
// export contains and STEP is what both sides were measured from.
//
// It needed a fix to OpenCASCADE to get here — see
// vendor/occt-sys/patches/0001-tangent-pinch-corner.patch. Before that the
// blend where the disc meets the plate returned a solid the kernel's own
// checker rejected.
//
// Axes are remapped from Fusion, whose plate normal was +Y: parcad (x, y, z) =
// Fusion (x, -z, y). That puts the plate normal on +Z, so every hole in the
// part is a plain Z cylinder and nothing needs rotating.

const W = 59.49;        // plate width, and the disc diameter — they are equal
const T = 13.314;       // plate thickness
const PLATE_L = 118.381; // to the disc centre; the disc caps the rest
const R_DISC = W / 2;   // 29.745
const H_DISC = 36.664;  // disc rises this far off the back face

// Disc bore, from the back: a drafted counterbore, then a straight bore out
// the front. The 3.70 degrees is the Draft feature in the Fusion timeline.
// The cone opens toward the back face, not away from it: the STEP puts radius
// 15 at the far end with the axis pointing back, so the mouth at z = 0 is the
// wide end. The STL's back face agrees — its inner boundary sits at 16.701,
// which is this radius and not the 13.3 a cone drafted the other way implies.
const DRAFT_TOP_R = 15;
const DRAFT_DEPTH = 26.3;
const DRAFT_DEG = 3.70003;
const DRAFT_BOTTOM_R = DRAFT_TOP_R + DRAFT_DEPTH * Math.tan((DRAFT_DEG * Math.PI) / 180);
const BORE_R = 10;
const BORE_CHAMFER = 1;

// The threaded hole is modelled at its pitch diameter, which is how Fusion
// leaves a cosmetic thread: no helix reached the B-rep, so this is a plain
// cylinder and the recreation is exact rather than approximate.
const THREAD_R = 7.51775;
const THREAD_X = 29.4901;
const THREAD_Y = 64;

// Bayonet slot: a channel down from the top face, turning left into a pocket.
const SLOT_W = 10.2;
const SLOT_X0 = 39.29;
const SLOT_X1 = 49.49;
const CHANNEL_END_Y = 37.926;   // where the pocket's lower wall lies
const POCKET_Y0 = 27.726;
const POCKET_Y1 = 37.926;
const POCKET_X_END = 27.49;     // centre of the round end, radius SLOT_W / 2
const TURN_R = 12;              // fillet on the outer corner of the turn
const INNER_R = 1.8;            // and on the inner corner
const LEAD_IN = 2.5;            // 45 degree chamfer at the slot mouth

// The threaded hole stays in the final Boolean below. Cutting it here, before
// the blended union, corrupts the result: the part comes out 61.06 mm wide
// against a 59.49 mm plate and loses 85% of its volume. A blend over a shape
// that already has a hole through it is the trigger; both work alone.
const plate = box(W, PLATE_L, T)
  .at(W / 2, PLATE_L / 2, T / 2)
  .tag("plate");

const disc = cylinder(R_DISC, H_DISC)
  .at(R_DISC, PLATE_L, H_DISC / 2)
  .tag("disc");

// The seam only exists where the disc wall meets the plate's front face, which
// is exactly where the reference has its r2 torus.
const stock = union(plate, disc, { blend: 2 }).tag("stock");

// Cut deeper than the disc on both ends; below the draft the counterbore is
// wider than the bore, so the overlap removes nothing extra.
const bore = cylinder(BORE_R, H_DISC * 2).at(R_DISC, PLATE_L, H_DISC);

const draftedBore = cone(DRAFT_BOTTOM_R, DRAFT_TOP_R, DRAFT_DEPTH)
  .at(R_DISC, PLATE_L, DRAFT_DEPTH / 2);

const channel = box(SLOT_X1 - SLOT_X0, CHANNEL_END_Y, T * 3)
  .at((SLOT_X0 + SLOT_X1) / 2, CHANNEL_END_Y / 2, T / 2);

const pocket = box(SLOT_X1 - POCKET_X_END, SLOT_W, T * 3)
  .at((POCKET_X_END + SLOT_X1) / 2, (POCKET_Y0 + POCKET_Y1) / 2, T / 2);

const pocketEnd = cylinder(SLOT_W / 2, T * 3)
  .at(POCKET_X_END, (POCKET_Y0 + POCKET_Y1) / 2, T / 2);

// Both corner radii of the turn are built into the cutter rather than applied
// as edge treatments. They are interior edges of a pocket, and the selector
// grammar reaches document extrema only — there is no term that names the
// vertical edge at (49.49, 37.93) without naming the other three corners too.
// See the note in DSL_GAPS.md. The geometry below is exact, not a stand-in:
// a box minus the fillet cylinder is precisely the material the arc leaves.
const TURN_CX = 37.4901;
const TURN_CY = 26;

const outerCorner = box(TURN_R, POCKET_Y1 - TURN_CY, T * 3)
  .at(TURN_CX + TURN_R / 2, (TURN_CY + POCKET_Y1) / 2, T / 2)
  .cut(cylinder(TURN_R, T * 4).at(TURN_CX, TURN_CY, T / 2));

const innerCorner = box(INNER_R, INNER_R, T * 3)
  .at(TURN_CX + INNER_R / 2, TURN_CY + INNER_R / 2, T / 2)
  .cut(cylinder(INNER_R, T * 4).at(TURN_CX, TURN_CY, T / 2));

const slot = union(channel, pocket, pocketEnd)
  .cut(outerCorner)
  .union(innerCorner);

// A right triangle swept through the thickness is the lead-in at each side of
// the slot mouth. The outline carries its own X and Y, so only the centred Z
// needs placing. These stay geometry rather than edge treatments because the
// edges they chamfer belong to the slot's own cut, and a treatment would have
// to name them after the fact.
const leadInLeft = extrude(
  [[SLOT_X0 - LEAD_IN, 0], [SLOT_X0, 0], [SLOT_X0, LEAD_IN]],
  T * 3,
).at(0, 0, T / 2);

const leadInRight = extrude(
  [[SLOT_X1, 0], [SLOT_X1 + LEAD_IN, 0], [SLOT_X1, LEAD_IN]],
  T * 3,
).at(0, 0, T / 2);

const threadHole = cylinder(THREAD_R, T * 3).at(THREAD_X, THREAD_Y, T / 2);

const drilled = stock
  .cut(bore, draftedBore, slot, leadInLeft, leadInRight)
  .tag("cuts");

// With the threaded hole still to come, the bore's is the only hole rim in
// this lineage on an upward face — the drafted bore's other rim faces down.
// Cutting the thread first would put a second +z rim in the same lineage and
// this selector would take both.
const chamfered = drilled
  .edges({ generatedBy: "cuts", curve: "circle", role: "hole", adjacentTo: { faceNormal: "+z" } })
  .expect({ count: 1 })
  .chamfer(BORE_CHAMFER)
  .tag("bore_rim");

return chamfered.cut(threadHole).tag("thread");
`,Ce=`// Untitled2 v1 — recreated from the Fusion 360 export.
//
// Two turned bodies: a wavy teardrop (Body1) standing clear inside a cup
// (Body2), each a revolved spline section. Fusion built them with Sketch,
// Revolve and Fillet. Measured in Fusion (measurements.json) and on the
// export's B-rep (\`parcad --probe-step\`), against this script's exact solids
// (its STEP export, probed the same way):
//
//   Body1  volume  36359.856 mm^3  Fusion  36356.903 (+0.008%)   export  36420.013 (-0.165%)
//          area     7027.379 mm^2  Fusion   7025.506 (+0.027%)   export   7024.176 (+0.046%)
//          bbox 36.85 x 36.85 x 81.01 mm, z 30.12..111.13     (the same)
//          faces 1: one surface of revolution; Fusion writes the same surface
//          as a rational NURBS
//   Body2  volume 126698.727 mm^3  Fusion 126698.252 (+0.0004%)  export 126430.918 (+0.21%)
//          area    26405.298 mm^2  Fusion  26405.273 (+0.0001%)  export  26400.340 (+0.02%)
//          bbox 97.09 x 97.09 x 41.93 mm                        (the same)
//          faces 6: 2 revolved walls, 2 fillets, 2 planes; Fusion's 4 NURBS
//          are the same walls and fillets
//
// The sections are the export's own curves, not a fit. Each revolved wall in
// the STEP is a surface of revolution of a clamped *uniform* degree-5
// B-spline (knots every 1/40 on Body1, every 1/15 on Body2), so its pole row
// copies verbatim into a \`{ bspline, degree: 5 }\` entry between the two
// corners it joins. The export's own B-rep reads a little off Fusion's
// figures on both bodies, and this recreation agrees with Fusion's rather
// than with the export's, so that gap is in how the export's rational
// surfaces integrate, not in the curves.
//
// Body2's walls in the export run on past their fillets — the outer wall's
// last pole is the sharp rim corner at z = 41.927706 — which is how the
// construction reads: revolve the section with a sharp rim, then fillet both
// rim edges. The radius is not in the export as a number; each rim's circular
// section edge is tangent to the top plane and passes through both of its
// ends at radius 5.000 mm.

// Body1: one revolved degree-5 B-spline, its 45 poles read off the export.
const drop = revolve([
  [0, 110.956529],
  [0, 31.010477],
  {
    bspline: [[0.434686, 30.846916], [1.307967, 30.557504], [2.629619, 30.236505], [4.415271, 30.034994], [6.678844, 30.162045], [8.950334, 30.670388], [11.183364, 31.562444], [13.297781, 32.841928], [15.188718, 34.504875], [16.744759, 36.525782], [17.863131, 38.84557], [18.466293, 41.357811], [18.517669, 43.897669], [18.020622, 46.289171], [17.018561, 48.394208], [15.593711, 50.15413], [13.869357, 51.651437], [11.999475, 53.093312], [10.148375, 54.732665], [8.472936, 56.805719], [7.104508, 59.466256], [6.1289, 62.711896], [5.589295, 66.41657], [5.49236, 70.378382], [5.813035, 74.361236], [6.49969, 78.139664], [7.479006, 81.540402], [8.660774, 84.490009], [9.942852, 87.046953], [11.217719, 89.34711], [12.378732, 91.560526], [13.326613, 93.8431], [13.975601, 96.292617], [14.260524, 98.896898], [14.134341, 101.560477], [13.564632, 104.140216], [12.530568, 106.477204], [11.019736, 108.430836], [9.025166, 109.909947], [6.54393, 110.861103], [4.169344, 111.179453], [2.169654, 111.165981], [0.739405, 111.045011]],
    degree: 5,
  },
]);

const cup = revolve([
  [0, 0],
  [31.41768, 0],
  {
    bspline: [[31.660743, 0.377641], [32.155253, 1.143033], [32.922166, 2.321445], [33.995368, 3.953755], [35.420329, 6.099194], [36.921685, 8.354726], [38.48245, 10.726729], [40.069618, 13.222056], [41.638334, 15.847665], [43.138939, 18.610216], [44.52321, 21.515156], [45.750413, 24.565748], [46.795755, 27.760978], [47.648505, 31.09665], [48.306297, 34.567528], [48.67852, 37.448406], [48.872704, 39.666054], [48.964664, 41.169631]],
    degree: 5,
  },
  [49.00127, 41.927706],
  [37.975465, 41.927706],
  {
    bspline: [[37.833177, 41.03918], [37.551404, 39.292794], [37.137153, 36.765221], [36.601733, 33.580466], [35.947159, 29.91391], [35.264678, 26.563895], [34.468106, 23.5234], [33.408751, 20.768284], [31.865399, 18.251355], [29.615743, 15.921851], [26.504055, 13.742413], [22.509251, 11.705599], [17.831331, 9.853358], [12.819886, 8.26233], [7.888254, 7.025318], [4.32704, 6.392207], [2.010378, 6.150094], [0.640955, 6.096291]],
    degree: 5,
  },
  [0, 6.096291],
])
  .edges({ curve: "circle", at: { z: "max" } })
  .expect({ count: 2 })
  .fillet(5);

return { body1: drop, body2: cup };
`,Be=`// UnTriangle v3 — recreated from the Fusion 360 export.
//
// An impossible-triangle sculpture: three straight bars of 10 mm square
// section whose axes draw an equilateral triangle, each bar twisting a
// quarter turn between its corners so the flat of one end arrives as the
// edge of the next.
//
// The document holds two solids, and its STEP export carries only one:
// Body12, whose numbers parcad's own STEP probe reads off the file
// (\`parcad --probe-step\`, or the probe_step_export tool). This script
// recreates that body, measured against the export:
//
//   volume   24800.00 mm^3 against the export's 24799.94   (+0.00024%)
//   area     11619.41 mm^2 against 11619.40                (+0.00005%)
//   bounding box  10.00 x 132.32 x 114.59 mm               (exact)
//   faces    30, and the same 30: 18 plane, 12 nurbs
//
// The other body — Body1, 31976.42 mm^3, the one this file's header used to
// record — is not in the export at all, so there is nothing to measure a
// recreation of it against. It differs by a 45° twist phase (its bounding box
// is 10*sqrt(2) wide) and carries three fillets. The header numbers before
// this recreation described a body the reference never contained.
//
// The walls are the part worth understanding. Fusion lofted each bar between
// two squares a quarter turn apart, and every wall in the export is a NURBS
// surface whose pole grid is exactly bilinear: a doubly-ruled patch fully
// determined by its four corners. parcad's default ruled \`loft\` builds the
// same patch from the same corners, so the surfaces here are not merely
// close to Fusion's — they are the same surfaces, measured to 1.2e-4 mm,
// which is the export's own vertex scatter. Getting the twist through OCCT
// needed one kernel-wrapper change: ThruSections' compatibility pass used to
// re-origin the section wires to *remove* twist, silently rebuilding this
// loft as a straight prism (see vendor/opencascade/PARCAD-CHANGES.md).
// Vertex pairing is by outline index, so listing the top square's outline a
// quarter turn on IS the twist.
//
// Fusion's own timeline (Extrude, Mirror, Loft, CircularPattern, Combine,
// six Drafts) is not reproduced move for move: the drafts left every plane
// normal exactly axis-aligned or at exactly 60°, so whatever they were for,
// the finished body is the union below. Fusion's stored bbox for Body12
// (10.0000 x 132.3209 x 114.5933) carries ~2e-4 mm of sketch scatter around
// the clean construction; the export's B-rep is what this script is held to.

const A = 10; // bar cross-section square
const SIDE = 115; // side of the triangle the three bar axes draw
const INSET = 9; // each bar's end square sits this far from its axis vertex

const S3 = Math.sqrt(3) / 2;
const R_IN = SIDE / (2 * Math.sqrt(3)); // axis-triangle inradius; bars sit here
const HALF = SIDE / 2 - INSET; // half-length of the twisted span

// One twisted bar, built along +Z and laid down along +Y at the bottom of
// the ring. The loft pairs section vertices by index, so the same square
// listed a quarter turn on twists every wall by one vertex — that is the
// sculpture.
const sq = [
  [A / 2, A / 2],
  [-A / 2, A / 2],
  [-A / 2, -A / 2],
  [A / 2, -A / 2],
];
const quarter = ([x, y]) => [-y, x];
const bar = loft([
  { z: -HALF, outline: sq },
  { z: HALF, outline: sq.map(quarter) },
])
  .rotate("x", -90)
  .at(0, 0, -R_IN);

// The corner block between an incoming bar (up-right at 60°) and the
// outgoing bar (along +Y): each bar's square section carried straight on
// past the vertex, trimmed where it meets the other bar. That region is not
// convex — the export shows the resulting notch as two sliver faces — so it
// is two convex quads in the ring's (y, z) plane, pushed through the
// thickness. Their shared edge lies on the incoming bar's inner wall plane.
const u = [0.5, S3]; // incoming bar direction
const n = [-S3, 0.5]; // its outward normal
const outerEnd = [INSET * u[0] + (A / 2) * n[0], INSET * u[1] + (A / 2) * n[1]];
const innerEnd = [INSET * u[0] - (A / 2) * n[0], INSET * u[1] - (A / 2) * n[1]];
const hit = (p, z) => [p[0] + ((z - p[1]) / u[1]) * u[0], z]; // along u to height z
const tip = hit(outerEnd, -A / 2); // the ring's outer corner
const innerCut = hit(innerEnd, -A / 2);
const innerTop = hit(innerEnd, A / 2);

const incoming = [tip, outerEnd, innerEnd, innerCut];
const outgoing = [innerCut, innerTop, [INSET, A / 2], [INSET, -A / 2]];

// extrude() runs along +Z; swing the prism to run through the thickness
// (+X), with the profile's coordinates landing on (y, z).
const prism = (quad) =>
  extrude(
    quad.map(([y, z]) => [-z, y]),
    A,
  ).rotate("y", 90);

const corner = union(prism(incoming), prism(outgoing)).at(0, -SIDE / 2, -R_IN);

// One bar plus one corner is a third of the ring; the pattern closes it.
// Union order follows the chain of contacts, as always.
const unit = union(corner, bar);
return union(unit, unit.rotate("x", 120), unit.rotate("x", 240));
`,Ie=`// An extruded-profile heat sink, 60 x 60, with a plain fin array.
//
// Fin pitch and thickness are the whole design: closer fins add surface area
// but choke natural convection, and 2 mm walls at a 6 mm pitch is the usual
// compromise for an extrusion. The two mounting holes are on the diagonal of
// a 30 mm square, which is the common pattern for clamping to a TO-247.

const plan = 60;
const baseT = 5;
const finT = 2;
const finH = 25;
const pitch = 6;
const fins = 9;

const base = box(plan, plan, baseT).at(0, 0, baseT / 2).tag("base");

// One fin, reused at every station. Because shapes are values, this is a
// single node in the graph with nine placements — not nine cylinders' worth
// of duplicated intent.
const fin = box(finT, plan, finH + baseT).at(0, 0, (finH + baseT) / 2);

const stations = Array.from({ length: fins }, (_, i) => [
  (i - (fins - 1) / 2) * pitch,
  0,
]);

const body = union(base, repeat(fin, stations)).tag("body");

const mount = cylinder(1.7, baseT * 4);
const drilled = body
  .cut(repeat(mount, [[-15, -15], [15, 15]]))
  .tag("drilled");

// Only the underside rims: that face beds against the device and must not
// carry a burr. The fin roots are untouched — a fillet there would be nice
// for casting but this profile is extruded, and the die makes that corner.
return drilled
  .edges({
    generatedBy: "drilled",
    curve: "circle",
    role: "hole",
    adjacentTo: { faceNormal: "-z" },
  })
  .expect({ count: 2 })
  .chamfer(0.4)
  .tag("seat_face_deburr");
`,Fe=`// An M3 hex standoff, 5.5 mm across the flats, 20 mm long, bored 2.5 mm for
// a tapped thread.
//
// There is no prism primitive, so the hexagon is the intersection of three
// slabs at 60° — the classic construction, and exact: each slab contributes
// one pair of opposite flats. Across-flats is the slab thickness, so 5.5 here
// is the wrench size, not the across-corners diameter (6.35).

const acrossFlats = 5.5;
const length = 20;
const drill = tapDrill("M3"); // 2.5, the coarse tap drill

// Each slab must be wide enough that only its own two flats can bound the
// result: across-corners is acrossFlats * 2 / sqrt(3), so 2x is ample.
const slab = box(acrossFlats, acrossFlats * 2, length);

const hex = intersect(slab, slab.rotate("z", 60), slab.rotate("z", 120)).tag("hex");

const bore = cylinder(drill / 2, length * 2).tag("tap_drill");

const drilled = hex.cut(bore).tag("drilled");

// Both end faces get a lead-in chamfer on the bore. Selecting by role and
// curve picks exactly the two rims and never a flat-to-flat vertical edge.
return drilled
  .edges({ generatedBy: "drilled", curve: "circle", role: "hole" })
  .expect({ count: 2 })
  .chamfer(0.4)
  .tag("thread_lead_in");
`,Ne=`// A bent hydraulic line with a flare fitting boss at each end, and the O-ring
// groove that seals one of them.
//
// \`pipe\` routes 12 mm tube through 20 mm bends — straight runs and partial
// tori, both exact, which is what a tube bender makes. A hose that curves
// continuously would be \`pipe({ spline: [...] }, d)\` instead.
//
// The inlet boss carries its O-ring gland, drawn the way a catalogue draws
// one: a rectangular section wider than the cord, with its bottom corners
// radiused. It was a round-bottomed torus cut until sections could hold a
// rounded corner, because a torus is a circle in section and nothing else
// was — see docs/DSL_GAPS.md, "arcs in a section". The coaxial seams a cut
// like this leaves in a cylinder wall once segfaulted \`UnifySameDomain\`;
// \`eval/cases/torus-gland.json\` holds that fix.

const tube = 12;
const wall = 1.5;
const bend = 20;      // centreline bend radius, 1.67 x diameter
const bossDia = 24;
const bossLen = 14;
const cord = 2;          // O-ring cord diameter, 2 mm metric
const glandDepth = 1.5;  // 25% squeeze: 0.5 mm of the cord stands proud
const glandWidth = 2.7;  // wider than the cord, so it has room to deform
const glandRadius = 0.3; // bottom corner radius, the catalogue's 0.2–0.4
const glandFromEnd = 4;  // back from the free end, clear of the fitting's lead-in

// The route: out of the pump, along, up, and across to the manifold. Each
// corner gets the same bend, which is what one tool setting gives you.
const route = [
  [0, 0, 0],
  [70, 0, 0],
  [70, 55, 0],
  [70, 55, 40],
  [130, 55, 40],
];

const line = pipe(route, tube, { bend }).tag("line");

// The cutter is the gland's section in (radius, z), revolved: its floor
// \`glandDepth\` under the boss surface with both floor corners rounded, and
// its outer side 1 mm proud of the boss so the cut breaks the surface
// cleanly. Turned onto the boss's X axis afterwards.
const floor = bossDia / 2 - glandDepth;
const proud = bossDia / 2 + 1;
const gland = revolve([
  { at: [floor, -glandWidth / 2], round: glandRadius },
  [proud, -glandWidth / 2],
  [proud, glandWidth / 2],
  { at: [floor, glandWidth / 2], round: glandRadius },
])
  .rotate("y", 90)
  .at(glandFromEnd, 0, 0)
  .tag("gland");

// A boss at each end, over the tube, for the fitting to thread into. Each sits
// on the run it belongs to, so it is placed by the route's own numbers.
const inlet = cylinder(bossDia / 2, bossLen)
  .rotate("y", 90)
  .at(bossLen / 2, 0, 0)
  .cut(gland);
const outlet = cylinder(bossDia / 2, bossLen)
  .rotate("y", 90)
  .at(130 - bossLen / 2, 55, 40);

const body = union(line, inlet, outlet).tag("body");

// The bore is the same route at the wall diameter — one \`pipe\` call cannot be
// reused at two sizes, but the route can, and that is the part that must not
// drift. Both ends are pushed 2 mm past the tube ends: a cutter that stops
// exactly on the face it exits leaves a zero-thickness sliver, and here it
// killed the kernel outright rather than producing a bad solid.
const past = 2;
const boreRoute = [
  [-past, 0, 0],
  ...route.slice(1, -1),
  [130 + past, 55, 40],
];
const bore = pipe(boreRoute, tube - 2 * wall, { bend }).tag("bore");

const machined = body.cut(bore).tag("machined");

// Both tube ends, where the fitting seats. Two rims, and the count is the
// assertion that the bore still runs the whole route: a bend radius that stops
// fitting would break the chain and leave a different number.
return machined
  .edges({ generatedBy: "machined", curve: "circle", role: "hole" })
  .expect({ count: 2 })
  .chamfer(0.5)
  .tag("seat_chamfer");
`,je=`// A knurled control knob for a 6 mm shaft with a flat — the D-bore that stops
// the knob from spinning on the shaft.
//
// The knurl is 24 axial flutes cut with a small cylinder each. That is what a
// moulded knob really has; a machined diamond knurl is a different process and
// would need a helical cut, which this DSL cannot express (docs/DSL_GAPS.md).

const knobDia = 30;
const knobH = 16;
const shaft = 6;
const flatDepth = 0.5;   // how much of the shaft is flatted (a "D" shaft)
const flutes = 24;
const fluteDia = 3;

const body = cylinder(knobDia / 2, knobH).tag("body");

// One flute, placed on the rim once and spun around the axis. Centred on the
// rim, so half of it cuts in and half cuts air — which is what makes the flute
// a scallop rather than a slot.
const flute = cylinder(fluteDia / 2, knobH * 1.2).at(knobDia / 2, 0);
const knurl = around(flute, flutes).tag("knurl");

// The D-bore: a round hole with one side flatted off. The flat is what
// transmits torque, so its depth is a fit dimension, not decoration.
//
// Intersection, not union. Adding a box to the cutter would push the flat out
// past the bore and cut a keyway slot into the knob instead — the shape is
// "the cylinder, trimmed" and has to be written that way.
const bore = intersect(
  cylinder(shaft / 2, knobH * 2),
  box(shaft, shaft - flatDepth, knobH * 2).at(0, -flatDepth / 2),
).tag("d_bore");

// A dished top, so a thumb sits in the knob rather than on it. A sphere large
// enough that only its bottom cap enters the material makes a shallow dish;
// its radius sets how shallow.
const dish = sphere(26).at(0, 0, knobH / 2 + 26 - 2).tag("dish");

const turned = body.cut(knurl, dish).tag("turned");

// The bore is cut in its own tagged step so the lead-in below can name it.
// Rolled into the same cut as the knurl, \`generatedBy\` would also cover the
// forty-odd flute edges around the bottom rim, and the count would say nothing.
const bored = turned.cut(bore).tag("bored");

// Break the bottom of the bore — the edge that is pushed onto the shaft.
//
// Two edges: a D-bore's outline is the straight flat plus one arc, and that is
// what the count asserts. It was five until the backend started merging
// same-domain faces, which welded the arc's four pieces — 4.71 + 2.95 + 4.71 +
// 2.95 mm — back into the single 15.33 mm curve they always described.
// \`role: "hole"\` would match nothing here either way, because it wants a closed
// circle and this outline is not one.
return bored
  .edges({ generatedBy: "bored", at: { z: "min" } })
  .expect({ count: 2 })
  .chamfer(0.5)
  .tag("bore_lead_in");
`,We=`// Lidded box: an open base and a lid with a locating lip, printed as two parts.
//
// The one thing this part shows is a script returning two bodies that stay
// two — \`return { base, lid }\` — so the report measures each one and says how
// they sit: the lip is drawn 0.3 mm inside the pocket all round and the lid
// stands 0.5 mm above the rim, so the closest the two come is 0.3 mm, and
// \`between_bodies\` reports exactly that on the built solids. Nothing here is
// fused, mated or constrained: the lid sits where this script placed it.
//
// Base: 60 x 40 x 20 with a 2 mm wall and floor, so 48000 - 56·36·18 =
// 11712 mm³. Lid: a 60 x 40 x 3 plate plus a 55.4 x 35.4 x 4 lip hanging
// under it, 7200 + 7844.64 = 15044.64 mm³.
const L = 60, W = 40, H = 20;
const wall = 2, floor = 2;
const lidT = 3, lipH = 4;
const fit = 0.3;      // lip to pocket wall, each side
const standoff = 0.5; // lid underside to the rim

const base = box(L, W, H)
  .at(0, 0, H / 2)
  // The pocket cutter runs 2 mm past the top face it leaves through.
  .cut(box(L - 2 * wall, W - 2 * wall, H).at(0, 0, floor + H / 2))
  .tag("base");

const plate = box(L, W, lidT).at(0, 0, H + standoff + lidT / 2).tag("plate");
const lip = box(L - 2 * wall - 2 * fit, W - 2 * wall - 2 * fit, lipH)
  .at(0, 0, H + standoff - lipH / 2)
  .tag("lip");
const lid = plate.union(lip).tag("lid");

return { base, lid };
`,qe=`// A hydraulic manifold block: a solid with cross-drilled galleries that meet
// inside it, plus the plugged drilling access every real manifold has.
//
// The interesting property of a manifold is that its function lives in the
// negative space. Nothing here is a feature on the surface — the part is a
// rectangle of metal and a set of intersecting bores, and whether it works
// depends on whether those bores actually meet.

const w = 80, d = 50, h = 40;
const gallery = 8;      // main flow bore
const port = 10;        // threaded port drill
const mountHole = 6.6;  // clearance for M6

const body = box(w, d, h).tag("body");

// The long gallery runs the length of the block on the centre plane, drilled
// in from one end. In a real block the far end is plugged; here it is simply
// drilled through, which is the honest representation of the cut.
const mainBore = cylinder(gallery / 2, w * 1.2).rotate("y", 90).tag("main_bore");

// Two ports drop from the top face and intersect the gallery. Their depth is
// what makes them meet it, and it has to overshoot: a port that bottoms on the
// gallery's centreline is tangent to the bore's lower half, which leaves a
// flat floor with its own rim instead of an opening. One millimetre past the
// far wall of the gallery is a real, unambiguous intersection.
const portDepth = h / 2 + gallery / 2 + 1;
const dropPort = cylinder(port / 2, portDepth * 2);
const ports = union(
  dropPort.at(-22, 0, h / 2),
  dropPort.at(22, 0, h / 2),
).tag("ports");

// A cross gallery on the other axis, meeting the main bore at the centre.
const crossBore = cylinder(gallery / 2, d * 1.2).rotate("x", 90).tag("cross_bore");

const mounting = repeat(
  cylinder(mountHole / 2, h * 2),
  grid(2, 2, w - 20, d - 20),
);

const drilled = body
  .cut(mainBore, ports, crossBore, mounting)
  .tag("drilled");

// Every rim that opens onto the top face gets a chamfer: two ports and four
// mounting holes. Port chamfers are functional here — a sealing fitting needs
// the lead-in — so this is not just deburring.
//
// \`at: { z: "max" }\` and not \`adjacentTo: { faceNormal: "+z" }\`, which is what
// the other examples use. A rim is "adjacent to" its own cylindrical wall as
// well as the flat face it sits in, and a bore drilled along X reports that
// wall as +Z-facing, so the face-normal form also picks up both end rims of
// the main gallery. See docs/DSL_GAPS.md; on a part made mostly of cross
// drillings, position is the selector that says what was meant.
return drilled
  .edges({
    generatedBy: "drilled",
    curve: "circle",
    role: "hole",
    at: { z: "max" },
  })
  .expect({ count: 6 })
  .chamfer(1)
  .tag("top_face_lead_ins");
`,Xe=`// A NEMA 17 stepper motor mount: an L-bracket whose face plate carries the
// standard motor pattern.
//
// The NEMA 17 interface is fixed by the standard and is the whole reason the
// part has these numbers: four M3 holes on a 31.0 mm square, and a 22 mm pilot
// boss that takes the load off the screws. Everything else is stock sizes.

const plateT = 6;
const faceW = 50;
const faceH = 50;
const footD = 45;         // how far the foot reaches back
const boltSquare = 31.0;  // NEMA 17
const boltDia = 3.4;      // clearance for M3
const pilot = 23;         // clearance around the 22 mm motor boss
const mountHole = 5.5;    // clearance for M5 into the frame

// The face plate stands in the XZ plane with its motor face on y = 0; the foot
// lies flat. Both are placed from their own centres, because primitives here
// are centred on the origin by construction.
const face = box(faceW, plateT, faceH)
  .at(0, plateT / 2, faceH / 2)
  .tag("face");

const foot = box(faceW, footD, plateT)
  .at(0, footD / 2, plateT / 2)
  .tag("foot");

// Gussets are what make an L-bracket stiff; without them the face plate hinges
// about the seam under belt tension. A triangular web is a box rotated 45° and
// intersected with a slab that gives it its thickness.
//
// They sit outboard at x = ±20 deliberately: a single central gusset would run
// straight through the 23 mm pilot bore, turning the bore's back rim into a
// pair of arcs and quietly breaking the rim count asserted at the end.
const leg = 20;   // how far the web runs up the face and back along the foot
const gusset = intersect(
  // The corner being braced: 4 mm thick, sitting in the positive quadrant.
  box(4, leg, leg).at(0, leg / 2, leg / 2),
  // The 45° hypotenuse. A cube rotated 45° about X is bounded by y + z = h,
  // where h is half its diagonal, so sizing it leg * sqrt(2) puts that plane
  // exactly through the two leg ends.
  box(leg * 2, leg * Math.SQRT2, leg * Math.SQRT2).rotate("x", 45),
);

// The seam blend is taken between the two plates alone, then the gussets are
// unioned on unblended. A blend across all four solids at once asks OCCT to
// fillet edges that the gussets land exactly on, which no radius can do —
// see docs/DSL_GAPS.md.
const shell = union(face, foot, { blend: 2 }).tag("shell");

const body = union(shell, gusset.at(-20, 0, 0), gusset.at(20, 0, 0)).tag("body");

// The motor pattern, drilled along Y through the face plate. grid() gives the
// four corners of the bolt square; they are lifted to the bore centre height.
const motorHole = cylinder(boltDia / 2, plateT * 4).rotate("x", 90);
const motorHoles = grid(2, 2, boltSquare, boltSquare).map(([x, z]) => [
  x,
  0,
  z + faceH / 2,
]);

const pilotBore = cylinder(pilot / 2, plateT * 4).rotate("x", 90).at(0, 0, faceH / 2);

const frameHoles = repeat(cylinder(mountHole / 2, plateT * 4), [
  [-16, footD - 10],
  [16, footD - 10],
]);

const drilled = body
  .cut(pilotBore, repeat(motorHole, motorHoles), frameHoles)
  .tag("drilled");

// Deburr the motor face only: the four screw holes and the pilot bore, on the
// one face that has to sit flat against the motor. Naming the face normal is
// what limits it to five — the same five holes have rims on the back face too,
// and those are none of this operation's business.
return drilled
  .edges({
    generatedBy: "drilled",
    curve: "circle",
    role: "hole",
    adjacentTo: { faceNormal: "-y" },
  })
  .expect({ count: 5 })
  .chamfer(0.5)
  .tag("motor_face_deburr");
`,Ye=`// A pillow block for a 20 mm bore ball bearing (a UCP-204 style housing,
// simplified to the shapes a machinist would actually cut from bar stock).
//
// The bore axis runs along Y, so the boss is a cylinder rotated 90° about X.
// Shaft height — bore centreline above the mounting face — is the dimension
// this part exists to hold, so it is written once and everything else follows.

const shaftHeight = 25;    // bore centreline above the base underside
const baseW = 90;          // along X
const baseD = 32;          // along Y, the bearing width
const baseT = 12;          // base plate thickness
const bossDia = 47;        // bearing outer diameter housing
const bore = 20;
const boltSpacing = 66;    // between mounting hole centres
const boltDia = 8.5;       // clearance for M8

const base = box(baseW, baseD, baseT)
  .at(0, 0, baseT / 2)
  .tag("base");

// The boss reaches down into the base rather than butting onto its top face.
// A blended union between two solids that only touch on a face cannot build
// and is refused — see docs/DSL_GAPS.md — and a real housing is one casting anyway.
const boss = cylinder(bossDia / 2, baseD)
  .rotate("x", 90)
  .at(0, 0, shaftHeight)
  .tag("boss");

const body = union(base, boss, { blend: 4 }).tag("body");

// Overlength through the whole housing; a bore that stops exactly on the face
// leaves a zero-thickness sliver for the boolean to trip over.
const bearingBore = cylinder(bore / 2, baseD * 3).rotate("x", 90).at(0, 0, shaftHeight);

// Slots, not holes, in a real pillow block — alignment is set at assembly.
// A slot is a box with two cylinders on its ends: the DSL has no 2D sketch,
// which is fine here because the shape is genuinely three primitives.
const slotEnds = boltDia / 2;
const slotTravel = 6;
const slot = union(
  box(slotTravel, boltDia, baseT * 3),
  cylinder(slotEnds, baseT * 3).at(-slotTravel / 2, 0),
  cylinder(slotEnds, baseT * 3).at(slotTravel / 2, 0),
);

const machined = body
  .cut(
    bearingBore,
    slot.at(-boltSpacing / 2, 0, baseT / 2),
    slot.at(boltSpacing / 2, 0, baseT / 2),
  )
  .tag("machined");

// Both bore rims get a lead-in so the bearing presses in square. Two, not six:
// \`role: "hole"\` matches complete circular rims, and a slot end is a pair of
// arcs joined to straight sides, not a circle. It also keeps the boss's own
// outside rim — same curve, same radius, convex — out of the selection.
return machined
  .edges({ generatedBy: "machined", curve: "circle", role: "hole" })
  .expect({ count: 2 })
  .chamfer(0.8)
  .tag("press_fit_lead_in");
`,Ze=`// A socket-weld pipe tee for 1" schedule 40 pipe.
//
// The pipe it joins is 33.4 OD, 3.38 wall, so the bore is 26.64. The fitting
// body is deliberately fatter than the pipe: its sockets have to swallow the
// pipe OD and still leave a wall, which is why the body diameters are not
// \`pipeOd\`. The run and branch bodies differ because a blended union of two
// equal-radius cylinders crossing at 90° is refused at this size —
// see docs/DSL_GAPS.md; unequal radii is also what a real fitting looks like. A
// socket cut wider than the body would not be a counterbore at all — it would
// saw the end off, and the reported bounding box is where that shows up.
//
// Two bodies of revolution crossing at 90° is the shape booleans are best at,
// and the reason this is a good part to test a kernel with: the branch
// intersection curve is a genuine 3D curve, not a circle, outside and in.

const pipeOd = 33.4;
const runOd = 48;      // fitting body around the run
const branchOd = 42;   // fitting body around the branch
const bore = 26.64;
const run = 100;      // end to end along X
const branch = 55;    // centre to branch face along Z
const socketDia = 33.9;  // slip fit over the pipe OD
const socketDepth = 12;

const runBody = cylinder(runOd / 2, run).rotate("y", 90).tag("run");

const branchBody = cylinder(branchOd / 2, branch)
  .at(0, 0, branch / 2)
  .tag("branch");

// The blend at the crotch is what a cast or forged fitting actually has, and
// it is structurally the point: the bare intersection of two cylinders is a
// stress concentration exactly where the pressure load is highest.
const body = union(runBody, branchBody, { blend: 2 }).tag("body");

// One bore through the run, one down the branch. They meet inside, so the
// hollow is a single connected volume — as it must be for a fitting.
const runBore = cylinder(bore / 2, run * 1.2).rotate("y", 90);
const branchBore = cylinder(bore / 2, branch * 1.2).at(0, 0, branch / 2);

// Sockets: a counterbore at each of the three ends that the pipe slips into,
// stopping on a shoulder. Modelled overlength outward for the same reason
// every other cutter here is.
const socket = cylinder(socketDia / 2, socketDepth * 2);
const sockets = union(
  socket.rotate("y", 90).at(run / 2, 0, 0),
  socket.rotate("y", 90).at(-run / 2, 0, 0),
  socket.at(0, 0, branch),
);

const bored = body.cut(runBore, branchBore, sockets).tag("bored");

// Break the branch's socket mouth, the rim a pipe is pushed into. Selecting on
// position rather than face normal is deliberate: a rim also borders its own
// cylindrical wall, and on this part those walls point in every direction.
return bored
  .edges({ generatedBy: "bored", curve: "circle", role: "hole", at: { z: "max" } })
  .expect({ count: 1 })
  .chamfer(1.5)
  .tag("branch_socket_lead_in");
`,Ue=`// A vertical dinner-plate stand for a cupboard shelf, drawn so the pegs stop
// breaking off.
//
// The stand this replaces (makerworld.com/models/1199653, after printables
// 227795) holds each plate between two rows of thin hooked pegs that taper to
// a point. Printed standing up, a peg is a stack of layers lying across the
// direction a leaning plate pushes it, so the root takes its bending moment on
// the weakest plane it has, and the hooked tip is a thin overhang. Both broke.
//
// Three changes. The peg is a straight cone leaning outward, twelve
// millimetres at the root instead of about eight: bending strength goes with
// the cube of the diameter, so that is over three times stronger before the
// blend is counted. The root is blended into the base at 4 mm, which spreads
// the load over many layers instead of one line and takes the stress
// concentration out of the corner. And the tip is a fillet rather than a
// point. The original's sizes are read off its photographs, not its file.
//
// It prints in one piece, base down, no supports: a 10° lean is well inside
// the overhang limit, and the pegs are still layers stacked across the load,
// which no one-piece orientation avoids. Four walls make a Ø12 peg solid
// perimeter, and PETG holds its layers together better than PLA. A peg that
// must be stronger again is printed lying down and pressed into the base,
// which is a different part.

const plates = 8;
const pitch = 28;         // plate to plate; vertical plates nest, so this is less than a plate is tall
const rowGap = 100;       // between the peg rows: see the note on plate sizes below
const margin = 12;        // base beyond the outermost peg, along the rows
const baseT = 6;
const cornerR = 10;

const pegRootD = 12;      // at the base surface
const pegTipD = 6;
const pegH = 40;          // above the base
const lean = 10;          // degrees outward, following the plate's edge
const rootBlend = 4;
const bury = 3;           // into the base, so the union has a seam to blend; short of the underside

const slotW = 16;
// Ends 5 mm short of the peg pads, whose reach is the root plus the blend.
const slotL = rowGap - 2 * (pegRootD / 2 + rootBlend + 5);

// A plate stands on the shelf through its slot and rests against a peg on
// each side, where its rim crosses the peg rows. The rows decide which plates
// that reaches, and the small ones are the test: a plate's rim rises steeply
// near its edge, so the rows must sit inside the smallest plate's chord. With
// the rows 100 mm apart, a 15 cm side plate meets the pegs 22 mm above the
// shelf and a 27 cm dinner plate 10 mm, against tips 45 mm up; a plate whose
// rim will not pass the slot stands 6 mm higher and still keeps 16 mm of peg.
// At 140 mm and a 20° lean, nearer the original, a 20 cm plate clears the
// tips and is not held at all, and at 120 mm a 15 cm one reaches the top 2 mm.
const L = plates * pitch + 2 * margin;
// Wide enough that the leaning tips stay over the base: the row, the lean's
// reach, the tip radius, and the same margin as the ends.
const W = 2 * (rowGap / 2 + pegH * Math.sin((lean * Math.PI) / 180) + pegTipD / 2 + margin);

// The base: rounded corners, a soft top edge, a small chamfer underneath so
// the first layer's elephant foot has nowhere to be.
const slab = box(L, W, baseT).at(0, 0, baseT / 2);
const base = slab
  .edges("|Z")
  .expect({ count: 4 })
  .fillet(cornerR)
  .edges(">Z")
  .expect({ count: 8 })
  .fillet(1.5)
  .edges("<Z")
  .expect({ count: 8 })
  .chamfer(0.8)
  .tag("base");

// One peg, standing on z = 0 and reaching \`bury\` below it; placed on the
// base's top face, so the buried end stops inside the base and never reaches
// the underside, which would give the union a second seam there. The cone is
// drawn from where it enters the base, so the root diameter above is the one
// at the surface rather than the one under it.
const taper = (pegRootD - pegTipD) / 2 / pegH;
const pegLen = pegH + bury;
const peg = cone(pegRootD / 2 + bury * taper, pegTipD / 2, pegLen)
  .at(0, 0, (pegH - bury) / 2)
  .edges(">Z")
  .expect({ count: 1 })
  .fillet(pegTipD / 2 - 0.5)
  .tag("peg");

// The rows. \`plates\` slots need \`plates + 1\` pegs a row; the front row leans
// toward +Y and the back row toward -Y, and rotating about X by a negative
// angle is what carries +Z toward +Y (see Shape.rotate).
const stations = Array.from({ length: plates + 1 }, (_, i) => (i - plates / 2) * pitch);
const front = stations.map((x) => peg.rotate("x", -lean).at(x, rowGap / 2, baseT));
const back = stations.map((x) => peg.rotate("x", lean).at(x, -rowGap / 2, baseT));

// Unioned flat rather than as two pre-fused rows: pegs do not touch each
// other, and a fuse that has to bridge eighteen separate lumps at once is the
// failure docs/GOTCHAS.md records for pipe bends.
const body = union(base, ...front, ...back, { blend: rootBlend }).tag("body");

// A slot under each plate, ends rounded. The cutter runs a millimetre past
// both faces of the base, as every cutter here must.
const slotEnd = (slotL - slotW) / 2;
const slot = union(
  box(slotW, slotL - slotW, baseT + 2),
  cylinder(slotW / 2, baseT + 2).at(0, slotEnd),
  cylinder(slotW / 2, baseT + 2).at(0, -slotEnd),
).at(0, 0, baseT / 2);
const slotAt = Array.from({ length: plates }, (_, i) => (i - (plates - 1) / 2) * pitch);
const slotted = body
  .cut(...slotAt.map((x) => slot.at(x, 0, 0)))
  .tag("slotted");

// Ease the top rim of every slot so a plate rim slides in rather than catching.
return slotted
  .edges({ generatedBy: "slotted", adjacentTo: { faceNormal: "+z" } })
  .expect({ count: plates * 4 })
  .chamfer(0.8)
  .tag("slot_rims");
`,Ge=`// A pleated pendant shade: two six-pointed stars, each divided evenly and
// every point pushed along its own normal — out on even points, in on odd —
// in equal increments, frozen where the outline would touch itself; each
// corner of the result rounded; a tween between the two with a sine swell;
// and a loft *surface* through the tweens, thickened into the printable wall.
scriptBudget(4);

const points = 48;          // divisions of each star: one pleat per two points
const increments = 20;      // steps the pleating is pushed out in
const height = 200;
const levels = 21;          // sections the surface is lofted through
const wall = 1.4;           // printed wall, centred on the surface
// The narrowest a pleat may close: two walls and the gap a nozzle needs
// between them, or the thickened walls would run into each other.
const closest = wall + 0.8;
const smoothing = 0.25;     // share of the vertex rule blended in each step
const flare = 0.22;         // the sine swell of the silhouette, as a share of the radius
// Corners are rounded to \`tipRadius\`; one whose folds are too short for that
// is rounded tighter, but never below \`tightest\`. A fitted curve turns
// tighter between its points than the points do, and \`thicken\` refuses a
// wall that would leave its inside skin all but folded, so both stay well
// clear of half the wall.
const tipRadius = 2.2;
const tightest = 1.2;

// A star as a smooth closed curve, divided into equal lengths.
function star(tips, outer, inner, twist) {
  const dense = 4000;
  const raw = Array.from({ length: dense }, (_, i) => {
    const a = (2 * Math.PI * i) / dense;
    const r = (outer + inner) / 2 + ((outer - inner) / 2) * Math.cos(tips * a);
    return [r * Math.cos(a + twist), r * Math.sin(a + twist)];
  });
  const run = [0];
  for (let i = 1; i <= dense; i++) {
    const p = raw[i - 1];
    const q = raw[i % dense];
    run.push(run[i - 1] + Math.hypot(q[0] - p[0], q[1] - p[1]));
  }
  const total = run[dense];
  const out = [];
  let k = 0;
  for (let i = 0; i < points; i++) {
    const target = (i * total) / points;
    while (run[k + 1] < target) k++;
    const f = (target - run[k]) / (run[k + 1] - run[k]);
    const p = raw[k];
    const q = raw[(k + 1) % dense];
    out.push([p[0] + f * (q[0] - p[0]), p[1] + f * (q[1] - p[1])]);
  }
  return out;
}

// Unit normal from the chord between a point's neighbours: outward for an
// anticlockwise ring.
function normals(ring) {
  const n = ring.length;
  return ring.map((_, i) => {
    const a = ring[(i + n - 1) % n];
    const b = ring[(i + 1) % n];
    const tx = b[0] - a[0];
    const ty = b[1] - a[1];
    const len = Math.sqrt(tx * tx + ty * ty);
    return [ty / len, -tx / len];
  });
}

// Radius of the circle through a point and its neighbours, and which side
// the ring turns toward there (+1 outward, -1 inward).
function bend(ring, i) {
  const n = ring.length;
  const p = ring[(i + n - 1) % n];
  const q = ring[i];
  const r = ring[(i + 1) % n];
  const cross = (q[0] - p[0]) * (r[1] - q[1]) - (q[1] - p[1]) * (r[0] - q[0]);
  const a = Math.hypot(q[0] - p[0], q[1] - p[1]);
  const b = Math.hypot(r[0] - q[0], r[1] - q[1]);
  const c = Math.hypot(r[0] - p[0], r[1] - p[1]);
  const radius = Math.abs(cross) < 1e-12 ? Infinity : (a * b * c) / (2 * Math.abs(cross));
  // An anticlockwise ring turning left bends toward its inside.
  return { radius, concave: cross > 0 ? -1 : 1 };
}

// The cubic B-spline vertex rule — Chaikin's corner cutting read back onto
// the points — blended in by \`smoothing\`.
function rebuild(ring) {
  const n = ring.length;
  return ring.map((q, i) => {
    const p = ring[(i + n - 1) % n];
    const r = ring[(i + 1) % n];
    const sx = p[0] / 8 + (3 * q[0]) / 4 + r[0] / 8;
    const sy = p[1] / 8 + (3 * q[1]) / 4 + r[1] / 8;
    return [q[0] + smoothing * (sx - q[0]), q[1] + smoothing * (sy - q[1])];
  });
}

// Push every point out or in by \`amplitude\` over \`increments\` steps. A point
// moving into a bend moves no further than most of that bend's radius, and a
// point whose ring would cross itself or close past \`closest\` is frozen where
// it was.
function pleat(base, amplitude) {
  const baseNormals = normals(base);
  const offset = new Array(points).fill(0);
  const frozen = new Array(points).fill(false);
  const place = () => rebuild(base.map((p, i) => [p[0] + baseNormals[i][0] * offset[i], p[1] + baseNormals[i][1] * offset[i]]));
  let ring = place();
  for (let step = 0; step < increments; step++) {
    const previous = offset.slice();
    for (let i = 0; i < points; i++) {
      if (frozen[i]) continue;
      const out = i % 2 === 0 ? 1 : -1;
      let move = (out * amplitude) / increments;
      const { radius, concave } = bend(ring, i);
      if (Math.sign(move) === concave) move = Math.sign(move) * Math.min(Math.abs(move), 0.8 * radius);
      offset[i] += move;
    }
    ring = place();
    const touching = new Set();
    for (const [i, j] of outlineCrossings(ring)) {
      for (const k of [i, i + 1, j, j + 1]) touching.add(k % points);
    }
    const gaps = outlineGaps(ring, { ignoreWithin: 2 * closest, upTo: closest });
    gaps.forEach((g, i) => { if (g < closest) touching.add(i); });
    if (touching.size) {
      for (const i of touching) {
        offset[i] = previous[i];
        frozen[i] = true;
      }
      ring = place();
    }
  }
  return ring;
}

// The pleated points as a curve with every corner rounded: an arc tangent to
// the two straight folds either side, then the fold. Every corner is sampled
// the same number of times on every star, so the points pair across the
// sections the surface is lofted through.
function filletRing(ring, radius, roundSamples, foldSamples) {
  const n = ring.length;
  const out = [];
  const corners = ring.map((q, i) => {
    const p = ring[(i + n - 1) % n];
    const r = ring[(i + 1) % n];
    const u = [q[0] - p[0], q[1] - p[1]];
    const v = [r[0] - q[0], r[1] - q[1]];
    const lu = Math.hypot(u[0], u[1]);
    const lv = Math.hypot(v[0], v[1]);
    const du = [u[0] / lu, u[1] / lu];
    const dv = [v[0] / lv, v[1] / lv];
    const turn = Math.acos(Math.max(-1, Math.min(1, du[0] * dv[0] + du[1] * dv[1])));
    // The round may take up to 45 % of the shorter fold either side.
    const round = Math.min(radius, (0.45 * Math.min(lu, lv)) / Math.tan(turn / 2));
    if (round < tightest) {
      throw new Error(\`a fold of \${Math.min(lu, lv).toFixed(2)} mm turning \${(turn * 180 / Math.PI).toFixed(0)}° can only be rounded to \${round.toFixed(2)} mm, under \${tightest}; make the pleats shallower\`);
    }
    const cut = round * Math.tan(turn / 2);
    const start = [q[0] - du[0] * cut, q[1] - du[1] * cut];
    const end = [q[0] + dv[0] * cut, q[1] + dv[1] * cut];
    const side = du[0] * dv[1] - du[1] * dv[0] > 0 ? 1 : -1;
    const centre = [start[0] - side * du[1] * round, start[1] + side * du[0] * round];
    return { start, end, centre, turn, side, round };
  });
  for (let i = 0; i < n; i++) {
    const c = corners[i];
    const a0 = Math.atan2(c.start[1] - c.centre[1], c.start[0] - c.centre[0]);
    for (let k = 0; k < roundSamples; k++) {
      const a = a0 + c.side * c.turn * (k / roundSamples);
      out.push([c.centre[0] + c.round * Math.cos(a), c.centre[1] + c.round * Math.sin(a)]);
    }
    const next = corners[(i + 1) % n].start;
    for (let k = 0; k < foldSamples; k++) {
      const t = k / foldSamples;
      out.push([c.end[0] + t * (next[0] - c.end[0]), c.end[1] + t * (next[1] - c.end[1])]);
    }
  }
  return out;
}

// The top star is turned π/4 against the bottom one. Points pair by index
// across the sections, so each pleat climbs to the matching pleat of the top
// star and the pleats twist as they rise.
const bottom = filletRing(pleat(star(6, 90, 72, 0), 12), tipRadius, 4, 10);
const top = filletRing(pleat(star(6, 64, 52, Math.PI / 4), 10), tipRadius, 4, 10);

const centroid = (ring) => ring.reduce((c, p) => [c[0] + p[0] / ring.length, c[1] + p[1] / ring.length], [0, 0]);
const sections = Array.from({ length: levels }, (_, k) => {
  const t = k / (levels - 1);
  const z = t * height;
  const tween = bottom.map((p, i) => [p[0] + t * (top[i][0] - p[0]), p[1] + t * (top[i][1] - p[1])]);
  const [cx, cy] = centroid(tween);
  const s = 1 + flare * Math.sin(Math.PI * t);
  return { z, curve: [{ fit: tween.map(([x, y]) => [cx + s * (x - cx), cy + s * (y - cy)]), tolerance: 0.05 }] };
});

const shade = surfaceLoft(sections, { closed: true, smooth: true }).tag("shade");
// A thickened wall's rims lean with the surface; a slab off each end leaves
// the flat ring a printer starts on and a clean top.
const rim = 0.4;
const slab = box(400, 400, 20);
return shade.thicken(wall).cut(slab.at(0, 0, rim - 10), slab.at(0, 0, height - rim + 10));
`,$e=`// Screw-top jar: a jar with a threaded neck and its cap, printed as two parts.
//
// The thread is modelled, not drawn as a tap drill: M40 × 3 (ISO 261 fine)
// with the ISO 68-1 basic profile, \`threadedRod\` on the neck and
// \`threadedHole\` in the cap, each given the same radial clearance so the pair
// turns freely off the printer.
//
// A thread and its mate only fit in phase. Both functions put the tooth on +X
// at z = 0 of their own frame; the neck's frame is at z = 48 and the cap's
// cutter's at z = 46, two millimetres, or 2/3 of a pitch, lower. Turning the
// cutter 360° × 2/3 = 240° backwards — the same as 120° forwards — puts it
// back in phase, and \`between_bodies\` reads the flank gap:
// clearance · (2/√3) / √(4/3 + (P / 2πr)²), 0.2499 mm at the cap's crest.
//
// Jar: a Ø50 × 45 body with a 2 mm wall and floor, the neck's 7.5 mm of thread
// above it, and a Ø34 bore through the neck. Cap: Ø48 × 12, the thread cut
// 10 mm up into it from its open face, 2 mm of roof above.
//
// A horizontal slice of a thread has the same area at every height, so any
// length L of one is π r1² L + 2π L / P · ∫ r w(r) dr, w the tooth's width:
// 8369.020 mm³ for the neck's 7.5 mm above the body and 11758.317 mm³ for the
// cap's 10 mm of cutter. Jar: 88357.293 + 8369.020 − 68138.003 (cavity) −
// 8625.243 (bore, 43 to 52.5) = 19963.068 mm³. Cap: 21714.688 − 11758.317 =
// 9956.371 mm³. The exact solids read 19963.054 and 9956.381.
const neck = { diameter: 40, pitch: 3 };
const play = 0.25; // clearance on each part, radially; tune it on the printer
const bodyR = 25, bodyH = 45, wall = 2, floor = 2;
const neckLength = 9, neckZ = 48; // thread from 43.5 to 52.5, 1.5 of it inside the body
const boreR = 17;
const capR = 24, capBottom = 46, capHeight = 12, capDepth = 10;

const jar = cylinder(bodyR, bodyH)
  .at(0, 0, bodyH / 2)
  .union(threadedRod(neck, neckLength, { clearance: play }).at(0, 0, neckZ))
  // The bore first, from past the neck's top down into where the cavity will
  // be, so the cavity's cut opens into it rather than sealing a void.
  .cut(cylinder(boreR, 13).at(0, 0, 48.5))
  .cut(cylinder(bodyR - wall, bodyH - floor - wall).at(0, 0, floor + (bodyH - floor - wall) / 2))
  .tag("jar");

const cap = cylinder(capR, capHeight)
  .at(0, 0, capBottom + capHeight / 2)
  .cut(
    threadedHole(neck, capDepth, { clearance: play })
      .rotate("x", 180) // enter from below, going up
      .rotate("z", 120) // back in phase with the neck; see above
      .at(0, 0, capBottom),
  )
  .tag("cap");

return { jar, cap };
`,Ke=`// A rigid set-screw shaft coupler: 8 mm motor shaft to 10 mm leadscrew.
//
// The two bores are different sizes and meet in the middle, which is the point
// of the part — and it is also the thing that makes it easy to get wrong. The
// bores are drilled to a depth each, not through, so there is a web of metal
// between them; that web is what stops the screw from being driven into the
// motor bearing.

const od = 25;
const length = 30;
const boreA = 8;      // motor side
const boreB = 10;     // leadscrew side
const boreDepth = 13; // each, leaving a 4 mm web at the centre
const setScrew = 4.2; // tap drill for M5 grub screws

const body = cylinder(od / 2, length).tag("body");

// Each bore is modelled overlength and placed so its open end sticks out past
// the coupler face: a cutting tool that stops exactly on a face leaves a
// zero-thickness sliver, which is a classic source of boolean failures.
const over = 5;
const motorBore = cylinder(boreA / 2, boreDepth + over)
  .at(0, 0, -length / 2 + (boreDepth + over) / 2 - over)
  .tag("motor_bore");

const screwBore = cylinder(boreB / 2, boreDepth + over)
  .at(0, 0, length / 2 - (boreDepth + over) / 2 + over)
  .tag("screw_bore");

// Two grub screws per side, at 90° to each other, so the shaft is pinched
// rather than pushed off centre. Radial holes are cylinders rotated onto X
// and Y and moved along the axis.
const grub = cylinder(setScrew / 2, od * 1.5);
const grubs = union(
  grub.rotate("y", 90).at(0, 0, -length / 2 + boreDepth / 2),
  grub.rotate("x", 90).at(0, 0, -length / 2 + boreDepth / 2),
  grub.rotate("y", 90).at(0, 0, length / 2 - boreDepth / 2),
  grub.rotate("x", 90).at(0, 0, length / 2 - boreDepth / 2),
);

const machined = body.cut(motorBore, screwBore, grubs).tag("machined");

// Lead-in on the leadscrew bore so a shaft enters without scraping. One edge:
// naming the +Z face normal excludes the motor bore's rim at the other end,
// and the grub screw holes contribute nothing either way — they break out
// into the bores as arcs, and their outer rims lie on the round outside face,
// whose normal is not an axis direction.
return machined
  .edges({
    generatedBy: "machined",
    curve: "circle",
    role: "hole",
    adjacentTo: { faceNormal: "+z" },
  })
  .expect({ count: 1 })
  .chamfer(0.8)
  .tag("screw_side_lead_in");
`,Je=`// A spur gear pair, module 2, 20 and 30 teeth, meshed at their centre
// distance: a 20 mm pitch-radius pinion on a 5 mm motor shaft driving a 30 mm
// pitch-radius wheel on an 8 mm shaft, 1.5 : 1.
//
// Every flank is the involute of its base circle, drawn by spurGearOutline as
// a curve the script certifies against the formula; the report's
// curve_bound_mm is how far that proof lets a flank stray. Tip and root are
// exact arcs; below the base circle the flank runs straight in to the
// root, where a hobbed gear would have a trochoid that nothing touches.
//
// Each tooth is thinned by 0.1 mm at the pitch circle. Meshed centred, that
// play is shared between the two flanks of every tooth space, and involute
// flanks in mesh stay the same distance apart along the line of action, so
// the two bodies come no closer than 0.1 · cos 20° = 0.094 mm anywhere —
// which \`between_bodies\` measures on the built solids.
const m = 2;
const pinionTeeth = 20;
const wheelTeeth = 30;
const width = 8;
const backlash = 0.1;
const centres = (m * (pinionTeeth + wheelTeeth)) / 2;

const pinion = extrude(spurGearOutline({ module: m, teeth: pinionTeeth, backlash }), width)
  .cut(cylinder(5 / 2, width * 2))
  .tag("pinion");

// A tooth of the pinion points along +X, at the wheel. The wheel's teeth also
// start on +X, so turning it half a tooth puts a space on -X to take it.
const wheel = extrude(spurGearOutline({ module: m, teeth: wheelTeeth, backlash }), width)
  .cut(cylinder(8 / 2, width * 2))
  .rotate("z", 180 / wheelTeeth)
  .at(centres, 0, 0)
  .tag("wheel");

return { pinion, wheel };
`,Ve=`// A 20-tooth GT2 timing pulley for 6 mm belt, on a 5 mm motor shaft.
//
// APPROXIMATE, and deliberately so. A real GT2 tooth is a curvilinear profile
// defined by the belt standard; here each groove is a cylinder on the pitch
// circle, which is the right depth and pitch but not the right flank shape.
// Printed, it runs; as a mould tool, it does not. A section can hold arcs now,
// so an arc-and-line groove extruded along the pulley axis is authorable
// (docs/DSL_GAPS.md, "arcs in a section"); what is missing is the numbers.
// The GT2 flank is published as a proprietary drawing, not as radii this file
// can cite, and inventing a "close enough" tooth would be exactly the
// approximation this project refuses to make silently — so it is called out
// here instead.

const teeth = 20;
const pitch = 2;                          // GT2: 2 mm belt pitch
const pitchDia = (teeth * pitch) / Math.PI; // 12.732 for 20 teeth
const beltWidth = 6;
const bodyH = beltWidth + 2;              // belt width plus a little
const flangeDia = pitchDia + 5;
const flangeT = 1.2;
const bore = 5;
const grubDia = 3.3;                      // tap drill for M4 grub screw
const hubDia = bore + 8;                  // 4 mm of wall each side to tap into
const hubH = 8;                           // how far the hub stands off the flange

const body = cylinder(pitchDia / 2 + 0.5, bodyH).tag("body");

// The tooth grooves. Each is a cylinder standing on the pitch circle, so the
// groove depth follows from where the pitch circle sits — that is the one
// dimension a belt actually cares about.
const groove = cylinder(0.62, beltWidth * 3).at(pitchDia / 2, 0);
const grooves = around(groove, teeth).tag("grooves");
const toothed = body.cut(grooves).tag("toothed");

// Flanges keep the belt on; the hub carries the grub screw. All three are
// unioned onto the body, which is one solid in the real part too — these
// pulleys are turned from one blank.
//
// They go on *after* the teeth are cut. Each groove cylinder is longer than the
// body so that it clears both ends, so a flange already in place would be
// drilled through by all twenty of them. Unioned afterwards it fills the groove
// ends instead, and the teeth stop where the belt does.
const flange = cylinder(flangeDia / 2, flangeT);
const blank = union(
  toothed,
  flange.at(0, 0, (bodyH - flangeT) / 2),
  flange.at(0, 0, -(bodyH - flangeT) / 2),
  // Buried 1 mm into the body, so the union has an overlap to work with rather
  // than two faces that exactly touch.
  cylinder(hubDia / 2, hubH + 1).at(0, 0, -(bodyH + hubH) / 2 + 0.5),
).tag("blank");

// The bore runs the whole height, hub included.
const shaftBore = cylinder(bore / 2, (bodyH + hubH) * 2).tag("bore");

// One radial grub screw into the bore, through the hub. A 1.2 mm flange has
// nowhere to put a thread and the belt path cannot be crossed, so the hub is
// the only place it can go — which is where a real pulley puts it. The tool
// starts inside the bore and stops outside the hub: centred on the axis, as a
// primitive is by default, it would drill straight out the far side too.
const grub = cylinder(grubDia / 2, 6)
  .rotate("y", 90)
  .at(hubDia / 2 - 1.5, 0, -(bodyH + hubH) / 2)
  .tag("grub");

// Each cut is its own tagged step. \`generatedBy\` names the node that made an
// edge, so one cut carrying several tools gives every tool the same name — and
// then the query below cannot tell the bore's rim from the twenty groove rims.
const machined = blank.cut(shaftBore).tag("bored").cut(grub).tag("machined");

// One lead-in, on the bottom of the bore. Position, not face normal: the grub
// screw hole is drilled across the part, and a rim reports its own cylindrical
// wall as adjacent, so a face-normal query would catch it too.
return machined
  .edges({ generatedBy: "bored", curve: "circle", role: "hole", at: { z: "min" } })
  .expect({ count: 1 })
  .chamfer(0.4)
  .tag("bore_lead_in");
`,Qe=`// Twisted planter and its drip saucer, printed as two parts.
//
// The planter is a six-point star lofted through nine sections, each wider
// and turned further than the last, so its walls come out as twisted facets.
// The saucer is a dish of smooth bumps in rings of 1, 6, 12 and 18, which the
// planter stands on.

const points = 6;
const height = 90;
const twist = 60; // degrees, base to rim
const layers = 8;
const wall = 3; // inset across the star; the facets' own wall measures 2.0 mm
const floor = 3;

// At height z: the star's tip radius, flaring fastest near the base, and its turn.
const radiusAt = (z) => 34 + 18 * Math.sin((z / height) * (Math.PI / 2));
const turnAt = (z) => (z / height) * twist;

function star(z, inset) {
  const outline = Array.from({ length: 2 * points }, (_, i) => {
    const r = (i % 2 === 0 ? 1 : 0.78) * radiusAt(z) - inset;
    const angle = ((i * 180) / points + turnAt(z)) * (Math.PI / 180);
    return [r * Math.cos(angle), r * Math.sin(angle)];
  });
  return { z, outline };
}

// Section heights shared by the outside and the cavity, so their facets stay parallel.
const levels = Array.from({ length: layers + 1 }, (_, i) => (i * height) / layers);

const outside = loft(levels.map((z) => star(z, 0)));
const cavity = loft([floor, ...levels.filter((z) => z > floor), height + 1].map((z) => star(z, wall)));

const drain = cylinder(2.5, 3 * floor);

const planter = outside
  .cut(
    cavity, // 1 mm past the rim, so the top opens cleanly
    ...polar(6, 14, { straddle: true }).map(([x, y]) => drain.at(x, y)), // between the saucer's first ring of bumps
  )
  .tag("planter");

// Wide enough that the 34 mm star base sits inside with room for runoff.
const saucerRadius = 42;
const saucerFloor = 2;
const bumpRadius = 5.4;
const bumpHeight = 2.4;
const ringSpacing = 11;
const ringCount = 3;

// Smootherstep: no slope and no curvature where a bump leaves the floor or at its top.
const ease = (x) => x * x * x * (x * (6 * x - 15) + 10);

function bump() {
  const z = (r) => saucerFloor + bumpHeight * (1 - ease(r / bumpRadius));
  const profile = [0.92, 0.82, 0.7, 0.58, 0.46, 0.34, 0.2].map((t) => [t * bumpRadius, z(t * bumpRadius)]);
  return revolve([
    [0, 0],
    [bumpRadius, 0],
    [bumpRadius, saucerFloor],
    { spline: profile, start: [-1, 0], end: [-1, 0] },
    [0, saucerFloor + bumpHeight],
  ]);
}

const dish = revolve([
  [0, 0],
  { at: [saucerRadius + 2, 0], round: 3 },
  [saucerRadius + 2, 11],
  { through: [saucerRadius + 1, 12] },
  [saucerRadius, 11],
  { at: [saucerRadius, saucerFloor], round: 2 },
  [0, saucerFloor],
]);

// One bump in the middle, then rings of 6, 12, 18: each ring's circumference grows
// by 2π·spacing, so six more keeps every bump the same distance from its neighbours.
// Even rings sit half a step round from the odd ones.
const one = bump();
const bumps = [one];
for (let ring = 1; ring <= ringCount; ring++) {
  const count = 6 * ring;
  const start = ring % 2 === 0 ? 180 / count : 0;
  for (const [x, y] of polar(count, ring * ringSpacing, { start })) bumps.push(one.at(x, y));
}

const saucer = dish.union(...bumps);

return {
  planter: planter.at(0, 0, saucerFloor + bumpHeight),
  saucer,
};
`,et=`// A toolroom V-block: a 90° vee that holds round stock on its axis, whichever
// diameter it is.
//
// The vee is a square block rotated 45° and subtracted from the top face. That
// is not a shortcut — a 90° included angle *is* the corner of a square, and
// cutting it this way means the angle cannot drift when the depth changes.

const size = 50;      // cube, the usual matched-pair stock size
const veeDepth = 18;  // how far the vee bites into the top face

const body = box(size, size, size).tag("body");

// The rotated cutter must be wide enough that only its two lower faces bound
// the groove; its half-diagonal has to clear the block, so size * 1.5 is safe.
const cutter = size * 1.5;

// Rotating about Y puts the vee's axis along Y — stock lies front to back. The
// apex sits veeDepth below the top face, and the cutter's own half-diagonal
// (cutter * sqrt(2) / 2) is how far the apex is below its centre.
const vee = box(cutter, size * 2, cutter)
  .rotate("y", 45)
  .at(0, 0, size / 2 - veeDepth + (cutter * Math.SQRT2) / 2)
  .tag("vee");

// Through hole for the clamp screw, plus a cross slot the clamp strap sits in.
const clampHole = cylinder(4.25, size * 2).rotate("x", 90).tag("clamp_hole");
const strapSlot = box(size * 2, 12, 6)
  .at(0, 0, -size / 2 + 3)
  .tag("strap_slot");

const machined = body.cut(vee, clampHole, strapSlot).tag("machined");

// Break every long edge on the top face: the two lips the vee cuts, and the
// two outside edges of the block. Four, not two — ">Z and |Y" says "at the top
// of the block, running along Y", and the outside edges are as much at the top
// as the vee lips are. These are the edges that get handled and the ones that
// mark up a workpiece, so all four wanting a break is the right answer here.
return machined
  .edges(">Z and |Y")
  .expect({ count: 4 })
  .chamfer(1)
  .tag("vee_lip_break");
`,tt=`// A wash bottle: a turned body whose outline is one section — straight
// walls, a spline shoulder, a neck with a rolled bead and a radiused base —
// hollowed by a second section, with a smooth spout tube curving out of the
// shoulder.
//
// Everything curved is drawn in section rather than filleted afterwards:
// \`{ at, round }\` for the base radius, \`{ spline }\` for the shoulder,
// \`{ through }\` for the bead on the neck. The spout is \`pipe({ spline })\`,
// one exact curve rather than runs and bends.

const bodyR = 32;
const shoulderZ = 70;
const neckR = 11;
const neckZ = 114;
const topZ = 132;
const wall = 2;

const outer = revolve([
  [0, 0],
  { at: [bodyR, 0], round: 6 },
  [bodyR, shoulderZ],
  { spline: [[28, 95], [16, 108]], start: [0, 1], end: [-0.4, 1] },
  [neckR, neckZ],
  [neckR, topZ - 6],
  { through: [neckR + 2, topZ - 4] },
  [neckR, topZ - 2],
  [neckR, topZ],
  [0, topZ],
]).tag("body");

// The cavity: the same outline brought in by the wall, open through the neck.
const cavity = revolve([
  [0, wall],
  { at: [bodyR - wall, wall], round: 4 },
  [bodyR - wall, shoulderZ],
  { spline: [[26, 94], [14, 106]], start: [0, 1], end: [-0.4, 1] },
  [neckR - wall, neckZ],
  [neckR - wall, topZ + 1],
  [0, topZ + 1],
]).tag("cavity");

const spout = pipe({ spline: [[20, 0, 96], [40, 0, 112], [62, 0, 116], [80, 0, 108]] }, 5).tag("spout");
const bore = pipe({ spline: [[20, 0, 96], [40, 0, 112], [62, 0, 116], [80, 0, 108]] }, 2.6);

return union(outer, spout).cut(cavity, bore);
`,nt="parcad-playground",m="parts",_="folders",j="meta",ne="this browser's storage — parts here stay on this device. The installed app keeps them as files and serves MCP.";let $;function z(){return $??=new Promise((t,e)=>{const n=indexedDB.open(nt,1);n.onupgradeneeded=()=>{const a=n.result;a.createObjectStore(m,{keyPath:"path"}),a.createObjectStore(_),a.createObjectStore(j)},n.onsuccess=()=>t(n.result),n.onerror=()=>e(new Error(`this browser refused the playground its storage (${n.error?.message}); a private window may not allow it`))}).then(async t=>(await at(t),t)),$}function M(t){return new Promise((e,n)=>{t.onsuccess=()=>e(t.result),t.onerror=()=>n(t.error)})}async function oe(t){const e=await z();return M(e.transaction(t).objectStore(t).getAll())}async function W(){const t=await z();return await M(t.transaction(_).objectStore(_).getAllKeys())}async function f(t,e){const a=(await z()).transaction(t,"readwrite");e(a.objectStore(t)),await new Promise((r,i)=>{a.oncomplete=()=>r(),a.onerror=()=>i(a.error)})}async function P(t){const e=await z();return M(e.transaction(m).objectStore(m).get(t))}const E=()=>Math.floor(Date.now()/1e3),ot=Object.assign({"../../../examples/bracket.js":Te,"../../../examples/cast-foot.js":ke,"../../../examples/clevis.js":_e,"../../../examples/cover-plate.js":De,"../../../examples/diamond-v19.js":Ae,"../../../examples/display-bezel.js":Re,"../../../examples/edge-fillets.js":Se,"../../../examples/enclosure.js":Le,"../../../examples/extrusion-2020.js":He,"../../../examples/field-instrument/control-box.js":Ee,"../../../examples/field-instrument/desk-stand.js":ze,"../../../examples/field-instrument/planter.js":Me,"../../../examples/flange.js":Pe,"../../../examples/fusion360/retainer-v1.js":Oe,"../../../examples/fusion360/untitled2-v1.js":Ce,"../../../examples/fusion360/untriangle-v3.js":Be,"../../../examples/heat-sink.js":Ie,"../../../examples/hex-standoff.js":Fe,"../../../examples/hydraulic-line.js":Ne,"../../../examples/knurled-knob.js":je,"../../../examples/lidded-box.js":We,"../../../examples/manifold-block.js":qe,"../../../examples/motor-mount.js":Xe,"../../../examples/pillow-block.js":Ye,"../../../examples/pipe-tee.js":Ze,"../../../examples/plate-stand.js":Ue,"../../../examples/pleated-shade.js":Ge,"../../../examples/screw-top-jar.js":$e,"../../../examples/shaft-coupler.js":Ke,"../../../examples/spur-gears.js":Je,"../../../examples/timing-pulley.js":Ve,"../../../examples/twisted-planter.js":Qe,"../../../examples/v-block.js":et,"../../../examples/wash-bottle.js":tt});async function at(t){const e=t.transaction([m,j],"readwrite"),n=e.objectStore(j),a=e.objectStore(m),r=new Set(await M(n.get("seeded"))??[]);for(const[i,s]of Object.entries(ot).sort(([l],[o])=>l.localeCompare(o))){const l=i.replace(/^.*\/examples\//,"").replace(/\.js$/,"");r.has(l)||(a.put({path:l,script:s,tags:[],modified:E()}),r.add(l))}n.put([...r],"seeded"),await new Promise((i,s)=>{e.oncomplete=()=>i(),e.onerror=()=>s(e.error)})}const K=t=>t.split("/").pop()??t,st=t=>t.replace(/[-_]/g," "),J=(t,e)=>t.name.toLowerCase()<e.name.toLowerCase()?-1:t.name.toLowerCase()>e.name.toLowerCase()?1:0;function ae(t,e,n=""){const a=o=>n?o.startsWith(`${n}/`):!0,r=o=>n?o.slice(n.length+1):o,i=new Set;for(const o of[...e,...t.map(h=>h.path)]){if(!a(o))continue;const h=r(o).split("/");(h.length>1||e.includes(o))&&i.add(h[0])}const s=[...i].map(o=>{const h=n?`${n}/${o}`:o;return{kind:"folder",name:o,path:h,children:ae(t,e,h)}}).sort(J),l=t.filter(o=>a(o.path)&&!r(o.path).includes("/")).map(o=>({kind:"part",name:K(o.path),path:o.path,title:o.title??st(K(o.path)),bundle:!0,thumbnail:o.preview!==void 0,tags:o.tags,modified:o.modified})).sort(J);return[...s,...l]}function v(t){const e=t.split("/");for(const n of e)if(!n||n.startsWith(".")||/\.(js|parcad)$/.test(n)||/[\\:]/.test(n)||[...n].some(a=>a<" "))throw new Error(`${JSON.stringify(t)} is not a project name: each part of it must be a plain name, with no leading dot, extension, backslash or colon.`);return t}async function rt(){const[t,e]=await Promise.all([oe(m),W()]);return{projects:t.map(n=>n.path).sort(),tree:ae(t,e),directory:ne,preferred:"twisted-planter"}}async function O(t){const e=await P(v(t));if(!e)throw new Error(`no project called ${JSON.stringify(t)} in this browser.`);return e}async function it(t){return{script:(await O(t)).script}}async function lt(t,e,n){const a=await P(v(t))??{path:t,script:e,tags:[],modified:E()};return await f(m,r=>r.put({...a,script:e,modified:E(),preview:n??a.preview})),{path:t}}async function ht(t,e){if(await P(v(t)))throw new Error(`${JSON.stringify(t)} already exists.`);return await f(m,n=>n.put({path:t,script:e,tags:[],modified:E()})),{path:t}}async function ct(t){if((await W()).includes(v(t)))throw new Error(`${JSON.stringify(t)} already exists.`);return await f(_,n=>n.put(!0,t)),{path:t}}async function dt(t,e){v(t),v(e);const n=await oe(m),a=await W();if(n.some(o=>o.path===e)||a.includes(e))throw new Error(`${JSON.stringify(e)} already exists.`);const r=o=>o===t||o.startsWith(`${t}/`),i=o=>e+o.slice(t.length),s=n.filter(o=>r(o.path)),l=a.filter(r);if(!s.length&&!l.length)throw new Error(`nothing at ${JSON.stringify(t)} to rename.`);return await f(m,o=>{for(const h of s)o.delete(h.path),o.put({...h,path:i(h.path)})}),await f(_,o=>{for(const h of l)o.delete(h),o.put(!0,i(h))}),{path:e}}async function ut(t,e){const n=await O(t);await f(m,a=>a.put({...n,title:e.trim()||void 0}))}async function pt(t){const e=await O(t);return await f(m,n=>n.delete(e.path)),{trashed:"removed from this browser's storage"}}async function mt(t,e){const n=await O(t);await f(m,a=>a.put({...n,preview:e}))}async function gt(t){return(await P(t))?.preview??null}const yt=Object.freeze(Object.defineProperty({__proto__:null,DIRECTORY:ne,create:ht,createFolder:ct,list:rt,preview:gt,read:it,remove:pt,rename:dt,save:lt,setPreview:mt,setTitle:ut},Symbol.toStringTag,{value:"Module"}));export{ft as kernel,wt as prebuilt,yt as store};
//# sourceMappingURL=index-CwhVDhtF.js.map
