<script lang="ts">
  /**
   * A small emoji panel. The glyphs come from the system emoji font, so
   * there are no image assets to load or keep in memory.
   */
  let { onpick, onclose }: { onpick: (emoji: string) => void; onclose: () => void } = $props();

  const RECENT_KEY = "oxigram.recentEmoji";
  const RECENT_MAX = 24;

  const GROUPS: { title: string; list: string }[] = [
    {
      title: "Смайлы",
      list: "😀 😃 😄 😁 😆 😅 😂 🤣 😊 😇 🙂 🙃 😉 😌 😍 🥰 😘 😗 😙 😚 😋 😛 😝 😜 🤪 🤨 🧐 🤓 😎 🥳 😏 😒 😞 😔 😟 😕 🙁 😣 😖 😫 😩 🥺 😢 😭 😤 😠 😡 🤬 🤯 😳 🥵 🥶 😱 😨 😰 😥 😓 🤗 🤔 🫡 🤭 🤫 🤥 😶 😐 😑 😬 🙄 😯 😦 😧 😮 😲 🥱 😴 🤤 😪 😵 🤐 🥴 🤢 🤮 🤧 😷 🤒 🤕 🤑 🤠 😈 👿 💀 👻 👽 🤖 💩 😺 😸 😹 😻 😼 😽 🙀 😿 😾",
    },
    {
      title: "Жесты",
      list: "👍 👎 👌 🤌 🤏 ✌️ 🤞 🫰 🤟 🤘 🤙 👈 👉 👆 👇 ☝️ ✋ 🤚 🖐 🖖 👋 🤝 🙏 👏 🙌 🫶 👐 🤲 💪 🦾 ✍️ 💅 🤳 👀 👁 🧠 🫀 👂 👃 👅 👄 💋",
    },
    {
      title: "Сердца",
      list: "❤️ 🧡 💛 💚 💙 💜 🖤 🤍 🤎 💔 ❣️ 💕 💞 💓 💗 💖 💘 💝 💟 ❤️‍🔥 ❤️‍🩹 🔥 ✨ 🌟 ⭐ 💫 💥 💯",
    },
    {
      title: "Животные и природа",
      list: "🐶 🐱 🐭 🐹 🐰 🦊 🐻 🐼 🐨 🐯 🦁 🐮 🐷 🐸 🐵 🙈 🙉 🙊 🐔 🐧 🐦 🦆 🦅 🦉 🐺 🐗 🐴 🦄 🐝 🦋 🐌 🐞 🐢 🐍 🐙 🦀 🐠 🐬 🐳 🦈 🌵 🌲 🌴 🌱 🍀 🍁 🌸 🌹 🌻 🌞 🌙 ⛅ 🌈 ❄️ ☔ 🌊",
    },
    {
      title: "Еда",
      list: "🍏 🍎 🍐 🍊 🍋 🍌 🍉 🍇 🍓 🫐 🍒 🍑 🥭 🍍 🥥 🥝 🍅 🥑 🥦 🌽 🥕 🥔 🍞 🧀 🥚 🍳 🥓 🍗 🍔 🍟 🍕 🌭 🌮 🍣 🍜 🍝 🍰 🎂 🍩 🍪 🍫 🍬 🍿 ☕ 🍵 🧃 🥤 🍺 🍷 🥂",
    },
    {
      title: "Занятия и предметы",
      list: "⚽ 🏀 🏈 🎾 🏐 🎮 🕹 🎲 🎯 🎳 🎸 🎧 🎤 🎬 🎨 🏆 🥇 🎁 🎉 🎊 🎈 💻 ⌨️ 🖥 📱 📷 💡 🔦 📚 📝 ✏️ 📌 📎 🔑 🔒 💰 💳 ⏰ 🚗 ✈️ 🚀 🏠 ⛺",
    },
    {
      title: "Символы",
      list: "✅ ❌ ⭕ ❗ ❓ ‼️ ⁉️ 💤 🆗 🆒 🆕 🔝 ➕ ➖ ✖️ ➗ ♾ 💲 ©️ ®️ ™️ 🔴 🟠 🟡 🟢 🔵 🟣 ⚫ ⚪ ⬆️ ⬇️ ⬅️ ➡️ 🔄 🔁 ▶️ ⏸ ⏹",
    },
  ];

  function loadRecent(): string[] {
    try {
      return JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    } catch {
      return [];
    }
  }
  let recent = $state<string[]>(loadRecent());

  function pick(emoji: string) {
    recent = [emoji, ...recent.filter((e) => e !== emoji)].slice(0, RECENT_MAX);
    try {
      localStorage.setItem(RECENT_KEY, JSON.stringify(recent));
    } catch {
      // Not remembered; fine.
    }
    onpick(emoji);
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      onclose();
    }
  }}
/>

<div class="emoji-backdrop" onclick={onclose} role="presentation"></div>
<div class="emoji-panel" role="dialog" aria-label="Эмодзи">
  {#if recent.length > 0}
    <p class="group">Недавние</p>
    <div class="grid">
      {#each recent as emoji (emoji)}
        <button onclick={() => pick(emoji)}>{emoji}</button>
      {/each}
    </div>
  {/if}
  {#each GROUPS as group (group.title)}
    <p class="group">{group.title}</p>
    <div class="grid">
      {#each group.list.split(" ") as emoji, index (index)}
        <button onclick={() => pick(emoji)}>{emoji}</button>
      {/each}
    </div>
  {/each}
</div>

<style>
  .emoji-backdrop {
    position: fixed;
    inset: 0;
    z-index: 24;
  }

  .emoji-panel {
    position: absolute;
    bottom: 60px;
    left: max(12px, calc((100% - 820px) / 2 + 16px));
    z-index: 25;
    width: 340px;
    max-height: 340px;
    padding: 6px 8px 10px;
    overflow-y: auto;
    border-radius: 12px;
    background: var(--bg-raised);
    box-shadow: 0 8px 30px rgb(0 0 0 / 45%);
  }

  .group {
    margin: 8px 4px 4px;
    color: var(--text-dim);
    font-size: 12.5px;
    font-weight: 600;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
  }

  .grid button {
    height: 36px;
    padding: 0;
    border: none;
    border-radius: 8px;
    background: transparent;
    font-size: 22px;
    line-height: 1;
  }

  .grid button:hover {
    background: var(--bg-hover);
  }
</style>
