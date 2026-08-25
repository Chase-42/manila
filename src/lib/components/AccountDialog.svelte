<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import * as Select from "$lib/components/ui/select";
  import { Button } from "$lib/components/ui/button";
  import { createAccount, updateAccount } from "$lib/accounts";
  import { ACCOUNT_TYPES } from "$lib/types/account";
  import type { Account } from "$lib/types/account";

  let {
    account = undefined,
    open = $bindable(false),
    onsaved,
  }: {
    account?: Account;
    open?: boolean;
    onsaved: () => void;
  } = $props();

  const isEdit = $derived(account !== undefined);

  let name = $state("");
  let account_type = $state("depository");
  let subtype = $state("");
  let institution = $state("");
  let currency = $state("USD");

  let error = $state<string | null>(null);
  let saving = $state(false);

  function initFormFields(acc: Account | undefined) {
    if (!acc) {
      name = "";
      account_type = "depository";
      subtype = "";
      institution = "";
      currency = "USD";
      error = null;
      return;
    }
    name = acc.name;
    account_type = acc.account_type;
    subtype = acc.subtype;
    institution = acc.institution;
    currency = acc.currency;
    error = null;
  }

  // Populate form fields whenever the dialog opens, using the current account prop
  $effect(() => {
    if (open) initFormFields(account);
  });

  function errorMessage(e: unknown): string {
    return e instanceof Error ? e.message : String(e);
  }

  async function handleSubmit(e: Event) {
    e.preventDefault();
    error = null;
    saving = true;
    try {
      if (isEdit) {
        await updateAccount({ id: account!.id, name, account_type, subtype, institution });
      } else {
        await createAccount({ name, account_type, subtype, institution, currency });
      }
      open = false;
      onsaved();
    } catch (err) {
      error = errorMessage(err);
    } finally {
      saving = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>{isEdit ? "Edit account" : "New account"}</Dialog.Title>
    </Dialog.Header>

    <form onsubmit={handleSubmit} class="form">
      <div class="field">
        <Label for="name">Name</Label>
        <Input id="name" bind:value={name} placeholder="e.g. Chase Checking" required />
      </div>

      <div class="field">
        <Label for="type">Type</Label>
        <Select.Root type="single" bind:value={account_type}>
          <Select.Trigger id="type" class="w-full">
            {account_type}
          </Select.Trigger>
          <Select.Content>
            {#each ACCOUNT_TYPES as type}
              <Select.Item value={type}>{type}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>

      <div class="field">
        <Label for="subtype">Subtype</Label>
        <Input id="subtype" bind:value={subtype} placeholder="e.g. checking, credit card" />
      </div>

      <div class="field">
        <Label for="institution">Institution</Label>
        <Input id="institution" bind:value={institution} placeholder="e.g. Chase" />
      </div>

      <div class="field">
        <Label for="currency">Currency</Label>
        <Input
          id="currency"
          value={currency}
          placeholder="USD"
          disabled={isEdit}
          class={isEdit ? "opacity-50 cursor-not-allowed" : ""}
          onchange={(e) => {
            if (!isEdit) currency = (e.target as HTMLInputElement).value;
          }}
        />
        {#if isEdit}
          <p class="hint">Currency cannot be changed after creation.</p>
        {/if}
      </div>

      {#if error}
        <p class="error">{error}</p>
      {/if}

      <Dialog.Footer>
        <Button type="button" variant="ghost" onclick={() => (open = false)}>Cancel</Button>
        <Button type="submit" disabled={saving}>
          {saving ? "Saving..." : isEdit ? "Save changes" : "Create account"}
        </Button>
      </Dialog.Footer>
    </form>
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

  .hint {
    font-size: 11px;
    color: var(--faint);
    margin: 0;
  }

  .error {
    font-size: 12px;
    color: var(--destructive);
    margin: 0;
    font-family: var(--font-mono);
  }
</style>
