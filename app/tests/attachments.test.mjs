// Taking attachments out of an email to free space on the server.
import { test } from "node:test";
import assert from "node:assert/strict";
import { openApp, card } from "./harness.mjs";

const tick = () => new Promise((r) => setTimeout(r, 120));
const MB = 1024 * 1024;

async function reading({ attachments, removed = [], remove_attachments = () => ({ saved: [], freed: 0 }) }) {
  const c = card(7, { subject: "Slides", has_attachments: true });
  const read_message = () => ({
    from: "Anna", reply_address: "anna@example.com", to: "me@example.com", all_recipients: [], cc: "", bcc: "",
    message_id: "<m7@example.com>", in_reply_to: null, references: [], subject: "Slides", date: c.date,
    text: "Here are the slides.", html: null, has_remote_content: false, invite: null, attachments, removed,
  });
  const app = await openApp({ data: { INBOX: [c, card(8)] }, handlers: { read_message, remove_attachments, load_thread: () => [] } });
  await app.run(`openReader("INBOX", state.cards.INBOX[0])`);
  await tick();
  return app;
}

const FILES = [
  { index: 0, name: "slides.pdf", mime: "application/pdf", size: 12 * MB },
  { index: 1, name: "photo.jpg", mime: "image/jpeg", size: 3 * MB },
];

test("Remove asks which attachments, saves them first, and the email leaves the list until reloaded", async () => {
  const asked = [];
  const app = await reading({
    attachments: FILES,
    remove_attachments: (args) => (asked.push(args), { saved: ["/Users/x/Downloads/slides.pdf"], freed: 12 * MB }),
  });
  const button = app.document.querySelector("#reader-attachments .att-remove");
  assert.ok(button, "a Remove button beside the attachments");
  button.click();
  await tick();
  assert.match(app.document.getElementById("dialog-message").textContent, /goes to the Trash/);
  const boxes = [...app.document.querySelectorAll("#dialog-extra input[type=checkbox]")];
  assert.equal(boxes.length, 3, "two attachments and Save first");
  assert.ok(boxes.every((b) => b.checked), "all ticked by default, saving included");
  boxes[1].checked = false; // keep the photo
  const loads = app.called("load_column").length;
  [...app.document.querySelectorAll("#dialog-buttons button")].find((b) => b.textContent === "Remove").click();
  await tick();
  assert.equal(asked.length, 1);
  assert.deepEqual(asked[0].indexes, [0]);
  assert.equal(asked[0].save, true);
  assert.equal(asked[0].folder, "INBOX");
  assert.equal(asked[0].uid, 7);
  assert.match(asked[0].day, /^\d{1,2} \w{3,4} \d{4}$/);
  assert.match(app.document.getElementById("toast").textContent, /Attachment removed · 12\.0 MB freed · saved in Downloads/);
  // The email is another one on the server now: the reader closes and the list is read again.
  assert.equal(app.run(`readerCard`), null);
  assert.ok(app.called("load_column").length > loads);
});

test("cancelling, or unticking everything, changes nothing", async () => {
  let calls = 0;
  const app = await reading({ attachments: FILES, remove_attachments: () => (calls++, null) });
  app.document.querySelector(".att-remove").click();
  await tick();
  [...app.document.querySelectorAll("#dialog-buttons button")].find((b) => b.textContent === "Cancel").click();
  await tick();
  app.document.querySelector(".att-remove").click();
  await tick();
  for (const b of app.document.querySelectorAll("#dialog-extra input")) b.checked = false;
  [...app.document.querySelectorAll("#dialog-buttons button")].find((b) => b.textContent === "Remove").click();
  await tick();
  assert.equal(calls, 0);
  // The save question cancelled in the backend: nothing removed, said so.
  for (const b of app.document.querySelectorAll(".att-remove")) b.click();
  await tick();
  [...app.document.querySelectorAll("#dialog-buttons button")].find((b) => b.textContent === "Remove").click();
  await tick();
  assert.equal(calls, 1);
  assert.match(app.document.getElementById("toast").textContent, /Nothing was removed/);
  assert.equal(app.get(`state.cards.INBOX.length`), 2);
});

test("attachments removed earlier show as names, with nothing to save", async () => {
  const app = await reading({ attachments: [], removed: [{ what: "slides.pdf, 12.0 MB", day: "27 Nov 2026" }] });
  const box = app.document.getElementById("reader-attachments");
  assert.equal(box.hidden, false);
  const chip = box.querySelector(".attachment.removed");
  assert.match(chip.textContent, /slides\.pdf, 12\.0 MB/);
  assert.match(chip.title, /Removed on 27 Nov 2026/);
  assert.equal(box.querySelector(".att-remove"), null, "nothing left to remove");
});
