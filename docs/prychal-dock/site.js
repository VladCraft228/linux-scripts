const preview = document.querySelector('#preview');
const title = document.querySelector('#preview-title');
const dock = document.querySelector('.dock');
const tiles = [...dock.querySelectorAll('.tile')];

let active = null;
let focused = null;
let hovered = null;
let hideTimer;

function show(tile) {
  clearTimeout(hideTimer);
  if (active !== tile) {
    active?.removeAttribute('data-active');
    active = tile;
    tile.setAttribute('data-active', '');
    title.textContent = tile.dataset.app + ' · Preview';
  }
  preview.setAttribute('data-visible', '');
}

function hide() {
  clearTimeout(hideTimer);
  active?.removeAttribute('data-active');
  active = null;
  preview.removeAttribute('data-visible');
}

function settle() {
  clearTimeout(hideTimer);
  const tile = hovered || focused;
  if (tile) show(tile);
  else hideTimer = setTimeout(hide, 110);
}

for (const tile of tiles) {
  tile.addEventListener('pointerenter', event => {
    if (event.pointerType === 'touch') return;
    hovered = tile;
    show(tile);
  });
  tile.addEventListener('pointerleave', () => {
    if (hovered === tile) hovered = null;
    settle();
  });
  tile.addEventListener('focus', () => {
    focused = tile.matches(':focus-visible') ? tile : null;
    if (focused) show(tile);
  });
  tile.addEventListener('blur', () => {
    if (focused === tile) focused = null;
    settle();
  });
  tile.addEventListener('click', () => show(tile));
}

dock.addEventListener('keydown', event => {
  if (event.key !== 'Escape') return;
  focused = null;
  hovered = null;
  hide();
});

document.addEventListener('pointerdown', event => {
  if (!dock.contains(event.target)) {
    hovered = null;
    focused = null;
    hide();
  }
});

document.querySelector('#copy').addEventListener('click', async () => {
  const commands = document.querySelector('#commands');
  const status = document.querySelector('#copied');
  try {
    await navigator.clipboard.writeText(commands.textContent);
    status.textContent = 'Commands copied.';
  } catch {
    const range = document.createRange();
    range.selectNodeContents(commands);
    window.getSelection().removeAllRanges();
    window.getSelection().addRange(range);
    status.textContent = 'Commands selected. Copy them with your keyboard.';
  }
});
