// The unread numbers of the sidebar.
import { test } from "node:test";
import assert from "node:assert/strict";
import { openApp, card } from "./harness.mjs";

const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const sevenUnread = () => Array.from({ length: 10 }, (_, i) => card(i + 1, { seen: i < 3 }));
// The number the sidebar shows next to a folder ("" = none).
const badge = (app, label) =>
  app.get(`(() => {
    const items = [...document.querySelectorAll("#folder-list .label")];
    const item = items.find((l) => l.textContent === ${JSON.stringify(label)});
    return item ? item.parentElement.querySelector(".badge")?.textContent || "" : "no folder";
  })()`);

test("Sent shows no unread number and its count is not asked", async () => {
  const app = await openApp({ data: { INBOX: [card(1)], "INBOX.Sent": sevenUnread() } });
  await app.run(`loadFolders()`);
  await wait(50);
  assert.equal(badge(app, "Sent"), "");
  assert.equal(badge(app, "Inbox"), "1", "the other folders keep theirs");
  for (const { folders } of app.called("folder_unseen")) assert.ok(!folders.includes("INBOX.Sent"));
});

test("opening a folder brings its number up to date", async () => {
  const app = await openApp({ data: { INBOX: [], "INBOX.Archive": sevenUnread() } });
  // Past the refreshes of the start, which would hide a missing one.
  await wait(5000);
  assert.equal(badge(app, "Archive"), "7");
  for (const c of app.data["INBOX.Archive"]) c.seen = true; // read on the phone
  await app.run(`openFolder(state.folders.find((f) => f.kind === "archive"))`);
  await app.run(`showBoard()`); // also when it is left at once
  await wait(3500);
  assert.equal(badge(app, "Archive"), "");
});
