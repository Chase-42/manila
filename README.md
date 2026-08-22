# manila

A zero-knowledge personal finance app. Envelope budgeting, transaction import, and sync across devices - where the sync server never sees your data in plaintext.

All crypto and storage live in Rust. The frontend only ever receives decrypted rows.

## Stack

- **Frontend**: SvelteKit 2, Svelte 5, TypeScript, Tailwind v4
- **Desktop**: Tauri v2 (macOS, Windows, Linux)
- **Storage**: SQLite via rusqlite (bundled), all access through Rust commands
- **Crypto**: RustCrypto only - no hand-rolled primitives, no sodiumoxide

## Development

```sh
pnpm install
pnpm tauri dev
```

Requires [Rust](https://rustup.rs/) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your platform.

## Commands

| Command | Description |
|---|---|
| `pnpm dev` | Frontend only (http://localhost:1420) |
| `pnpm tauri dev` | Full desktop app |
| `pnpm check` | TypeScript typecheck |
| `pnpm test` | Vitest unit tests |
| `cargo test` | Rust unit tests (run from `src-tauri/`) |
| `pnpm build` | Production frontend build |
| `pnpm tauri build` | Signed desktop bundle |
