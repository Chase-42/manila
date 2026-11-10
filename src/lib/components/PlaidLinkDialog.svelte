<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { upsertPlaidConnection } from "$lib/plaid";
  import {
    loadPlaidScript,
    extractConnectionParams,
  } from "$lib/plaidLink";
  import type { PlaidSuccessMetadata } from "$lib/plaidLink";

  let {
    open = $bindable(false),
    onsuccess,
  }: {
    open?: boolean;
    onsuccess: () => void;
  } = $props();

  type Step = "linking" | "exchanging" | "error";

  let step = $state<Step>("linking");
  let error = $state<string | null>(null);
  let subscriptionRequired = $state(false);
  let linkToken = $state<string | null>(null);

  function getOrCreateClientId(): string {
    const existing = localStorage.getItem("manila.plaidClientId");
    if (existing) return existing;
    const id = crypto.randomUUID();
    localStorage.setItem("manila.plaidClientId", id);
    return id;
  }

  function errorMsg(e: unknown): string {
    return e instanceof Error ? e.message : String(e);
  }

  function isSubscriptionError(msg: string): boolean {
    return msg.toLowerCase().includes("subscription required");
  }

  $effect(() => {
    if (!open) return;
    step = "linking";
    error = null;
    subscriptionRequired = false;
    linkToken = null;

    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<{ url: string }>("plaid-oauth-redirect", async (event) => {
      const { getAllWebviewWindows } = await import(
        "@tauri-apps/api/webviewWindow"
      );
      const windows = await getAllWebviewWindows();
      for (const win of windows) {
        if (win.label !== "main") await win.close().catch(() => {});
      }
      if (linkToken) openPlaidLink(linkToken, event.payload.url);
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    startLink();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  });

  async function startLink() {
    try {
      const token = await invoke<string>("get_plaid_link_token", {
        clientUserId: getOrCreateClientId(),
        redirectUri: "manila://plaid-oauth",
      });
      linkToken = token;
      await loadPlaidScript();
      openPlaidLink(token, null);
    } catch (e) {
      const msg = errorMsg(e);
      step = "error";
      error = msg;
      subscriptionRequired = isSubscriptionError(msg);
    }
  }

  function openPlaidLink(token: string, receivedRedirectUri: string | null) {
    const handler = window.Plaid!.create({
      token,
      ...(receivedRedirectUri ? { receivedRedirectUri } : {}),
      onSuccess: handlePlaidSuccess,
      onExit: handlePlaidExit,
    });
    handler.open();
  }

  async function handlePlaidSuccess(
    publicToken: string,
    metadata: PlaidSuccessMetadata,
  ) {
    step = "exchanging";
    try {
      const result = await invoke<{ access_token: string; item_id: string }>(
        "exchange_plaid_token",
        { publicToken },
      );
      const params = extractConnectionParams(
        result.access_token,
        result.item_id,
        metadata,
      );
      await upsertPlaidConnection(params);
      open = false;
      onsuccess();
    } catch (e) {
      const msg = errorMsg(e);
      step = "error";
      error = msg;
      subscriptionRequired = isSubscriptionError(msg);
    }
  }

  function handlePlaidExit(
    err: { error_message: string } | null,
    _metadata: unknown,
  ) {
    if (err) {
      step = "error";
      error = err.error_message;
      subscriptionRequired = false;
    } else {
      open = false;
    }
  }

  function retry() {
    step = "linking";
    error = null;
    subscriptionRequired = false;
    linkToken = null;
    startLink();
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Connect a bank</Dialog.Title>
    </Dialog.Header>

    {#if step === "linking" || step === "exchanging"}
      <div class="status">
        <p class="status-msg">
          {step === "linking" ? "Opening Plaid Link…" : "Saving connection…"}
        </p>
      </div>
    {:else if step === "error"}
      <div class="error-section">
        {#if subscriptionRequired}
          <div class="sub-callout">
            <p class="sub-title">Server error</p>
            <p class="sub-body">
              Unable to reach Manila's server. Check your connection and try again.
            </p>
          </div>
        {:else}
          <p class="error">{error}</p>
        {/if}
        <Dialog.Footer>
          <Button type="button" variant="ghost" onclick={() => (open = false)}>
            Close
          </Button>
          <Button onclick={retry}>Try again</Button>
        </Dialog.Footer>
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>

<style>
  .error-section {
    display: flex;
    flex-direction: column;
    gap: 16px;
    margin-top: 4px;
  }

  .error {
    font-size: 12px;
    color: var(--destructive);
    margin: 0;
    font-family: var(--font-mono);
  }

  .sub-callout {
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .sub-title {
    font-size: 13px;
    font-weight: 600;
    margin: 0;
    color: var(--foreground);
  }

  .sub-body {
    font-size: 12px;
    color: var(--muted-foreground);
    margin: 0;
    line-height: 1.5;
  }

  .status {
    padding: 24px 0 8px;
    text-align: center;
  }

  .status-msg {
    font-size: 13px;
    color: var(--muted-foreground);
    margin: 0;
  }
</style>
