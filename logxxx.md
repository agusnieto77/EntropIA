$env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path", "User") + ";C:\Users\agusn\.cargo\bin"; pnpm --filter @entropia/desktop tauri dev

  VITE v6.4.2  ready in 15541 ms

  ➜  Local:   http://localhost:1420/
  ➜  Network: use --host to expose
     Running DevCommand (`cargo  run --no-default-features --features paddle-ocr --color always --`)
        Info Watching F:\POSITRON\EntropIA\apps\desktop\src-tauri for changes...
warning: function `compute_and_store` is never used
   --> src\nlp\embeddings.rs:183:8
    |
183 | pub fn compute_and_store(
    |        ^^^^^^^^^^^^^^^^^
    |
    = note: `#[warn(dead_code)]` on by default

warning: function `upsert_vec_item` is never used
   --> src\nlp\embeddings.rs:370:4
    |
370 | fn upsert_vec_item(conn: &Connection, item_id: &str, blob: &[u8]) -> Result<(), String> {
    |    ^^^^^^^^^^^^^^^

warning: `entropia-desktop` (lib) generated 2 warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.92s
     Running `target\debug\entropia-desktop.exe`
[setup] keeping current sqlite bundle (current_score=1831, legacy_score=432)
[setup] merged legacy app dir into current app dir: C:\Users\agusn\AppData\Roaming\com.entropia.app -> C:\Users\agusn\AppData\Roaming\com.entropia.desktop
[setup] extractions.method: no legacy CHECK constraint found — skipping migration
[setup] layouts schema ensured
[setup] app_settings table ensured
[pdf] Found pdfium at resource path: F:\POSITRON\EntropIA\apps\desktop\src-tauri\target\debug\resources\lib\pdfium.dll
[pdf] ✅ Pdfium native library resolved: F:\POSITRON\EntropIA\apps\desktop\src-tauri\target\debug\resources\lib\pdfium.dll
[llm-local] OCRC configured as text-only (multimodal disabled)
[llm-local] Scheduling background warmup: C:\Users\agusn\AppData\Roaming\com.entropia.desktop\models\gemma-4-E2B-it-Q4_K_M.gguf
The device supports: i8sdot:0, fp16:0, i8mm: 0, sve2: 0, sme2: 0
[OCR] ✅ Orientation model loaded — automatic rotation correction enabled
[OCR] Provider ready: paddle
[llm-local] Model loaded: C:\Users\agusn\AppData\Roaming\com.entropia.desktop\models\gemma-4-E2B-it-Q4_K_M.gguf (n_ctx=4096)
[llm-local] Running in text-only mode
[llm-local] Engine ready (background warmup): C:\Users\agusn\AppData\Roaming\com.entropia.desktop\models\gemma-4-E2B-it-Q4_K_M.gguf
[paddle_vl] Python resolver hit (paddle_vl, source=persisted_cache): C:\Users\agusn\AppData\Local\r-miniconda\python.exe
[OCR] High OCR mode available via PaddleOCR-VL