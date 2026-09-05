# syntax=docker/dockerfile:1

# Toolchain image for regenerating the git-tailor demo GIF and promo video.
#
# It is built on charmbracelet's official vhs image, which already bundles a
# headless-Chromium + ttyd + ffmpeg stack that is known to work inside a
# container. On top of that we add a Rust toolchain so git-tailor itself can be
# built and driven by the tape scripts. Keeping every tool here means the
# artifacts can be regenerated on any machine with only Docker installed.
FROM ghcr.io/charmbracelet/vhs:latest

# We always build against the latest stable Rust (rebuild the image to pick up
# newer releases). The C toolchain, cmake and openssl headers are what the
# git2/libgit2-sys and openssl-sys build scripts need to compile.
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        libssl-dev \
        cmake \
        git \
        vim \
        curl \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --default-toolchain stable --profile minimal \
    && chmod -R a+w "$RUSTUP_HOME" "$CARGO_HOME" \
    && printf 'export PATH=%s/bin:$PATH\n' "$CARGO_HOME" > /etc/profile.d/rust-cargo.sh \
    && rustc --version && cargo --version

# gifsicle: post-render GIF optimization (palette reduction + lossy LZW) so the
# recorded artifacts stay small without shrinking their pixel dimensions. Kept
# in its own layer to leave the (slow) Rust layer above cached.
RUN apt-get update && apt-get install -y --no-install-recommends gifsicle \
    && rm -rf /var/lib/apt/lists/*

# Kokoro: the neural TTS voice used for the promo video narration. Runs locally
# on onnxruntime, so renders stay offline, free and deterministic — no API key
# and no per-render cost. espeak-ng is the grapheme-to-phoneme backend.
#
# Only the venv and the model files live in the image (they are large and never
# change); the driver script stays in the repo at demo/promo/scripts/kokoro_tts.py so
# the voice can be tuned without rebuilding this image. Own layer, so the slow
# Rust layer above stays cached.
ENV KOKORO_HOME=/opt/tts
RUN apt-get update && apt-get install -y --no-install-recommends \
        python3-venv \
        espeak-ng \
    && rm -rf /var/lib/apt/lists/* \
    && python3 -m venv "$KOKORO_HOME/venv" \
    && "$KOKORO_HOME/venv/bin/pip" install --no-cache-dir --disable-pip-version-check \
        kokoro-onnx==0.5.0 \
        onnxruntime==1.28.0 \
        soundfile==0.14.0 \
    && mkdir -p "$KOKORO_HOME/models" \
    && curl -fsSL -o "$KOKORO_HOME/models/kokoro-v1.0.onnx" \
        https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/kokoro-v1.0.onnx \
    && curl -fsSL -o "$KOKORO_HOME/models/voices-v1.0.bin" \
        https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin

# Pillow, for rasterizing emoji out of the color font (demo/promo/scripts/make-emoji.py).
# ffmpeg's drawtext cannot: color emoji are bitmap glyphs and it rejects the
# font outright. Its own layer so the slow model download above stays cached.
RUN "$KOKORO_HOME/venv/bin/pip" install --no-cache-dir --disable-pip-version-check \
        Pillow==11.3.0

# A UTF-8 locale so git-tailor's TUI renders box-drawing and other Unicode
# glyphs (the fragmap) correctly; only the C/C.UTF-8 locales exist here.
ENV LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

# git-tailor's reword flow shells out to $EDITOR; point it at vim so the tape
# scripts can drive commit-message editing with deterministic keystrokes.
ENV EDITOR=vim \
    VISUAL=vim \
    GIT_EDITOR=vim

# Pin git-tailor's own colors so recordings look the same whatever the
# recording terminal's theme is. Set here rather than as `--palette=dark+` on
# the command line in the tapes: the flag would appear in every recording, and
# it is not something a viewer needs to type to get this result.
ENV GT_PALETTE=dark+

# The vhs base image sets ENTRYPOINT ["vhs"]; reset it so the container is a
# general-purpose shell that the build scripts can drive (cargo build, then vhs,
# then ffmpeg).
ENTRYPOINT []
WORKDIR /work
CMD ["bash"]
