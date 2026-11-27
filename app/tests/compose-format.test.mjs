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
