// Who a reply goes to.
import { test } from "node:test";
import assert from "node:assert/strict";
import { openApp, card } from "./harness.mjs";

const tick = () => new Promise((r) => setTimeout(r, 30));
const ME = "me@example.com";
const sender = { email: "help@example.com", display_name: "", smtp_server: "mail.example.com", smtp_port: 465, signature: "", signature_html: "" };

/// Opens an email from `from` to `to` and `cc`, and replies to it. `thread`:
/// the other emails of its conversation, as [from_address, hours ago].
async function replyTo({ from = ME, to = [], cc = [], all = true, folder = "INBOX.Sent", settings = {}, thread = [] }) {
  const c = card(1, { from_address: from, date: 1_000_000 });
  const entries = thread.map(([from_address, ago], i) => ({ folder: "INBOX", card: card(50 + i, { from_address, date: 1_000_000 - ago * 3600 }) }));
  const read_message = () => ({
    from: "X", reply_address: from, to: to.join(", "), all_recipients: [...to, ...cc], cc: cc.join(", "), bcc: "",
    message_id: "<m1@example.com>", in_reply_to: null, references: ["<m50@example.com>"], subject: "Offer", date: c.date,
    text: "x", html: null, has_remote_content: false, invite: null, attachments: [],
  });
  const app = await openApp({ data: { INBOX: [], [folder]: [c] }, settings: { senders: [], ...settings }, handlers: { read_message, load_thread: () => [{ folder, card: c }, ...entries] } });
  await app.run(`openReader(${JSON.stringify(folder)}, ${JSON.stringify(c)})`);
  await tick();
  app.run(`replyTo(${all})`);
  await tick();
  // The addresses only: the composer adds the names it knows.
  return app.get(`(() => {
    const f = composeDraft(), emails = (t) => parseAddresses(t).map((p) => p.email).join(", ");
    return { to: emails(f.to), cc: emails(f.cc), from: compose.from };
  })()`);
}

test("replying to all on my own email goes to the people I wrote to", async () => {
  const r = await replyTo({ to: ["client@acme.example"], cc: ["colleague@example.com", ME] });
  assert.deepEqual(r, { to: "client@acme.example", cc: "colleague@example.com", from: "" });
});

test("a plain reply to my own email leaves Cc out and keeps every To", async () => {
  const r = await replyTo({ to: ["client@acme.example", "boss@acme.example"], cc: ["colleague@example.com"], all: false });
  assert.deepEqual(r, { to: "client@acme.example, boss@acme.example", cc: "", from: "" });
});

test("my own email sent only to me goes to the last other person before it", async () => {
  const r = await replyTo({ to: [ME], thread: [["old@acme.example", 9], ["client@acme.example", 2], [ME, 1], ["later@acme.example", -3]] });
  assert.equal(r.to, "client@acme.example");
});

test("with nobody else in the conversation, a note to myself stays to me", async () => {
  assert.equal((await replyTo({ to: [ME] })).to, ME);
});

const ALIASES = { my_addresses: ["Alias@example.com", "second@example.com"] };

test("an email from one of my aliases is mine", async () => {
  const r = await replyTo({ from: "alias@example.com", to: ["client@acme.example"], settings: ALIASES });
  assert.deepEqual([r.to, r.cc], ["client@acme.example", ""]);
});

test("an alias kept in Cc stays there, on my emails and on those of others", async () => {
  const mine = await replyTo({ to: ["client@acme.example"], cc: ["colleague@example.com", "ALIAS@example.com", ME], settings: ALIASES });
  assert.deepEqual([mine.to, mine.cc], ["client@acme.example", "colleague@example.com, ALIAS@example.com"]);
  const theirs = await replyTo({ from: "client@acme.example", to: [ME], cc: ["alias@example.com"], folder: "INBOX", settings: ALIASES });
  assert.deepEqual([theirs.to, theirs.cc], ["client@acme.example", "alias@example.com"]);
});

test("an alias the email was sent to is not answered", async () => {
  const theirs = await replyTo({ from: "client@acme.example", to: ["alias@example.com", "colleague@example.com"], cc: ["second@example.com"], folder: "INBOX", settings: ALIASES });
  assert.deepEqual([theirs.to, theirs.cc], ["client@acme.example", "colleague@example.com, second@example.com"]);
  const mine = await replyTo({ to: ["client@acme.example", "alias@example.com"], settings: ALIASES });
  assert.deepEqual([mine.to, mine.cc], ["client@acme.example", ""]);
});

test("a plain reply leaves the alias in Cc out, like everyone else in Cc", async () => {
  const r = await replyTo({ to: ["client@acme.example"], cc: ["alias@example.com"], all: false, settings: ALIASES });
  assert.deepEqual([r.to, r.cc], ["client@acme.example", ""]);
});

test("an email I sent as an extra sender is answered as that sender", async () => {
  const r = await replyTo({ from: sender.email, to: ["client@acme.example"], settings: { senders: [sender] } });
  assert.deepEqual([r.to, r.from.toLowerCase()], ["client@acme.example", sender.email]);
});

test("an email received from the mailbox of an extra sender still goes back to it", async () => {
  const r = await replyTo({ from: sender.email, to: [ME], folder: "INBOX", settings: { senders: [sender] } });
  assert.equal(r.to, sender.email);
});

test("replying to someone else's email is as before", async () => {
  const r = await replyTo({ from: "client@acme.example", to: [ME, "colleague@example.com"], cc: ["boss@acme.example"], folder: "INBOX" });
  assert.deepEqual([r.to, r.cc], ["client@acme.example", "colleague@example.com, boss@acme.example"]);
});
