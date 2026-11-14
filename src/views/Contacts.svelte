<script lang="ts">
  import { api, describeError, isOnline, presenceText, type Contact } from "../lib/ipc";
  import Avatar from "../lib/Avatar.svelte";
  import VirtualList from "../lib/VirtualList.svelte";

  let {
    now,
    onpick,
    onclose,
  }: {
    /** Unix seconds, for the presence lines. */
    now: number;
    onpick: (contact: Contact) => void;
    onclose: () => void;
  } = $props();

  let contacts = $state.raw<Contact[]>([]);
  let loading = $state(true);
  let error = $state("");
  let query = $state("");

  api.contacts().then(
    (list) => {
      contacts = list;
      loading = false;
    },
    (err) => {
      error = describeError(err);
      loading = false;
    },
  );

  const shown = $derived.by(() => {
    const needle = query.trim().toLocaleLowerCase();
    if (!needle) return contacts;
    return contacts.filter((contact) => contact.title.toLocaleLowerCase().includes(needle));
  });
</script>

<svelte:window onkeydown={(event) => event.key === "Escape" && onclose()} />

<div class="backdrop" onclick={onclose} role="presentation"></div>
<div class="modal picker" role="dialog" aria-label="Контакты">
  <header>
    <h2>Контакты</h2>
    <button class="close" onclick={onclose} aria-label="Закрыть">✕</button>
  </header>
  <!-- svelte-ignore a11y_autofocus -->
  <input class="search" type="search" bind:value={query} placeholder="Поиск" spellcheck="false" autofocus />
  <div class="list">
    {#if error}
      <p class="error">{error}</p>
    {:else if shown.length === 0}
      <p class="dim note">{loading ? "Загрузка…" : query ? "Ничего не найдено" : "Контактов нет"}</p>
    {:else}
      <VirtualList items={shown} rowHeight={58} key={(contact) => contact.id}>
        {#snippet row(contact)}
          <button class="contact" onclick={() => onpick(contact)}>
            <Avatar peerId={contact.id} photoId={contact.photoId} title={contact.title} size={42} />
            <span class="body">
              <span class="title">{contact.title || "Без имени"}</span>
              <span class="status" class:online={isOnline(contact.presence, now)}
                >{presenceText(contact.presence, now)}</span
              >
            </span>
          </button>
        {/snippet}
      </VirtualList>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 15;
    background: rgb(0 0 0 / 45%);
  }

  .modal {
    position: fixed;
    inset: 50% auto auto 50%;
    z-index: 16;
    width: min(400px, calc(100% - 32px));
    height: min(560px, calc(100% - 48px));
    transform: translate(-50%, -50%);
    display: flex;
    flex-direction: column;
    border-radius: 12px;
    background: var(--bg-raised);
    box-shadow: 0 10px 40px rgb(0 0 0 / 50%);
    /* clip, not hidden: focusing the search field must not scroll it sideways. */
    overflow: clip;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 10px 6px 18px;
  }

  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
  }

  .close {
    width: 34px;
    height: 34px;
    padding: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-dim);
  }

  .close:hover {
    background: var(--bg-hover);
  }

  .search {
    min-width: 0;
    margin: 6px 14px 10px;
    height: 34px;
    padding: 0 14px;
    border: 1px solid transparent;
    border-radius: 17px;
    background: var(--bg-input);
  }

  .list {
    flex: 1;
    min-height: 0;
  }

  .note,
  .error {
    margin: 0;
    padding: 16px;
    text-align: center;
  }

  .contact {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 14px;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--text);
    font-weight: 400;
    text-align: left;
  }

  .contact:hover {
    background: var(--bg-hover);
  }

  .body {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .title {
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status {
    color: var(--text-dim);
    font-size: 13px;
  }

  .status.online {
    color: var(--accent);
  }
</style>
