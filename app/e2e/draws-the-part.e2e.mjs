/**
 * The part is actually drawn, in the window the user has.
 *
 * Everything else in this suite asks the viewport what it *knows* — which edge
 * is under the pointer, what the camera projects to. All of that answers
 * correctly while nothing is on screen: on 2026-09-22 the desktop window
 * showed an empty scene for a part that had built, measured and reported 6 822
 * triangles, and the source-to-viewport spec beside this one passed throughout.
 * A browser could not see it either; only WebKit was affected, which is the
 * case this whole suite exists for.
 *
 * So this one asks whether pixels changed. It renders the scene twice, once
 * with the part hidden and once with it shown, and compares the two frames —
 * a differential, so it needs to know nothing about the background, the grid,
 * the shadow or the theme, and cannot be satisfied by a scene that draws
 * everything except the part.
 */

/** How much of the frame the part covers, as a fraction of the pixels read. */
async function partCoverage() {
  return browser.execute(() => {
    const viewport = window.__viewport;
    const renderer = viewport.renderer;
    const gl = renderer.getContext();
    const width = gl.drawingBufferWidth;
    const height = gl.drawingBufferHeight;
    if (!width || !height) throw new Error("the viewport has no drawing buffer");

    const frame = () => {
      renderer.render(viewport.scene, viewport.camera);
      const pixels = new Uint8Array(width * height * 4);
      gl.readPixels(0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
      return pixels;
    };

    const shown = viewport.partGroup.visible;
    viewport.partGroup.visible = false;
    const without = frame();
    viewport.partGroup.visible = true;
    const with_ = frame();
    viewport.partGroup.visible = shown;

    let differing = 0;
    for (let i = 0; i < without.length; i += 4) {
      if (
        Math.abs(without[i] - with_[i]) > 8 ||
        Math.abs(without[i + 1] - with_[i + 1]) > 8 ||
        Math.abs(without[i + 2] - with_[i + 2]) > 8
      ) {
        differing += 1;
      }
    }
    return { coverage: differing / (width * height), width, height, meshes: (() => {
      let n = 0;
      viewport.partGroup.traverse((o) => { if (o.isMesh) n += 1; });
      return n;
    })() };
  });
}

describe("the window draws the part", () => {
  it("puts the bracket on screen, not only in the report", async () => {
    await browser.$("#project").click();
    const card = await browser.$('.browser-card [aria-label$="(bracket)"]');
    await card.waitForDisplayed({ timeout: 5_000 });
    await card.click();

    const status = await browser.$("#status");
    await browser.waitUntil(async () => (await status.getText()).includes("tris"), {
      timeout: 20_000,
      timeoutMsg: `the bracket never finished building (status: ${await status.getText()})`,
    });
    await (await browser.$("#viewport canvas")).waitForDisplayed();

    // The language service starts on idle and is most of a compiler; if it is
    // going to cost the scene its geometry, it has done so by now.
    await browser.pause(6_000);

    const { coverage, width, height, meshes } = await partCoverage();
    if (meshes < 1) throw new Error("the part group holds no mesh at all");
    if (coverage < 0.02) {
      throw new Error(
        `the part is not on screen: hiding it changed ${(coverage * 100).toFixed(3)}% ` +
          `of a ${width}x${height} frame, across ${meshes} mesh(es). ` +
          `A bracket filling a third of the viewport should change tens of percent.`,
      );
    }
  });
});
