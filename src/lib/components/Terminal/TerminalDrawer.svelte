<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import * as api from "../../platform/api";
  import { settings } from "../../domain/stores/settings";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import Icon from "../common/Icon.svelte";

  interface Props {
    repoId: string;
    repoName: string;
    repoPath: string;
    height: number;
    onclose?: () => void;
  }

  let { repoId, repoName, repoPath, height, onclose }: Props = $props();

  let termContainer: HTMLDivElement;
  let term: Terminal | null = null;
  let fitAddon: FitAddon | null = null;
  let unlistenOutput: UnlistenFn | null = null;
  let unlistenExit: UnlistenFn | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let themeObserver: MutationObserver | null = null;
  let isExited = $state(false);
  let isRestarting = $state(false);
  let pendingAction = $state<"hide" | "restart" | null>(null);

  let displayPath = $derived.by(() => {
    return repoPath.replace(/^\/Users\/[^/]+/, "~");
  });

  function fontSizeSetting(): number {
    return Math.max(10, Math.min(20, $settings.terminalFontSize || 13));
  }

  function getTerminalTheme(isDark: boolean) {
    if (isDark) {
      return {
        background: "#161b22",
        foreground: "#e6edf3",
        cursor: "#58a6ff",
        cursorAccent: "#161b22",
        selectionBackground: "rgba(56, 139, 253, 0.35)",
        selectionForeground: "#ffffff",
        // ANSI 16 palette (macOS Dark / GitHub Dark)
        black: "#484f58",
        red: "#ff7b72",
        green: "#3fb950",
        yellow: "#d29922",
        blue: "#58a6ff",
        magenta: "#bc8cff",
        cyan: "#39c5cf",
        white: "#b1bac4",
        brightBlack: "#6e7681",
        brightRed: "#ffa198",
        brightGreen: "#56d364",
        brightYellow: "#e3b341",
        brightBlue: "#79c0ff",
        brightMagenta: "#d2a8ff",
        brightCyan: "#56d4dd",
        brightWhite: "#f0f6fc",
      };
    } else {
      return {
        background: "#ffffff",
        foreground: "#1f2328",
        cursor: "#0969da",
        cursorAccent: "#ffffff",
        selectionBackground: "rgba(84, 174, 255, 0.32)",
        selectionForeground: "#1f2328",
        // ANSI 16 palette (macOS Light / GitHub Light)
        black: "#24292f",
        red: "#cf222e",
        green: "#1a7f37",
        yellow: "#9a6700",
        blue: "#0969da",
        magenta: "#8250df",
        cyan: "#1b7c83",
        white: "#6e7781",
        brightBlack: "#57606a",
        brightRed: "#a40e26",
        brightGreen: "#116329",
        brightYellow: "#6f4e00",
        brightBlue: "#0550ae",
        brightMagenta: "#6639ba",
        brightCyan: "#055d64",
        brightWhite: "#24292f",
      };
    }
  }

  function updateTheme() {
    if (!term) return;
    const isDark = document.documentElement.getAttribute("data-theme") === "dark";
    term.options.theme = getTerminalTheme(isDark);
  }

  onMount(async () => {
    const isDark = document.documentElement.getAttribute("data-theme") === "dark";

    term = new Terminal({
      cursorBlink: true,
      cursorStyle: "block",
      fontSize: fontSizeSetting(),
      lineHeight: 1.3,
      fontFamily: '"SF Mono", "SFMono-Regular", ui-monospace, Menlo, Monaco, "Cascadia Code", "Liberation Mono", monospace',
      letterSpacing: 0,
      fontWeight: "400",
      fontWeightBold: "600",
      allowTransparency: true,
      scrollback: 5000,
      theme: getTerminalTheme(isDark),
    });

    // Custom key event handler: support Cmd+K to clear, Cmd+C copy with selection
    term.attachCustomKeyEventHandler((e) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k" && e.type === "keydown") {
        handleClear();
        return false;
      }
      if (e.metaKey && e.key.toLowerCase() === "c" && term?.hasSelection()) {
        return false;
      }
      return true;
    });

    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(termContainer);

    requestAnimationFrame(() => {
      fitAddon?.fit();
      if (term) {
        api.termResize(repoId, term.cols, term.rows);
      }
    });

    // Listen to keystrokes from xterm
    term.onData((data) => {
      api.termInput(repoId, data);
    });

    // Listen to output from Rust PTY
    unlistenOutput = await api.onTermOutput((payload) => {
      if (payload.repoId === repoId && term) {
        term.write(payload.data);
      }
    });

    unlistenExit = await api.onTermExit((payload) => {
      if (payload.repoId === repoId) {
        isExited = true;
      }
    });

    // Open terminal session in backend
    try {
      const res = await api.termOpen(repoId, term.cols, term.rows);
      if (res.replay) {
        term.write(res.replay);
      }
    } catch (e) {
      console.error("Failed to open terminal session:", e);
    }

    // Handle resize
    resizeObserver = new ResizeObserver(() => {
      if (fitAddon && term) {
        fitAddon.fit();
        api.termResize(repoId, term.cols, term.rows);
      }
    });
    resizeObserver.observe(termContainer);

    // Watch for theme changes
    themeObserver = new MutationObserver(() => updateTheme());
    themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });

    // Initial focus
    term.focus();
  });

  onDestroy(() => {
    if (resizeObserver) resizeObserver.disconnect();
    if (themeObserver) themeObserver.disconnect();
    if (unlistenOutput) unlistenOutput();
    if (unlistenExit) unlistenExit();
    if (term) term.dispose();
  });

  // Apply font-size changes immediately, then refit the PTY so the shell sees
  // the new column/row count.
  $effect(() => {
    const size = fontSizeSetting();
    if (!term || term.options.fontSize === size) return;
    term.options.fontSize = size;
    setTimeout(() => {
      if (fitAddon && term) {
        fitAddon.fit();
        api.termResize(repoId, term.cols, term.rows);
      }
    }, 0);
  });

  function handleClear(e?: MouseEvent) {
    if (e) {
      e.preventDefault();
      e.stopPropagation();
    }
    term?.clear();
    // Send form feed (Ctrl+L) to shell so prompt resets cleanly at top
    api.termInput(repoId, "\x0c").catch(() => {});
    term?.focus();
  }

  async function handleRestart(e?: MouseEvent) {
    if (e) {
      e.preventDefault();
      e.stopPropagation();
    }
    if (isRestarting) return;
    isRestarting = true;
    isExited = false;
    pendingAction = null;
    term?.reset();
    try {
      const res = await api.termRestart(repoId);
      if (res && res.replay) {
        term?.write(res.replay);
      }
      fitAddon?.fit();
      if (term) {
        await api.termResize(repoId, term.cols, term.rows);
      }
      term?.focus();
    } catch (e) {
      console.error("Failed to restart terminal:", e);
    } finally {
      isRestarting = false;
    }
  }

  function requestHide(e?: MouseEvent) {
    if (e) {
      e.preventDefault();
      e.stopPropagation();
    }
    if ($settings.confirmTerminalClose && !isExited) {
      pendingAction = "hide";
      return;
    }
    onclose?.();
  }

  function requestRestart(e?: MouseEvent) {
    if (e) {
      e.preventDefault();
      e.stopPropagation();
    }
    if ($settings.confirmTerminalRestart && !isExited) {
      pendingAction = "restart";
      return;
    }
    handleRestart();
  }

  function confirmPending() {
    const action = pendingAction;
    pendingAction = null;
    if (action === "hide") onclose?.();
    else if (action === "restart") handleRestart();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="terminal-drawer"
  style="height: {height}px;"
  onclick={() => term?.focus()}
  role="region"
  aria-label="Terminal drawer"
>
  <!-- Header strip -->
  <div class="term-header">
    <div class="header-left">
      <span class="term-icon-wrap" aria-hidden="true">
        <Icon name="terminal" size={13} />
      </span>
      <span class="term-title">{repoName}</span>
      <span class="term-shell-badge">zsh</span>
      <span class="term-cwd" title={repoPath}>{displayPath}</span>
    </div>

    <div class="header-right">
      {#if isExited}
        <span class="exit-badge" title="Shell process exited">
          <span class="exit-dot"></span>
          Exited
        </span>
      {/if}
      {#if pendingAction}
        <span class="confirm-text">
          {pendingAction === "hide" ? "Hide running terminal?" : "Restart the shell?"}
        </span>
        <button class="term-btn confirm" onclick={confirmPending} title="Confirm">
          {pendingAction === "hide" ? "Hide" : "Restart"}
        </button>
        <button class="term-btn" onclick={() => (pendingAction = null)} title="Cancel">
          Cancel
        </button>
      {:else}
        <button
          type="button"
          class="term-icon-btn"
          onclick={handleClear}
          title="Clear Terminal (⌘K)"
          aria-label="Clear Terminal"
        >
          <Icon name="trash" size={14} />
        </button>
        <button
          type="button"
          class="term-icon-btn"
          class:is-loading={isRestarting}
          disabled={isRestarting}
          onclick={requestRestart}
          title="Restart Terminal"
          aria-label="Restart Terminal"
        >
          <Icon name="refresh" size={14} class={isRestarting ? 'spin' : ''} />
        </button>
        <button
          type="button"
          class="term-icon-btn"
          onclick={requestHide}
          title="Hide Terminal (⌘`)"
          aria-label="Hide Terminal"
        >
          <Icon name="chevron-down" size={14} />
        </button>
      {/if}
    </div>
  </div>

  <!-- Terminal body -->
  <div class="term-body" bind:this={termContainer}></div>
</div>

<style>
  .terminal-drawer {
    width: 100%;
    display: flex;
    flex-direction: column;
    background-color: var(--bg);
    border-top: 1px solid var(--border);
    flex-shrink: 0;
    position: relative;
    box-sizing: border-box;
  }

  .term-header {
    height: 32px;
    background-color: var(--bg-subtle, #f6f8fa);
    border-bottom: 1px solid var(--border-subtle, #e1e4e8);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 10px;
    user-select: none;
    flex-shrink: 0;
    box-sizing: border-box;
  }
  :global([data-theme="dark"]) .term-header {
    background-color: #161b22;
    border-bottom-color: #30363d;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 8px;
    overflow: hidden;
    min-width: 0;
  }

  .term-icon-wrap {
    color: var(--text-muted);
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }

  .term-title {
    color: var(--text);
    font-size: 12px;
    font-weight: 600;
    font-family: var(--font-ui);
    white-space: nowrap;
    line-height: 1;
  }

  .term-shell-badge {
    padding: 1.5px 5px;
    font-size: 10px;
    font-family: var(--font-mono, monospace);
    font-weight: 500;
    color: var(--text-muted);
    background-color: rgba(0, 0, 0, 0.05);
    border-radius: var(--r-sm);
    flex-shrink: 0;
    line-height: 1.2;
  }
  :global([data-theme="dark"]) .term-shell-badge {
    background-color: rgba(255, 255, 255, 0.08);
    color: var(--text-muted);
  }

  .term-cwd {
    color: var(--text-muted);
    font-size: 11px;
    font-family: var(--font-mono, monospace);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    opacity: 0.8;
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .exit-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 7px;
    border-radius: var(--r-sm);
    font-size: 11px;
    font-weight: 500;
    background-color: var(--danger-bg, #ffebed);
    color: var(--danger, #bc494c);
    border: 1px solid rgba(188, 73, 76, 0.2);
  }
  .exit-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background-color: currentColor;
  }

  .term-icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    background: transparent;
    border-radius: var(--r-sm);
    color: var(--text-muted);
    cursor: pointer;
    user-select: none;
    transition: background-color 100ms ease, color 100ms ease;
  }
  .term-icon-btn:hover:not(:disabled) {
    background-color: rgba(0, 0, 0, 0.08);
    color: var(--text);
  }
  .term-icon-btn:active:not(:disabled) {
    background-color: rgba(0, 0, 0, 0.14);
    color: var(--text);
  }
  .term-icon-btn:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  .term-icon-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  :global([data-theme="dark"]) .term-icon-btn {
    color: var(--text-muted);
  }
  :global([data-theme="dark"]) .term-icon-btn:hover:not(:disabled) {
    background-color: rgba(255, 255, 255, 0.1);
    color: var(--text);
  }
  :global([data-theme="dark"]) .term-icon-btn:active:not(:disabled) {
    background-color: rgba(255, 255, 255, 0.16);
    color: var(--text);
  }

  :global(.spin) {
    animation: term-spin 700ms linear infinite;
  }
  @keyframes term-spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .term-btn.confirm {
    background-color: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
  }

  .confirm-text {
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .term-body {
    flex: 1;
    overflow: hidden;
    padding: 6px 8px 4px 10px;
    background-color: #ffffff;
    box-sizing: border-box;
  }
  :global([data-theme="dark"]) .term-body {
    background-color: #161b22;
  }

  .term-body :global(.xterm) {
    height: 100%;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }
  .term-body :global(.xterm-viewport) {
    background-color: transparent !important;
  }
  .term-body :global(.xterm-screen) {
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }
</style>
