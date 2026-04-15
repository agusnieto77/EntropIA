#!/usr/bin/env bash
# Downloads OCR model files required by the ocrs engine if they are missing.
#
# The ocrs Rust OCR engine requires two .rten model files:
#   - text-detection.rten  (~2.4 MB)
#   - text-recognition.rten (~9.3 MB)
#
# These files are NOT committed to the repository. This script downloads them
# from the official ocrs-models S3 bucket into the Tauri resources directory,
# where tauri.conf.json bundles them automatically.
#
# Safe to run multiple times - skips files that already exist.
#
# Model source: https://github.com/robertknight/ocrs
# Bucket: https://ocrs-models.s3-accelerate.amazonaws.com/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RESOURCES_DIR="$(cd "$SCRIPT_DIR/../resources" && pwd)"

DETECTION_NAME="text-detection.rten"
RECOGNITION_NAME="text-recognition.rten"
BASE_URL="https://ocrs-models.s3-accelerate.amazonaws.com"

mkdir -p "$RESOURCES_DIR"

downloaded=0

# Process detection model
target_path="$RESOURCES_DIR/$DETECTION_NAME"
if [ -f "$target_path" ]; then
  size=$(du -m "$target_path" | cut -f1)
  echo "[OK]   $DETECTION_NAME already exists (${size} MB) - skipping"
else
  url="$BASE_URL/$DETECTION_NAME"
  echo "[..] Downloading $DETECTION_NAME (~2.4 MB)..."
  curl -fsSL -o "$target_path" "$url"
  size=$(du -m "$target_path" | cut -f1)
  echo "[OK]   $DETECTION_NAME downloaded (${size} MB)"
  downloaded=$((downloaded + 1))
fi

# Process recognition model
target_path="$RESOURCES_DIR/$RECOGNITION_NAME"
if [ -f "$target_path" ]; then
  size=$(du -m "$target_path" | cut -f1)
  echo "[OK]   $RECOGNITION_NAME already exists (${size} MB) - skipping"
else
  url="$BASE_URL/$RECOGNITION_NAME"
  echo "[..] Downloading $RECOGNITION_NAME (~9.3 MB)..."
  curl -fsSL -o "$target_path" "$url"
  size=$(du -m "$target_path" | cut -f1)
  echo "[OK]   $RECOGNITION_NAME downloaded (${size} MB)"
  downloaded=$((downloaded + 1))
fi

echo ""
if [ "$downloaded" -gt 0 ]; then
  echo "Downloaded $downloaded model(s). OCR engine is ready."
else
  echo "All models already present. Nothing to do."
fi
