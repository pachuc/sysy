# Development image for Codex agents working on sysy inside Daytona sandboxes.
# Environment only: the launcher clones the repository at runtime.

FROM node:22-bookworm

ARG RUST_VERSION=1.98.1

# GPUI needs the X11, Wayland, fontconfig, and Vulkan development packages.
RUN apt-get update && apt-get install -y --no-install-recommends \
    bash git python3 tar ca-certificates curl jq ripgrep \
    build-essential cmake pkg-config clang libssl-dev \
    libx11-xcb-dev libxcb1-dev libxcb-xkb-dev libxkbcommon-dev libxkbcommon-x11-dev \
    libwayland-dev libfontconfig1-dev libvulkan-dev libasound2-dev \
    && rm -rf /var/lib/apt/lists/*

USER node
WORKDIR /home/node
ENV PATH=/home/node/.cargo/bin:$PATH

RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --no-modify-path \
      --default-toolchain "${RUST_VERSION}" --profile minimal \
      --component clippy --component rustfmt \
    && cargo --version && cargo clippy --version && cargo fmt --version

# Login shells source /etc/profile, which resets PATH.
RUN printf '\nexport PATH="$HOME/.cargo/bin:$PATH"\n' >> /home/node/.profile \
    && printf '\nexport PATH="$HOME/.cargo/bin:$PATH"\n' >> /home/node/.bashrc \
    && bash -lc 'cargo --version'
