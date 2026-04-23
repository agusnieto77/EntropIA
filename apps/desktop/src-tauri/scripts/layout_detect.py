"""
EntropIA Layout Detection — DocLayout-YOLO subprocess.

Called by the Rust backend via std::process::Command.
Receives an image file path, outputs JSON to stdout.

Usage:
    python layout_detect.py <image_path> [--model-dir <dir>]

Output (stdout): Sentinelled JSON — starts with SENTINEL_BEGIN\n and ends with \nSENTINEL_END
                  This allows the Rust side to extract clean JSON even if other
                  libraries write to stdout unexpectedly.
Errors (stderr): Human-readable error messages.
Exit codes: 0=success, 1=error
"""

import sys
import os
import json
import warnings
import argparse
import io

# Suppress warnings that could corrupt JSON output on stdout
warnings.filterwarnings("ignore")

# Disable HuggingFace Hub symlink-based download and transfer.
# On Windows, symlinks in the HuggingFace cache can't be read from
# MSI-installed app contexts (WinError 448: "untrusted mount point").
os.environ["HF_HUB_ENABLE_HF_TRANSFER"] = "0"
os.environ["HF_HUB_SYMLINK_STORAGE"] = "0"

# If --model-dir is provided (from Rust), use it for HuggingFace cache and
# local model download. We set this BEFORE any HF imports so
# huggingface_hub picks it up.
_pre_model_dir = None
for _i, _arg in enumerate(sys.argv):
    if _arg == "--model-dir" and _i + 1 < len(sys.argv):
        _pre_model_dir = sys.argv[_i + 1]
        break

if _pre_model_dir:
    os.makedirs(_pre_model_dir, exist_ok=True)
    os.environ["HF_HUB_CACHE"] = _pre_model_dir
    os.environ["HF_HOME"] = _pre_model_dir

# Force stdout to be unbuffered so the Rust side sees output promptly
# and to avoid partial writes on crash.
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", line_buffering=True)

SENTINEL_BEGIN = "===LAYOUT_JSON_BEGIN==="
SENTINEL_END = "===LAYOUT_JSON_END==="

# Category mapping matching the Rust LayoutCategory enum order.
# Class indices from DocLayout-YOLO DocStructBench model.
CATEGORIES = [
    "title",            # 0
    "plain_text",       # 1
    "abandoned",        # 2
    "figure",           # 3
    "figure_caption",   # 4
    "table",            # 5
    "table_caption",    # 6
    "table_footnote",   # 7
    "isolate_formula",  # 8
    "formula_caption",  # 9
]

MODEL_ID = "juliozhao/DocLayout-YOLO-DocStructBench"
# The HuggingFace repo contains a single .pt file:
MODEL_FILENAME = "doclayout_yolo_docstructbench_imgsz1024.pt"


def load_model(model_dir, hf_cache_dir):
    """Load the DocLayout-YOLO model.

    We cannot use YOLOv10.from_pretrained() directly because its internal
    _from_pretrained() creates YOLOv10() with the default model="yolov10n.pt",
    which may not exist on disk. Instead, we download the weights file from
    HuggingFace ourselves and load it with YOLOv10(local_path).
    """
    from doclayout_yolo import YOLOv10
    from huggingface_hub import hf_hub_download

    # Strategy 1: If model_dir contains the cached HuggingFace repo, load directly.
    # HuggingFace hub caches repos as: <cache>/models--juliozhao--DocLayout-YOLO-DocStructBench/
    if model_dir and os.path.isdir(model_dir):
        repo_dir = os.path.join(model_dir, f"models--{MODEL_ID.replace('/', '--')}")
        if os.path.isdir(repo_dir):
            # Search for the .pt file in the cache (could be in refs/ or blobs/)
            for root, _dirs, files in os.walk(repo_dir):
                for f in files:
                    if f == MODEL_FILENAME:
                        local_path = os.path.join(root, f)
                        sys.stderr.write(
                            f"[layout] Loading model from local cache: {local_path}\n"
                        )
                        return YOLOv10(local_path)

    # Strategy 2: Download from HuggingFace (cached after first download).
    sys.stderr.write(f"[layout] Downloading/loading model '{MODEL_ID}' from HuggingFace...\n")
    local_path = hf_hub_download(
        repo_id=MODEL_ID,
        filename=MODEL_FILENAME,
        cache_dir=hf_cache_dir or model_dir,
    )
    sys.stderr.write(f"[layout] Model file: {local_path}\n")
    return YOLOv10(local_path)


def main():
    parser = argparse.ArgumentParser(
        description="Detect document layout regions using DocLayout-YOLO"
    )
    parser.add_argument("image_path", help="Path to the image file")
    parser.add_argument(
        "--model-dir",
        default=None,
        help="Directory to cache/download the model (default: ~/.cache/huggingface/hub)",
    )
    args = parser.parse_args()

    try:
        model = load_model(args.model_dir, _pre_model_dir)

        sys.stderr.write(f"[layout] Running detection on: {args.image_path}\n")

        # Run inference — imgsz=1024 for DocLayout-YOLO (recommended)
        det_res = model.predict(args.image_path, imgsz=1024, conf=0.2, device="cpu")

        regions = []
        if det_res and len(det_res) > 0 and det_res[0].boxes is not None:
            for box in det_res[0].boxes:
                # box.xyxy is shape (1, 4): [x1, y1, x2, y2]
                x1, y1, x2, y2 = box.xyxy[0].tolist()
                conf = float(box.conf[0])
                cls = int(box.cls[0])

                category = CATEGORIES[cls] if cls < len(CATEGORIES) else "plain_text"

                regions.append({
                    "category": category,
                    "bbox": {
                        "x": int(x1),
                        "y": int(y1),
                        "width": int(x2 - x1),
                        "height": int(y2 - y1),
                    },
                    "confidence": round(conf, 4),
                })

        sys.stderr.write(f"[layout] Detected {len(regions)} regions\n")

        result = {"regions": regions}
        json_str = json.dumps(result, ensure_ascii=False)
        sys.stdout.write(f"{SENTINEL_BEGIN}\n{json_str}\n{SENTINEL_END}\n")
        sys.stdout.flush()

    except ImportError:
        error_msg = "doclayout_yolo not installed. Run: pip install doclayout-yolo"
        sys.stderr.write(f"Error: {error_msg}\n")
        error_json = json.dumps({"error": error_msg})
        sys.stdout.write(f"{SENTINEL_BEGIN}\n{error_json}\n{SENTINEL_END}\n")
        sys.stdout.flush()
        sys.exit(1)
    except Exception as e:
        error_msg = str(e)
        sys.stderr.write(f"Error: {error_msg}\n")
        error_json = json.dumps({"error": error_msg})
        sys.stdout.write(f"{SENTINEL_BEGIN}\n{error_json}\n{SENTINEL_END}\n")
        sys.stdout.flush()
        sys.exit(1)


if __name__ == "__main__":
    main()