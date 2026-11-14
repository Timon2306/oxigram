<script lang="ts">
  import type { Entity } from "./ipc";

  /**
   * A message text with Telegram's formatting: bold, italic, code, spoilers,
   * quotes, links and mentions. Offsets are UTF-16 units, which is exactly
   * how JavaScript indexes strings.
   */
  let {
    text,
    entities = [],
    onlink,
    onmention,
    onmentionid,
  }: {
    text: string;
    entities?: Entity[];
    /** A URL (shown or hidden behind words) was clicked. */
    onlink: (url: string) => void;
    /** An @username was clicked. */
    onmention: (username: string) => void;
    /** A mention of a user without a username was clicked. */
    onmentionid: (peerId: number) => void;
  } = $props();

  /** Plain links in texts that came without entities (older cache rows). */
  const URL_PATTERN = /https?:\/\/[^\s<>"']+[^\s<>"'.,;:!?)\]]/g;

  type Piece = {
    text: string;
    styles: Set<string>;
    /** What a click does, if anything. */
    link?: { kind: "url" | "mention" | "mentionId" | "email" | "phone"; value: string };
    spoiler: number;
  };

  let revealed = $state<Set<number>>(new Set());

  const pieces = $derived.by((): Piece[] => {
    const spans: Entity[] = entities.length > 0 ? entities : autoLinks(text);
    if (spans.length === 0) return [{ text, styles: new Set(), spoiler: -1 }];

    // Cut the text at every entity edge, then give each slice the entities
    // that cover it.
    const cuts = new Set<number>([0, text.length]);
    for (const span of spans) {
      cuts.add(clamp(span.offset));
      cuts.add(clamp(span.offset + span.length));
    }
    const edges = [...cuts].sort((a, b) => a - b);
    const out: Piece[] = [];
    for (let i = 0; i < edges.length - 1; i++) {
      const start = edges[i];
      const end = edges[i + 1];
      if (start === end) continue;
      const slice = text.slice(start, end);
      const piece: Piece = { text: slice, styles: new Set(), spoiler: -1 };
      spans.forEach((span, index) => {
        if (span.offset > start || span.offset + span.length < end) return;
        const whole = text.slice(span.offset, span.offset + span.length);
        switch (span.kind) {
          case "url":
            piece.link = { kind: "url", value: whole };
            break;
          case "textUrl":
            piece.link = { kind: "url", value: span.target ?? whole };
            break;
          case "mention":
            piece.link = { kind: "mention", value: whole.replace(/^@/, "") };
            break;
          case "mentionName":
            piece.link = { kind: "mentionId", value: span.target ?? "" };
            break;
          case "email":
            piece.link = { kind: "email", value: whole };
            break;
          case "phone":
            piece.link = { kind: "phone", value: whole };
            break;
          case "spoiler":
            piece.spoiler = index;
            piece.styles.add("spoiler");
            break;
          default:
            piece.styles.add(span.kind);
        }
      });
      out.push(piece);
    }
    return out;
  });

  function clamp(index: number): number {
    return Math.max(0, Math.min(text.length, index));
  }

  function autoLinks(source: string): Entity[] {
    return [...source.matchAll(URL_PATTERN)].map((match) => ({
      kind: "url",
      offset: match.index ?? 0,
      length: match[0].length,
    }));
  }

  function follow(event: MouseEvent, piece: Piece) {
    event.preventDefault();
    const link = piece.link;
    if (!link) return;
    if (link.kind === "mention") onmention(link.value);
    else if (link.kind === "mentionId") onmentionid(Number(link.value));
    else if (link.kind === "email") onlink(`mailto:${link.value}`);
    else if (link.kind === "phone") navigator.clipboard.writeText(link.value);
    else onlink(/^[a-z][a-z0-9+.-]*:/i.test(link.value) ? link.value : `https://${link.value}`);
  }

  function reveal(piece: Piece) {
    if (piece.spoiler < 0 || revealed.has(piece.spoiler)) return;
    revealed = new Set([...revealed, piece.spoiler]);
  }

  function classes(piece: Piece): string {
    const names = [...piece.styles];
    if (piece.spoiler >= 0 && revealed.has(piece.spoiler)) names.push("shown");
    return names.join(" ");
  }
</script>

<span class="rich"
  >{#each pieces as piece, index (index)}{#if piece.link && !(piece.spoiler >= 0 && !revealed.has(piece.spoiler))}<a
        href={piece.link.kind === "url" ? piece.link.value : "#"}
        class={classes(piece)}
        onclick={(event) => follow(event, piece)}>{piece.text}</a
      >{:else if piece.styles.size > 0}<!-- svelte-ignore a11y_no_noninteractive_tabindex --><span
        class={classes(piece)}
        onclick={() => reveal(piece)}
        role={piece.spoiler >= 0 ? "button" : undefined}
        tabindex={piece.spoiler >= 0 ? 0 : undefined}
        onkeydown={(event) => event.key === "Enter" && reveal(piece)}>{piece.text}</span
      >{:else}{piece.text}{/if}{/each}</span
>

<style>
  .rich {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .bold {
    font-weight: 700;
  }

  .italic {
    font-style: italic;
  }

  .underline {
    text-decoration: underline;
  }

  .strike {
    text-decoration: line-through;
  }

  .underline.strike {
    text-decoration: underline line-through;
  }

  .code,
  .pre {
    padding: 1px 4px;
    border-radius: 4px;
    background: rgb(0 0 0 / 22%);
    font-family: "Cascadia Mono", Consolas, "DejaVu Sans Mono", monospace;
    font-size: 0.92em;
  }

  .pre {
    display: block;
    margin: 4px 0;
    padding: 8px 10px;
    border-radius: 8px;
    white-space: pre-wrap;
  }

  .quote {
    display: block;
    margin: 4px 0;
    padding: 2px 0 2px 10px;
    border-left: 3px solid currentColor;
    opacity: 0.9;
  }

  .spoiler {
    border-radius: 4px;
    background: var(--text-dim);
    color: transparent;
    cursor: pointer;
    user-select: none;
  }

  .spoiler.shown {
    background: none;
    color: inherit;
    cursor: text;
    user-select: text;
  }

  .hashtag,
  .command {
    color: var(--link);
  }

  a {
    color: var(--link);
    text-decoration: none;
  }

  a:hover {
    text-decoration: underline;
  }
</style>
