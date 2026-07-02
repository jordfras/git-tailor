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

# A UTF-8 locale so git-tailor's TUI renders box-drawing and other Unicode
# glyphs (the fragmap) correctly; only the C/C.UTF-8 locales exist here.
ENV LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

# git-tailor's reword flow shells out to $EDITOR; point it at vim so the tape
# scripts can drive commit-message editing with deterministic keystrokes.
ENV EDITOR=vim \
    VISUAL=vim \
    GIT_EDITOR=vim

# The vhs base image sets ENTRYPOINT ["vhs"]; reset it so the container is a
# general-purpose shell that the build scripts can drive (cargo build, then vhs,
# then ffmpeg).
ENTRYPOINT []
WORKDIR /work
CMD ["bash"]
