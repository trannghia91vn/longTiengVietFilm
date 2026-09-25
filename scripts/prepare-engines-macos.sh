#!/usr/bin/env bash
set -euo pipefail

WHISPER_VERSION="v1.9.4"
LLAMA_VERSION="v0.5.0"
FFMPEG_VERSION="8.0.1"
AUDIO_CPP_REV="b0176a0559b6aec722e80154f53e2802b7ac8367"
SEA_G2P_REV="e69c65d4b752c582a51f3618b68036f347958bb4"

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
ENGINE_DIR="$ROOT_DIR/src-tauri/engines"
WORK_DIR="$(mktemp -d)"
JOBS="$(sysctl -n hw.ncpu 2>/dev/null || echo 8)"

cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

mkdir -p "$ENGINE_DIR"

if [[ ! -x "$ENGINE_DIR/whisper-cli" ]]; then
  echo "Building whisper.cpp $WHISPER_VERSION"
  curl -fL "https://github.com/ggml-org/whisper.cpp/archive/refs/tags/$WHISPER_VERSION.tar.gz" -o "$WORK_DIR/whisper.tar.gz"
  tar -xzf "$WORK_DIR/whisper.tar.gz" -C "$WORK_DIR"
  WHISPER_SOURCE="$WORK_DIR/whisper.cpp-${WHISPER_VERSION#v}"
  cmake -S "$WHISPER_SOURCE" -B "$WHISPER_SOURCE/build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DBUILD_SHARED_LIBS=OFF \
    -DGGML_METAL=ON \
    -DGGML_METAL_EMBED_LIBRARY=ON \
    -DWHISPER_BUILD_EXAMPLES=ON
  cmake --build "$WHISPER_SOURCE/build" --config Release --target whisper-cli -j "$JOBS"
  cp "$WHISPER_SOURCE/build/bin/whisper-cli" "$ENGINE_DIR/whisper-cli"
  cp "$WHISPER_SOURCE/LICENSE" "$ENGINE_DIR/LICENSE.whisper.txt"
fi

if [[ ! -x "$ENGINE_DIR/llama-cli" || ! -x "$ENGINE_DIR/llama-server" ]]; then
  echo "Building llama.cpp $LLAMA_VERSION"
  curl -fL "https://github.com/ggml-org/llama.cpp/archive/refs/tags/$LLAMA_VERSION.tar.gz" -o "$WORK_DIR/llama.tar.gz"
  tar -xzf "$WORK_DIR/llama.tar.gz" -C "$WORK_DIR"
  LLAMA_SOURCE="$WORK_DIR/llama.cpp-${LLAMA_VERSION#v}"
  cmake -S "$LLAMA_SOURCE" -B "$LLAMA_SOURCE/build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DBUILD_SHARED_LIBS=OFF \
    -DGGML_METAL=ON \
    -DGGML_METAL_EMBED_LIBRARY=ON \
    -DLLAMA_CURL=OFF \
    -DLLAMA_OPENSSL=OFF \
    -DLLAMA_BUILD_SERVER=ON \
    -DLLAMA_BUILD_UI=OFF \
    -DLLAMA_USE_PREBUILT_UI=OFF \
    -DLLAMA_BUILD_TESTS=OFF
  cmake --build "$LLAMA_SOURCE/build" --config Release --target llama-completion llama-server -j "$JOBS"
  cp "$LLAMA_SOURCE/build/bin/llama-completion" "$ENGINE_DIR/llama-cli"
  cp "$LLAMA_SOURCE/build/bin/llama-server" "$ENGINE_DIR/llama-server"
  cp "$LLAMA_SOURCE/LICENSE" "$ENGINE_DIR/LICENSE.llama.txt"
fi

if [[ ! -x "$ENGINE_DIR/ffmpeg" || ! -x "$ENGINE_DIR/ffprobe" ]]; then
  echo "Building FFmpeg $FFMPEG_VERSION"
  curl -fL "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz" -o "$WORK_DIR/ffmpeg.tar.xz"
  tar -xJf "$WORK_DIR/ffmpeg.tar.xz" -C "$WORK_DIR"
  FFMPEG_SOURCE="$WORK_DIR/ffmpeg-$FFMPEG_VERSION"
  (
    cd "$FFMPEG_SOURCE"
    ./configure \
      --prefix="$FFMPEG_SOURCE/install" \
      --disable-shared \
      --enable-static \
      --disable-doc \
      --disable-debug \
      --disable-network \
      --disable-autodetect \
      --enable-zlib \
      --disable-iconv \
      --enable-audiotoolbox \
      --enable-videotoolbox
    make -j "$JOBS" ffmpeg ffprobe
  )
  cp "$FFMPEG_SOURCE/ffmpeg" "$ENGINE_DIR/ffmpeg"
  cp "$FFMPEG_SOURCE/ffprobe" "$ENGINE_DIR/ffprobe"
  cp "$FFMPEG_SOURCE/LICENSE.md" "$ENGINE_DIR/LICENSE.ffmpeg.md"
fi

if [[ ! -x "$ENGINE_DIR/audiocpp_cli" || ! -f "$ENGINE_DIR/libsea_g2p_rs.dylib" || ! -f "$ENGINE_DIR/sea_g2p.bin" ]]; then
  echo "Building SEA-G2P $SEA_G2P_REV"
  curl -fL "https://github.com/pnnbao97/sea-g2p/archive/$SEA_G2P_REV.tar.gz" -o "$WORK_DIR/sea-g2p.tar.gz"
  tar -xzf "$WORK_DIR/sea-g2p.tar.gz" -C "$WORK_DIR"
  SEA_G2P_SOURCE="$WORK_DIR/sea-g2p-$SEA_G2P_REV"
  cargo build --manifest-path "$SEA_G2P_SOURCE/Cargo.toml" --release --no-default-features --features capi

  echo "Building audio.cpp $AUDIO_CPP_REV with Vietnamese text frontend"
  curl -fL "https://github.com/pnnbao97/audio.cpp/archive/$AUDIO_CPP_REV.tar.gz" -o "$WORK_DIR/audio-cpp.tar.gz"
  tar -xzf "$WORK_DIR/audio-cpp.tar.gz" -C "$WORK_DIR"
  AUDIO_CPP_SOURCE="$WORK_DIR/audio.cpp-$AUDIO_CPP_REV"
  if xcrun --sdk macosx --find metal >/dev/null 2>&1; then
    (
      cd "$AUDIO_CPP_SOURCE"
      scripts/build_metal.sh \
        --build-type Release \
        --archs arm64 \
        --deployment-build \
        --model-set custom \
        --models vieneu_v3_turbo,mel_band_roformer \
        --target audiocpp_cli
    )
    AUDIO_CPP_BINARY="$AUDIO_CPP_SOURCE/build/macos-metal-release/bin/audiocpp_cli"
  else
    echo "Metal compiler not found; building portable CPU sidecar"
    cmake -S "$AUDIO_CPP_SOURCE" -B "$AUDIO_CPP_SOURCE/build/macos-cpu-release" \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_OSX_ARCHITECTURES=arm64 \
      -DENGINE_ENABLE_CUDA=OFF \
      -DENGINE_ENABLE_VULKAN=OFF \
      -DENGINE_ENABLE_METAL=OFF \
      -DENGINE_ENABLE_OPENMP=OFF \
      -DGGML_OPENMP=OFF \
      -DAUDIOCPP_DEPLOYMENT_BUILD=ON \
      -DAUDIOCPP_MODEL_SET=custom \
      -DAUDIOCPP_MODELS=vieneu_v3_turbo,mel_band_roformer
    cmake --build "$AUDIO_CPP_SOURCE/build/macos-cpu-release" --config Release --target audiocpp_cli -j "$JOBS"
    AUDIO_CPP_BINARY="$AUDIO_CPP_SOURCE/build/macos-cpu-release/bin/audiocpp_cli"
  fi
  cp "$AUDIO_CPP_BINARY" "$ENGINE_DIR/audiocpp_cli"
  cp "$SEA_G2P_SOURCE/target/release/libsea_g2p_rs.dylib" "$ENGINE_DIR/libsea_g2p_rs.dylib"
  cp "$SEA_G2P_SOURCE/python/sea_g2p/sea_g2p.bin" "$ENGINE_DIR/sea_g2p.bin"
  cp "$AUDIO_CPP_SOURCE/LICENSE" "$ENGINE_DIR/LICENSE.audio-cpp.txt"
  cp "$SEA_G2P_SOURCE/LICENSE" "$ENGINE_DIR/LICENSE.sea-g2p.txt"
fi

chmod +x "$ENGINE_DIR/ffmpeg" "$ENGINE_DIR/ffprobe" "$ENGINE_DIR/whisper-cli" "$ENGINE_DIR/llama-cli" "$ENGINE_DIR/llama-server" "$ENGINE_DIR/audiocpp_cli" "$ENGINE_DIR/libsea_g2p_rs.dylib"
echo "Bundled engines are ready in $ENGINE_DIR"
