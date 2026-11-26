// The open email: what its buttons say, and where it is found again.
import { test } from "node:test";
import assert from "node:assert/strict";
import { openApp, card } from "./harness.mjs";

const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const GONE = "the email no longer exists (moved by another program?)";

test("the conversation button follows the new email being read by itself", async () => {
  const old = card(1, { seen: true });
  const fresh = card(2, { thread_refs: ["<m1@example.com>"] });
  const entries = () => [{ folder: "INBOX", card: { ...old } }, { folder: "INBOX", card: { ...fresh } }];
  const app = await openApp({
    data: { INBOX: [fresh, old] },
    settings: { group_threads: true, mark_read_after_seconds: 1 },
    handlers: { load_thread: entries, local_thread: entries },
  });
  await app.run(`openReader("INBOX", state.cards.INBOX[0])`);
  await wait(200);
  const icon = () => app.$("conv-seen").querySelector("use").getAttribute("href");
  assert.equal(app.$("conv-bar").hidden, false);
  assert.equal(icon(), "#i-envelope-open", "unread: the button reads it");
  await wait(1300);
  assert.equal(app.get(`readerCard.card.seen`), true);
  assert.equal(icon(), "#i-envelope", "read: the button makes it unread again");
});

test("a minimized email whose conversation was moved opens where it is now", async () => {
  const data = { INBOX: [card(1, { subject: "Offer" })], "INBOX.Archive": [] };
  const read_message = ({ folder, uid }) => {
    const c = (data[folder] || []).find((x) => x.uid === uid);
    if (!c) throw GONE;
    return { from: c.from, reply_address: c.from_address, to: c.to, all_recipients: [c.to], cc: "", bcc: "", message_id: c.message_id, in_reply_to: null, references: [], subject: c.subject, date: c.date, text: "Body of the offer", html: null, has_remote_content: false, invite: null, attachments: [] };
  };
  const where = () => Object.entries(data).flatMap(([folder, list]) => list.map((c) => ({ folder, card: { ...c } })));
  const app = await openApp({ data, handlers: { read_message, load_thread: where } });
  await app.run(`openReader("INBOX", state.cards.INBOX[0])`);
  await wait(100);
  await app.run(`minimizeReader()`);
  // Archived meanwhile, by another program: a new UID in another folder.
  data["INBOX.Archive"] = [card(7, { subject: "Offer", message_id: "<m1@example.com>" })];
  data.INBOX = [];
  app.document.querySelector("#reader-dock .reader-bar-open").click();
  await wait(200);
  assert.deepEqual(app.get(`[readerCard.folder, readerCard.card.uid]`), ["INBOX.Archive", 7]);
  assert.match(app.$("reader-body").textContent, /Body of the offer/);
});

test("an email gone from everywhere says so plainly", async () => {
  const read_message = () => { throw GONE; };
  const app = await openApp({ data: { INBOX: [card(1)] }, handlers: { read_message } });
  await app.run(`openReader("INBOX", state.cards.INBOX[0])`);
  await wait(200);
  assert.match(app.$("reader-body").textContent, /moved or deleted/);
});
