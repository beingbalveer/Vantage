<script lang="ts">
  import type { FileChange } from "../../types";
  import { splitPath } from "../../format";
  import { registerDismiss } from "../../commands";
  import Icon from "../common/Icon.svelte";

  interface Props {
    files: FileChange[];
    selectedFile: string | null;
    isUncommitted: boolean;
    checkedPaths: string[];
    showCommitControls?: boolean;
    focusDiff?: boolean;
    hasCommitDetails?: boolean;
    showCommitDetails?: boolean;
    onbrowseall?: () => void;
    oncommitrequest?: () => void;
    ontogglecommitdetails?: () => void;
    ontogglefocusdiff?: () => void;
    onselectfile?: (path: string) => void;
    ontogglepath?: (path: string) => void;
    ontoggleall?: (select: boolean) => void;
  }

  let {
    files,
    selectedFile,
    isUncommitted,
    checkedPaths,
    showCommitControls = false,
    focusDiff = false,
    hasCommitDetails = false,
    showCommitDetails = false,
    ontogglefocusdiff,
    onselectfile,
    ontogglepath,
    ontoggleall,
    onbrowseall,
    oncommitrequest,
    ontogglecommitdetails,
  }: Props = $props();

  let allChecked = $derived(
    files.length > 0 && files.every((f) => checkedPaths.includes(f.path))
  );

  interface FileTriage {
    kind: "dependencies" | "config" | "testsRemoved" | "secrets" | "bigFile" | "binary";
    severity: "info" | "caution" | "warn";
    label: string;
    detail: string;
  }

  function getFileTriage(file: FileChange): FileTriage | null {
    const p = file.path.toLowerCase();
    const name = file.path.split("/").pop() || "";
    const nameLower = name.toLowerCase();

    if (
      nameLower === ".env" ||
      (nameLower.startsWith(".env.") &&
        !nameLower.includes("example") &&
        !nameLower.includes("sample") &&
        !nameLower.includes("template"))
    ) {
      return { kind: "secrets", severity: "warn", label: "Secrets", detail: "Sensitive environment file" };
    }

    const isTest =
      p.includes("/test/") ||
      p.includes("/tests/") ||
      p.includes("/__tests__/") ||
      nameLower.includes("test") ||
      nameLower.includes("spec");

    if (isTest && (file.status === "deleted" || (file.deletions >= 10 && file.deletions > file.additions))) {
      return {
        kind: "testsRemoved",
        severity: "warn",
        label: "TestsRemoved",
        detail: file.status === "deleted" ? "Test file deleted" : `−${file.deletions} test lines removed`,
      };
    }

    const depFiles = [
      "package.json", "package-lock.json", "yarn.lock", "pnpm-lock.yaml", "bun.lockb",
      "cargo.toml", "cargo.lock", "go.mod", "go.sum", "pyproject.toml", "poetry.lock",
      "gemfile", "gemfile.lock", "composer.json", "composer.lock", "pom.xml",
    ];
    if (
      depFiles.includes(nameLower) ||
      (nameLower.startsWith("requirements") && nameLower.endsWith(".txt")) ||
      nameLower.startsWith("pipfile") ||
      nameLower.startsWith("build.gradle")
    ) {
      return { kind: "dependencies", severity: "info", label: "Dependencies", detail: `${name} dependency file` };
    }

    if (
      p.includes(".github/workflows") ||
      nameLower.startsWith("dockerfile") ||
      nameLower.startsWith("docker-compose") ||
      p.includes("/migrations/") ||
      nameLower.endsWith(".tf") ||
      nameLower.endsWith(".tfvars") ||
      nameLower === ".gitlab-ci.yml" ||
      p.includes("/k8s/") ||
      p.includes("/helm/") ||
      (nameLower.startsWith("nginx") && nameLower.endsWith(".conf"))
    ) {
      return { kind: "config", severity: "caution", label: "Config", detail: "Infrastructure or workflow configuration" };
    }

    if (file.binary) {
      return { kind: "binary", severity: "info", label: "Binary", detail: "Binary file" };
    }

    if (file.additions + file.deletions > 500) {
      return { kind: "bigFile", severity: "caution", label: "BigFile", detail: `+${file.additions} −${file.deletions} lines` };
    }

    return null;
  }

  function statusGlyph(status: FileChange["status"]): string {
    switch (status) {
      case "modified":
      case "typeChanged":
        return "•";
      case "deleted":
        return "−";
      case "renamed":
        return "→";
      case "conflicted":
        return "!";
      default:
        return "+";
    }
  }

  let showOptions = $state(false);

  $effect(() => {
    const unregister = registerDismiss(() => {
      if (!showOptions) return false;
      showOptions = false;
      return true;
    });
    return unregister;
  });
</script>

<div class="file-list-container">
  <div class="file-list-header">
    <span class="header-title">
      {files.length} changed {files.length === 1 ? "file" : "files"}
    </span>


    <button
      class="files-more"
      type="button"
      aria-label="File actions"
      aria-haspopup="menu"
      aria-expanded={showOptions}
      title="File actions"
      onclick={() => (showOptions = !showOptions)}
    >
      <Icon name="more-horizontal" size={14} />
    </button>

    {#if showOptions}
      <div class="files-options" role="menu" tabindex="-1">
        {#if isUncommitted}
          <button
            role="menuitem"
            onclick={() => {
              ontoggleall?.(!allChecked);
              showOptions = false;
            }}
          >
            {allChecked ? "Deselect" : "Select"} all files for commit
          </button>
          <button
            role="menuitem"
            onclick={() => {
              oncommitrequest?.();
              showOptions = false;
            }}
          >
            Commit selected files…
          </button>
        {:else if hasCommitDetails}
          <button
            role="menuitem"
            onclick={() => {
              ontogglecommitdetails?.();
              showOptions = false;
            }}
          >
            {showCommitDetails ? "Hide" : "Show"} commit details
          </button>
        {/if}
        <button
          role="menuitem"
          onclick={() => {
            ontogglefocusdiff?.();
            showOptions = false;
          }}
        >
          {focusDiff ? "Show commit list" : "Focus diff"}
        </button>
        {#if onbrowseall}
          <button
            role="menuitem"
            onclick={() => {
              onbrowseall?.();
              showOptions = false;
            }}
          >
            Browse project files
          </button>
        {/if}
      </div>
    {/if}
  </div>

  <div class="file-rows">
    {#if files.length === 0}
      <div class="empty-files text-sm text-muted">No changed files</div>
    {/if}

    {#each files as file (file.path + "-" + file.status + "-" + file.additions + "-" + file.deletions)}
      {@const { dir, name } = splitPath(file.path)}
      {@const isSelected = selectedFile === file.path}
      {@const isChecked = checkedPaths.includes(file.path)}
      {@const triage = getFileTriage(file)}

      <div
        class="file-row {isSelected ? 'selected' : ''}"
        data-file-path={file.path}
        onclick={() => onselectfile?.(file.path)}
        onkeydown={(e) => {
          if (e.key === " ") {
            e.preventDefault();
            onselectfile?.(file.path);
          }
        }}
        role="button"
        tabindex="0"
        title="{file.path} ({file.status}{file.additions > 0 ? ', +' + file.additions : ''}{file.deletions > 0 ? ', −' + file.deletions : ''}){triage ? ' · ' + triage.label + ': ' + triage.detail : ''}"
      >
        {#if showCommitControls}
          <input
            type="checkbox"
            checked={isChecked}
            aria-label="Select {file.path} for commit"
            onclick={(e) => e.stopPropagation()}
            onchange={() => ontogglepath?.(file.path)}
            class="row-cb"
          />
        {/if}

        <div class="path-wrap">
          {#if dir}
            <span class="dir-text">{dir}/</span>
          {/if}
          <span class="file-name">{name}</span>
        </div>

        <div class="row-right">
          <span class="status-badge {file.status}" aria-label={file.status} title={file.status}>
            {statusGlyph(file.status)}
          </span>
        </div>
      </div>
    {/each}
  </div>
</div>

<style>
  .file-list-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    min-height: 0;
    background-color: var(--bg);
    overflow: hidden;
  }

  .file-list-header {
    position: relative;
    height: 43px;
    min-height: 43px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    background-color: var(--bg);
    border-bottom: 1px solid #dcdfe3;
    flex-shrink: 0;
  }

  .header-title {
    font-size: 12px;
    color: #444a51;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .files-more {
    margin-left: auto;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 0;
    background: none;
    color: var(--text-muted);
    padding: 2px;
    cursor: pointer;
    opacity: 0;
    transition: opacity 100ms ease;
  }
  .file-list-header:hover .files-more,
  .files-more:focus-visible,
  .files-more[aria-expanded="true"] {
    opacity: 1;
  }

  .files-options {
    position: absolute;
    right: 5px;
    top: 39px;
    z-index: 30;
    background: var(--bg);
    box-shadow: var(--shadow-pop);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    padding: 5px;
    min-width: 200px;
  }
  .files-options button {
    display: block;
    width: 100%;
    font-size: 12px;
    padding: 8px;
    text-align: left;
    background: none;
    border: 0;
    color: var(--text);
    border-radius: 4px;
    cursor: pointer;
  }
  .files-options button:hover {
    background: var(--bg-muted);
  }

  .file-rows {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    min-height: 0;
    background-color: var(--bg);
  }

  .empty-files {
    padding: 24px 16px;
    text-align: center;
  }

  .file-row {
    display: flex;
    align-items: center;
    gap: 9px;
    height: 36px;
    padding: 0 12px;
    cursor: pointer;
    border-bottom: 1px solid #dfe3e7;
    background-color: var(--bg);
    position: relative;
  }
  .file-row:hover {
    background: #f3f4f6;
  }
  .file-row.selected {
    background: #e6e9ed;
  }
  .file-row:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .row-cb {
    cursor: pointer;
    width: 14px;
    height: 14px;
    margin: 0;
    flex-shrink: 0;
    accent-color: var(--accent);
  }

  .path-wrap {
    flex: 1;
    min-width: 0;
    display: flex;
    white-space: nowrap;
    font-size: 12px;
    color: #30363d;
  }
  .dir-text {
    color: #717b86;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .file-name {
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 0;
    max-width: 100%;
  }

  .row-right {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .status-badge {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 13px;
    height: 13px;
    border: 1.5px solid #a38622;
    color: #a38622;
    border-radius: 1px;
    font-size: 12px;
    line-height: 1;
    flex-shrink: 0;
  }
  .status-badge.added,
  .status-badge.untracked,
  .status-badge.copied {
    color: #44824a;
    border-color: #44824a;
  }
  .status-badge.deleted,
  .status-badge.conflicted {
    color: var(--danger);
    border-color: var(--danger);
  }
  .status-badge.renamed {
    color: #4a6fa5;
    border-color: #4a6fa5;
  }

  /* Prototype compact overrides (approved-prototype.html · @media max-width:1400px). */
  @media (max-width: 1400px) {
    .file-list-header {
      padding: 0 10px;
    }
    .header-title {
      font-size: 11px;
    }
    .file-row {
      padding: 0 10px;
    }
    .path-wrap {
      font-size: 11.5px;
    }
  }
</style>
