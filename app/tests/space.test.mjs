// The space emails take on the server: how full the mailbox is, and the largest emails.
import { test } from "node:test";
import assert from "node:assert/strict";
import { openApp, card } from "./harness.mjs";

const tick = () => new Promise((r) => setTimeout(r, 120));
const MB = 1024 * 1024;

test("larger: in the search box is a size in bytes", async () => {
  const app = await openApp();
  const larger = (text) => app.run(`parseQuery(${JSON.stringify(text)}).larger`);
  assert.equal(larger("larger:5mb"), 5 * MB);
  assert.equal(larger("larger:500kb"), 500 * 1024);
  assert.equal(larger("larger:10"), 10 * MB, "a bare number is megabytes");
  assert.equal(larger("larger:1,5gb"), 1.5 * 1024 * MB);
  assert.equal(larger("report"), null);
  // Not a size: it stays a word to search for.
  assert.deepEqual(app.get(`parseQuery("larger:huge").words`), ["larger:huge"]);
  assert.equal(app.run(`formatSize(${4.2 * 1024 * MB})`), "4.2 GB");
});

test("Largest searches every folder and lists each email biggest first, with its size", async () => {
  // A conversation of two big emails stays two rows, each with its own size.
  const sized = {
    INBOX: [card(1, { size: 6 * MB, subject: "Photos" }), card(2, { size: 30 * MB, subject: "Video", thread_refs: ["<m1@example.com>"] })],
    "INBOX.Archive": [card(3, { size: 12 * MB, subject: "Slides" })],
  };
  const handlers = {
    search_emails: ({ query, folders }) =>
      folders.flatMap((f) => (sized[f] || []).filter((c) => c.size > query.larger).map((c) => ({ folder: f, card: c }))),
  };
  const app = await openApp({ settings: { group_threads: true }, handlers });
  app.document.getElementById("nav-all").click();
  await tick();
  const button = [...app.document.querySelectorAll(".column-head .head-btn")].find((b) => b.textContent === "Largest");
  button.click();
  await tick();
  const rows = app.get(`[...document.querySelectorAll(".card")].map((c) => [c.querySelector(".subject, .row-subject")?.textContent || c.textContent.match(/Photos|Video|Slides/)[0], c.querySelector(".wait-tag.size")?.textContent])`);
  assert.deepEqual(rows.map((r) => r[1]), ["30.0 MB", "12.0 MB", "6.0 MB"]);
  assert.ok(rows[0][0].includes("Video") && rows[2][0].includes("Photos"));
  // Every folder, asked for emails above 5 MB, without searching the text.
  const asked = app.called("search_emails");
  assert.ok(asked.length >= 2 && asked.every((a) => a.query.larger === 5 * MB && a.inBody === false && a.scope === "full"));
  assert.equal(app.called("search_local").length, 0, "this Mac does not know sizes");
  // A smaller threshold asks again and finds more.
  const pick = app.document.querySelector(".search-opts select");
  pick.value = "10";
  pick.dispatchEvent(new app.window.Event("change"));
  await tick();
  assert.equal(app.run(`document.querySelectorAll(".wait-tag.size").length`), 2);
  // Escape in the search box goes back to All Mail.
  app.document.getElementById("search").dispatchEvent(new app.window.KeyboardEvent("keydown", { key: "Escape" }));
  await tick();
  assert.equal(app.run(`state.view.folder`), "__all__");
});

test("the settings show how full the mailbox is, or say the server does not tell", async () => {
  let space = { used: 4.2 * 1024 * MB, limit: 10 * 1024 * MB };
  const app = await openApp({ handlers: { server_space: () => space, search_emails: () => [] } });
  app.document.getElementById("open-settings").click();
  await tick();
  assert.equal(app.document.getElementById("server-space").textContent, "4.2 GB of 10.0 GB used (42%)");
  assert.equal(app.document.getElementById("server-space-bar").hidden, false);
  assert.equal(app.run(`$("server-space-bar").firstElementChild.style.width`), "42%");
  assert.equal(app.run(`$("server-space-bar").classList.contains("full")`), false);
  space = { used: 9.5 * 1024 * MB, limit: 10 * 1024 * MB };
  await app.run(`showServerSpace()`);
  assert.equal(app.run(`$("server-space-bar").classList.contains("full")`), true);
  space = null;
  await app.run(`showServerSpace()`);
  assert.match(app.document.getElementById("server-space").textContent, /does not say/);
  assert.equal(app.document.getElementById("server-space-bar").hidden, true);
  // From there, straight to the largest emails.
  app.document.getElementById("show-largest").click();
  await tick();
  assert.equal(app.document.getElementById("settings").hidden, true);
  assert.equal(app.run(`state.view.largest`), true);
});

test("a mailto link fills in its addresses, copies, subject and text", async () => {
  const app = await openApp();
  const f = (href) => app.get(`mailtoFields(${JSON.stringify(href)})`);
  assert.deepEqual(f("mailto:anna@example.com"), { to: "anna@example.com", cc: "", bcc: "", subject: "", body: "" });
  assert.equal(f("mailto:?to=anna@example.com").to, "anna@example.com");
  assert.equal(f("mailto:anna@example.com?To=bob@example.com").to, "anna@example.com, bob@example.com");
  assert.deepEqual(f("mailto:anna@example.com?cc=bob@example.com&BCC=carl@example.com&Subject=Q3%20plan%20%26%20budget&body=Hi%0D%0Athere"), {
    to: "anna@example.com", cc: "bob@example.com", bcc: "carl@example.com", subject: "Q3 plan & budget", body: "Hi\r\nthere",
  });
  assert.equal(f("mailto:anna+work@example.com").to, "anna+work@example.com", "a plus stays a plus");
  assert.equal(f("mailto:Anna%20Bianchi%20%3Canna%40example.com%3E").to, "Anna Bianchi <anna@example.com>");
  assert.equal(f("mailto:bad%E0%A4%A@example.com").to, "bad%E0%A4%A@example.com", "broken escapes do not stop it");
  assert.equal(f("mailto:anna@example.com?constructor=x&__proto__=y").to, "anna@example.com");
  // Clicked, the new email has them in its fields.
  app.run(`openLink("mailto:?to=anna@example.com&cc=bob@example.com&subject=Hello")`);
  await tick();
  assert.deepEqual(app.get(`compose.rcpt["compose-to"].map((r) => r.email)`), ["anna@example.com"]);
  assert.deepEqual(app.get(`compose.rcpt["compose-cc"].map((r) => r.email)`), ["bob@example.com"]);
  assert.equal(app.document.getElementById("compose-subject").value, "Hello");
});
