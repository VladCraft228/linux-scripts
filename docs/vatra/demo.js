const demo = document.querySelector('.preview-window');
const get = selector => demo.querySelector(selector);
const all = selector => [...demo.querySelectorAll(selector)];
const tabs = all('[role="tab"]');

function openTab(tab) {
  tabs.forEach(item => {
    const selected = item === tab;
    item.setAttribute('aria-selected', String(selected));
    item.tabIndex = selected ? 0 : -1;
    get('#' + item.getAttribute('aria-controls')).hidden = !selected;
  });
}

tabs.forEach((tab, index) => {
  tab.addEventListener('click', () => openTab(tab));
  tab.addEventListener('keydown', event => {
    const moves = {
      ArrowLeft: (index + tabs.length - 1) % tabs.length,
      ArrowRight: (index + 1) % tabs.length,
      ArrowUp: (index + tabs.length - 1) % tabs.length,
      ArrowDown: (index + 1) % tabs.length,
      Home: 0,
      End: tabs.length - 1,
    };
    if (!Object.hasOwn(moves, event.key)) return;
    event.preventDefault();
    const next = tabs[moves[event.key]];
    openTab(next);
    next.focus();
  });
});

all('[data-open]').forEach(button => button.addEventListener('click', () => {
  const tab = get('#tab-' + button.dataset.open);
  openTab(tab);
  tab.focus();
}));

const presets = {
  automatic: { name: 'Automatic', cap: 4374, epp: 'balance_power' },
  balanced: { name: 'Balanced', cap: 3581, epp: 'balance_performance' },
  performance: { name: 'Performance', cap: 4374, epp: 'performance' },
  saver: { name: 'Power Saver', cap: 2789, epp: 'power' },
};
const applied = { ac: { ...presets.saver, id: 'saver' }, bat: { ...presets.saver, id: 'saver' } };
const source = get('[data-source]');
const mode = get('[data-preset]');
const preference = get('[data-epp]');
const speed = get('#demo-speed');
const set = (selector, value) => all(selector).forEach(node => { node.textContent = value; });
const MHz = value => value + ' MHz';
const options = (id, powerSource) => ({
  ...presets[id],
  epp: id === 'automatic' && powerSource === 'ac' ? 'balance_performance' : presets[id].epp,
});

function updateReadings() {
  set('[data-live-mode]', applied.bat.name);
  set('[data-live-cap]', applied.bat.cap);
  set('[data-live-epp]', applied.bat.epp);
  Object.entries(applied).forEach(([key, value]) => set('[data-config-' + key + ']', value.epp + ' · Max ' + MHz(value.cap)));
  all('[data-mode]').forEach(button => button.setAttribute('aria-pressed', String(button.dataset.mode === applied.bat.id)));
}

const modeReview = get('[data-mode-review]');
let pickedMode;
all('[data-mode]').forEach(button => button.addEventListener('click', () => {
  pickedMode = button;
  const sample = options(button.dataset.mode, 'bat');
  set('[data-review-title]', 'Review ' + sample.name);
  set('[data-review-values]', 'BAT · ' + MHz(applied.bat.cap) + ' → ' + MHz(sample.cap) + ' · EPP: ' + sample.epp);
  modeReview.hidden = false;
  get('[data-apply-mode]').focus();
}));
function closeModeReview() {
  modeReview.hidden = true;
  pickedMode.focus();
}
get('[data-apply-mode]').addEventListener('click', () => {
  const id = pickedMode.dataset.mode;
  ['ac', 'bat'].forEach(key => { applied[key] = { ...options(id, key), id }; });
  updateReadings();
  loadAdvanced();
  closeModeReview();
});
get('[data-cancel-mode]').addEventListener('click', closeModeReview);

const advancedReview = get('[data-advanced-review]');
function updateAdvanced() {
  const current = applied[source.value];
  set('[data-cap-label]', MHz(speed.value));
  set('[data-current-epp]', current.epp);
  set('[data-next-epp]', preference.value);
  set('[data-current-cap]', MHz(current.cap));
  set('[data-next-cap]', MHz(speed.value));
  advancedReview.hidden = true;
}
function loadAdvanced() {
  const current = applied[source.value];
  mode.value = current.id;
  speed.value = current.cap;
  preference.value = current.epp;
  updateAdvanced();
}
source.addEventListener('change', loadAdvanced);
mode.addEventListener('change', () => {
  const sample = mode.value === 'custom' ? applied[source.value] : options(mode.value, source.value);
  speed.value = sample.cap;
  preference.value = sample.epp;
  updateAdvanced();
});
preference.addEventListener('change', updateAdvanced);
speed.addEventListener('input', updateAdvanced);
get('[data-review-advanced]').addEventListener('click', () => {
  const current = applied[source.value];
  const change = MHz(current.cap) + ' → ' + MHz(speed.value) + ' · EPP: ' + preference.value;
  set('[data-advanced-summary]', (speed.valueAsNumber > current.cap ? 'Higher CPU limits can increase power use. ' : '') + change);
  advancedReview.hidden = false;
  get('[data-apply-advanced]').focus();
});
function closeAdvancedReview() {
  advancedReview.hidden = true;
  get('[data-review-advanced]').focus();
}
get('[data-apply-advanced]').addEventListener('click', () => {
  const id = mode.value;
  applied[source.value] = { name: presets[id]?.name ?? 'Custom', id, cap: speed.valueAsNumber, epp: preference.value };
  updateReadings();
  updateAdvanced();
  closeAdvancedReview();
});
get('[data-cancel-advanced]').addEventListener('click', closeAdvancedReview);

const colorInput = get('[data-custom-color]');
function chooseColor(color) {
  demo.style.setProperty('--demo-accent', color);
  colorInput.value = color;
  all('[data-color]').forEach(button => button.setAttribute('aria-pressed', String(button.dataset.color === color)));
}
all('[data-color]').forEach(button => button.addEventListener('click', () => chooseColor(button.dataset.color)));
colorInput.addEventListener('input', () => chooseColor(colorInput.value));
const percentage = get('[data-tray-percentage]');
const bolt = get('[data-tray-bolt]');
function updateTray() {
  get('[data-tray-inside]').hidden = percentage.value !== 'inside';
  get('[data-tray-beside]').hidden = percentage.value !== 'beside';
  get('[data-tray-symbol]').hidden = bolt.value === 'hidden';
  get('[data-tray-symbol]').style.color = bolt.value === 'white' ? '#fff' : '';
}
percentage.addEventListener('change', updateTray);
bolt.addEventListener('change', updateTray);

updateReadings();
loadAdvanced();
updateTray();
