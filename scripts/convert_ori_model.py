#!/usr/bin/env python3
"""
Convert PP-LCNet document orientation model from Paddle format to MNN format.

This script downloads the PP-LCNet_x1_0_doc_ori model from PaddleOCR's official
repository and converts it to MNN format for use with ocr-rs.

Prerequisites:
    pip install paddlepaddle paddle2onnx

    MNNConvert must also be available. Install from:
    https://github.com/alibaba/MNN/blob/master/docs/converters/ONNX2MNN.md

    Or build from source:
    git clone https://github.com/alibaba/MNN.git
    cd MNN && mkdir build && cd build
    cmake .. -DMNN_BUILD_CONVERTER=ON && make -j4

Usage:
    python convert_ori_model.py [--output-dir DIR]

Output:
    PP-LCNet_x1_0_doc_ori.mnn in the specified output directory
    (default: ../apps/desktop/src-tauri/resources/models/ocr/)
"""

import argparse
import os
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

# Official PaddleOCR model URL for PP-LCNet document orientation
MODEL_URL = "https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-LCNet_x1_0_doc_ori_infer.tar"

MODEL_NAME = "PP-LCNet_x1_0_doc_ori_infer"


def download_model(target_dir: str) -> str:
    """Download and extract the Paddle inference model."""
    tar_path = os.path.join(target_dir, f"{MODEL_NAME}.tar")
    if not os.path.exists(os.path.join(target_dir, MODEL_NAME)):
        print(f"📥 Downloading model from {MODEL_URL}...")
        urllib.request.urlretrieve(MODEL_URL, tar_path)
        print(f"📦 Extracting {tar_path}...")
        with tarfile.open(tar_path, "r:*") as tar:
            tar.extractall(target_dir)
        os.remove(tar_path)
    else:
        print(f"✅ Model already downloaded at {os.path.join(target_dir, MODEL_NAME)}")

    model_dir = os.path.join(target_dir, MODEL_NAME)
    # Verify required files exist
    for ext in [".pdmodel", ".pdiparams"]:
        if not os.path.exists(os.path.join(model_dir, f"inference{ext}")):
            print(f"❌ Missing inference{ext} in {model_dir}")
            sys.exit(1)

    return model_dir


def paddle_to_onnx(model_dir: str, output_dir: str) -> str:
    """Convert Paddle inference model to ONNX format."""
    onnx_path = os.path.join(output_dir, "PP-LCNet_x1_0_doc_ori.onnx")

    if os.path.exists(onnx_path):
        print(f"✅ ONNX model already exists at {onnx_path}")
        return onnx_path

    pdmodel_path = os.path.join(model_dir, "inference.pdmodel")
    pdparams_path = os.path.join(model_dir, "inference.pdiparams")

    print("🔄 Converting Paddle → ONNX...")
    cmd = [
        sys.executable, "-m", "paddle2onnx",
        "--model_filename", pdmodel_path,
        "--params_filename", pdparams_path,
        "--save_file", onnx_path,
        "--opset_version", "12",
        "--enable_onnx_checker",
    ]

    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(f"❌ paddle2onnx failed:\n{result.stderr}")
        sys.exit(1)

    print(f"✅ ONNX model saved to {onnx_path}")
    return onnx_path


def onnx_to_mnn(onnx_path: str, output_dir: str) -> str:
    """Convert ONNX model to MNN format."""
    mnn_path = os.path.join(output_dir, "PP-LCNet_x1_0_doc_ori.mnn")

    if os.path.exists(mnn_path):
        print(f"✅ MNN model already exists at {mnn_path}")
        return mnn_path

    # Check for MNNConvert
    mnn_convert = os.environ.get("MNNCONVERT_PATH", "MNNConvert")

    print("🔄 Converting ONNX → MNN...")
    cmd = [
        mnn_convert,
        "-f", "ONNX",
        "--modelFile", onnx_path,
        "--MNNModel", mnn_path,
        "--bizCode", "EntropIA",
    ]

    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(f"❌ MNNConvert failed:\n{result.stderr}")
        print("\n💡 Make sure MNNConvert is available:")
        print("   export MNNCONVERT_PATH=/path/to/MNN/build/MNNConvert")
        sys.exit(1)

    print(f"✅ MNN model saved to {mnn_path}")
    return mnn_path


def main():
    default_output = os.path.normpath(os.path.join(
        os.path.dirname(__file__),
        "..", "apps", "desktop", "src-tauri", "resources", "models", "ocr"
    ))

    parser = argparse.ArgumentParser(description="Convert PP-LCNet orientation model to MNN")
    parser.add_argument("--output-dir", default=default_output,
                        help=f"Output directory for MNN model (default: {default_output})")
    parser.add_argument("--keep-intermediate", action="store_true",
                        help="Keep intermediate ONNX file after conversion")
    args = parser.parse_args()

    output_dir = os.path.abspath(args.output_dir)
    os.makedirs(output_dir, exist_ok=True)

    print(f"🎯 Output directory: {output_dir}\n")

    with tempfile.TemporaryDirectory() as tmpdir:
        # Step 1: Download Paddle model
        model_dir = download_model(tmpdir)

        # Step 2: Convert Paddle → ONNX
        onnx_path = paddle_to_onnx(model_dir, output_dir)

        # Step 3: Convert ONNX → MNN
        mnn_path = onnx_to_mnn(onnx_path, output_dir)

        # Step 4: Verify
        size_mb = os.path.getsize(mnn_path) / (1024 * 1024)
        print(f"\n🎉 Conversion complete!")
        print(f"   Model: {mnn_path}")
        print(f"   Size:  {size_mb:.2f} MB")
        print(f"\n📋 The orientation model will be loaded automatically by PaddleOcrProvider")
        print(f"   if placed in: {output_dir}")

        if not args.keep_intermediate and os.path.exists(onnx_path):
            os.remove(onnx_path)
            print(f"   (Removed intermediate ONNX file)")


if __name__ == "__main__":
    main()