# Medusa for lemonfiber

**Watches for new episodes and fetches them.**
Medusa is a curator of series lemonfiber can run in place of Sonarr.
Without it: Find and download episodes yourself.

## How lemonfiber carries it

lemonfiber installs Medusa with the adapter this repository holds, which speaks
lemonfiber's `indexer.pull` and `library.curate` contracts in front of Medusa: a
Rust crate built on the core's adapter kit, into the image
`ghcr.io/lemonfiber/medusa-adapter`, published from every commit on `main` and
from each tag the release train cuts. It tells Medusa which Prowlarr or
NZBHydra2 to take its indexers from, brings Medusa level with what Prowlarr holds
each time lemonfiber seeds, and registers and reads its download clients, root
folders, quality presets, series and what it has in flight.
lemonfiber reaches Medusa through this adapter alone. lemonfiber mints Medusa's
key and the name and password of its own pages, writes them into Medusa's
settings before it first starts, and writes them into the adapter's
`upstream.json`.

## Sign in to Medusa

lemonfiber mints the name and password of Medusa's own pages.
`lemonfiber credentials` lists them, and
`lemonfiber credentials --reveal medusa:web-username` and
`--reveal medusa:web-password` print them, asking first.

## Requests

The request gate asks this adapter for a household's requests under a key the
core writes for it as `request.key` beside the adapter's own. That key opens the
request operations of `library.curate` and `quality_profiles`, `root_folders` and
`lookup`, and nothing else.

Medusa looks up a series it does not hold by name alone, never by its TheTVDB
id, so by default `catalogued` answers only the series Medusa holds, and a
household's request reaches Medusa only for a series it already holds. With
`medusa:thetvdb-lookup` on, the adapter asks TheTVDB itself for the name, year
and seasons of a series Medusa does not hold, under your own TheTVDB API key,
and that series can be requested too. It never uses the key Medusa ships with.

## Settings

| Setting | Default | What it decides |
| --- | --- | --- |
| `medusa:import-stuck-after` | `60` | The minutes a finished download may wait for Medusa to import it before the adapter reports it stuck. `0` reports none |
| `medusa:thetvdb-lookup` | `off` | Whether the adapter asks TheTVDB for a series Medusa does not hold: `off`, `on` under your key, or `on-with-pin` under your key and your subscriber PIN |
| `medusa:thetvdb-key` | none | Your TheTVDB API key, asked for while the lookup is on |
| `medusa:thetvdb-pin` | none | Your TheTVDB subscriber PIN, asked for while the lookup is `on-with-pin` |

`lemonfiber config set medusa:import-stuck-after <minutes>` changes it. A stuck
download is one `lemonfiber stuck` names, and `downloading` answers it as a
warning. The clock starts when the adapter first reads the download as finished,
and starts again when the adapter restarts. A shorter wait names a stuck import
sooner, and may name one a slow disk or a long unpack was about to finish; a
longer one leaves a download Medusa will never import waiting longer before it
is named. The default is twelve passes of Medusa's download handler, which the
adapter runs every five minutes: every import the rig saw began in the same pass
that read its client as finished.

The TheTVDB lookup is the one request the adapter makes beyond this machine, and
lemonfiber lists it with what the stack's services reach. It is off until you
switch it on, and while it is off nothing goes to TheTVDB. Switched on, it signs
in to `api4.thetvdb.com` with your key, and your PIN where your key needs one,
and reads one series' episodes each time the request service looks up a series
Medusa does not hold. Switching it off again means a series Medusa does not hold
cannot be requested.

## What Medusa cannot do

- **A qBittorrent torrent holding one file inside a folder never imports.**
  qBittorrent names that file as the torrent's content, Medusa looks for it in the
  save folder itself, and the import waits until it is reported stuck. A single
  file, a folder of several files, and every shape on Transmission and Deluge
  import.
- **Medusa keeps no download's progress.** `downloading` answers what is left as
  the whole size until the client finishes, and no expected completion.

## What is here

| Path | What it is |
| --- | --- |
| `plugin.toml` | The part's manifest |
| `config/medusa/` | Where Medusa's starting files go |
| `recordings/medusa/` | The responses its proofs are held to |
| `src/` | The adapter |
| `conformance/indexer.pull@1/` | The adapter's answer to each conformance case, in front of the pinned Medusa |
| `conformance/library.curate@1/` | The adapter's answer to each conformance case, in front of the pinned Medusa |
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
