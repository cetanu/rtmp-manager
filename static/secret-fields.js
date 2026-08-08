(() => {
  document.addEventListener("click", (event) => {
    const button = event.target.closest("[data-secret-toggle]");
    if (!button) return;

    const input = document.getElementById(button.dataset.secretToggle);
    if (!input) return;
    const visible = input.type === "text";
    input.type = visible ? "password" : "text";
    button.setAttribute("aria-pressed", String(!visible));
    button.setAttribute("aria-label", visible ? "Show value" : "Hide value");
    button.title = visible ? "Show value" : "Hide value";
    button.querySelector('[data-secret-visible="false"]').hidden = !visible;
    button.querySelector('[data-secret-visible="true"]').hidden = visible;
    input.focus({ preventScroll: true });
  });
})();
