import { useEffect, useRef, useState } from "react";
import type { BufferGeometry, NormalBufferAttributes } from "three";
import { asset } from "../content";
import { cn } from "./frame";

type Geometry = BufferGeometry<NormalBufferAttributes>;
type Viewer = { show(src: string): Promise<void>; pause(paused: boolean): void; dispose(): void };

/**
 * A kernel-exported STL on a turntable. three.js is imported on mount, so the
 * page paints (and prerenders) without it.
 */
export function PartViewer({ src, paused = false, className }: { src: string; paused?: boolean; className?: string }) {
  const host = useRef<HTMLDivElement>(null);
  const viewer = useRef<Promise<Viewer> | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    viewer.current = createViewer(host.current!);
    return () => {
      viewer.current?.then((v) => v.dispose());
    };
  }, []);

  useEffect(() => {
    setLoading(true);
    viewer.current?.then((v) => v.show(asset(src))).finally(() => setLoading(false));
  }, [src]);

  useEffect(() => {
    viewer.current?.then((v) => v.pause(paused));
  }, [paused]);

  return (
    <div ref={host} className={cn("absolute inset-0 overflow-hidden cursor-grab touch-pan-y active:cursor-grabbing", className)}>
      <span
        className={cn(
          "pointer-events-none absolute inset-0 grid place-items-center font-mono text-[11px] tracking-wider text-ink-faint uppercase transition-opacity",
          loading ? "opacity-100" : "opacity-0",
        )}
      >
        Loading mesh
      </span>
    </div>
  );
}

async function createViewer(el: HTMLDivElement): Promise<Viewer> {
  const THREE = await import("three");
  const { STLLoader } = await import("three/examples/jsm/loaders/STLLoader.js");
  const { OrbitControls } = await import("three/examples/jsm/controls/OrbitControls.js");
  const { toCreasedNormals } = await import("three/examples/jsm/utils/BufferGeometryUtils.js");

  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  el.appendChild(renderer.domElement);
  Object.assign(renderer.domElement.style, { position: "absolute", inset: "0", width: "100%", height: "100%" });

  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(28, 1, 0.1, 5000);
  scene.add(new THREE.HemisphereLight(0xdfe8ff, 0x1a1c20, 1.6));
  const key = new THREE.DirectionalLight(0xffffff, 2.2);
  key.position.set(1, 2, 1.4);
  const rim = new THREE.DirectionalLight(0x9fc2ff, 0.9);
  rim.position.set(-1.5, 0.6, -1);
  scene.add(key, rim);

  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableZoom = false;
  controls.enablePan = false;
  controls.enableDamping = true;
  controls.autoRotate = !matchMedia("(prefers-reduced-motion: reduce)").matches;
  controls.autoRotateSpeed = 0.9;

  const stage = new THREE.Group();
  scene.add(stage);
  const loader = new STLLoader();
  const cache = new Map<string, Promise<Geometry>>();

  const load = (url: string) => {
    if (!cache.has(url)) {
      cache.set(
        url,
        loader.loadAsync(url).then((raw) => {
          raw.rotateX(-Math.PI / 2); // parcad is Z-up
          raw.computeBoundingBox();
          const box = raw.boundingBox!;
          const c = box.getCenter(new THREE.Vector3());
          raw.translate(-c.x, -box.min.y, -c.z);
          const geo = toCreasedNormals(raw, Math.PI / 7) as unknown as Geometry;
          geo.computeBoundingBox();
          geo.computeBoundingSphere();
          return geo;
        }),
      );
    }
    return cache.get(url)!;
  };

  const material = new THREE.MeshStandardMaterial({ color: 0x8b919b, metalness: 0.25, roughness: 0.48 });
  const edgeMaterial = new THREE.LineBasicMaterial({ color: 0x0b0c0f, transparent: true, opacity: 0.55 });

  const clear = () => {
    for (const child of [...stage.children]) {
      stage.remove(child);
      if (child instanceof THREE.LineSegments || child instanceof THREE.GridHelper) child.geometry.dispose();
    }
  };

  const frame = (geo: Geometry) => {
    const sphere = geo.boundingSphere!;
    const r = sphere.radius;
    const height = geo.boundingBox!.max.y;
    const target = new THREE.Vector3(0, height / 2, 0);
    const dist = (r / Math.sin(THREE.MathUtils.degToRad(camera.fov / 2))) * 1.3;
    const dir = new THREE.Vector3(1, 0.72, 1.15).normalize();
    camera.position.copy(target).addScaledVector(dir, dist);
    camera.near = dist / 50;
    camera.far = dist * 10;
    camera.updateProjectionMatrix();
    controls.target.copy(target);
    controls.update();

    const size = Math.ceil((r * 3.2) / 10) * 10;
    const grid = new THREE.GridHelper(size, size / 5, 0x2b3038, 0x1b1e23);
    const axes = new THREE.LineSegments(
      new THREE.BufferGeometry().setFromPoints([
        new THREE.Vector3(-size / 2, 0.01, 0),
        new THREE.Vector3(size / 2, 0.01, 0),
        new THREE.Vector3(0, 0.01, -size / 2),
        new THREE.Vector3(0, 0.01, size / 2),
      ]),
      new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, opacity: 0.55 }),
    );
    axes.geometry.setAttribute(
      "color",
      new THREE.Float32BufferAttribute([0.9, 0.35, 0.35, 0.9, 0.35, 0.35, 0.45, 0.85, 0.5, 0.45, 0.85, 0.5], 3),
    );
    stage.add(grid, axes);
  };

  let current = "";
  const show = async (url: string) => {
    current = url;
    const geo = await load(url);
    if (current !== url) return;
    clear();
    stage.add(new THREE.Mesh(geo, material));
    stage.add(new THREE.LineSegments(new THREE.EdgesGeometry(geo, 35), edgeMaterial));
    frame(geo);
  };

  const resize = () => {
    const { width, height } = el.getBoundingClientRect();
    if (!width || !height) return;
    renderer.setSize(width, height, false);
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
  };
  const ro = new ResizeObserver(resize);
  ro.observe(el);
  resize();

  let visible = true;
  let paused = false;
  const io = new IntersectionObserver(([entry]) => (visible = entry.isIntersecting));
  io.observe(el);

  renderer.setAnimationLoop(() => {
    if (!visible || paused) return;
    controls.update();
    renderer.render(scene, camera);
  });

  return {
    show,
    pause(p) {
      paused = p;
    },
    dispose() {
      renderer.setAnimationLoop(null);
      ro.disconnect();
      io.disconnect();
      controls.dispose();
      clear();
      renderer.dispose();
      renderer.domElement.remove();
    },
  };
}
