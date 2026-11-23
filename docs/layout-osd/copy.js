const button = document.querySelector('#copy');
const commands = document.querySelector('#commands');
const status = document.querySelector('#copied');

button.hidden = false;
button.addEventListener('click', async () => {
  try {
    await navigator.clipboard.writeText(commands.textContent);
    status.textContent = 'Commands copied.';
  } catch {
    const range = document.createRange();
    range.selectNodeContents(commands);
    const selection = window.getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
    status.textContent = 'Commands selected. Copy them with your keyboard.';
  }
});
