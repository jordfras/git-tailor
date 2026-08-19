# demo/promo/

The promo video: one directory per beat under [`scenes/`](scenes), narrated with
a local neural voice, scored, and composed into `demo/out/promo.mp4` by
[`scripts/compose.sh`](scripts/compose.sh).

Rendered with `demo/build.sh video` — see [`../README.md`](../README.md) for the
toolchain, the other subcommands, and how to add a second video.

## Changing something

**A narration line.** Edit `narration.txt`, run `demo/build.sh cues`, apply any
drift it reports to the `Sleep` before each cued command, re-run until clean,
then render. If the line is longer than what it replaced the scene may also need
its `SPEED` retuned — see Pacing.

**A word the narrator mispronounces.** Check what it is actually being asked to
say (`espeak-ng -v en-gb -q --ipa "word"`) before respelling; sometimes the
phonemes are already right and only rewording helps. Whatever you settle on,
**leave a `#` comment above it saying why** — several lines here are deliberately
misspelt or reworded, and they look like typos to anyone who does not know.

**A keystroke.** Tapes navigate absolutely (`g`, then a count of `Down`s) so
they survive an operation moving the selection. Row counts depend on the
fixture, so changing `make-repo.sh`'s commit list means re-deriving them.

**Adding a scene.** Make a directory under `scenes/` with the three files; the
numeric prefix sets the order, since scenes are composed in name order. Leave
gaps in the numbering if you expect to insert more. The fixture has to support
what the tape does — a scene that films an operation needs a branch shaped for
it, and `make-repo.sh` verifies its own end state, so extending it means
extending that check too.

**Iterating on one scene.** `demo/build.sh video promo 04-pitch` renders and composes
just that one, into `demo/out/promo-preview.mp4` — the full video keeps its own
filename. `TTS_ENGINE=flite` swaps the neural voice for a robotic one that needs
no model, which is the fast way to smoke-test a pipeline change (but useless for
timing, since the words land elsewhere).

## The video

An infomercial parody: exaggerate the pain, then relieve it — which is the
before/after the video needs anyway. Eight scenes, 2m20s, each rendering on its
own.

Three rules the scenes follow:

- **One branch, one story.** The video opens on the branch you *want*, cuts to
  the branch you *have*, and ends by turning the second into the first. The
  opening and the payoff are the same six commits, not two similar-looking
  takes.
- **git-tailor appears when it is introduced, not before.** Scenes 01–03 are
  plain git; the tool first appears in 04, after the manual grind has earned it.
- **Direct keys, never the operation picker**, and every beat is real. The
  narration says what is happening, so the screen does not also need a menu
  explaining itself, and `p` reads faster than `Space` → arrow → `Enter` while
  teaching the shortcut the viewer will actually use. The narrator can oversell;
  the terminal must not.

| # | Scene | Shows | Tool |
|---|-------|-------|------|
| 01 | dream | `git log` on `tidy` — six commits, each doing one thing | plain git |
| 02 | reality | `git log` on your branch, then `git show --stat` opening up the bundled commit | plain git |
| 03 | grind | real `rebase -i` → `edit` → `reset` → `add -p`. The **split only** | plain git |
| 04 | pitch | `p` split, `f` fixup, `f` fixup, `d` drop — then the result log, identical to 01 | git-tailor |
| 05 | bonus | the hunk group matrix | git-tailor |
| 06 | bonus | reorder (`m`) | git-tailor |
| 07 | bonus | undo / redo (`u`, `Ctrl-r`) | git-tailor |
| 08 | close | `cargo install`, and the releases page for people without Rust | plain git |

Scenes 01 and 04 print subjects only (`--format='%s'`), so the two logs match
character for character instead of differing by hash.

### What the split scene needs from the fixture

The commit the video splits must bundle two fixes belonging to two *different*
earlier commits — that is what makes split-then-fixup the repair rather than an
arbitrary demo, and it is what gives the matrix two columns to show. A bundled
`feat:` does not work: two halves of one feature have no separate homes, so the
payoff is "now there are two commits" rather than "now each change is where it
belongs".

It is split **per hunk group** rather than per file. Both give the same two
commits on this fixture, but only one of them is a thing git cannot already do,
and it is what the narration describes.

## Anatomy of a scene

Each directory under [`scenes/`](scenes) is one beat of the video and holds
everything that defines it:

| File | What it is |
|------|------------|
| `scene.tape` | the vhs capture |
| `narration.txt` | prose, rendered to speech |
| `scene.conf` | everything else about the scene — see below |

`scene.conf` is sourced shell, so every key is optional:

| Key | Default | What it does |
|-----|---------|--------------|
| `TITLE`, `SUBTITLE` | — | the card. Sentence case, no trailing period; the font size is derived from the length so a long one cannot run off the screen |
| `TITLE_HOLD` | `3.3` | seconds the card stays up. The bonus round wants ~2s bursts where the opening wants longer |
| `TITLE_AT` | `0.4` | when the card fades in. The closing sign-off sets this late, so it lands with the last line of narration rather than captioning the typing |
| `NARRATION_DELAY` | `0.8` | lead-in before the voice starts, so the picture lands first |
| `SFX` | `none` | `whoosh` or `ding` at the moment the scene arrives |
| `SFX_AT` | `0` | when the cue fires, in scene time. The bonus scenes nudge it off zero so the ding lands with the card rather than under the cross-fade |
| `GRAYSCALE` | `0` | the infomercial "problem" look. Applied before the card, so the card stays white |
| `SPEED` | `1` | fast-forward the capture. Needs no filter — the same frames over a shorter time is only a higher input rate |
| `LOGO` | — | image to spin in, repo-relative |
| `LOGO_AT` | `0.3` | when the spin starts, in scene time |
| `LOGO_HOLD` | `2.2` | how long it stays after landing |
| `STAMP` | — | an emoji to pop in over the picture |
| `STAMP_AT`, `STAMP_HOLD` | `1.0`, `2.5` | when, and for how long |
| `STAMP_SIZE` | `260` | px on its longest side |
| `STAMP_X`, `STAMP_Y` | right of center | overlay expressions. `W`/`H` are the frame and `w`/`h` are the stamp — mixing them up parks the picture in the corner |
| `MUSIC_DULL` | `0` | muffle and pitch the music down over this scene |
| `MARKERS` | — | boxes drawn around what the narration is naming, one per array element: `"AT HOLD X Y W H [SHAPE]"` — seconds, then the marked box in frame pixels, then `rect` (default) / `arrow-up` / `arrow-down` |

Where `MUSIC_DULL` is set, the bed is filtered *and* dropped two semitones, so
the music sags with the story rather than merely receding — muffling alone reads
as distant, not sad. Adjacent dulled scenes are merged into one stretch. The
amounts are constants in [`scripts/compose.sh`](scripts/compose.sh):
`MUSIC_DULL_PITCH` (0.8909, or 2^-2/12), `MUSIC_DULL_HZ`, `MUSIC_DULL_BASS` —
which puts back the weight the lowpass removes — and `MUSIC_DULL_GAIN`.

`MARKERS` exists because a tutorial names things the viewer then has to find:
"each column is one group of hunks" only helps if you know which column. The box
is the thing being marked, and a rounded rectangle is drawn clear of it by
`MARKER_GAP` — a constant in [`scripts/compose.sh`](scripts/compose.sh) — so a
marker reads as *around* rather than *on*. Always a rectangle: a ring suits a
target about as wide as it is tall, but stretched down a whole column it
degenerates into a sliver, and one shape for everything is one less thing to get
wrong.

Coordinates are frame pixels, and the way to get them is to pull the frame at
that timestamp and read them off it — **per scene**. The terminal grid does not
move, but which column a thing sits in does: the same green square is in
different columns in different scenes, which is exactly how a marker ends up one
column out. Check each against its own scene's frame.

Two things worth knowing before placing one. A marker fires on scene time, so
subtract the scene's start from any timestamp read off the finished video. And
it must wait for the tape to have drawn its subject — `gt` is typically not on
screen for the first few seconds, and a marker before that rings an empty
terminal.

The video opens on a channel ident for the fake shopping network the
infomercial belongs to, [`assets/hsn-bumper.svg`](assets/hsn-bumper.svg). It is
prepended in [`scripts/compose.sh`](scripts/compose.sh) — `BUMPER`,
`BUMPER_HOLD`, `BUMPER_FADE` — rather than being a scene, since it has no tape
and nothing to capture. It fades to `BUMPER_TO`, the terminal's own background
color, so the join into the first scene is invisible: the card dissolves and
the terminal is already there.

The logo spins in from small through three rotations, overshoots, snaps back and
lands with a smack — the cue is timed to `LOGO_AT + LOGO_SPIN`, so it follows
automatically if the spin is retimed. `LOGO_SPIN` is a `video.conf` key: `0`
gives a logo that simply appears, with no impact cue under it, which is what the
tutorial uses.

Stamps are rasterized by
[`scripts/make-emoji.py`](scripts/make-emoji.py), because ffmpeg cannot draw
them: color emoji are bitmap glyphs and `drawtext` rejects the font outright.
They are overlaid after any `GRAYSCALE`, so a stamp keeps its color on a
deliberately drab scene — which is usually what you want from a joke.

[`make-repo.sh`](make-repo.sh) builds the fixture: two branches from one set of
file states —
`feature/calc`, the branch the video cleans up, and `tidy`, what it should end
up as. The cleanup is verified to produce `tidy` exactly, same subjects and same
tree, which is what lets the video open on the result and land on it again.

The pieces:

- [`scripts/tts.sh`](scripts/tts.sh) — narration → WAV. `TTS_ENGINE=kokoro`
  (default) is a local neural voice; `TTS_ENGINE=flite` needs nothing beyond
  ffmpeg and is much faster, so it's the quick way to smoke-test a change to the
  pipeline. Every engine hides behind this one script.
- [`scripts/audition-voice.sh`](scripts/audition-voice.sh) — renders one line
  through several Kokoro voices, raw and processed, to compare by ear.
- [`scripts/broadcast-filter.sh`](scripts/broadcast-filter.sh) — the compression
  and EQ that make the narration sound like an announcer rather than a text
  reader. Shared with the audition script, so a voice test predicts the mix.
- [`scripts/make-audio-beds.sh`](scripts/make-audio-beds.sh) — synthesizes the
  music bed and the whoosh/ding/smack cues with ffmpeg, so nothing waits on
  licensed audio. Drop real files into `demo/promo/assets/` under the same base names
  (`music.*`, `whoosh.*`, `ding.*`, `smack.*`) and they win.
- [`scripts/compose.sh`](scripts/compose.sh) — title cards, cross-fades, and a
  music bed ducked under the voice. A scene lasts as long as whichever is
  longer, its capture or its narration; the shorter one is padded.
- [`scripts/tape-duration.sh`](scripts/tape-duration.sh) — how many seconds of
  visible action a tape scripts, and with `--positions` where each command
  falls. See Capture for why frame counts cannot answer this.
- [`scripts/cue-check.sh`](scripts/cue-check.sh) — whether each cued keystroke
  still lands where the narration puts it.
- [`scripts/make-emoji.py`](scripts/make-emoji.py) — one emoji to a transparent
  PNG, since ffmpeg cannot draw them itself.

## Narration

Narration files control their own pacing. Blank lines separate paragraphs, which
are synthesized one at a time; `[1.2]` alone on a line sets an exact gap,
`{speed=0.9}` changes the read speed from there on, and a `#` line is a comment.
Comments matter here because a narration file is a script for the ear: some
words are deliberately misspelt so they are said correctly, and without a note
the next person will "fix" the spelling and silently regress the audio.

```
There has to be a better way!

[0.9]
There is.
```

Kokoro pads every clip with silence of its own, which is what makes an unbroken
script sound uniformly machine-timed. Each paragraph is trimmed back to where
sound actually starts before the requested gap is inserted, so the pause written
is the pause heard, and gaps don't compound down a long script.

To align a tape to the voice rather than guess, ask where each line lands:

```bash
demo/build.sh shell
demo/promo/scripts/tts.sh --timings demo/promo/scenes/04-pitch/narration.txt
```

It prints each line's start time; add the scene's `NARRATION_DELAY` to get scene
time. Go through `tts.sh` rather than calling `kokoro_tts.py` directly — the
engine's own defaults are a different voice at a different speed, so a
hand-written invocation that forgets a flag reports times the finished video
will not have.

Keystrokes that must follow the voice are declared as cues in the tape — see
below — so this is for setting them, not for policing them afterwards.

The phonemizer language follows the voice (`bf_`/`bm_` → en-gb, otherwise
en-us). Feeding a British voice American phonemes does not merely sound
American, it sounds broken: en-us "tomorrow" is /təmˈɑːɹoʊ/, with a broad vowel
and a rhotic r a non-rhotic voice has no good way to say. When a word still
comes out wrong, check what it is actually being asked to say before reaching
for a phonetic respelling —

```bash
espeak-ng -v en-gb -q --ipa "prebuilt"    # pɹɪbˈɪlt  — the short vowel
espeak-ng -v en-gb -q --ipa "pre-built"   # pɹˈiːbˈɪlt — the fix
```

— since sometimes the phonemes are already right and the model simply renders
that vowel oddly, in which case rewording is the only cure.

## Cues

A `Sleep` encodes something the tape cannot otherwise say: that the keystroke
below it should land just after the narrator asks for it. Edit a word earlier in
the narration and every line after it moves, so the keystrokes slide out from
under the voice with nothing failing until someone watches the result. Tapes
therefore declare their cues:

```
#@cue 4 +0.06
Type "p"
```

— "this command should start 0.06s after narration line 4 begins", counting
spoken lines only (blank lines, `#` comments, `[0.9]` gaps and `{speed=…}` do
not count).

```bash
demo/build.sh cues              # every scene
demo/build.sh cues promo 04-pitch   # one of them
```

reports each cue's actual lead and its drift from the declared one, and exits
non-zero past 0.15s. It only synthesizes narration — no capture — so it is
seconds, not minutes, and is the thing to run after any narration edit.

The drift it prints is the correction: a uniform `-2.04` on every cue in a scene
means the first `Sleep` before them should lose 2.04s. The declared leads run
from 0.05s to 0.6s because a keystroke may land anywhere inside the sentence
that calls it; they record what the video was tuned with, so do not flatten
them.

Only three scenes have voice-coupled keystrokes (02, 04, 08). A scene with no
cues is skipped for free.

## Capture

The scene tapes pin the terminal to a navy theme (`Set Theme "Cobalt2"`). The
gray scenes get their look from desaturation, and a black-and-white terminal
desaturates to itself — the color is there to be taken away. git-tailor paints
its own `dark+` palette regardless, so this only touches the shell stretches,
which is where the gray scenes are.

Scenes are captured as **lossless PNG frame sequences**, not vhs MP4 or GIF, so
there are no palette or chroma artefacts and the video is encoded exactly once.
The frames land on the cache volume rather than in `demo/out/` (a few hundred MB
per render), so `demo/build.sh clean` clears them with the rest of the volume.

How long a scene runs comes from
[`scripts/tape-duration.sh`](scripts/tape-duration.sh), which reads it off the
tape — **not** from the number of frames captured. At 1080p vhs samples the
terminal more slowly than the frame rate it asks for, and how much more slowly
depends on machine load: the same tape has produced anywhere from 308 to 673
frames here. Encoding those at the nominal rate would make the video play too
fast by a factor that changes every run (vhs's own MP4 output of a 22.8s scene
came out 7.5s). Spreading whatever frames arrived across the tape's scripted
duration makes the result depend on the tape alone — fewer frames just means
slightly less smooth motion.

git-tailor is compiled into a Docker **cache volume** (not the host's `./target`)
so repeat renders reuse crate builds and the host tree is left untouched.

## Pacing

The gray "problem" stretch (02 + 03) earns the payoff, but every second of it
is a second before the viewer has seen the product. Keep it as short as the
jokes allow.

A scene lasts `max(tape ÷ SPEED, NARRATION_DELAY + voice + TAIL)`, which means
past a certain point **the narration, not the tape, sets the length** — winding
`SPEED` on further buys nothing, and leaving it short of that point strands the
tail of the scene in silence. `SPEED` therefore has to be retuned whenever a
script changes. It also means the only remaining lever is the script itself,
which is why the gray scenes were tightened by trimming the gaps *between* lines
rather than by speeding the capture alone.

To shorten a scene, reach for the levers in this order — the figures are what
each was worth on scene 03, and the ranking has held wherever it has been tried:

1. **The `[n]` gaps between lines** (~3.7s). Nothing is lost; an infomercial
   read wants less air than a default anyway.
2. **A sentence that repeats a beat** (~1.4s). Cheaper than it sounds: a joke
   landed twice is a joke landed once, slower.
3. **`{speed=…}` on the whole scene** (~1.0s). The only lever with a cost — the
   narrator is audibly more clipped — so it is the first to drop if it grates,
   and it is worth least. Leave the emphatic closing line at its own speed.

`SPEED` on the capture is not on that list because it is not a lever once the
narration sets the length; it only has to be retuned to *follow* the script.

The same floor means a tape *shorter* than its narration is free. Scene 02 types
at 60ms rather than 90ms for exactly that reason: the time comes back as two
seconds of looking at the log instead of watching it appear.

## Traps and conventions

- **Navigation is absolute** — `g` to the top, then down. Every operation
  reloads the list and may move the selection, so counting from a known edge is
  what keeps a tape reproducible. The fixup picker in particular opens on the
  *source* commit rather than the row above it, and getting that off by one
  still produces a correct-*looking* branch — same subjects, same final tree —
  because every change is still present, just attached to the wrong commit. Only
  a per-commit comparison catches it.
- **No conflicts anywhere.** The fixture is built so the WIP commit is the only
  thing touching `lexer.rs` after the bundled fix, which is what makes dropping
  it clean.
- **Cards are outlined, not boxed.** White glyphs with a black outline scaled to
  the font size, so they stay readable over the TUI's bright separator bar and
  green header without a slab across the picture. Sentence case, no trailing
  period — all-caps reads as shouting and no-caps as unfinished. Title text goes
  through `drawtext`'s `textfile=` and never `text=`, so a colon or comma in a
  title cannot break the filter graph.
- **`amix` ends with its shortest input** once two inputs share an `asplit`
  ancestor, whatever `duration=longest` claims. The voice bus is padded to full
  length before the split, which also stops `sidechaincompress` cutting the
  music off at the last spoken word.
