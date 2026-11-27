// Working through emails without a project tool: a one-line next step on an
// email, and the to-do column in the order chosen by hand. Both are kept by
// the backend in a file of the account (notes.json), not on the mail server.

state.notes = { next: {}, todo_order: [] };

/// The notes as the backend now has them: everything showing them follows.
function setNotes(notes) {
  state.notes = { next: notes?.next || {}, todo_order: notes?.todo_order || [] };
  renderSoon();
  renderReaderStep();
}

// ---------- next step ----------

/// The next step of an email or conversation: { id, text } or null. An email
/// of the conversation counts first, then the emails it answers, so a step
/// written on an earlier email still shows when a reply arrives.
function stepOf(members) {
  const next = state.notes.next;
  const own = members.map((m) => mailId(m.card.message_id)).filter(Boolean);
  const refs = members.flatMap((m) => cardIds(m.card));
  for (const id of [...own, ...refs]) if (next[id]) return { id, text: next[id] };
  return null;
}

/// The step on a card: one line under the preview, a click changes it.
function stepLine(members) {
  const step = stepOf(members);
  if (!step) return null;
  const line = el("button", "next-step");
  line.type = "button";
  line.draggable = false;
  line.title = "Next step: click to change it";
  line.append(icon("step"), el("span", "", step.text));
  line.addEventListener("click", (ev) => {
    ev.stopPropagation();
    editStep(line, members);
  });
  return line;
}

/// A small field to write, change or remove the next step.
function editStep(anchor, members, at = null) {
  const step = stepOf(members);
  const fill = (menu) => {
    menu.classList.add("task-menu");
    const input = menuInput("Next step, e.g. call Anna first", step?.text || "", (text) => {
      closeConvMenu();
      saveStep(members, text);
    });
    menu.append(el("div", "menu-head", "Next step"), input);
    if (step) menu.append(menuButton("close", "Remove the next step", () => (closeConvMenu(), saveStep(members, ""))));
  };
  // Under the step, from its left edge: it is read where it is written.
  const r = anchor.getBoundingClientRect();
  const menu = at ? popMenuAt(at.x, at.y, fill) : popMenuAt(r.left, r.bottom + 6, fill);
  // At once, so the first letters typed right after the click are not lost.
  menu.querySelector("input")?.focus();
  return menu;
}

async function saveStep(members, text) {
  const step = stepOf(members);
  const id = step?.id || mailId(members[0]?.card.message_id);
  if (!id) return toast("This email has no Message-ID: it cannot have a next step.");
  // One step per conversation: the others found on it go away.
  const clear = [...new Set(members.flatMap((m) => cardIds(m.card)))].filter((x) => x !== id && state.notes.next[x]);
  try {
    setNotes(await invoke("set_next_step", { id, text, clear }));
  } catch (e) {
    toast("Next step not saved: " + e);
  }
}

/// The open email's step in the reader, or a quiet button to add one.
function renderReaderStep() {
  const box = $("reader-step");
  if (!box) return;
  if (!readerCard || readerCard.folder === FILE_FOLDER) return box.replaceChildren();
  const members = [readerCard];
  const line = stepLine(members);
  if (line) return box.replaceChildren(line);
  const add = el("button", "add-step");
  add.type = "button";
  add.append(icon("plus"), el("span", "", "Next step"));
  add.title = "Write what to do next with this email";
  add.addEventListener("click", (ev) => {
    ev.stopPropagation();
    editStep(add, members);
  });
  box.replaceChildren(add);
}

// ---------- the to-do column in my order ----------

/// Only the to-do column of the board keeps an order of its own.
const keepsOrder = (folder) => !state.view && !!folder && folder === todoFolder();

/// Dragging inside the column changes the order only when it shows everything.
const filtersOn = () => !!(state.onlyUnread || state.onlyImportant || state.filter || $("search").value.trim());

/// Puts units (cards or conversations) in the order chosen by hand: pinned
/// first as ever, then those not placed yet (newest first), then the placed
/// ones in their order. With no order chosen this is the usual newest first.
function byHand(folder, units, membersOf, dateOf) {
  if (!keepsOrder(folder) || !state.notes.todo_order.length) return units;
  const rank = new Map(state.notes.todo_order.map((id, i) => [id, i]));
  const info = new Map(
    units.map((u) => {
      const members = membersOf(u);
      const ranks = members.map((m) => rank.get(mailId(m.card.message_id))).filter((r) => r !== undefined);
      return [u, { pinned: members.some((m) => m.card.flagged), rank: ranks.length ? Math.min(...ranks) : null }];
    }),
  );
  return [...units].sort((a, b) => {
    const x = info.get(a), y = info.get(b);
    if (x.pinned !== y.pinned) return y.pinned - x.pinned;
    if ((x.rank === null) !== (y.rank === null)) return x.rank === null ? -1 : 1;
    if (x.rank === null) return dateOf(b) - dateOf(a);
    return x.rank - y.rank;
  });
}

/// "New here" on an email that arrived after the order was chosen: it waits
/// on top until it is dragged where it belongs.
function unplacedTag(folder, members) {
  if (!keepsOrder(folder) || !state.notes.todo_order.length) return null;
  const placed = new Set(state.notes.todo_order);
  if (members.some((m) => placed.has(mailId(m.card.message_id)))) return null;
  if (!members.some((m) => mailId(m.card.message_id))) return null;
  const tag = el("span", "wait-tag unplaced", "New here");
  tag.title = "Arrived after you put this column in order: drag it where it belongs";
  return tag;
}

/// Forgets emails that left the column, once it was read from the server.
function forgetGone(folder, all) {
  if (!keepsOrder(folder) || !state.notes.todo_order.length || !state.fresh.has(folder)) return;
  if (all.length >= (state.limits[folder] || state.defaultLimit)) return; // older emails may not be loaded
  const here = new Set(all.map((c) => mailId(c.message_id)).filter(Boolean));
  const kept = state.notes.todo_order.filter((id) => here.has(id));
  if (kept.length !== state.notes.todo_order.length) saveOrder(kept);
}

async function saveOrder(ids) {
  state.notes.todo_order = ids;
  try {
    const notes = await invoke("set_todo_order", { ids });
    state.notes.todo_order = notes.todo_order;
  } catch (e) {
    toast("Order not saved: " + e);
  }
}

/// Where a card dropped at `y` lands in the column: { cards, index }, the cards
/// shown and the place among them, with a line drawn at that place.
function dropPlace(column, y) {
  const cards = [...column.querySelectorAll(".cards > .card")];
  clearDropLine(column);
  let index = cards.length;
  for (let i = 0; i < cards.length; i++) {
    const r = cards[i].getBoundingClientRect();
    if (y < r.top + r.height / 2) {
      index = i;
      break;
    }
  }
  if (cards.length) {
    if (index < cards.length) cards[index].classList.add("drop-before");
    else cards.at(-1).classList.add("drop-after");
  }
  return { cards, index };
}

function clearDropLine(column) {
  for (const c of column.querySelectorAll(".drop-before, .drop-after")) c.classList.remove("drop-before", "drop-after");
}

/// The new order once `dragged` (emails) lands at `index` among `cards`: the
/// dragged cards keep their own order and move together.
function orderAfterDrop(cards, index, dragged) {
  const moving = new Set(dragged.map((m) => mailId(m.card.message_id)).filter(Boolean));
  const idsOf = (c) => (c.menuInfo?.members || []).map((m) => mailId(m.card.message_id)).filter(Boolean);
  const before = [], after = [];
  cards.forEach((c, i) => {
    const ids = idsOf(c).filter((id) => !moving.has(id));
    (i < index ? before : after).push(...ids);
  });
  return [...new Set([...before, ...moving, ...after])];
}

/// Moves an email or conversation to the top or the bottom of the column.
function placeAtEnd(folder, members, top) {
  const column = columnEl(folder);
  if (!column) return;
  const cards = [...column.querySelectorAll(".cards > .card")];
  saveOrder(orderAfterDrop(cards, top ? 0 : cards.length, members));
  renderCards(folder);
}
