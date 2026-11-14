<script lang="ts">
  import { api, describeError, type UiState } from "./lib/ipc";
  import Keys from "./views/Keys.svelte";
  import Login from "./views/Login.svelte";
  import Main from "./views/Main.svelte";
  import ContextMenu from "./lib/ContextMenu.svelte";
  import Logo from "./lib/Logo.svelte";
  import { closeMenu } from "./lib/menu.svelte";

  let ui = $state<UiState | null>(null);
  let startupError = $state<string | null>(null);

  async function load() {
    startupError = null;
    try {
      ui = await api.getState();
    } catch (err) {
      startupError = describeError(err);
    }
  }

  // Way out when the saved keys or session keep the app from starting.
  async function changeKeys() {
    try {
      ui = await api.clearKeys();
      startupError = null;
    } catch (err) {
      startupError = describeError(err);
    }
  }

  // The WebView's own menu (Back, Reload, Inspect) makes no sense in an app.
  // Text fields keep it for cut, copy and paste; everything else either
  // opened our menu already or gets none.
  function oncontextmenu(event: MouseEvent) {
    const target = event.target;
    if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return;
    if (event.defaultPrevented) return;
    event.preventDefault();
    closeMenu();
  }

  load();
</script>

<svelte:document {oncontextmenu} />

<main>
  {#if ui === null}
    <section class="center">
      <Logo size={96} />
      {#if startupError}
        <p class="error">{startupError}</p>
        <button onclick={load}>Повторить</button>
        <button class="link" onclick={changeKeys}>Сменить ключи API</button>
      {:else}
        <p class="dim">Подключение…</p>
      {/if}
    </section>
  {:else if ui.stage === "needKeys"}
    <Keys onstate={(next) => (ui = next)} />
  {:else if ui.stage === "ready"}
    <Main me={ui.me} onstate={(next) => (ui = next)} />
  {:else}
    <Login state={ui} onstate={(next) => (ui = next)} />
  {/if}
</main>

<ContextMenu />

<style>
  main {
    height: 100%;
    overflow: auto;
  }

  .center {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 24px;
    text-align: center;
  }
</style>
