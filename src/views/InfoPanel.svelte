<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import {
    api,
    describeError,
    formatDay,
    formatFileSize,
    messageFileUrl,
    messageImageUrl,
    type Message,
    type PeerInfo,
    type SharedKind,
  } from "../lib/ipc";
  import Avatar from "../lib/Avatar.svelte";
  import Progress from "../lib/Progress.svelte";
  import MessageImage from "../lib/MessageImage.svelte";

  let {
    peerId,
    profile = false,
    docked = false,
    onclose,
    onmuted,
  }: {
    peerId: number;
    /** Show the own account as a profile rather than as "Saved Messages". */
    profile?: boolean;
    /** A fixed column beside the chat (wide windows) instead of an overlay. */
    docked?: boolean;
    onclose: () => void;
    /** Notifications for the chat were switched on or off here. */
    onmuted?: (muted: boolean) => void;
  } = $props();

  type Page = { items: Message[]; done: boolean; loading: boolean; error: string };
  const EMPTY: Page = { items: [], done: false, loading: false, error: "" };
  const TAB_LABELS: Record<SharedKind, string> = {
    media: "Медиа",
    file: "Файлы",
    link: "Ссылки",
    voice: "Голосовые",
    audio: "Музыка",
  };
  const TAB_ORDER: SharedKind[] = ["media", "file", "link", "voice", "audio"];
  const MONTHS = [
    "января", "февраля", "марта", "апреля", "мая", "июня",
    "июля", "августа", "сентября", "октября", "ноября", "декабря",
  ];

  let info = $state<PeerInfo | null>(null);
  let error = $state("");
  let active = $state<SharedKind | null>(null);
  let pages = $state.raw<Partial<Record<SharedKind, Page>>>({});
  let saved = $state.raw<Record<number, string>>({});
  let copied = $state(false);
  let muteError = $state("");
  let viewer = $state<Message | null>(null);
  let sentinel = $state<HTMLElement>();
  /** Bumped when the peer changes so stale responses are ignored. */
  let generation = 0;

  $effect(() => {
    const id = peerId;
    let current = true;
    generation++;
    info = null;
    error = "";
    active = null;
    pages = {};
    saved = {};
    viewer = null;
    muteError = "";
    api.peerInfo(id).then(
      (result) => {
        if (!current) return;
        info = result;
      },
      (err) => current && (error = describeError(err)),
    );
    return () => (current = false);
  });

  const title = $derived(
    !info
      ? ""
      : info.kind === "saved" && !profile
        ? "Избранное"
        : info.title || "Без названия",
  );

  function plural(n: number, one: string, few: string, many: string): string {
    const tens = n % 100;
    const ones = n % 10;
    if (tens >= 11 && tens <= 14) return many;
    if (ones === 1) return one;
    if (ones >= 2 && ones <= 4) return few;
    return many;
  }

  const subtitle = $derived.by(() => {
    if (!info) return "";
    if (profile) return "Мой профиль";
    if (info.kind === "user") return "Личный чат";
    if (info.kind === "saved") return "Избранное";
    if (info.members == null) return info.kind === "channel" ? "Канал" : "Группа";
    const n = info.members;
    const fmt = n.toLocaleString("ru");
    if (info.kind === "channel") return `${fmt} ${plural(n, "подписчик", "подписчика", "подписчиков")}`;
    const text = `${fmt} ${plural(n, "участник", "участника", "участников")}`;
    return info.online ? `${text}, ${info.online.toLocaleString("ru")} в сети` : text;
  });

  const isGroupLike = $derived(info?.kind === "group" || info?.kind === "channel");
  const canMute = $derived(!!info && !profile && info.kind !== "saved");
  const shareLink = $derived(
    info?.username ? `https://t.me/${info.username}` : (info?.inviteLink ?? null),
  );

  function birthdayText(value: string): string {
    const [day, month, year] = value.split(".");
    const name = MONTHS[Number(month) - 1];
    if (!name) return value;
    return year ? `${Number(day)} ${name} ${year}` : `${Number(day)} ${name}`;
  }

  const tabs = $derived.by(() => {
    if (!info) return [];
    const counts: Record<SharedKind, number> = { media: 0, file: 0, link: 0, voice: 0, audio: 0 };
    for (const { kind, count } of info.media) {
      if (kind === "photo" || kind === "video" || kind === "gif") counts.media += count;
      else counts[kind] += count;
    }
    return TAB_ORDER.filter((kind) => counts[kind] > 0).map((kind) => ({
      kind,
      label: TAB_LABELS[kind],
      count: counts[kind],
    }));
  });

  // Show the first tab, loaded lazily, once the info arrives.
  $effect(() => {
    if (tabs.length && (active === null || !tabs.some((tab) => tab.kind === active))) {
      select(tabs[0].kind);
    }
  });

  function select(kind: SharedKind) {
    active = kind;
    if (!pages[kind]) void loadMore(kind);
  }

  async function loadMore(kind: SharedKind) {
    const page = pages[kind] ?? EMPTY;
    if (page.loading || page.done) return;
    const gen = generation;
    const id = peerId;
    pages = { ...pages, [kind]: { ...page, loading: true, error: "" } };
    const last = page.items.length ? page.items[page.items.length - 1].id : null;
    try {
      const batch = await api.sharedMedia(id, kind, last, 60);
      if (gen !== generation) return;
      const old = pages[kind] ?? EMPTY;
      pages = {
        ...pages,
        [kind]: { items: [...old.items, ...batch], done: batch.length === 0, loading: false, error: "" },
      };
    } catch (err) {
      if (gen !== generation) return;
      const old = pages[kind] ?? EMPTY;
      pages = { ...pages, [kind]: { ...old, loading: false, done: true, error: describeError(err) } };
    }
  }

  const current = $derived(active ? (pages[active] ?? EMPTY) : EMPTY);

  // Infinite scroll: re-armed after every page so a still-visible sentinel loads on.
  $effect(() => {
    const kind = active;
    void current.items.length;
    if (!sentinel || !kind) return;
    const observer = new IntersectionObserver(
      (entries) => entries.some((entry) => entry.isIntersecting) && void loadMore(kind),
      { rootMargin: "300px" },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  });

  async function toggleMute() {
    if (!info) return;
    const next = !info.muted;
    muteError = "";
    info = { ...info, muted: next };
    try {
      await api.setMuted(peerId, next);
      onmuted?.(next);
    } catch (err) {
      if (info) info = { ...info, muted: !next };
      muteError = describeError(err);
    }
  }

  async function copyLink() {
    if (!shareLink) return;
    try {
      await navigator.clipboard.writeText(shareLink);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      /* clipboard unavailable */
    }
  }

  async function save(message: Message) {
    saved = { ...saved, [message.id]: "Сохранение…" };
    try {
      await api.saveFile(message.peerId, message.id);
      saved = { ...saved, [message.id]: "Сохранено" };
    } catch (err) {
      saved = { ...saved, [message.id]: describeError(err) };
    }
  }

  function firstUrl(text: string): string | null {
    return /https?:\/\/[^\s<>"']+/i.exec(text)?.[0] ?? null;
  }

  function fileTitle(message: Message): string {
    return message.fileName || (message.kind === "voice" ? "Голосовое сообщение" : "Файл");
  }

  // Full-screen viewer for media.
  let thumb = $state<string | null>(null);
  let full = $state<string | null>(null);
  let viewerError = $state("");
  let viewerBytes = $state(0);
  const viewerVideo = $derived(
    viewer?.kind === "video" || viewer?.kind === "gif" || viewer?.kind === "round",
  );

  $effect(() => {
    const message = viewer;
    thumb = null;
    full = null;
    viewerError = "";
    viewerBytes = 0;
    if (!message) return;
    let live = true;
    messageImageUrl(message.peerId, message.id).then(
      (url) => live && (thumb = url),
      () => {},
    );
    if (message.playable) {
      messageFileUrl(message.peerId, message.id, (bytes) => live && (viewerBytes = bytes)).then(
        (url) => live && (full = url),
        (err) => live && (viewerError = describeError(err)),
      );
    }
    return () => (live = false);
  });

  function onkeydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    if (viewer) viewer = null;
    else if (!docked) onclose();
  }

  /**
   * A vertical mouse wheel scrolls the tab strip sideways. Added by hand:
   * Svelte's own wheel handlers are passive and cannot stop the page scroll.
   */
  function sidewaysWheel(strip: HTMLElement) {
    const onwheel = (event: WheelEvent) => {
      if (strip.scrollWidth <= strip.clientWidth || event.deltaX !== 0) return;
      event.preventDefault();
      strip.scrollLeft += event.deltaY;
    };
    strip.addEventListener("wheel", onwheel, { passive: false });
    return () => strip.removeEventListener("wheel", onwheel);
  }
</script>

<svelte:window {onkeydown} />

{#snippet icon(d: string)}
  <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path {d} /></svg>
{/snippet}

{#if !docked}
  <div class="backdrop" onclick={onclose} role="presentation"></div>
{/if}
<aside class="panel" class:docked aria-label="Информация о чате">
  <header>
    <button class="close" onclick={onclose} aria-label="Закрыть">✕</button>
    <span>{profile ? "Мой профиль" : "Информация"}</span>
  </header>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if !info}
    <p class="dim loading">Загрузка…</p>
  {:else}
    <div class="hero">
      <Avatar
        peerId={info.id}
        photoId={info.photoId}
        {title}
        saved={info.kind === "saved" && !profile}
        size={120}
      />
      <h2>{title}</h2>
      <p class="dim">{subtitle}</p>
    </div>

    {#if canMute || (isGroupLike && shareLink)}
      <div class="actions">
        {#if canMute}
          <button class="tile" onclick={toggleMute}>
            {#if info.muted}
              {@render icon("M18 8a6 6 0 0 0-9.3-5M6.3 6.3A6 6 0 0 0 6 8c0 7-3 9-3 9h14M10.3 21a2 2 0 0 0 3.4 0M3 3l18 18")}
            {:else}
              {@render icon("M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9M10.3 21a2 2 0 0 0 3.4 0")}
            {/if}
            <span>Уведомления: {info.muted ? "Выкл." : "Вкл."}</span>
          </button>
        {/if}
        {#if isGroupLike && shareLink}
          <button class="tile" onclick={copyLink}>
            {@render icon("M10 14a4.5 4.5 0 0 0 6.4 0l3-3a4.5 4.5 0 0 0-6.4-6.4l-1 1M14 10a4.5 4.5 0 0 0-6.4 0l-3 3a4.5 4.5 0 0 0 6.4 6.4l1-1")}
            <span>{copied ? "Скопировано" : "Ссылка"}</span>
          </button>
        {/if}
      </div>
      {#if muteError}<p class="error small" role="alert">{muteError}</p>{/if}
    {/if}

    {#if info.phone || info.username || info.birthday || info.about || info.commonChats || info.inviteLink}
      <dl class="card">
        {#if info.phone}
          <div class="row">
            <span class="ico" style:background="hsl(205 60% 45%)">{@render icon("M5 4h4l2 5-2.5 1.5a11 11 0 0 0 5 5L15 13l5 2v4a2 2 0 0 1-2 2A16 16 0 0 1 3 6a2 2 0 0 1 2-2")}</span>
            <div><dd>+{info.phone}</dd><dt>Телефон</dt></div>
          </div>
        {/if}
        {#if info.username}
          <div class="row">
            <span class="ico" style:background="hsl(150 45% 40%)">{@render icon("M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0M16 12v1.5a2.5 2.5 0 0 0 5 0V12a9 9 0 1 0-3.5 7.1")}</span>
            <div>
              <dd>
                <button class="link" onclick={() => openUrl(`https://t.me/${info?.username}`)}>@{info.username}</button>
              </dd>
              <dt>{isGroupLike ? "Ссылка" : "Имя пользователя"}</dt>
            </div>
          </div>
        {/if}
        {#if info.birthday}
          <div class="row">
            <span class="ico" style:background="hsl(35 75% 48%)">{@render icon("M4 11h16v9H4zM12 11V7M12 7c-1.5 0-2-1-2-2 1 0 2 .5 2 2M12 7c1.5 0 2-1 2-2-1 0-2 .5-2 2")}</span>
            <div><dd>{birthdayText(info.birthday)}</dd><dt>День рождения</dt></div>
          </div>
        {/if}
        {#if info.about}
          <div class="row">
            <span class="ico" style:background="hsl(265 45% 55%)">{@render icon("M12 8h.01M11 12h1v5h1M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18")}</span>
            <div><dd class="about">{info.about}</dd><dt>{isGroupLike ? "Описание" : "О себе"}</dt></div>
          </div>
        {/if}
        {#if info.commonChats}
          <div class="row">
            <span class="ico" style:background="hsl(340 55% 52%)">{@render icon("M16 20v-1a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v1M10 11a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7M20 20v-1a3 3 0 0 0-2-2.8M16 4.2a3.5 3.5 0 0 1 0 6.6")}</span>
            <div>
              <dd>{info.commonChats} {plural(info.commonChats, "общая группа", "общие группы", "общих групп")}</dd>
              <dt>Общие группы</dt>
            </div>
          </div>
        {/if}
        {#if info.inviteLink}
          <div class="row">
            <span class="ico" style:background="hsl(190 55% 42%)">{@render icon("M10 14a4.5 4.5 0 0 0 6.4 0l3-3a4.5 4.5 0 0 0-6.4-6.4l-1 1M14 10a4.5 4.5 0 0 0-6.4 0l-3 3a4.5 4.5 0 0 0 6.4 6.4l1-1")}</span>
            <div>
              <dd>
                <button class="link" onclick={() => openUrl(info?.inviteLink ?? "")}>{info.inviteLink}</button>
              </dd>
              <dt>Ссылка-приглашение</dt>
            </div>
          </div>
        {/if}
      </dl>
    {/if}

    {#if tabs.length}
      <div
        class="tabs"
        role="tablist"
        tabindex="-1"
        {@attach sidewaysWheel}
      >
        {#each tabs as tab (tab.kind)}
          <button
            role="tab"
            aria-selected={active === tab.kind}
            class:on={active === tab.kind}
            onclick={(event) => {
              select(tab.kind);
              event.currentTarget.scrollIntoView({ block: "nearest", inline: "nearest", behavior: "smooth" });
            }}
          >
            {tab.label} <span class="count">{tab.count}</span>
          </button>
        {/each}
      </div>

      <div class="content">
        {#if active === "media"}
          <div class="grid">
            {#each current.items as message (message.id)}
              <div class="cell">
                <MessageImage {message} cover onopen={() => (viewer = message)} />
              </div>
            {/each}
          </div>
        {:else if active === "link"}
          <ul class="list">
            {#each current.items as message (message.id)}
              {@const url = firstUrl(message.text)}
              <li class="item">
                <span class="ico small" style:background="hsl(190 55% 42%)">{@render icon("M10 14a4.5 4.5 0 0 0 6.4 0l3-3a4.5 4.5 0 0 0-6.4-6.4l-1 1M14 10a4.5 4.5 0 0 0-6.4 0l-3 3a4.5 4.5 0 0 0 6.4 6.4l1-1")}</span>
                <div class="grow">
                  {#if url}
                    <button class="link url" onclick={() => openUrl(url)}>{url}</button>
                  {/if}
                  <p class="snippet">{message.text}</p>
                  <p class="dim meta">{formatDay(message.date)}</p>
                </div>
              </li>
            {/each}
          </ul>
        {:else if active}
          <ul class="list">
            {#each current.items as message (message.id)}
              <li>
                <button class="item file" onclick={() => save(message)}>
                  <span class="ico small" style:background={active === "file" ? "hsl(205 60% 45%)" : "hsl(35 75% 48%)"}>
                    {#if active === "file"}
                      {@render icon("M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5")}
                    {:else}
                      {@render icon("M9 18V6l10-2v12M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0M19 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0")}
                    {/if}
                  </span>
                  <span class="grow">
                    <span class="name">{fileTitle(message)}</span>
                    <span class="dim meta">
                      {saved[message.id] ?? `${formatFileSize(message.fileSize)} · ${formatDay(message.date)}`}
                    </span>
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}

        {#if current.error}<p class="error small" role="alert">{current.error}</p>{/if}
        {#if current.loading}<p class="dim center">Загрузка…</p>{/if}
        {#if !current.items.length && current.done && !current.error}
          <p class="dim center">Пусто</p>
        {/if}
        {#if !current.done}<div class="sentinel" bind:this={sentinel}></div>{/if}
      </div>
    {/if}
  {/if}
</aside>

{#if viewer}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="viewer" onclick={() => (viewer = null)} role="dialog" aria-modal="true" aria-label="Просмотр" tabindex="-1">
    <div class="viewer-bar">
      <button onclick={(event) => { event.stopPropagation(); viewer = null; }} aria-label="Закрыть">✕</button>
      {#if viewer.playable}
        {@const message = viewer}
        <button class="save" onclick={(event) => { event.stopPropagation(); void save(message); }}>
          {saved[message.id] ?? "Сохранить"}
        </button>
      {/if}
    </div>
    <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
    <div class="stage" onclick={(event) => event.stopPropagation()}>
      {#if viewerVideo && full}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video src={full} controls autoplay></video>
      {:else if full && !viewerVideo}
        <img src={full} alt="" />
      {:else if viewer.playable && !viewerError}
        <Progress done={viewerBytes} total={viewer.fileSize} size={84} />
      {:else if thumb}
        <img src={thumb} alt="" />
      {:else}
        <Progress size={84} />
      {/if}
      {#if viewerError}<p class="error small" role="alert">{viewerError}</p>{/if}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    z-index: 5;
    background: rgb(0 0 0 / 35%);
  }

  .panel {
    position: absolute;
    inset: 0 0 0 auto;
    z-index: 6;
    width: min(360px, 100%);
    display: flex;
    flex-direction: column;
    background: var(--bg-raised);
    box-shadow: -4px 0 20px rgb(0 0 0 / 35%);
    overflow-y: auto;
  }

  .panel > * {
    flex: none;
  }

  .panel.docked {
    position: static;
    width: 100%;
    height: 100%;
    border-left: 1px solid var(--border);
    box-shadow: none;
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 52px;
    padding: 0 8px;
    font-weight: 600;
  }

  .close {
    width: 34px;
    height: 34px;
    padding: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-dim);
    font-weight: 400;
  }

  .close:hover {
    background: var(--bg-hover);
  }

  .hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 8px 16px 16px;
    text-align: center;
  }

  h2 {
    margin: 10px 0 0;
    font-size: 19px;
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  p {
    margin: 0;
  }

  .loading,
  .error {
    padding: 16px;
  }

  .error.small {
    padding: 4px 16px 8px;
    font-size: 12.5px;
  }

  .center {
    padding: 12px 16px;
    text-align: center;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
    padding: 0 16px 16px;
  }

  .tile {
    flex: 1 1 110px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 10px 8px;
    border-radius: 12px;
    background: var(--bg-input);
    color: var(--accent);
    font-size: 12px;
    font-weight: 500;
  }

  .tile:hover {
    background: var(--bg-hover);
  }

  .tile span {
    color: var(--text);
    text-align: center;
  }

  .card {
    margin: 0 16px 12px;
    padding: 4px 0;
    border-radius: 12px;
    background: var(--bg-input);
  }

  .row {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    padding: 10px 16px;
    min-width: 0;
  }

  .row > div {
    min-width: 0;
  }

  .ico {
    flex: none;
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    color: #fff;
  }

  .ico :global(svg) {
    width: 18px;
    height: 18px;
  }

  .ico.small {
    width: 40px;
    height: 40px;
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
    user-select: text;
    -webkit-user-select: text;
  }

  .about {
    white-space: pre-wrap;
  }

  dt {
    margin-top: 2px;
    color: var(--text-dim);
    font-size: 12.5px;
  }

  .link {
    padding: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    text-align: left;
    overflow-wrap: anywhere;
  }

  .tabs {
    display: flex;
    overflow-x: auto;
    border-bottom: 1px solid var(--border);
    scrollbar-width: thin;
  }

  .tabs button {
    flex: none;
    padding: 12px 11px;
    border-radius: 0;
    background: transparent;
    color: var(--text-dim);
    font-size: 13.5px;
    font-weight: 500;
    border-bottom: 2px solid transparent;
  }

  .tabs button.on {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }

  .count {
    opacity: 0.7;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 2px;
  }

  .cell {
    aspect-ratio: 1;
    min-width: 0;
  }

  .list {
    margin: 0;
    padding: 4px 0;
    list-style: none;
  }

  .item {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    width: 100%;
    padding: 8px 16px;
    min-width: 0;
    text-align: left;
  }

  button.item {
    background: transparent;
    color: var(--text);
    font-weight: 400;
    border-radius: 0;
  }

  button.item:hover {
    background: var(--bg-hover);
  }

  .grow {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    font-size: 12.5px;
  }

  .url {
    font-size: 14px;
  }

  .snippet {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    overflow-wrap: anywhere;
    font-size: 13px;
  }

  .sentinel {
    height: 1px;
  }

  .viewer {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    flex-direction: column;
    background: rgb(0 0 0 / 92%);
  }

  .viewer-bar {
    flex: none;
    display: flex;
    justify-content: space-between;
    padding: 10px 12px;
  }

  .viewer-bar button {
    padding: 8px 14px;
    border-radius: 18px;
    background: rgb(255 255 255 / 12%);
    color: #fff;
  }

  .stage {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 0 16px 16px;
  }

  .stage img,
  .stage video {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
</style>
