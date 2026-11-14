<script lang="ts">
  import { formatFileSize } from "./ipc";

  let {
    files,
    onsend,
    onadd,
    onpick,
    onremove,
    onclose,
  }: {
    files: File[];
    /** `asPhoto`: send pictures compressed as photos rather than as files. */
    onsend: (files: File[], caption: string, asPhoto: boolean) => void;
    /** Opens the file picker to add more. */
    onadd: () => void;
    /** Files pasted while the box is open. */
    onpick: (files: FileList) => void;
    onremove: (index: number) => void;
    onclose: () => void;
  } = $props();

  function onpaste(event: ClipboardEvent) {
    const pasted = event.clipboardData?.files;
    if (pasted && pasted.length > 0) {
      event.preventDefault();
      event.stopPropagation();
      onpick(pasted);
    }
  }

  /** Formats Telegram accepts as a compressed photo. */
  const PHOTO_TYPES = new Set(["image/jpeg", "image/png", "image/webp", "image/bmp"]);
  /** Telegram's limit for a photo; larger pictures go as files. */
  const PHOTO_MAX = 10 * 1024 * 1024;

  const pictures = $derived(files.filter((file) => PHOTO_TYPES.has(file.type) && file.size <= PHOTO_MAX));
  let asPhoto = $state(true);
  let caption = $state("");

  // Previews only for pictures; revoked when the dialog closes.
  const previews = $derived(
    files.map((file) => (PHOTO_TYPES.has(file.type) ? URL.createObjectURL(file) : null)),
  );
  $effect(() => {
    const urls = previews;
    return () => urls.forEach((url) => url && URL.revokeObjectURL(url));
  });

  const title = $derived.by(() => {
    if (files.length === 1) return pictures.length === 1 && asPhoto ? "Отправить фото" : "Отправить файл";
    return `Отправить ${files.length} ${files.length < 5 ? "файла" : "файлов"}`;
  });

  function submit() {
    onsend(files, caption, asPhoto && pictures.length > 0);
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      onclose();
    } else if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      submit();
    }
  }
</script>

<svelte:window {onkeydown} onpastecapture={onpaste} />

<div class="backdrop" onclick={onclose} role="presentation"></div>
<div class="sendbox" role="dialog" aria-label={title}>
  <h2>{title}</h2>

  <div class="items">
    {#each files as file, index (index)}
      <div class="item" class:photo={!!previews[index] && asPhoto}>
        {#if previews[index]}
          <img src={previews[index]} alt="" />
        {:else}
          <span class="icon" aria-hidden="true">
            <svg viewBox="0 0 24 24" width="22" height="22"
              ><path
                fill="currentColor"
                d="M6 2h8l5 5v14a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1zm7 1.5V8h4.5L13 3.5z"
              /></svg
            >
          </span>
        {/if}
        {#if !previews[index] || !asPhoto}
          <span class="meta">
            <span class="name">{file.name || "Файл"}</span>
            <span class="size">{formatFileSize(file.size)}</span>
          </span>
        {/if}
        <button class="remove" onclick={() => onremove(index)} aria-label="Убрать">✕</button>
      </div>
    {/each}
  </div>

  {#if pictures.length > 0}
    <label class="check">
      <input type="checkbox" bind:checked={asPhoto} />
      Сжать изображение{pictures.length > 1 ? "я" : ""}
    </label>
  {/if}

  <!-- svelte-ignore a11y_autofocus -->
  <textarea bind:value={caption} rows="1" maxlength="1024" placeholder="Подпись" autofocus></textarea>

  <div class="actions">
    <button class="ghost add" onclick={onadd}>Добавить</button>
    <button class="ghost" onclick={onclose}>Отмена</button>
    <button class="primary" onclick={submit}>Отправить</button>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 30;
    background: rgb(0 0 0 / 50%);
  }

  .sendbox {
    position: fixed;
    inset: 50% auto auto 50%;
    z-index: 31;
    width: min(420px, calc(100% - 32px));
    max-height: calc(100% - 48px);
    transform: translate(-50%, -50%);
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 18px;
    border-radius: 12px;
    background: var(--bg-raised);
    box-shadow: 0 10px 40px rgb(0 0 0 / 50%);
    overflow: clip;
  }

  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
  }

  .items {
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .remove {
    flex: none;
    margin-left: auto;
    width: 26px;
    height: 26px;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }

  .remove:hover {
    background: var(--bg-input);
    color: var(--text);
  }

  .add {
    margin-right: auto;
  }

  .item img {
    width: 56px;
    height: 56px;
    flex: none;
    border-radius: 8px;
    object-fit: cover;
  }

  .item.photo img {
    width: 100%;
    height: auto;
    max-height: 280px;
    object-fit: contain;
    background: var(--bg-input);
  }

  .icon {
    width: 48px;
    height: 48px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    background: var(--accent);
    color: #fff;
  }

  .meta {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .size {
    color: var(--text-dim);
    font-size: 13px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
    cursor: pointer;
  }

  .check input {
    flex: none;
    width: 16px;
    height: 16px;
    margin: 0;
    padding: 0;
    accent-color: var(--accent);
  }

  textarea {
    min-width: 0;
    resize: none;
    padding: 9px 12px;
    border: 1px solid transparent;
    border-radius: 10px;
    background: var(--bg-input);
    color: var(--text);
    font: inherit;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .primary {
    background: var(--accent);
    color: #fff;
  }
</style>
