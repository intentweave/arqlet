/**
 * Renders the rows of `containmentRows` as a WAI-ARIA tree (ADR 0010). The tree emits intents only: selection through
 * `onSelect`, and expanding or collapsing a container through `onToggle`. Nothing here changes the scene or the engine
 * state; the host sends the intent to the engine and renders the accepted result.
 *
 * The settings of the selected container are shown directly below its row (`detail`), so the list does not have to be
 * scrolled past them. A text filter (`filter`) shows only the rows whose label matches, and the selected row always stays.
 */
import {nextRowId, sizingEntryOf} from './model.js';
import {chosenPhrase, formValueOf, originOf, sourceOf} from './settings.js';

export function renderContainmentTree(
  root,
  rows,
  {selected = null, onSelect, onToggle, onShape, disabled = false, filter = '', detail, document = root.ownerDocument} = {},
) {
  const needle = filter.trim().toLowerCase();
  const visible = needle ? rows.filter(row => row.id === selected || row.label.toLowerCase().includes(needle)) : rows;
  // The row that has keyboard focus keeps it across a redraw; otherwise the selected row (or the first) is the tab stop.
  const focusedId = root.contains(document.activeElement) ? document.activeElement.dataset?.id ?? null : null;
  const list = document.createElement('ul');
  list.setAttribute('role', 'tree');
  list.setAttribute('aria-label', 'Containment');
  const has = id => id !== null && visible.some(row => row.id === id);
  const stop = has(focusedId) ? focusedId : has(selected) ? selected : visible[0]?.id ?? null;
  let selectedItem = null;
  for (const row of visible) {
    const item = document.createElement('li');
    item.setAttribute('role', 'treeitem');
    item.setAttribute('aria-level', String(row.depth + 1));
    item.setAttribute('aria-selected', String(row.id === selected));
    if (row.container) item.setAttribute('aria-expanded', String(row.expanded));
    item.tabIndex = row.id === stop ? 0 : -1;
    item.dataset.id = row.id;
    item.style.paddingLeft = `${row.depth * 14 + 4}px`;
    if (row.container) {
      // A real control for expanding and collapsing, so a row can be opened with the pointer as well as the keyboard.
      const toggle = document.createElement('button');
      toggle.type = 'button';
      toggle.className = 'tree-toggle';
      toggle.tabIndex = -1;
      toggle.textContent = row.expanded ? '▾' : '▸';
      toggle.setAttribute('aria-label', `${row.expanded ? 'Collapse' : 'Expand'} ${row.label}`);
      toggle.onclick = event => {
        event.stopPropagation();
        onToggle?.(row.id);
      };
      item.append(toggle, ' ');
    }
    item.append(document.createTextNode(describe(row)));
    // The shape of the selected node is chosen in its row: a container takes a rectangle or a circle, a leaf also a square.
    if (row.id === selected && onShape && row.id !== '__view__') {
      const shape = document.createElement('select');
      shape.className = 'tree-shape';
      shape.disabled = disabled;
      shape.setAttribute?.('aria-label', `Shape of ${row.label}`);
      for (const [value, text] of row.container ? [['rect', 'Rectangle'], ['circle', 'Circle']] : [['rect', 'Rectangle'], ['square', 'Square'], ['circle', 'Circle']]) {
        const option = document.createElement('option');
        option.value = value;
        option.textContent = text;
        shape.append(option);
      }
      shape.value = row.shape === 'square' && row.container ? 'rect' : row.shape;
      shape.onclick = event => event.stopPropagation();
      shape.onchange = () => onShape(row.id, shape.value);
      item.append(' ', shape);
    }
    if (row.conflict) item.title = row.conflict;
    else if (row.conflicts?.length) item.title = row.conflicts.map(c => `${c.kind}: ${c.reason}`).join('\n');
    item.onclick = () => onSelect?.(row.id);
    list.append(item);
    if (row.id === selected) {
      selectedItem = item;
      // The settings of the selected container sit under its row, as a presentational part of the list.
      const panel = detail?.(row);
      if (panel) {
        const holder = document.createElement('li');
        holder.setAttribute('role', 'none');
        holder.style.paddingLeft = `${row.depth * 14 + 4}px`;
        holder.append(panel);
        list.append(holder);
      }
    }
  }
  list.onkeydown = event => {
    const item = event.target.closest?.('[role="treeitem"]');
    if (!item) return;
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onSelect?.(item.dataset.id);
      return;
    }
    // As in the WAI-ARIA tree pattern: Right opens a closed container, Left closes an open one; otherwise they move.
    const row = visible.find(r => r.id === item.dataset.id);
    if (row?.container && ((event.key === 'ArrowRight' && !row.expanded) || (event.key === 'ArrowLeft' && row.expanded))) {
      event.preventDefault();
      onToggle?.(row.id);
      return;
    }
    const next = nextRowId(visible, item.dataset.id, event.key);
    if (next === item.dataset.id) return;
    event.preventDefault();
    const target = [...list.children].find(li => li.dataset.id === next);
    if (!target) return;
    item.tabIndex = -1;
    target.tabIndex = 0;
    target.focus();
  };
  root.replaceChildren(list);
  // Keep keyboard focus on the tree when the canvas redraws it, and scroll only when the selection changes.
  if (focusedId !== null) [...list.children].find(li => li.dataset.id === stop)?.focus();
  if (root.dataset.selected !== String(selected ?? '')) {
    root.dataset.selected = String(selected ?? '');
    selectedItem?.scrollIntoView?.({block: 'nearest'});
  }
  return list;
}

/**
 * The settings panel of one container (ADR 0010, step 3): its size per axis, the size it is applied with, its layout
 * settings with their source, its conflict, and expanding or collapsing it. Nothing is applied here: `onApply` receives
 * the entry for `containerSizing` (or `null` to follow the content), `onChoice` receives a layout setting, and the host
 * sends them to the engine. The fields show the accepted state, and `disabled` holds them while an operation runs.
 */
export function settingsElementOf(document, row, {disabled = false, onApply, onChoice, errors = {}} = {}) {
  const panel = document.createElement('div');
  panel.className = 'settings';
  if (!row || (!row.container && !row.root)) {
    panel.textContent = 'Select a container or the view to set its settings.';
    return panel;
  }
  const head = document.createElement('p');
  head.textContent = row.label;
  panel.append(head);

  // Size: a change is sent at once, as one `containerSizing` entry for the container. A value that is not a positive
  // number is shown and not sent; "reset" returns one axis to content.
  const size = group(document, 'Size');
  const message = document.createElement('p');
  message.setAttribute('role', 'status');
  const fields = {};
  const sendSize = () => {
    try {
      const entry = sizingEntryOf({
        width: {mode: fields.width.mode.value, value: fields.width.number.value},
        height: {mode: fields.height.mode.value, value: fields.height.number.value},
      });
      message.textContent = '';
      onApply?.(entry);
    } catch (error) {
      message.textContent = error.message;
    }
  };
  // The view (root) has no size policy; only containers have one (ADR 0010).
  for (const axis of row.root ? [] : ['width', 'height']) {
    const state = row.size[axis];
    const mode = document.createElement('select');
    mode.disabled = disabled;
    for (const [value, text] of [['auto', 'content'], ['min', 'at least'], ['fixed', 'fixed']]) {
      const option = document.createElement('option');
      option.value = value;
      option.textContent = text;
      mode.append(option);
    }
    mode.value = state.state;
    mode.onchange = () => {
      // Switching to content needs no value; switching to a size keeps the value that is there.
      if (mode.value === 'auto') number.value = '';
      sendSize();
    };
    const number = document.createElement('input');
    number.type = 'number';
    number.min = '1';
    number.step = 'any';
    number.disabled = disabled;
    number.value = state.value ?? '';
    number.onchange = sendSize;
    fields[axis] = {mode, number};
    const reset = button(document, 'Reset', disabled || state.state === 'auto', () => onApply?.(entryWithout(row, axis)));
    reset.setAttribute('aria-label', `Reset ${axis} to content`);
    const label = document.createElement('label');
    label.textContent = `${axis} `;
    label.append(mode, ' ', number);
    const source = document.createElement('small');
    source.textContent = ` ${state.state === 'auto' ? 'default' : 'set here'} `;
    size.append(label, source, reset);
  }
  size.append(message);
  if (!row.root) panel.append(size);

  // Layout: direction, strategy and routing; for the view these are the whole view's values.
  if (row.settings) panel.append(layoutFieldsElementOf(document, row.settings, {disabled, onChoice, errors}));

  // Conflict: named where it is.
  const conflict = document.createElement('p');
  conflict.textContent = row.conflict ? `Conflict: ${row.conflict}` : 'No conflict.';
  panel.append(conflict);
  // Every conflict that names this node, whatever its kind, with the other parties.
  for (const entry of row.conflicts ?? []) {
    if (entry.reason === row.conflict) continue;
    const line = document.createElement('p');
    line.className = 'conflict-entry';
    const parties = [...entry.with, ...entry.routes.map(id => `edge ${id}`)];
    line.textContent = `${entry.kind}: ${entry.reason}${parties.length ? ` (with ${parties.join(', ')})` : ''}`;
    panel.append(line);
  }
  return panel;
}

/** The text of a layout choice; the option value stays the engine's own. */
const CHOICE_LABELS = {
  inherit: 'inherit',
  default: 'not declared',
  auto: 'auto',
  allowed: 'allowed set …',
  lr: 'left → right',
  rl: 'right → left',
  tb: 'top → bottom',
  bt: 'bottom → top',
};
const labelOf = value => CHOICE_LABELS[value] ?? value;

/** How a value in force reads: a fixed value, `auto` (the engine chooses), or the allowed set. */
export function describeChoice(value) {
  if (Array.isArray(value)) return `allowed: ${value.map(labelOf).join(' / ')}`;
  if (value === undefined || value === null) return 'engine default';
  return labelOf(value);
}

/**
 * The layout settings form (direction, strategy, routing) of a container or of the whole view (`settings` from
 * `settingsOf` or `rootSettingsOf`). Each field offers inherit (containers) or "not declared" (where the root may clear
 * it), `auto`, each fixed value, and an allowed set, edited as ticks. Nothing is applied here: `onChoice(field, value)`
 * receives `null` to clear, a name, `'auto'`, or a list of at least two names, and the host sends it to the engine. The
 * fields show the accepted state; a refused value is not kept.
 */
export function layoutFieldsElementOf(document, settings, {disabled = false, onChoice, errors = {}} = {}) {
  const layout = group(document, 'Layout');
  for (const field of ['direction', 'strategy', 'routing']) {
    const setting = settings[field];
    // A root value that is not declared shows the engine default, unless the root may leave it undeclared (routing).
    const undeclared = setting.clearable ? 'default' : formValueOf(setting.effective.value);
    const value = setting.root && setting.own === undefined ? undeclared : formValueOf(setting.own);
    const clear = setting.root ? (setting.clearable ? ['default'] : []) : ['inherit'];
    const select = document.createElement('select');
    select.disabled = disabled;
    for (const option of [...clear, 'auto', ...setting.choices, 'allowed']) {
      const element = document.createElement('option');
      element.value = option;
      element.textContent = labelOf(option);
      select.append(element);
    }
    select.value = value;
    // The allowed set: one tick per choice. It is sent once at least two are ticked; one tick would be a fixed value.
    const allowed = document.createElement('div');
    allowed.className = 'allowed';
    allowed.hidden = value !== 'allowed';
    const ticked = new Set(Array.isArray(setting.own) ? setting.own : []);
    const hint = document.createElement('small');
    const sendAllowed = () => {
      const list = setting.choices.filter(choice => ticked.has(choice));
      if (list.length < 2) {
        hint.textContent = 'Tick at least two.';
        return;
      }
      hint.textContent = '';
      onChoice?.(field, list);
    };
    for (const choice of setting.choices) {
      const box = document.createElement('input');
      box.type = 'checkbox';
      box.checked = ticked.has(choice);
      box.disabled = disabled;
      box.onchange = () => {
        if (box.checked) ticked.add(choice);
        else ticked.delete(choice);
        sendAllowed();
      };
      const tick = document.createElement('label');
      tick.className = 'tick';
      tick.append(box, ` ${labelOf(choice)}`);
      allowed.append(tick);
    }
    allowed.append(hint);
    select.onchange = () => {
      if (select.value === 'allowed') {
        allowed.hidden = false;
        sendAllowed();
        return;
      }
      allowed.hidden = true;
      onChoice?.(field, select.value === 'inherit' || select.value === 'default' ? null : select.value);
    };
    const label = document.createElement('label');
    label.textContent = `${field} `;
    label.append(select);
    const info = document.createElement('small');
    // An unset value is the engine's own choice; it is not the strategy or routing named "none".
    const effective = setting.effective.value;
    const chose = chosenPhrase(setting);
    info.textContent = effective === undefined || effective === null ? ' engine default' : ` ${describeChoice(effective)} · ${sourceOf(setting)}${chose ? ` · ${chose}` : ''}`;
    if (info.dataset) info.dataset.origin = originOf(setting);
    layout.append(label, info, allowed);
    // A value the engine refused is shown at the field it was set in, with the engine's reason.
    if (errors[field]) {
      const problem = document.createElement('small');
      problem.className = 'field-error';
      problem.setAttribute?.('role', 'alert');
      if (problem.dataset) problem.dataset.field = field;
      problem.textContent = ` ${errors[field]}`;
      layout.append(problem);
    }
  }
  return layout;
}

/** A titled group of settings. */
function group(document, title) {
  const element = document.createElement('section');
  const heading = document.createElement('h4');
  heading.textContent = title;
  element.append(heading);
  return element;
}

/** One setting: its name, its controls, and where its value comes from (or the info line that replaces it). */
function settingRow(document, name, controls, source, resetDisabled, onReset, info = null) {
  const element = document.createElement('div');
  const label = document.createElement('label');
  label.textContent = `${name} `;
  label.append(...controls);
  element.append(label);
  if (info) element.append(info);
  else {
    const text = document.createElement('small');
    text.textContent = ` ${source}`;
    element.append(text);
  }
  if (onReset) element.append(button(document, `Reset ${name}`, resetDisabled, onReset));
  return element;
}

/** The `containerSizing` entry of a container with one axis reset to content and the other kept. */
function entryWithout(row, axis) {
  const kept = (other) => {
    const state = row.size[other];
    if (state.state === 'min') return {min: state.value};
    if (state.state === 'fixed') return {fixed: state.value};
    return undefined;
  };
  const other = axis === 'width' ? 'height' : 'width';
  const entry = {};
  const keptOther = kept(other);
  if (keptOther) entry[other] = keptOther;
  return Object.keys(entry).length ? entry : null;
}

function button(document, text, disabled, onClick) {
  const element = document.createElement('button');
  element.textContent = text;
  element.disabled = disabled;
  element.onclick = onClick;
  return element;
}

/** The text of a row: its label, then the state that is set. Sizes are named only when they are authored. */
export function describe(row) {
  const parts = [row.label];
  if (row.container) {
    if (row.size.width.state !== 'auto') parts.push(`w ${row.size.width.label}`);
    if (row.size.height.state !== 'auto') parts.push(`h ${row.size.height.label}`);
  }
  if (row.pinned) parts.push('pinned');
  if (row.conflict) parts.push('conflict');
  else if (row.conflicts?.length) parts.push(row.conflicts.length === 1 ? '1 conflict' : `${row.conflicts.length} conflicts`);
  if (row.invalid) parts.push('invalid');
  return parts.join(' · ');
}
