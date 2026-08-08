import assert from "node:assert/strict";

/**
 * The parts picker's `···` menu and the two dialogs it opens.
 *
 * Both dialogs moved out of project-browser.tsx into components/Dialog.tsx, and
 * the confirm half had no coverage at all — its only trigger is destructive, so
 * it is the one path nobody exercises by hand twice. This drives it and cancels.
 *
 * The panels are told apart by the scrim rather than by button text: the part
 * menu lives inside the same `<dialog>` and has a "Move to trash" button of its
 * own, so text alone matches two different things at two different moments.
 */
const onScreen = () =>
  browser.execute(() => ({
    scrims: document.querySelectorAll("dialog .absolute.inset-0").length,
    panelButtons: [...document.querySelectorAll("dialog .absolute.inset-0 button")].map((b) =>
      b.textContent.trim(),
    ),
  }));

async function openMenu() {
  await browser.$("#project").click();
  const card = await browser.$('.browser-card [aria-label$="(bracket)"]');
  await card.waitForDisplayed({ timeout: 5_000 });
  await (await browser.$('[aria-label="More actions for bracket"]')).click();
  await (await browser.$("//button[text()='Move to trash']")).waitForDisplayed({ timeout: 5_000 });
}

describe("the parts picker's per-part menu", () => {
  it("opens the rename dialog, and Escape leaves the part alone", async () => {
    await openMenu();
    await (await browser.$("//button[starts-with(text(), 'Rename')]")).click();

    const field = await browser.$("dialog input[type='text']");
    await field.waitForDisplayed({ timeout: 5_000 });
    assert.equal(await field.getValue(), "bracket");

    await browser.keys(["Escape"]);
    await field.waitForDisplayed({ timeout: 5_000, reverse: true });
  });

  it("opens the confirm dialog from Move to trash, and Cancel deletes nothing", async () => {
    await openMenu();
    assert.equal((await onScreen()).scrims, 0, "a panel was open before the menu item was pressed");

    await (await browser.$("//button[text()='Move to trash']")).click();
    await browser.waitUntil(async () => (await onScreen()).scrims === 1, {
      timeout: 5_000,
      timeoutMsg: "Move to trash opened no confirm dialog",
    });
    // Cancel first, then the destructive one: a confirm that led with the red
    // button would be a different bug worth failing on.
    assert.deepEqual((await onScreen()).panelButtons, ["Cancel", "Move to trash"]);

    await (await browser.$("//dialog//div[contains(@class,'inset-0')]//button[text()='Cancel']")).click();
    await browser.waitUntil(async () => (await onScreen()).scrims === 0, {
      timeout: 5_000,
      timeoutMsg: "Cancel did not dismiss the confirm dialog",
    });

    // Nothing may be deleted by this test, ever.
    await (await browser.$('.browser-card [aria-label$="(bracket)"]')).waitForDisplayed({
      timeout: 5_000,
    });
  });
});
