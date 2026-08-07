/**
 * The viewport, and the four panels that float over it.
 *
 * three.js is the second island: it owns a canvas, a render loop and a
 * raycaster, so this component gives it one empty `<div>` and then leaves it
 * alone. Everything else here — the view tools, the inspector, the source
 * target, the report — is ordinary markup reading the signals the viewport's
 * callbacks write.
 *
 * The layout claim worth stating, because it is the one that changed: the right
 * edge is a *column*. The controls that decide what you are looking at sit at
 * the top of it and the inspector for whatever you are pointing at stacks under
 * them, so the two never fight for the same corner and the inspector's arrival
 * pushes nothing sideways.
 */

import { useEffect, useRef } from "preact/hooks";

import * as engine from "../engine";
import * as S from "../state";
import { Viewport } from "../viewport";
import { EntityInspector } from "./entity-inspector";
import { Report } from "./report";
import { TargetPreview } from "./target-preview";
import { ViewTools } from "./view-tools";

export function ViewportPane() {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!host.current) return;

    const viewport = new Viewport(host.current, {
      onFaceHover: (face) => {
        S.hoveredFace.value = face && { face: face.face, triangles: face.triangles };
      },
      onHover: (edge) => {
        S.hoveredEdge.value = edge;
        engine.highlightTreatmentForEdge(edge);
      },
      onSelect: (edge) => {
        S.selectedEdge.value = edge;
        engine.focusTreatmentForEdge(edge);
      },
      onVertexHover: (vertex) => {
        S.hoveredVertex.value = vertex;
        if (vertex) engine.highlightTreatmentForEdge(undefined);
      },
      onVertexSelect: (vertex) => {
        S.selectedVertex.value = vertex;
        if (vertex) engine.focusTreatmentForEdge(undefined);
      },
    });

    S.viewportRef.current = viewport;
    // Handle for poking at the scene from the console during development.
    (window as unknown as Record<string, unknown>).__viewport = viewport;

    return () => {
      viewport.dispose();
      S.viewportRef.current = undefined;
    };
  }, []);

  return (
    <section class="relative flex-1 min-w-0">
      <div id="viewport" ref={host} class="absolute inset-0" />

      {/* Everything that changes what you are looking at, and then everything
          that describes what you are pointing at, down one edge. */}
      <div class="absolute top-3.5 right-3.5 flex flex-col items-end gap-2.5">
        <ViewTools />
        <EntityInspector />
      </div>

      <TargetPreview />
      <Report />
    </section>
  );
}
