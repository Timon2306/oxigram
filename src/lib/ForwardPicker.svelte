<script lang="ts">
  import { dialogTitle, type Dialog } from "./ipc";
  import Avatar from "./Avatar.svelte";

  let {
    dialogs,
    onpick,
    onclose,
  }: {
    dialogs: Dialog[];
    onpick: (dialog: Dialog) => void;
    onclose: () => void;
  } = $props();

  const LIMIT = 100;
  let query = $state("");
  let input = $state<HTMLInputElement>();

  const shown = $derived.by(() => {
    const needle = query.trim().toLocaleLowerCase();
    const all = needle
      ? dialogs.filter((dialog) => dialogTitle(dialog).toLocaleLowerCase().includes(needle))
      : dialogs;
    return all.slice(0, LIMIT);
  });

  $effect(() => input?.focus());
</script>

<svelte:window onkeydown={(event) => event.key === "Escape" && onclose()} />

<div class="picker">
  <button class="backdrop" aria-label="Закрыть" onclick={onclose}></button>
  <div class="box" role="dialog" aria-label="Переслать">
    <h2>Переслать</h2>
    <input bind:this={input} bind:value={query} type="search" placeholder="Поиск" spellcheck="false" />
    <div class="list">
      {#each shown as dialog (dialog.id)}
        <button class="row" onclick={() => onpick(dialog)}>
          <Avatar
            peerId={dialog.id}
            photoId={dialog.photoId}
            title={dialogTitle(dialog)}
            saved={dialog.kind === "saved"}
            size={36}
          />
          <span class="name">{dialogTitle(dialog)}</span>
        </button>
      {:else}
        <p class="empty">Ничего не найдено</p>
      {/each}
    </div>
  </div>
</div>

<style>
  .picker {
    position: fixed;
    inset: 0;
    z-index: 35;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .backdrop {
    position: absolute;
    inset: 0;
    border: none;
    border-radius: 0;
    background: rgb(0 0 0 / 45%);
  }

  .box {
    position: relative;
    width: min(380px, calc(100% - 32px));
    max-height: min(520px, calc(100% - 48px));
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 18px 12px 10px;
    border-radius: 12px;
    background: var(--bg-raised);
    box-shadow: 0 10px 40px rgb(0 0 0 / 50%);
  }

  h2 {
    margin: 0 10px;
    font-size: 17px;
    font-weight: 600;
  }

  input {
    flex: none;
    width: auto;
    min-width: 0;
    margin: 0 10px;
  }

  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 10px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--text);
    font-weight: 400;
    text-align: left;
  }

  .row:hover {
    background: var(--bg-hover);
  }

  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .empty {
    margin: 0;
    padding: 16px;
    text-align: center;
    color: var(--text-dim);
  }
</style>
