(() => {
  const container = document.querySelector("#chat");
  if (!container) return;

  const rendered = new Set();
  const query = new URLSearchParams(window.location.search);
  const events = new EventSource(`/overlay/chat/events?${query}`);
  const showBadges = document.body.dataset.showBadges === "true";
  const showAvatars = document.body.dataset.showAvatars === "true";
  const fadeDuration = Number(document.body.dataset.fadeDuration || "0");

  const addText = (parent, className, text) => {
    const element = document.createElement("span");
    element.className = className;
    element.textContent = text;
    parent.append(element);
  };

  const render = (message) => {
    if (rendered.has(message.id)) return;
    rendered.add(message.id);

    const item = document.createElement("article");
    item.className = "message";
    item.dataset.messageId = String(message.id);

    if (showAvatars && message.avatar_url) {
      const avatar = document.createElement("img");
      avatar.className = "avatar";
      avatar.src = message.avatar_url;
      avatar.alt = "";
      avatar.referrerPolicy = "no-referrer";
      item.append(avatar);
    }
    if (showBadges) addText(item, `badge ${message.source}`, message.source);

    const content = document.createElement("div");
    addText(content, "author", message.author);
    addText(content, "text", message.text);
    item.append(content);
    container.append(item);

    if (fadeDuration > 0) {
      item.classList.add("fading");
      window.setTimeout(() => item.remove(), fadeDuration * 1000);
    }
    while (container.childElementCount > 20) container.firstElementChild.remove();
  };

  events.addEventListener("messages", (event) => {
    const snapshot = JSON.parse(event.data);
    snapshot.messages.forEach(render);
  });
})();
