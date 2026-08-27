<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { page } from "$app/stores";
  import { LayoutDashboard, ArrowLeftRight, Wallet, Tag } from '@lucide/svelte';
  import type { Component } from 'svelte';

  let { children } = $props();

  let dbError = $state<string | null>(null);
  let dbReady = $state(false);

  onMount(async () => {
    if (import.meta.env.DEV) {
      await import('@wdio/tauri-plugin');
    }
    try {
      await invoke("init_db");
      dbReady = true;
    } catch (e) {
      dbError = e instanceof Error ? e.message : String(e);
    }
  });

  interface NavLink {
    href: string;
    label: string;
    Icon: Component<{ size?: number }>;
  }

  const navLinks: NavLink[] = [
    { href: "/budget", label: "Budget", Icon: LayoutDashboard },
    { href: "/transactions", label: "Transactions", Icon: ArrowLeftRight },
    { href: "/accounts", label: "Accounts", Icon: Wallet },
    { href: "/categories", label: "Categories", Icon: Tag },
  ];
</script>

{#if dbError}
  <div class="db-error">
    <h1>Database error</h1>
    <p>{dbError}</p>
    <p class="hint">Restart the app. If this persists, check that the app data directory is writable.</p>
  </div>
{:else if dbReady}
  <div class="shell">
    <nav class="sidebar">
      <div class="wordmark">manila</div>
      <ul class="nav">
        {#each navLinks as { href, label, Icon } (href)}
          <li>
            <a
              href={href}
              class="nav-item"
              class:active={$page.url.pathname.startsWith(href)}
            >
              <Icon size={16} />
              <span>{label}</span>
            </a>
          </li>
        {/each}
      </ul>
    </nav>
    <main class="content">
      {@render children()}
    </main>
  </div>
{/if}

<style>
  .shell {
    display: flex;
    height: 100vh;
    overflow: hidden;
  }

  .sidebar {
    width: 240px;
    min-width: 240px;
    background: var(--sidebar);
    border-right: 1px solid var(--sidebar-border);
    display: flex;
    flex-direction: column;
  }

  .wordmark {
    padding: 20px 20px 16px;
    font-size: 22px;
    font-weight: 700;
    color: var(--sidebar-primary);
    letter-spacing: 0.06em;
    border-bottom: 1px solid var(--sidebar-border);
  }

  .nav {
    list-style: none;
    margin: 0;
    padding: 12px 0;
    flex: 1;
    display: flex;
    flex-direction: column;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 20px;
    color: var(--sidebar-foreground);
    font-size: 13px;
    font-weight: 500;
    text-decoration: none;
    letter-spacing: 0.02em;
    border-left: 2px solid transparent;
  }

  .nav-item:hover {
    color: var(--foreground);
    background: color-mix(in srgb, var(--sidebar-accent) 40%, transparent);
  }

  .nav-item.active {
    color: var(--sidebar-primary);
    border-left-color: var(--sidebar-primary);
    background: var(--sidebar-accent);
  }

  .content {
    flex: 1;
    overflow: auto;
    display: flex;
    flex-direction: column;
  }

  .db-error {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100vh;
    gap: 12px;
    color: var(--text);
    font-family: var(--font-display);
  }

  .db-error h1 {
    color: var(--destructive);
    font-size: 18px;
    margin: 0;
  }

  .db-error p {
    margin: 0;
    color: var(--muted-foreground);
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .db-error .hint {
    color: var(--faint);
    font-family: var(--font-display);
  }
</style>
