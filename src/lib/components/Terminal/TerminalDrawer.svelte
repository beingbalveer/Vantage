<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import * as api from "../../api";
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

  onMount(async () => {
    // derive palette from CSS tokens
    const isDark = document.documentElement.getAttribute("data-theme") === "dark";

    term = new Terminal({
      cursorBlink: true,
      fontSize: 13,
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

  async function handleClear() {
    term?.clear();
  }

  async function handleRestart() {
    isExited = false;
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
      <button class="term-btn" onclick={handleClear} title="Clear">
        Clear
      </button>
      <button class="term-btn" onclick={handleRestart} title="Restart shell">
        Restart
      </button>
      <button class="term-btn" onclick={onclose} title="Hide terminal">
        Hide
      </button>
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

  .term-body {
    flex: 1;
    overflow: hidden;
    padding: 6px;
  }
</style>
