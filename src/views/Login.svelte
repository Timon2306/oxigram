<script lang="ts">
  import { api, describeError, errorCode, type UiState } from "../lib/ipc";
  import Card from "./Card.svelte";

  type LoginState = Extract<UiState, { stage: "needPhone" | "needCode" | "needPassword" }>;

  let { state: login, onstate }: { state: LoginState; onstate: (next: UiState) => void } =
    $props();

  let phone = $state("");
  let code = $state("");
  let password = $state("");
  let busy = $state(false);
  let error = $state("");
  /** Telegram rejected the API keys themselves: offer to re-enter them. */
  let keysRejected = $state(false);

  const title = $derived(
    login.stage === "needPhone"
      ? "Вход в Telegram"
      : login.stage === "needCode"
        ? "Код подтверждения"
        : "Облачный пароль",
  );

  async function run(action: () => Promise<UiState>) {
    if (busy) return;
    busy = true;
    error = "";
    keysRejected = false;
    try {
      const next = await action();
      code = "";
      password = "";
      onstate(next);
    } catch (err) {
      error = describeError(err);
      const failure = errorCode(err);
      keysRejected = failure === "API_ID_INVALID" || failure === "API_ID_PUBLISHED_FLOOD";
      // The backend dropped the pending step: resync with it.
      if (failure === "WRONG_STAGE" || failure === "PHONE_CODE_EXPIRED") {
        api.getState().then(onstate, () => {});
      }
    } finally {
      busy = false;
    }
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (login.stage === "needPhone") run(() => api.requestCode(phone));
    else if (login.stage === "needCode") run(() => api.submitCode(code));
    else run(() => api.submitPassword(password));
  }

  function focus(node: HTMLInputElement) {
    node.focus();
  }
</script>

<Card {title}>
  <form onsubmit={submit}>
    {#if login.stage === "needPhone"}
      <label class="field">
        <span>Номер телефона в международном формате</span>
        <input
          bind:value={phone}
          use:focus
          type="tel"
          autocomplete="off"
          placeholder="+7 900 000 00 00"
        />
      </label>
    {:else if login.stage === "needCode"}
      <p class="dim">
        Код отправлен на {login.phone} — в приложение Telegram на другом устройстве или по SMS.
      </p>
      <label class="field">
        <span>Код</span>
        <input
          bind:value={code}
          use:focus
          inputmode="numeric"
          autocomplete="one-time-code"
          placeholder="12345"
        />
      </label>
    {:else}
      <p class="dim">На аккаунте включена двухэтапная проверка.</p>
      <label class="field">
        <span>Пароль{login.hint ? ` (подсказка: ${login.hint})` : ""}</span>
        <input bind:value={password} use:focus type="password" autocomplete="off" />
      </label>
    {/if}

    <p class="error" role="alert">{error}</p>

    <button
      type="submit"
      disabled={busy ||
        (login.stage === "needPhone" && phone.trim().length < 5) ||
        (login.stage === "needCode" && code.trim().length === 0) ||
        (login.stage === "needPassword" && password.length === 0)}
    >
      {busy ? "Подождите…" : login.stage === "needPhone" ? "Получить код" : "Войти"}
    </button>
  </form>

  <div class="links">
    {#if login.stage !== "needPhone"}
      <button class="link" type="button" disabled={busy} onclick={() => run(api.backToPhone)}>
        Другой номер
      </button>
    {/if}
    <button
      class="link"
      class:attention={keysRejected}
      type="button"
      disabled={busy}
      onclick={() => run(api.clearKeys)}
    >
      Сменить ключи API
    </button>
  </div>
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

  .links {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: 13px;
  }

  .links .attention {
    font-weight: 600;
    text-decoration: underline;
  }
</style>
