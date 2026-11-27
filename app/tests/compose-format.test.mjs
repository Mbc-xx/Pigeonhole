// Changing the format of the email being written, and the eraser.
import { test } from "node:test";
import assert from "node:assert/strict";
import { openApp } from "./harness.mjs";

const tick = () => new Promise((r) => setTimeout(r, 60));

async function composing() {
  const app = await openApp();
  app.document.querySelector('[title="Write a new email"]').click();
  await tick();
  await app.run(`setFormat("rich")`);
  return app;
}

const HTML =
  '<div>Hello <b>Anna</b>, see <a href="https://example.com/doc">the doc</a>.</div>' +
  "<ul><li>first <i>point</i></li><li>second</li></ul><h2>Agenda</h2>" +
  "<blockquote><div>quoted line</div></blockquote><br><div data-signature>-- <br>Jane Doe</div>";

test("going back to rich with the text unchanged gives the formatting back", async () => {
  const app = await composing();
  app.run(`$("compose-rich").innerHTML = ${JSON.stringify(HTML)}`);
  const before = app.run(`$("compose-rich").innerHTML`);
  for (const via of ["plain", "markdown"]) {
    await app.run(`setFormat(${JSON.stringify(via)})`);
    await app.run(`setFormat("rich")`);
    assert.equal(app.run(`$("compose-rich").innerHTML`), before, `through ${via}`);
  }
  // Plain, then Markdown, then back, all without touching the text.
  await app.run(`setFormat("plain")`);
  await app.run(`setFormat("markdown")`);
  assert.match(app.run(`$("compose-body").value`), /\*\*Anna\*\*/);
  await app.run(`setFormat("rich")`);
  assert.equal(app.run(`$("compose-rich").innerHTML`), before);
});

test("to Markdown the formatting is written as Markdown", async () => {
  const app = await composing();
  const md = app.run(`mdFromHtml(${JSON.stringify(HTML)})`);
  assert.equal(
    md,
    "Hello **Anna**, see [the doc](https://example.com/doc).\n- first *point*\n- second\n## Agenda\n> quoted line\n\n-- \nJane Doe\n",
  );
  // Markers stay outside spaces, and a bare link stays a bare link.
  assert.equal(app.run(`mdFromHtml('<div><b>bold </b>then <a href="https://x.example">https://x.example</a></div>')`), "**bold** then https://x.example\n");
});

test("the eraser clears only what is wholly selected and keeps lists and quotes", async () => {
  const app = await composing();
  app.run(`$("compose-rich").innerHTML = '<div id="a">Keep <b>bold</b> here</div><div id="b"><span style="color:red">red</span> <i>it</i></div><h2 id="c">Title</h2><ul><li id="d"><u>under</u></li></ul>'`);
  // Select from the red text to the end of the list item; "Keep bold here" stays out.
  const html = app.run(`(() => {
    const ed = $("compose-rich");
    const r = document.createRange();
    r.setStartBefore(ed.querySelector("#b span"));
    r.setEndAfter(ed.querySelector("#d u"));
    const sel = getSelection();
    sel.removeAllRanges();
    sel.addRange(r);
    clearFormatting(ed);
    return ed.innerHTML;
  })()`);
  assert.equal(html, '<div id="a">Keep <b>bold</b> here</div><div id="b">red it</div><div>Title</div><ul><li id="d">under</li></ul>');
  // Nothing selected: nothing to do.
  assert.equal(app.run(`getSelection().collapseToStart(), clearFormatting($("compose-rich"))`), false);
});

test("remote pictures of a quoted email wait while writing and go out unchanged", async () => {
  const app = await composing();
  const quoted = '<p>Hi <img src="https://track.example/p.gif" width="1"></p><div style="background:url(https://img.example/bg.png) no-repeat">x</div><img src="data:image/png;base64,AAAA"><div data-signature><img src="https://logo.example/me.png"></div>';
  app.run(`readerRemoteAllowed = false`);
  const held = app.run(`emailBodyHtml({ html: ${JSON.stringify(quoted)} })`);
  assert.doesNotMatch(held, /(?<!-)src="https:\/\/track/, "the tracker does not load");
  assert.match(held, /style="background:none no-repeat"/, "nor the background");
  assert.match(held, /data:image\/png/, "pictures inside the email stay");
  assert.match(held, /(?<!-)src="https:\/\/logo\.example/, "the signature's own picture stays");
  // What is sent is the email as it was.
  app.run(`$("compose-rich").innerHTML = ${JSON.stringify(held)}`);
  const sent = app.run(`composeDraft().html`);
  assert.match(sent, /<img[^>]*(?<!-)src="https:\/\/track\.example\/p\.gif"/);
  assert.doesNotMatch(sent, /Remote picture/, "the placeholder title goes away");
  assert.match(sent, /(?<!-)style="background:url\(https:\/\/img\.example\/bg\.png\) no-repeat"/);
  assert.doesNotMatch(sent, /data-held/);
  // Pictures allowed in the reader load in the reply too.
  app.run(`readerRemoteAllowed = true`);
  assert.match(app.run(`emailBodyHtml({ html: ${JSON.stringify(quoted)} })`), /(?<!-)src="https:\/\/track/);
});
