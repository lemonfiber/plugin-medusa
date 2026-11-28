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
`medusa:thetvdb-lookup` set to `tvmaze`, the adapter asks
[TVmaze](https://www.tvmaze.com)'s public API, which takes no key, for the name,
year and seasons of a series Medusa does not hold, by its TheTVDB id; set to
`on`, it asks TheTVDB itself, under your own TheTVDB API key. Either way that
series can be requested too. It never uses the key Medusa ships with.

TVmaze numbers seasons as its own catalogue does. Where that differs from
TheTVDB's, which Medusa follows, a household asks for the seasons TVmaze names:
TVmaze has no season 0, so specials cannot be asked for, and a series TVmaze
numbers by year, as it numbers many daily shows and long-running anime, offers
seasons Medusa does not hold. A series TVmaze does not know cannot be requested
with it.

## How it tells lemonfiber what happened

Asked by lemonfiber's `events`, the adapter tells it each episode Medusa imports
and each grab or import that fails, naming the series by its TheTVDB id and the
episode as Medusa's history names it. It never names a member or an account.

Medusa's notifiers post only to fixed services' addresses, so Medusa never
pushes to the adapter. The adapter reads Medusa's history every interval
lemonfiber asks (`LEMONFIBER_EVENTS_POLL`, 60 seconds where unset), and tells
what follows the newest row it already told: a `Downloaded` row as `imported`
and a `Failed` row as `failed`. Medusa's history names no reason for a failure,
so `failed` carries none, and Medusa's API names no problem with itself, so the
adapter sends no `health`.

- **The key it tells lemonfiber what happened with**, which lemonfiber mints and
  writes to `config/medusa-adapter/events.key` each time it wires the stack.
- **Its cursor**, in `config/medusa-adapter/events.json`: where lemonfiber is
  told to send, and the id of the newest history row already told.

The first ask sends nothing older than that moment, and a stop drops the cursor.

## Settings

| Setting | Default | What it decides |
| --- | --- | --- |
| `medusa:import-stuck-after` | `60` | The minutes a finished download may wait for Medusa to import it before the adapter reports it stuck. `0` reports none |
| `medusa:thetvdb-lookup` | `off` | Where the adapter looks up a series Medusa does not hold, by its TheTVDB id: `off` nowhere, `tvmaze` on TVmaze with no key, `on` on TheTVDB under your key, or `on-with-pin` on TheTVDB under your key and your subscriber PIN |
| `medusa:thetvdb-key` | none | Your TheTVDB API key, asked for while the lookup is `on` or `on-with-pin` |
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

The lookup is the one request the adapter makes beyond this machine, and
lemonfiber lists it with what the stack's services reach. It is off until you
switch it on, and while it is off nothing goes to TVmaze or TheTVDB. Each time
the request service looks up a series Medusa does not hold, the adapter, set to
`tvmaze`, asks `api.tvmaze.com` which show has that TheTVDB id and reads that
show and its episodes, sending no key, no account and nothing else about you or
your library; it follows no redirect, reads only a show at `api.tvmaze.com`, and
asks again after two seconds, at most twice, while TVmaze answers that it is
asked too often. Set to `on` or `on-with-pin`, it signs in to `api4.thetvdb.com`
with your key, and your PIN where your key needs one, and reads that series'
episodes. Switching it off again means a series Medusa does not hold cannot be
requested.

The series information TVmaze answers is TVmaze's, under
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/), credited to
[TVmaze](https://www.tvmaze.com).

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
