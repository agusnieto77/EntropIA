#!/usr/bin/env python3
"""
Convert PP-LCNet document orientation model from Paddle format to MNN format.

This script attempts to download the PP-LCNet_x1_0_doc_ori model and convert it
through the Paddle -> ONNX -> MNN pipeline for use with ocr-rs's OriModel.

**KNOWN ISSUE (as of 2026-04)**: paddle2onnx is incompatible with PaddlePaddle 3.x
because Paddle 3.x removed the `paddle.fluid` module that paddle2onnx depends on.
This script will fail at the Paddle -> ONNX step until paddle2onnx is updated.

ALTERNATIVE CONVERSION METHODS:
1. Use PaddlePaddle 2.x in a conda environment:
   conda create -n paddle2 python=3.10
   conda activate paddle2
   pip install paddlepaddle==2.6.1 paddle2onnx
   python convert_ori_model.py

2. Use a pre-built MNNConvert binary to convert Paddle -> MNN directly:
   MNNConvert -f PADDLE --modelFile inference.pdmodel --paramsFile inference.pdiparams \
       --MNNModel PP-LCNet_x1_0_doc_ori.mnn --bizCode EntropIA

3. Obtain a pre-converted .mnn file from the ocr-rs community or the PaddleOCR project.

Once the .mnn file is obtained, simply place it in the model directory:
   apps/desktop/src-tauri/resources/models/ocr/PP-LCNet_x1_0_doc_ori.mnn

The PaddleOcrProvider will automatically detect and load it for orientation correction.

Prerequisites (for PaddlePaddle 2.x):
    pip install paddlepaddle==2.6.1 paddle2onnx

    MNNConvert must also be available. Build from source:
    git clone https://github.com/alibaba/MNN.git
    cd MNN && mkdir build && cd build
    cmake .. -DMNN_BUILD_CONVERTER=ON && cmake --build . --config Release

Usage:
    python convert_ori_model.py [--output-dir DIR] [--use-paddlex]

Options:
    --output-dir DIR       Output directory for the MNN model file
    --use-paddlex          Use PaddleX 3.x model (inference.json format) instead of legacy
                           Paddle format (inference.pdmodel). Required if you only have the
                           PaddleX-format model.
    --keep-intermediate    Keep intermediate ONNX file after conversion
"""

import argparse
import os
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

# PaddleOCR v3 (PaddleX format) model URL
MODELPADX_URL = "https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-LCNet_x1_0_doc_ori_infer.tar"

# PaddleOCR v2 (legacy format) model URL (if available)
MODELPADLLE_V2_URL = "https://paddle-model-ecology.bj.bcebos.com/whli/models/PP-LCNet_x1_0_doc_ori_infer.tar"


def download_model(target_dir: str, use_paddlex: bool = False) -> str:
    """Download and extract the Paddle inference model."""
    url = MODELPADX_URL if use_paddlex else MODELPADLLE_V2_URL
    model_name = "PP-LCNet_x1_0_doc_ori_infer"

    model_dir = os.path.join(target_dir, model_name)
    if os.path.exists(model_dir):
        print(f"Model already downloaded at {model_dir}")
        return model_dir

    tar_path = os.path.join(target_dir, f"{model_name}.tar")
    print(f"Downloading model from {url}...")
    try:
        urllib.request.urlretrieve(url, tar_path)
    except Exception as e:
        if not use_paddlex:
            print(f"Legacy format download failed: {e}")
            print("Falling back to PaddleX 3.x format...")
            url = MODELPADX_URL
            try:
                urllib.request.urlretrieve(url, tar_path)
            except Exception as e2:
                print(f"PaddleX format also failed: {e2}")
                sys.exit(1)
        else:
            print(f"Download failed: {e}")
            sys.exit(1)

    print(f"Extracting {tar_path}...")
    with tarfile.open(tar_path, "r:*") as tar:
        tar.extractall(target_dir)
    os.remove(tar_path)

    return model_dir


def check_paddle2onnx_compat() -> bool:
    """Check if paddle2onnx is compatible with the installed PaddlePaddle version."""
    try:
        import paddle
        paddle_version = paddle.__version__
        print(f"PaddlePaddle version: {paddle_version}")

        # PaddlePaddle 3.x removed paddle.fluid, which paddle2onnx depends on
        if paddle_version.startswith("3."):
            print("WARNING: PaddlePaddle 3.x detected. paddle2onnx is incompatible")
            print("         with Paddle 3.x (removed paddle.fluid module).")
            print("         Use PaddlePaddle 2.x in a separate environment:")
            print("           conda create -n paddle2 python=3.10")
            print("           conda activate paddle2")
            print("           pip install paddlepaddle==2.6.1 paddle2onnx")
            return False
        return True
    except ImportError:
        print("PaddlePaddle not installed. Install it first:")
        print("  pip install paddlepaddle==2.6.1")
        return False


def paddle_to_onnx(model_dir: str, output_dir: str) -> str:
    """Convert Paddle inference model to ONNX format."""
    onnx_path = os.path.join(output_dir, "PP-LCNet_x1_0_doc_ori.onnx")

    if os.path.exists(onnx_path):
        print(f"ONNX model already exists at {onnx_path}")
        return onnx_path

    # Check for legacy format (.pdmodel) vs PaddleX format (.json)
    pdmodel_path = os.path.join(model_dir, "inference.pdmodel")
    pdparams_path = os.path.join(model_dir, "inference.pdiparams")
    paddlex_json = os.path.join(model_dir, "inference.json")

    if not check_paddle2onnx_compat():
        print("\npaddle2onnx is incompatible with the installed PaddlePaddle version.")
        print("See the docstring at the top of this script for alternative conversion methods.")
        sys.exit(1)

    if os.path.exists(pdmodel_path):
        # Legacy Paddle format
        print("Converting Paddle (legacy) -> ONNX...")
        cmd = [
            sys.executable, "-m", "paddle2onnx",
            "--model_filename", pdmodel_path,
            "--params_filename", pdparams_path,
            "--save_file", onnx_path,
            "--opset_version", "12",
            "--enable_onnx_checker",
        ]
    elif os.path.exists(paddlex_json):
        # PaddleX 3.x format
        print("Converting PaddleX 3.x -> ONNX...")
        cmd = [
            sys.executable, "-m", "paddle2onnx",
            "--model_dir", model_dir,
            "--model_filename", "inference.json",
            "--params_filename", "inference.pdiparams",
            "--save_file", onnx_path,
            "--opset_version", "12",
        ]
    else:
        print(f"No .pdmodel or .json found in {model_dir}")
        sys.exit(1)

    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(f"paddle2onnx failed:\n{result.stderr}")
        sys.exit(1)

    print(f"ONNX model saved to {onnx_path}")
    return onnx_path


def onnx_to_mnn(onnx_path: str, output_dir: str) -> str:
    """Convert ONNX model to MNN format."""
    mnn_path = os.path.join(output_dir, "PP-LCNet_x1_0_doc_ori.mnn")

    if os.path.exists(mnn_path):
        print(f"MNN model already exists at {mnn_path}")
        return mnn_path

    mnn_convert = os.environ.get("MNNCONVERT_PATH", "MNNConvert")

    print("Converting ONNX -> MNN...")
    cmd = [
        mnn_convert,
        "-f", "ONNX",
        "--modelFile", onnx_path,
        "--MNNModel", mnn_path,
        "--bizCode", "EntropIA",
    ]

    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(f"MNNConvert failed:\n{result.stderr}")
        print("\nMake sure MNNConvert is available:")
        print("  Build from https://github.com/alibaba/MNN (cmake -DMNN_BUILD_CONVERTER=ON)")
        print("  Or set MNNCONVERT_PATH to the binary location")
        sys.exit(1)

    print(f"MNN model saved to {mnn_path}")
    return mnn_path


def main():
    default_output = os.path.normpath(os.path.join(
        os.path.dirname(__file__),
        "..", "apps", "desktop", "src-tauri", "resources", "models", "ocr"
    ))

    parser = argparse.ArgumentParser(
        description="Convert PP-LCNet orientation model to MNN",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument("--output-dir", default=default_output,
                        help=f"Output directory for MNN model (default: {default_output})")
    parser.add_argument("--use-paddlex", action="store_true",
                        help="Use PaddleX 3.x model format (inference.json)")
    parser.add_argument("--keep-intermediate", action="store_true",
                        help="Keep intermediate ONNX file after conversion")
    args = parser.parse_args()

    output_dir = os.path.abspath(args.output_dir)
    os.makedirs(output_dir, exist_ok=True)

    print(f"Output directory: {output_dir}\n")

    with tempfile.TemporaryDirectory() as tmpdir:
        # Step 1: Download Paddle model
        model_dir = download_model(tmpdir, use_paddlex=args.use_paddlex)

        # Verify model files
        has_pdmodel = os.path.exists(os.path.join(model_dir, "inference.pdmodel"))
        has_json = os.path.exists(os.path.join(model_dir, "inference.json"))
        has_params = os.path.exists(os.path.join(model_dir, "inference.pdiparams"))

        print(f"Model format: {'PaddleX 3.x' if has_json else 'Legacy Paddle' if has_pdmodel else 'Unknown'}")
        print(f"  inference.pdmodel: {has_pdmodel}")
        print(f"  inference.json:    {has_json}")
        print(f"  inference.pdiparams: {has_params}")

        if not has_params:
            print("ERROR: inference.pdiparams not found!")
            sys.exit(1)

        # Step 2: Convert Paddle -> ONNX
        onnx_path = paddle_to_onnx(model_dir, output_dir)

        # Step 3: Convert ONNX -> MNN
        mnn_path = onnx_to_mnn(onnx_path, output_dir)

        # Step 4: Verify
        size_mb = os.path.getsize(mnn_path) / (1024 * 1024)
        print(f"\nConversion complete!")
        print(f"  Model: {mnn_path}")
        print(f"  Size:  {size_mb:.2f} MB")
        print(f"\nThe orientation model will be loaded automatically by PaddleOcrProvider")
        print(f"if placed in: {output_dir}")

        if not args.keep_intermediate and os.path.exists(onnx_path):
            os.remove(onnx_path)
            print(f"  (Removed intermediate ONNX file)")


if __name__ == "__main__":
    main()