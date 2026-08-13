# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Kokoro backend for demo/promo/scripts/tts.sh. Kept in the repo rather than baked
# into the toolchain image so the narration voice can be tuned without an image
# rebuild; the image only carries the (large, immutable) venv and model files.
#
# Narration format
# ----------------
# Blank-line-separated paragraphs are synthesised separately and rejoined with
# a pause of your choosing. Hard-wrapped lines inside a paragraph are one
# sentence, as they read on the page. Two directives, each alone on a line:
#
#     [1.2]           pause this many seconds here, instead of the default
#     {speed=0.9}     speed for everything after it
#     # ...           a comment, not spoken
#
# The comment syntax earns its keep because a narration file is a script for the
# ear, not prose: some words are deliberately misspelt to be said correctly, and
# without a note the next person will "fix" the spelling and silently regress
# the audio.
#
# Kokoro has no prosody controls, and handing it a whole script at once gives
# every sentence break the same mechanical length — the giveaway that a machine
# is reading. Pauses are timing, though, not prosody: synthesising paragraph by
# paragraph, trimming the silence Kokoro pads each one with, and inserting an
# exact gap puts the pacing entirely under the script's control.

import argparse
import pathlib
import re
import sys

KOKORO_HOME = pathlib.Path("/opt/tts")

# Kokoro leaves a little silence at both ends of every clip. Left in place it
# would add to whatever pause the script asks for — and compound paragraph by
# paragraph — so each segment is trimmed back to where sound actually starts.
SILENCE_FLOOR = 0.01
KEEP_MS = 15


def lang_for(voice):
    """Phonemizer language matching a Kokoro voice.

    Kokoro names voices `<accent><gender>_name`, and the phonemes it is fed
    have to match the accent it was trained on. Handing a British voice
    American phonemes does not merely sound American — it sounds broken:
    en-us "tomorrow" is /təmˈɑːɹoʊ/, with a broad vowel and a rhotic r that a
    non-rhotic voice has no good way to say, and the same goes for every
    -ar- and -o- word in the script.
    """
    return "en-gb" if voice.startswith(("bf_", "bm_")) else "en-us"


def parse(text):
    """Split narration into ("speak", str) and ("pause", seconds) steps."""
    steps = []
    speed = None
    paragraph = []

    def flush():
        if paragraph:
            steps.append(("speak", " ".join(paragraph), speed))
            paragraph.clear()

    for raw in text.splitlines():
        line = raw.strip()
        if not line:
            flush()
            continue
        if line.startswith("#"):
            continue
        pause = re.fullmatch(r"\[\s*([0-9]*\.?[0-9]+)\s*]", line)
        if pause:
            flush()
            steps.append(("pause", float(pause.group(1)), None))
            continue
        rate = re.fullmatch(r"\{\s*speed\s*=\s*([0-9]*\.?[0-9]+)\s*}", line)
        if rate:
            flush()
            speed = float(rate.group(1))
            continue
        paragraph.append(line)
    flush()
    return steps


def trim(samples, np):
    loud = np.nonzero(np.abs(samples) > SILENCE_FLOOR)[0]
    if loud.size == 0:
        return samples[:0]
    return samples[loud[0] : loud[-1] + 1]


def main() -> int:
    ap = argparse.ArgumentParser(description="Render narration text to a WAV file.")
    ap.add_argument("--text-file", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--voice", default="af_heart")
    ap.add_argument("--speed", type=float, default=0.95)
    ap.add_argument(
        "--lang",
        help="phonemizer language; derived from the voice when omitted",
    )
    ap.add_argument(
        "--timings",
        action="store_true",
        help="print each line's start time, for aligning a tape to the voice",
    )
    ap.add_argument(
        "--pause",
        type=float,
        default=0.45,
        help="seconds between paragraphs with no explicit [n] marker",
    )
    args = ap.parse_args()

    import numpy as np
    import soundfile as sf
    from kokoro_onnx import Kokoro

    steps = parse(pathlib.Path(args.text_file).read_text())
    if not any(kind == "speak" for kind, _, _ in steps):
        print(f"kokoro: {args.text_file} has nothing to say", file=sys.stderr)
        return 1

    kokoro = Kokoro(
        str(KOKORO_HOME / "models" / "kokoro-v1.0.onnx"),
        str(KOKORO_HOME / "models" / "voices-v1.0.bin"),
    )

    lang = args.lang or lang_for(args.voice)
    pieces = []
    rate = None
    pending = None  # an explicit [n] overrides the default gap that follows it
    for kind, value, speed in steps:
        if kind == "pause":
            pending = value
            continue
        samples, rate = kokoro.create(
            value, voice=args.voice, speed=speed or args.speed, lang=lang
        )
        samples = trim(np.asarray(samples), np)
        if pieces:
            gap = args.pause if pending is None else pending
            pieces.append(np.zeros(int(gap * rate), dtype=samples.dtype))
        if args.timings:
            start = (sum(len(p) for p in pieces) / rate) + KEEP_MS / 1000
            print(f"{start:7.2f}  {value}")
        pieces.append(samples)
        pending = None

    # A breath of silence at each end so the mix has something to fade against.
    edge = np.zeros(int(KEEP_MS / 1000 * rate), dtype=pieces[0].dtype)
    sf.write(args.out, np.concatenate([edge, *pieces, edge]), rate)
    return 0


if __name__ == "__main__":
    sys.exit(main())
