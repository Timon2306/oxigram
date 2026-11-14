<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, describeError, type UiState } from "../lib/ipc";
  import Card from "./Card.svelte";

  let { onstate }: { onstate: (next: UiState) => void } = $props();

  let apiId = $state("");
  let apiHash = $state("");
  let busy = $state(false);
  let error = $state("");

  const idValid = $derived(/^\d{1,10}$/.test(apiId.trim()));
  const hashValid = $derived(/^[0-9a-fA-F]{32}$/.test(apiHash.trim()));

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy || !idValid || !hashValid) return;
    busy = true;
    error = "";
    try {
      onstate(await api.saveKeys(Number(apiId.trim()), apiHash.trim()));
    } catch (err) {
      error = describeError(err);
    } finally {
      busy = false;
    }
  }
</script>

<Card title="Ключи Telegram API">
  <p class="dim">
    Oxigram не содержит встроенных ключей. Получите свои бесплатно на
    <button class="link" type="button" onclick={() => openUrl("https://my.telegram.org/apps")}>
      my.telegram.org
    </button>
    → «API development tools». Ключи сохраняются только на этом компьютере.
  </p>

  <form onsubmit={submit}>
    <label class="field">
      <span>api_id</span>
      <input
        bind:value={apiId}
        inputmode="numeric"
        autocomplete="off"
        spellcheck="false"
        placeholder="1234567"
      />
    </label>
    <label class="field">
      <span>api_hash</span>
      <input
        bind:value={apiHash}
        type="password"
        autocomplete="off"
        spellcheck="false"
        placeholder="32 символа"
      />
    </label>
    <p class="error" role="alert">{error}</p>
    <button type="submit" disabled={busy || !idValid || !hashValid}>
      {busy ? "Сохранение…" : "Продолжить"}
    </button>
  </form>
</Card>

<style>
  p {
    margin: 0;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
