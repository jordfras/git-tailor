# demo/promo/assets/

Audio for the promo video. `compose.sh` prefers anything here over the
placeholders `make-audio-beds.sh` synthesises, matching on base name, so
replacing a sound means dropping a file in — no code change. Everything is now
supplied here; the synthesised beds survive only as a fallback.

## Provenance

| File | Source | Downloaded as | Licence |
|------|--------|---------------|---------|
| `music.mp3` | [rhavinga — *regal bank infomercial*](https://freesound.org/people/rhavinga/sounds/100262/) | `100262__rhavinga__regal-bank-infomercial.mp3` | CC0 1.0 |
| `whoosh.mp3` | [oMzig — *Deep Whoosh Speed Ramp*](https://freesound.org/people/oMzig/sounds/789075/) | `789075__omzig__deep-whoosh-speed-ramp.mp3` | CC0 1.0 |
| `ding.wav` | [Sadiquecat — *Pop in sfx*](https://freesound.org/people/Sadiquecat/sounds/824189/) | `824189__sadiquecat__pop-in-sfx.wav` | CC0 1.0 |
| `smack.ogg` | Kenney [*Impact Sounds*](https://kenney.nl/assets/impact-sounds) | `impactPunch_heavy_001.ogg` | CC0 1.0 |

Files are exactly as the artists uploaded them — no trimming, no levelling. All
of that happens at compose time, so the originals stay replaceable. They are
renamed on the way in, because `compose.sh` finds them by base name; the column
above keeps the link back to the upload, which is otherwise lost.

The Freesound entries are the **original** downloads, not the previews the site
serves without a login. Those previews are re-encodes: the music one runs 60ms
longer than the master, which is enough to throw off loop points derived from
it. If you re-download, take the original.

Credit is not required by any of these licences. It is recorded anyway: knowing
where a file came from is worth having whether or not a licence compels it.

## The music is looped, and where

The track ends with a sung jingle for someone else's bank, which would be quite
a thing to leave in. `compose.sh` therefore loops a section of it —
`MUSIC_LOOP_START`/`MUSIC_LOOP_END`, currently 5.410s to 26.959s. That is eight
bars at 89.1bpm, measured off the track's own downbeat grid rather than picked
by eye, and it stops five seconds clear of the vocal. The audio just past the
end is crossfaded back over the beginning, so the wrap does not click.

Two things to know if you re-derive those bounds. Take them from *this* file,
not from a Freesound preview: the preview is a re-encode and runs 60ms longer,
which was enough to put the bounds half a beat out. And a tempo detector will
happily report 178bpm here — double time, locking onto the eighth-note pulse —
which halves every bar it then proposes.

## Why CC0 specifically, and not merely "no attribution required"

These files are committed to a public repository, which is redistribution. CC0
is a public-domain dedication and permits that without qualification.

Several sites advertising "no attribution required" do **not**. Pixabay's
licence waives attribution but states *"You cannot sell or distribute Content
(either in digital or physical form) on a Standalone basis"* — and a file in a
git repo is exactly that. Fine for a video, wrong for a repo. Look for a
public-domain dedication, not just the absence of an attribution clause.

Genuinely CC0 sources: [Freesound](https://freesound.org) filtered to CC0,
[Kenney](https://kenney.nl/assets/category:Audio), and
[OpenGameArt's CC0 collections](https://opengameart.org/content/cc0-music).
