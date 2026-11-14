<script lang="ts">
  let {
    title,
    text = "",
    checkbox = "",
    confirmLabel,
    onconfirm,
    oncancel,
  }: {
    title: string;
    text?: string;
    /** Label of an optional checkbox (checked by default). */
    checkbox?: string;
    confirmLabel: string;
    onconfirm: (checked: boolean) => void;
    oncancel: () => void;
  } = $props();

  let checked = $state(true);
</script>

<svelte:window onkeydown={(event) => event.key === "Escape" && oncancel()} />

<div class="confirm">
  <button class="backdrop" aria-label="Отмена" onclick={oncancel}></button>
  <div class="box" role="alertdialog" aria-label={title}>
    <h2>{title}</h2>
    {#if text}<p>{text}</p>{/if}
    {#if checkbox}
      <label class="check"><input type="checkbox" bind:checked /> {checkbox}</label>
    {/if}
    <div class="actions">
      <button class="ghost" onclick={oncancel}>Отмена</button>
      <button class="danger" onclick={() => onconfirm(checked)}>{confirmLabel}</button>
    </div>
  </div>
</div>

<style>
  .confirm {
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
    width: min(400px, calc(100% - 32px));
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 20px 22px;
    border-radius: 12px;
    background: var(--bg-raised);
    box-shadow: 0 10px 40px rgb(0 0 0 / 50%);
  }

  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
  }

  p {
    margin: 0;
    color: var(--text-dim);
    overflow-wrap: anywhere;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }

  .check input {
    width: auto;
    padding: 0;
    margin: 0;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  .danger {
    background: var(--danger);
    color: #fff;
  }
</style>
