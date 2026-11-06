# Medusa for lemonfiber

**Watches for new episodes and fetches them.**
Medusa is a curator of series.
Without it: Find and download episodes yourself.

## How lemonfiber carries it

This repository holds Medusa's manifest and the adapter that speaks
lemonfiber's `indexer.pull` and `library.curate` contracts in front of Medusa: a
Rust crate built on the core's adapter kit, into the image
`ghcr.io/lemonfiber/medusa-adapter`, published from every commit on `main` and
from each tag the release train cuts. The adapter tells Medusa which Prowlarr or
NZBHydra2 to take its indexers from, and registers and reads its download
clients, root folders, quality presets, series and what it has in flight.
lemonfiber mints Medusa's key and the name and password of its own pages, and
writes them into Medusa's settings before it first starts.

## What is here

| Path | What it is |
| --- | --- |
| `plugin.toml` | The part's manifest |
| `config/medusa/` | Where Medusa's starting files go |
| `recordings/medusa/` | The responses its proofs are held to |
| `src/` | The adapter |
| `Dockerfile` | The adapter's image |
| `targets.toml` | The lemonfiber its checks run against, and the core the adapter is built on |
| `proofs.json` | What those checks said, committed |

## Checks

The plugin verdict is one job of spec's
[`first-party-plugin`](https://github.com/lemonfiber/spec/blob/main/.github/workflows/first-party-plugin.yml)
workflow: the manifest as the lemonfiber `targets.toml` names reads it, every
proof against its recording, and every pinned digest. The adapter is held to its
format, its lints, its tests, full line coverage and its dependency policy, its
image runs as lemonfiber runs it, and `recordings` takes every recording again
against the pinned Medusa. `just ci` runs the gates that need nothing but this
tree.

## Licence

The code and data in this repository are under the licence in `LICENSE`.
Medusa itself is under GPL-3.0-only and is not distributed here: this
repository names an image, it does not contain one.
