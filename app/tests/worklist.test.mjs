// The to-do column in the order chosen by hand, and the next step on emails.
import { test } from "node:test";
import assert from "node:assert/strict";
import { openApp, card } from "./harness.mjs";

const tick = () => new Promise((r) => setTimeout(r, 120));
const TODO = "INBOX.Todo";

/// A fake notes.json: the commands change it as the backend would.
function notesBackend(start = {}) {
  const notes = { next: {}, todo_order: [], ...start };
  const bare = (id) => id.replace(/^<|>$/g, "").toLowerCase();
  return {
    notes,
    handlers: {
      list_notes: () => structuredClone(notes),
      set_todo_order: ({ ids }) => ((notes.todo_order = [...new Set(ids.map(bare))]), structuredClone(notes)),
      set_next_step: ({ id, text, clear }) => {
        for (const x of clear || []) delete notes.next[bare(x)];
        if (text.trim()) notes.next[bare(id)] = text.trim();
        else delete notes.next[bare(id)];
        return structuredClone(notes);
      },
    },
  };
}

const order = (app) => app.get(`[...columnEl("${TODO}").querySelectorAll(".card")].map((c) => +c.dataset.uid)`);

test("with no order chosen the to-do column stays newest first", async () => {
  const { handlers } = notesBackend();
  const app = await openApp({ data: { [TODO]: [card(1), card(2), card(3)] }, handlers });
  assert.deepEqual(order(app), [1, 2, 3]);
  assert.equal(app.run(`columnEl("${TODO}").querySelectorAll(".unplaced").length`), 0);
});

test("the chosen order wins over the date; new arrivals wait on top, pinned stay first", async () => {
  const { handlers } = notesBackend({ todo_order: ["m3@example.com", "m1@example.com", "m2@example.com"] });
  const app = await openApp({
    data: { [TODO]: [card(1), card(2), card(3), card(4), card(5, { flagged: true })] },
    handlers,
  });
  await tick();
  assert.deepEqual(order(app), [5, 4, 3, 1, 2]);
  const tagged = app.get(`[...columnEl("${TODO}").querySelectorAll(".card")].filter((c) => c.querySelector(".unplaced")).map((c) => +c.dataset.uid)`);
  assert.deepEqual(tagged, [5, 4], "only the emails not placed yet say New here");
  // Other columns keep the date.
  assert.equal(app.get(`byHand("INBOX", [1, 2], () => [], (x) => x)`).join(), "1,2");
});

test("dropping a card inside the column moves it there and saves the order", async () => {
  const { handlers, notes } = notesBackend();
  const app = await openApp({ data: { [TODO]: [card(1), card(2), card(3)] }, handlers });
  const drop = (type, uid, y) => {
    const ev = new app.window.Event(type, { bubbles: true, cancelable: true });
    Object.defineProperty(ev, "clientY", { value: y });
    Object.defineProperty(ev, "dataTransfer", { value: { dropEffect: "" } });
    app.run(`state.dragging = { folder: "${TODO}", uid: ${uid}, members: [{ folder: "${TODO}", card: state.cards["${TODO}"].find((c) => c.uid === ${uid}) }] }`);
    app.run(`columnEl("${TODO}")`).dispatchEvent(ev);
  };
  // jsdom has no layout: every card is at 0, so above 0 is the top.
  drop("dragover", 3, -1);
  assert.equal(app.run(`columnEl("${TODO}").querySelectorAll(".drop-before").length`), 1, "a line shows where it lands");
  drop("drop", 3, -1);
  await tick();
  assert.deepEqual(order(app), [3, 1, 2]);
  assert.deepEqual(notes.todo_order, ["m3@example.com", "m1@example.com", "m2@example.com"]);
  assert.equal(app.run(`columnEl("${TODO}").querySelectorAll(".drop-before, .drop-after").length`), 0);
  // With a filter on, part of the column is hidden: no reordering then.
  app.run(`state.onlyUnread = true`);
  assert.equal(app.run(`filtersOn()`), true);
});

test("a dragged conversation moves as one block", async () => {
  const app = await openApp();
  const ids = app.get(`(() => {
    const cards = ["a", "b", "c"].map((x) => { const c = document.createElement("div"); c.menuInfo = { members: [{ card: { message_id: "<" + x + "@x>" } }] }; return c; });
    cards[2].menuInfo.members.push({ card: { message_id: "<c2@x>" } });
    return orderAfterDrop(cards, 0, cards[2].menuInfo.members);
  })()`);
  assert.deepEqual(ids, ["c@x", "c2@x", "a@x", "b@x"]);
});

test("emails that left the column are forgotten once it is read from the server", async () => {
  const { handlers, notes } = notesBackend({ todo_order: ["m2@example.com", "gone@example.com", "m1@example.com"] });
  const app = await openApp({ data: { [TODO]: [card(1), card(2)] }, handlers });
  await app.until(() => app.called("set_todo_order").length > 0, "the order to be cleaned");
  assert.deepEqual(notes.todo_order, ["m2@example.com", "m1@example.com"]);
  assert.deepEqual(order(app), [2, 1]);
});

test("a next step shows on the card, follows the replies and is one per conversation", async () => {
  const { handlers, notes } = notesBackend({ next: { "m1@example.com": "send the quote" } });
  const app = await openApp({
    data: { [TODO]: [card(1), card(2, { thread_refs: ["<m1@example.com>"] }), card(3)] },
    handlers,
  });
  await tick();
  const steps = () => app.get(`[...columnEl("${TODO}").querySelectorAll(".card")].map((c) => [+c.dataset.uid, c.querySelector(".next-step")?.textContent || ""])`);
  // The reply to email 1 shows its step too.
  assert.deepEqual(steps(), [[1, "send the quote"], [2, "send the quote"], [3, ""]]);
  // Written on the conversation, it replaces the old one.
  await app.run(`saveStep([{ folder: "${TODO}", card: state.cards["${TODO}"][1] }, { folder: "${TODO}", card: state.cards["${TODO}"][0] }], "call Anna first")`);
  await tick();
  assert.deepEqual(notes.next, { "m1@example.com": "call Anna first" });
  // Removing it leaves nothing.
  await app.run(`saveStep([{ folder: "${TODO}", card: state.cards["${TODO}"][0] }], "")`);
  await tick();
  assert.deepEqual(notes.next, {});
  assert.deepEqual(steps(), [[1, ""], [2, ""], [3, ""]]);
});

test("the reader shows the next step, or a button to add one", async () => {
  const { handlers } = notesBackend({ next: { "m3@example.com": "wait for the invoice" } });
  const app = await openApp({ data: { [TODO]: [card(1), card(3)] }, handlers });
  await tick();
  await app.run(`openReader("${TODO}", state.cards["${TODO}"][0])`);
  await tick();
  assert.match(app.$("reader-step").textContent, /^Next step$/);
  await app.run(`openReader("${TODO}", state.cards["${TODO}"][1])`);
  await tick();
  assert.equal(app.$("reader-step").textContent, "wait for the invoice");
});
