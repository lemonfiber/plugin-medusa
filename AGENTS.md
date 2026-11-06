# AGENTS.md — plugin-medusa

> **Start at the roadmap and board on [lemonfiber.app](https://lemonfiber.app),
> rendered from the report of where every unreleased version stands. Then the
> rules** every repository shares:
> [working in the repositories](https://github.com/lemonfiber/spec/blob/main/50-governance/working-in-the-repositories.md)
> and [the rules for agents](https://github.com/lemonfiber/spec/blob/main/50-governance/ai-contributors.md).
> This file holds only what is true of this repository.

## What this repo is

Medusa as a first-party lemonfiber plugin: `plugin.toml` and the adapter that
speaks the `indexer.pull@1` and `library.curate@1` contracts in front of Medusa,
the second curator of series, which stands in for Sonarr where `F14-R14` proves
`library.curate` by a substitute. The adapter is the Rust crate at the root,
built on `lemonfiber-adapter-kit` from the core at the commit `Cargo.toml` pins,
into the image the `Dockerfile` builds. `release.yml` publishes that image from
every commit on `main` and from the train's tag.

The adapter asks Medusa's API v2, the one Medusa's own interface drives, with the
key lemonfiber minted. Where that API has no answer, as for testing a download
client, it signs in to Medusa's own pages with the account lemonfiber minted and
asks the page Medusa's interface asks. Medusa holds one Usenet client and one
torrent client, named `nzb` and `torrent`; what it has in flight is read from its
history and the state its download handler reads from each client.

## The rules you cannot break

- **The plugin id is the service's id** (`ARCH-R266`). The container, the
  configuration directory, the settings and the journal keep their names.
- **Its proofs' recordings sit under `recordings/medusa/`**, and its starting
  files under `config/medusa/` (`ARCH-R271`).
- **Every service declares `reaches` and `asks_for`** (`ARCH-R270`).
- **The reader is spec's `first-party-plugin` workflow's,** and lemonfiber
  records the recordings itself. There is no Python here.
- **No `unwrap`, `expect`, `panic` or indexing** outside tests. The lints in
  `Cargo.toml` are the core's.
- **The adapter never writes its credentials.** Medusa takes neither its key nor
  its account from its environment, so lemonfiber mints all three and writes them
  into `config/medusa/config.ini` before Medusa first starts (`ARCH-R315`,
  `ARCH-R316`, `ARCH-R318`), and into the adapter's `upstream.json` as `api_key`,
  `username` and `password` (`ARCH-R317`); the adapter reads them for every call
  and never logs them.
- **Medusa never starts without its account.** A Medusa with no account on its
  own pages hands its key to anybody at `/getkey/` and answers its API to an
  empty sign-in, which the proof `medusa.keeps-its-key` holds the pinned image to.
- **Medusa pulls its indexers from Prowlarr and from NZBHydra2,** named in the
  servarr and nzbhydra2 APIs (`ARCH-R332`, `ARCH-R333`).
- **What Medusa cannot do is answered `unsupported`** (`ARCH-R334`).
- **The core's crates move with the train.** The `rev` in `Cargo.toml` is the
  core commit `targets.toml` names, the same for every `lemonfiber-*` crate.
- **Moving Medusa's digest** means recording every response again against the
  new image in the same change, moving `RELEASE` and `DIGEST` in
  `src/adapter.rs` with it, and committing the `proofs.json` the checks write.

## Checks

```
just ci
```

The gates that need nothing but this tree. The plugin verdict and the
recordings run in CI; `just recordings` takes the recordings again locally, with
Docker.
