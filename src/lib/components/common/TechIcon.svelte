<script lang="ts">
  interface Props {
    name: string;
    size?: number | string;
    class?: string;
  }

  let { name, size = 20, class: className = "" }: Props = $props();

  let s = $derived(typeof size === "number" ? `${size}px` : size);
</script>

<span
  class="tech-icon {className}"
  style="width:{s};height:{s};"
  aria-hidden="true"
>
  {#if name === "rust"}
    <!-- Rust Logo -->
    <svg viewBox="0 0 32 32" width="100%" height="100%">
      <g fill="#CE412B">
        <circle cx="16" cy="16" r="14" fill="#CE412B" />
        <path
          d="M16 5a11 11 0 1 0 11 11A11 11 0 0 0 16 5zm0 18.5a7.5 7.5 0 1 1 7.5-7.5 7.5 7.5 0 0 1-7.5 7.5z"
          fill="#fff"
        />
        <text
          x="16"
          y="20.5"
          font-size="13"
          font-weight="bold"
          font-family="system-ui, sans-serif"
          text-anchor="middle"
          fill="#CE412B">R</text
        >
      </g>
    </svg>
  {:else if name === "react"}
    <!-- React Logo -->
    <svg viewBox="-11.5 -10.23174 23 20.46348" width="100%" height="100%">
      <circle cx="0" cy="0" r="2.05" fill="#61dafb" />
      <g stroke="#61dafb" stroke-width="1" fill="none">
        <ellipse rx="11" ry="4.2" />
        <ellipse rx="11" ry="4.2" transform="rotate(60)" />
        <ellipse rx="11" ry="4.2" transform="rotate(120)" />
      </g>
    </svg>
  {:else if name === "nextjs"}
    <!-- Next.js Logo -->
    <svg viewBox="0 0 24 24" width="100%" height="100%">
      <circle cx="12" cy="12" r="11" fill="#000" />
      <path
        d="M9.5 8v8M14.5 8l-5 8M14.5 8v5"
        stroke="#fff"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        fill="none"
      />
    </svg>
  {:else if name === "vue"}
    <!-- Vue.js Logo -->
    <svg viewBox="0 0 24 24" width="100%" height="100%">
      <polygon points="12,21 1.5,3 5.5,3 12,14.5 18.5,3 22.5,3" fill="#42b883" />
      <polygon points="12,14.5 6.5,5 9.5,5 12,9.5 14.5,5 17.5,5" fill="#35495e" />
    </svg>
  {:else if name === "svelte"}
    <!-- Svelte Logo -->
    <svg viewBox="0 0 24 24" width="100%" height="100%">
      <path
        d="M19.5 7.2a5.4 5.4 0 0 0-7.8-2L7.3 7.8A5.4 5.4 0 0 0 4.6 14l2.5-1.5a2.5 2.5 0 0 1 1.2-2.9l4.4-2.6a2.5 2.5 0 0 1 3.6 1 2.5 2.5 0 0 1-.3 2.6l-3.3 2a5.4 5.4 0 0 0-2.4 4.5 5.4 5.4 0 0 0 7.8 2l4.4-2.6a5.4 5.4 0 0 0 2.7-6.2l-2.5 1.5a2.5 2.5 0 0 1-1.2 2.9l-4.4 2.6a2.5 2.5 0 0 1-3.6-1 2.5 2.5 0 0 1 .3-2.6l3.3-2a5.4 5.4 0 0 0 2.4-4.5z"
        fill="#ff3e00"
      />
    </svg>
  {:else if name === "angular"}
    <!-- Angular Logo -->
    <svg viewBox="0 0 24 24" width="100%" height="100%">
      <polygon points="12,2 21.5,5.5 19.8,17.5 12,22 4.2,17.5 2.5,5.5" fill="#dd0031" />
      <polygon points="12,4.5 12,19.5 18,16 19.4,6.2" fill="#c3002f" />
      <path d="M12 6.5L8 16.5h2l.9-2.3h2.2l.9 2.3h2L12 6.5zm-.1 3.5l.8 2.2h-1.6l.8-2.2z" fill="#fff" />
    </svg>
  {:else if name === "typescript"}
    <!-- TypeScript Logo -->
    <svg viewBox="0 0 24 24" width="100%" height="100%">
      <rect width="22" height="22" x="1" y="1" rx="4" fill="#3178c6" />
      <path
        d="M11 11h-2.5v7h-2v-7H4v-1.8h7V11zm4.7 4.7c.6.4 1.3.6 2 .6.8 0 1.3-.3 1.3-.8 0-.5-.4-.7-1.4-1.1-1.3-.5-2.2-1.2-2.2-2.4 0-1.6 1.3-2.7 3.2-2.7 1 0 1.9.3 2.6.7l-.6 1.6c-.6-.4-1.3-.6-1.9-.6-.7 0-1.2.3-1.2.8 0 .5.4.7 1.5 1.1 1.4.5 2.1 1.2 2.1 2.4 0 1.7-1.3 2.8-3.4 2.8-1.2 0-2.2-.3-3-.9l.7-1.5z"
        fill="#fff"
      />
    </svg>
  {:else if name === "nodejs"}
    <!-- Node.js Logo -->
    <svg viewBox="0 0 24 24" width="100%" height="100%">
      <path
        d="M12 2.2l8.5 4.9v9.8L12 21.8 3.5 16.9V7.1L12 2.2z"
        fill="#539e43"
      />
      <path
        d="M12 5.5l6 3.5v7l-6 3.5-6-3.5v-7l6-3.5z"
        fill="#3e863d"
      />
      <text
        x="12"
        y="14.8"
        font-size="8.5"
        font-weight="bold"
        font-family="system-ui, sans-serif"
        text-anchor="middle"
        fill="#fff">JS</text
      >
    </svg>
  {:else if name === "python"}
    <!-- Python Logo -->
    <svg viewBox="0 0 24 24" width="100%" height="100%">
      <path
        d="M11.9 2C8.5 2 6.5 3.5 6.5 5.5V8h5.5v1H4.5C2.5 9 1 10.8 1 13.5c0 2.5 1.8 4.2 4.2 4.2H7v-2.3c0-2.2 1.8-4 4-4h4.5c1.2 0 2.2-.9 2.2-2.1V5.5C17.7 3.5 15.3 2 11.9 2zm-2.2 2.2c.6 0 1.1.5 1.1 1.1 0 .6-.5 1.1-1.1 1.1-.6 0-1.1-.5-1.1-1.1 0-.6.5-1.1 1.1-1.1z"
        fill="#3776ab"
      />
      <path
        d="M12.1 22c3.4 0 5.4-1.5 5.4-3.5V16h-5.5v-1h7.5c2 0 3.5-1.8 3.5-4.5 0-2.5-1.8-4.2-4.2-4.2H17v2.3c0 2.2-1.8 4-4 4H8.5c-1.2 0-2.2.9-2.2 2.1v3.8c0 2 2.4 3.5 5.8 3.5zm2.2-2.2c-.6 0-1.1-.5-1.1-1.1 0-.6.5-1.1 1.1-1.1.6 0 1.1.5 1.1 1.1 0 .6-.5 1.1-1.1 1.1z"
        fill="#ffd438"
      />
    </svg>
  {:else if name === "go"}
    <!-- Go Logo -->
    <svg viewBox="0 0 24 24" width="100%" height="100%">
      <rect width="22" height="22" x="1" y="1" rx="5" fill="#00add8" />
      <text
        x="12"
        y="16.5"
        font-size="12"
        font-weight="900"
        font-family="system-ui, -apple-system, sans-serif"
        text-anchor="middle"
        fill="#fff"
        letter-spacing="-0.5">GO</text
      >
    </svg>
  {:else if name === "flutter"}
    <!-- Flutter Logo -->
    <svg viewBox="0 0 24 24" width="100%" height="100%">
      <polygon points="14,2 4,12 7.5,15.5 21,2" fill="#42a5f5" />
      <polygon points="14,14 8,20 11.5,23.5 17.5,17.5 21,21 21,17.5" fill="#0d47a1" />
      <polygon points="11.5,16.5 15,13 18.5,16.5 15,20" fill="#29b6f6" />
    </svg>
  {:else if name === "swift"}
    <!-- Swift Logo -->
    <svg viewBox="0 0 24 24" width="100%" height="100%">
      <rect width="22" height="22" x="1" y="1" rx="5" fill="#f05138" />
      <path
        d="M18.8 17.5c-2.4 2.1-5.7 2.3-8.8 1.4 3.1-1.8 4.4-4.5 4.4-4.5s-2.9 1.4-5.5.3c2.4-1.9 3.8-4.7 3.8-4.7s-3.2 2-6.2.2c-.3-.2-.5-.4-.7-.6 1.8-1.5 3.9-3.2 5.9-4.2C8.6 6 5.5 8.7 4 12.3c2.6-1.5 4.8-1.6 4.8-1.6s-3.5 3-4 6.3c3.4 3.5 8.7 3.8 12.7 1.2.6-.4.9-.7 1.3-1.1z"
        fill="#fff"
      />
    </svg>
  {:else if name === "java"}
    <!-- Java Logo -->
    <svg viewBox="0 0 24 24" width="100%" height="100%">
      <path
        d="M8.2 19.3c3.5.3 7-.2 10.3-.7-.2.6-.9 1.1-2 1.4-3.2.7-7.4.7-9.5-.1.4-.2.8-.4 1.2-.6zm-.5-2.2c3.2.2 6.5-.1 9.6-.6-.2.5-.8.9-1.8 1.1-3 .5-6.8.6-8.8 0 .3-.1.7-.3 1-.5zm4.8-5.3c.7.8.2 1.8-.7 2.4-1.3.8-2.6.9-3.3.4-.6-.4-.3-1.3.6-1.9 1.1-.7 2.6-1.3 3.4-.9zm-3.6 5c-1.3-.2-2.3-.6-2-1.3.3-.6 1.5-.7 2.8-.7h.5c-1.1.5-1.5 1.3-1.3 2zm8.5-7.3c-.8.8-2.6 1.4-3.9 1.5.8-.8 1.5-1.8 1.4-2.8-.2-1.1-1.3-2-2.3-2.9 1.7.5 3.2 1.7 3.6 2.8.2.4.2.9.2 1.4zm-2.8-4c.4.7.7 1.5.5 2.3-.5.1-1-.1-1.4-.4-.8-.7-.9-1.8-.4-2.6.4.2.9.4 1.3.7z"
        fill="#e76f00"
      />
      <path
        d="M6 21.5c4 .6 9 .5 13 0 1 .4.8.9-.3 1.2-3.8.9-9.5.8-13.4 0-.8-.3-.3-.9.7-1.2z"
        fill="#5382a1"
      />
    </svg>
  {:else if name === "docker"}
    <!-- Docker Logo -->
    <svg viewBox="0 0 24 24" width="100%" height="100%">
      <rect x="5.5" y="9.5" width="2.2" height="2" rx="0.3" fill="#2496ed" />
      <rect x="8.3" y="9.5" width="2.2" height="2" rx="0.3" fill="#2496ed" />
      <rect x="11.1" y="9.5" width="2.2" height="2" rx="0.3" fill="#2496ed" />
      <rect x="8.3" y="7" width="2.2" height="2" rx="0.3" fill="#2496ed" />
      <rect x="11.1" y="7" width="2.2" height="2" rx="0.3" fill="#2496ed" />
      <rect x="13.9" y="9.5" width="2.2" height="2" rx="0.3" fill="#2496ed" />
      <path
        d="M22.5 12.3c-.4-.3-1.3-.4-2-.2-.2-.8-.7-1.4-1.4-1.7l-.5-.2-.3.4c-.4.7-.6 1.5-.6 2.3 0 .2 0 .4.1.6-.6.3-1.6.4-2.7.4H2.5c-.3 1.1 0 2.4.6 3.4 1.3 2.1 3.7 3.2 6.5 3.2 5.5 0 9.8-3.3 11.2-7 .6-.1 1.4-.4 1.7-.8z"
        fill="#2496ed"
      />
    </svg>
  {:else if name === "ruby"}
    <!-- Ruby Logo -->
    <svg viewBox="0 0 24 24" width="100%" height="100%">
      <polygon points="6.5,3.5 17.5,3.5 22,8.5 12,21.5 2,8.5" fill="#cc342d" />
      <polygon points="6.5,3.5 12,8.5 2,8.5" fill="#e05a47" />
      <polygon points="17.5,3.5 12,8.5 22,8.5" fill="#a8251e" />
      <polygon points="6.5,3.5 12,3.5 12,8.5" fill="#f07662" />
      <polygon points="12,8.5 12,21.5 2,8.5" fill="#da4536" />
      <polygon points="12,8.5 12,21.5 22,8.5" fill="#931e18" />
    </svg>
  {:else if name === "php"}
    <!-- PHP Logo -->
    <svg viewBox="0 0 24 24" width="100%" height="100%">
      <ellipse cx="12" cy="12" rx="11" ry="7.5" fill="#777bb4" />
      <text
        x="12"
        y="14.5"
        font-size="8"
        font-weight="bold"
        font-family="system-ui, sans-serif"
        text-anchor="middle"
        fill="#fff"
        letter-spacing="0.5">php</text
      >
    </svg>
  {:else if name === "server"}
    <!-- Server Rack -->
    <svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <rect x="3" y="4" width="18" height="6" rx="2" />
      <rect x="3" y="14" width="18" height="6" rx="2" />
      <line x1="7" y1="7" x2="7.01" y2="7" stroke-width="2.5" />
      <line x1="7" y1="17" x2="7.01" y2="17" stroke-width="2.5" />
      <line x1="11" y1="7" x2="15" y2="7" />
      <line x1="11" y1="17" x2="15" y2="17" />
    </svg>
  {:else if name === "globe"}
    <!-- Globe -->
    <svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="12" r="9" />
      <line x1="3" y1="12" x2="21" y2="12" />
      <path d="M12 3a14 14 0 0 1 0 18" />
      <path d="M12 3a14 14 0 0 0 0 18" />
    </svg>
  {:else if name === "smartphone"}
    <!-- Smartphone / Mobile -->
    <svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <rect x="6" y="2" width="12" height="20" rx="3" />
      <line x1="11" y1="18" x2="13" y2="18" />
    </svg>
  {:else if name === "database"}
    <!-- Database -->
    <svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <ellipse cx="12" cy="5" rx="8" ry="3" />
      <path d="M4 5v6c0 1.66 3.58 3 8 3s8-1.34 8-3V5" />
      <path d="M4 11v6c0 1.66 3.58 3 8 3s8-1.34 8-3v-6" />
    </svg>
  {:else if name === "cloud"}
    <!-- Cloud -->
    <svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z" />
    </svg>
  {:else if name === "package"}
    <!-- Package / Library -->
    <svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M12 2l9 5.2v9.6L12 22 3 16.8V7.2L12 2z" />
      <line x1="12" y1="2" x2="12" y2="12" />
      <line x1="12" y1="12" x2="21" y2="7.2" />
      <line x1="12" y1="12" x2="3" y2="7.2" />
    </svg>
  {:else if name === "terminal"}
    <!-- Terminal -->
    <svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <polyline points="4 17 10 11 4 5" />
      <line x1="12" y1="19" x2="20" y2="19" />
    </svg>
  {:else if name === "git-branch"}
    <!-- Git Branch -->
    <svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <line x1="6" x2="6" y1="3" y2="15" />
      <circle cx="18" cy="6" r="3" />
      <circle cx="6" cy="18" r="3" />
      <path d="M18 9a9 9 0 0 1-9 9" />
    </svg>
  {:else}
    <!-- Generic folder / fallback -->
    <svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2z" />
    </svg>
  {/if}
</span>

<style>
  .tech-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    vertical-align: middle;
  }
  .tech-icon :global(svg) {
    display: block;
    width: 100%;
    height: 100%;
  }
</style>
