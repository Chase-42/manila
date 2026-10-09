<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Button } from "$lib/components/ui/button";
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

  type Step = "setup" | "linking" | "exchanging" | "error";

  let step = $state<Step>("setup");
  let error = $state<string | null>(null);
  let urlError = $state<string | null>(null);
  let proxyUrl = $state("");
  let linkToken = $state<string | null>(null);

  function getOrCreateClientId(): string {
    const existing = localStorage.getItem("manila.plaidClientId");
    if (existing) return existing;
    const id = crypto.randomUUID();
    localStorage.setItem("manila.plaidClientId", id);
    return id;
  }

  function validateUrl(url: string): string | null {
    if (!url.trim()) return "Proxy URL is required.";
    if (!url.startsWith("http://") && !url.startsWith("https://")) {
      return "Proxy URL must start with http:// or https://";
    }
    return null;
  }

  function errorMsg(e: unknown): string {
    return e instanceof Error ? e.message : String(e);
  }

  async function fetchLinkToken(
    base: string,
    clientUserId: string,
  ): Promise<string> {
    const res = await fetch(`${base}/plaid/link-token`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        client_user_id: clientUserId,
        redirect_uri: "manila://plaid-oauth",
      }),
    });
    const data = (await res.json()) as { link_token?: string; error?: string };
    if (!res.ok || !data.link_token) {
      throw new Error(data.error ?? `Proxy returned ${res.status}`);
    }
    return data.link_token;
  }

  async function exchangeToken(
    base: string,
    publicToken: string,
  ): Promise<{ accessToken: string; itemId: string }> {
    const res = await fetch(`${base}/plaid/exchange`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ public_token: publicToken }),
    });
    const data = (await res.json()) as {
      access_token: string;
      item_id: string;
      error?: string;
    };
    if (!res.ok) throw new Error(data.error ?? `Exchange failed: ${res.status}`);
    return { accessToken: data.access_token, itemId: data.item_id };
  }

  $effect(() => {
    if (!open) return;
    step = "setup";
    error = null;
    urlError = null;
    linkToken = null;
    proxyUrl = localStorage.getItem("manila.plaidProxyUrl") ?? "";

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

    return () => {
      cancelled = true;
      unlisten?.();
    };
  });

  async function handleConnect() {
    urlError = validateUrl(proxyUrl);
    if (urlError) return;

    const base = proxyUrl.trim();
    localStorage.setItem("manila.plaidProxyUrl", base);
    step = "linking";
    error = null;

    try {
      const token = await fetchLinkToken(base, getOrCreateClientId());
      linkToken = token;
      await loadPlaidScript();
      openPlaidLink(token, null);
    } catch (e) {
      step = "error";
      error = errorMsg(e);
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
      const { accessToken, itemId } = await exchangeToken(
        proxyUrl.trim(),
        publicToken,
      );
      const params = extractConnectionParams(accessToken, itemId, metadata);
      await upsertPlaidConnection(params);
      open = false;
      onsuccess();
    } catch (e) {
      step = "error";
      error = errorMsg(e);
    }
  }

  function handlePlaidExit(
    err: { error_message: string } | null,
    _metadata: unknown,
  ) {
    if (err) {
      step = "error";
      error = err.error_message;
    } else {
      step = "setup";
    }
  }

  function reset() {
    step = "setup";
    error = null;
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Connect a bank</Dialog.Title>
    </Dialog.Header>

    {#if step === "setup"}
      <form
        class="form"
        onsubmit={(e) => {
          e.preventDefault();
          handleConnect();
        }}
      >
        <div class="field">
          <Label for="proxy-url">Proxy server URL</Label>
          <Input
            id="proxy-url"
            bind:value={proxyUrl}
            placeholder="https://your-proxy.onrender.com"
          />
          {#if urlError}
            <p class="field-error">{urlError}</p>
          {/if}
        </div>

        <Dialog.Footer>
          <Button type="button" variant="ghost" onclick={() => (open = false)}>
            Cancel
          </Button>
          <Button type="submit">Connect</Button>
        </Dialog.Footer>
      </form>
    {:else if step === "linking" || step === "exchanging"}
      <div class="status">
        <p class="status-msg">
          {step === "linking" ? "Opening Plaid Link…" : "Saving connection…"}
        </p>
      </div>
    {:else if step === "error"}
      <div class="form">
        <p class="error">{error}</p>
        <Dialog.Footer>
          <Button type="button" variant="ghost" onclick={() => (open = false)}>
            Cancel
          </Button>
          <Button onclick={reset}>Try again</Button>
        </Dialog.Footer>
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    margin-top: 4px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field-error {
    font-size: 11px;
    color: var(--destructive);
    margin: 0;
  }

  .error {
    font-size: 12px;
    color: var(--destructive);
    margin: 0;
    font-family: var(--font-mono);
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
