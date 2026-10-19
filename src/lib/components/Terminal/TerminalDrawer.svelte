<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import * as api from "../../api";
  import { settings } from "../../stores/settings";
  import type { UnlistenFn } from "@tauri-apps/api/event";

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
  let isExited = $state(false);
  let pendingAction = $state<"hide" | "restart" | null>(null);

  function fontSizeSetting(): number {
    return Math.max(10, Math.min(20, $settings.terminalFontSize || 13));
  }

  onMount(async () => {
    // derive palette from CSS tokens
    const isDark = document.documentElement.getAttribute("data-theme") === "dark";

    term = new Terminal({
      cursorBlink: true,
      fontSize: fontSizeSetting(),
      fontFamily: 'var(--font-mono), monospace',
      theme: {
        background: isDark ? "#171b20" : "#f0f2f5",
        foreground: isDark ? "#d1d7e0" : "#1f2328",
        cursor: isDark ? "#4493f8" : "#0969da",
        selectionBackground: isDark ? "#16314f" : "#ddf4ff",
      },
    });

    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(termContainer);
    fitAddon.fit();

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
  });

  onDestroy(() => {
    if (resizeObserver) resizeObserver.disconnect();
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

  async function handleClear() {
    term?.clear();
  }

  async function handleRestart() {
    isExited = false;
    pendingAction = null;
    term?.clear();
    try {
      const res = await api.termRestart(repoId);
      if (res.replay) {
        term?.write(res.replay);
      }
    } catch (e) {
      console.error("Failed to restart terminal:", e);
    }
  }

  function requestHide() {
    if ($settings.confirmTerminalClose && !isExited) {
      pendingAction = "hide";
      return;
    }
    onclose?.();
  }

  function requestRestart() {
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

<div class="terminal-drawer" style="height: {height}px;">
  <!-- Header strip -->
  <div class="term-header">
    <div class="header-left">
      <span class="term-title text-strong">{repoName}</span>
      <span class="term-cwd text-sm">{repoPath}</span>
    </div>

    <div class="header-right">
      {#if isExited}
        <span class="exit-badge text-xs">Exited</span>
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
        <button class="term-btn" onclick={handleClear} title="Clear">
          Clear
        </button>
        <button class="term-btn" onclick={requestRestart} title="Restart shell">
          Restart
        </button>
        <button class="term-btn" onclick={requestHide} title="Hide terminal">
          Hide
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
    background-color: var(--bg-inset);
    border-top: none;
    flex-shrink: 0;
  }

  .term-header {
    height: 28px;
    background-color: var(--bg-subtle);
    border-bottom: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 10px;
    user-select: none;
  }

  .header-left {
    display: flex;
    align-items: baseline;
    gap: 8px;
    overflow: hidden;
  }

  .term-title {
    color: var(--text);
    font-size: 12px;
  }

  .term-cwd {
    color: var(--text-muted);
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .exit-badge {
    padding: 1px 4px;
    border-radius: var(--r-sm);
    background-color: var(--danger);
    color: #ffffff;
  }

  .term-btn {
    height: 20px;
    padding: 0 6px;
    font-size: 11px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background-color: var(--bg);
    color: var(--text);
    cursor: pointer;
  }
  .term-btn:hover {
    background-color: var(--bg-muted);
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
    padding: 6px;
  }
</style>
