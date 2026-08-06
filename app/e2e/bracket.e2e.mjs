import assert from "node:assert/strict";

/**
 * Return a canvas-relative point halfway along a generated mount-hole fillet.
 * The model itself chooses the curve: this keeps the physical pointer action
 * honest while avoiding hard-coded screen coordinates that fail on a resized
 * Tauri window or a different display scale.
 */
async function mountHoleFilletPoint() {
  return browser.execute(() => {
    const viewport = window.__viewport;
    const line = viewport.edgeLines.find(
      (candidate) => candidate.userData.edge?.treatment_node === 10,
    );
    if (!line) throw new Error("the bracket returned no generated mount-hole fillet edge");

    const positions = line.geometry.getAttribute("position");
    const midpoint = line.position.clone().set(
      (positions.getX(0) + positions.getX(1)) / 2,
      (positions.getY(0) + positions.getY(1)) / 2,
      (positions.getZ(0) + positions.getZ(1)) / 2,
    );
    line.localToWorld(midpoint);
    viewport.camera.updateMatrixWorld();
    midpoint.project(viewport.camera);

    const canvas = viewport.renderer.domElement;
    const bounds = canvas.getBoundingClientRect();
    const x = ((midpoint.x + 1) / 2) * bounds.width;
    const y = ((1 - midpoint.y) / 2) * bounds.height;
    if (x < 0 || x > bounds.width || y < 0 || y > bounds.height) {
      throw new Error("the generated mount-hole fillet edge is outside the viewport");
    }
    return { x, y };
  });
}

describe("bracket source-to-viewport links", () => {
  it("inspects a generated fillet and previews the selector under the caret", async () => {
    // The app opens on the bracket, but opening it here makes the fixture an
    // explicit part of the regression contract rather than an incidental
    // default. Through the picker, because that is now the only way in — and a
    // path rather than a `<select>` value, since parts live in folders.
    await browser.$("#project").click();
    const card = await browser.$('.browser-card [aria-label$="(bracket)"]');
    await card.waitForDisplayed({ timeout: 5_000 });
    await card.click();

    const status = await browser.$("#status");
    const error = await browser.$("#error");
    try {
      await browser.waitUntil(
        async () => (await status.getText()).includes("tris"),
        { timeout: 5_000 },
      );
    } catch {
      throw new Error(
        `the desktop bracket did not finish exact evaluation (status: ${await status.getText()}; error: ${await error.getText()})`,
      );
    }

    const canvas = await browser.$("#viewport canvas");
    await canvas.waitForDisplayed();
    const point = await mountHoleFilletPoint();
    const { width, height } = await canvas.getSize();
    // WebdriverIO measures an element move from its centre, unlike the canvas
    // projection above, whose origin is the top-left corner.
    await canvas.moveTo({
      xOffset: Math.round(point.x - width / 2),
      yOffset: Math.round(point.y - height / 2),
    });

    const origin = await browser.$("#edge-origin");
    try {
      await origin.waitForDisplayed({ timeout: 5_000 });
    } catch {
      const hover = await browser.execute(() => {
        const viewport = window.__viewport;
        return viewport.hoveredEdge?.userData.edge;
      });
      throw new Error(
        `hovering a generated fillet did not expose its source call (hovered: ${JSON.stringify(hover)})`,
      );
    }
    assert.match(await origin.getText(), /^from \.fillet\(…\)/);

    // The native WKWebView driver delivers mouse events but not CodeMirror's
    // Cmd-F shortcut. Place the editor's regular selection at the selector;
    // its update listener still drives the production preview request.
    await browser.execute(() => {
      const editor = window.__editor;
      const selector = editor.state.doc.toString().indexOf("generatedBy");
      if (selector < 0) throw new Error("the bracket source lost its mount-hole selector");
      editor.focus();
      editor.dispatch({ selection: { anchor: selector } });
    });

    const preview = await browser.$("#target-preview");
    await preview.waitForDisplayed({
      timeoutMsg: "a selector caret did not request the exact target preview",
    });
    assert.match(await preview.getText(), /\.fillet input edges/);
    assert.match(await preview.getText(), /4 selected edges/);

    const sourceHighlight = await browser.$("#editor .cm-treatment-hover");
    await sourceHighlight.waitForDisplayed();
    assert.equal(await sourceHighlight.isDisplayed(), true);
  });
});
