if (navigator.clipboard) {
  document.querySelectorAll('[data-copy]').forEach(button => {
    button.hidden = false;
    let reset;
    button.addEventListener('click', async () => {
      window.clearTimeout(reset);
      const commands = document.getElementById(button.dataset.copy);
      try {
        await navigator.clipboard.writeText(commands.textContent.trim());
        button.textContent = 'Copied';
      } catch {
        commands.focus();
        button.textContent = 'Select and copy manually';
      }
      reset = window.setTimeout(() => { button.textContent = 'Copy commands'; }, 2400);
    });
  });
}
