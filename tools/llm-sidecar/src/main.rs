//! LLM Vision Sidecar for EntropIA
//!
//! Isolated process that loads Gemma + mmproj for multimodal inference.
//! Communicates via JSON over stdin/stdout with sentinel markers.
//!
//! This exists because loading mmproj inside the Tauri process causes
//! STATUS_STACK_BUFFER_OVERRUN due to a conflict with pdfium/ort/tesseract.
//! By running in a separate process, a crash doesn't kill the main app.
//!
//! Protocol:
//!   Input:  JSON lines on stdin, each command prefixed with `>>>LLM<<<`
//!   Output: JSON lines on stdout, each response prefixed with `===LLM_JSON_BEGIN===`
//!           and suffixed with `===LLM_JSON_END===`
//!
//! Commands:
//!   {"cmd":"generate","prompt":"...","max_tokens":256}
//!   {"cmd":"generate_with_image","image_path":"...","prompt":"...","max_tokens":256}
//!   {"cmd":"ping"}
//!   {"cmd":"shutdown"}
//!
//! Responses:
//!   {"status":"ok","result":"..."}   for successful inference
//!   {"status":"error","error":"..."} for failures
//!   {"status":"pong"}                for ping

use std::ffi::CString;
use std::io::{self, BufRead, Write};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::pin::pin;

use encoding_rs::UTF_8;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::mtmd::{MtmdBitmap, MtmdContext, MtmdContextParams, MtmdInputText};
use llama_cpp_2::sampling::LlamaSampler;
use serde::{Deserialize, Serialize};

// ── Sentinel markers for JSON IPC ──────────────────────────────────────────

const INPUT_MARKER: &str = ">>>LLM<<<";
const JSON_BEGIN: &str = "===LLM_JSON_BEGIN===";
const JSON_END: &str = "===LLM_JSON_END===";

// ── Command / Response types ───────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(tag = "cmd")]
enum Command {
    #[serde(rename = "generate")]
    Generate { prompt: String, max_tokens: i32 },
    #[serde(rename = "generate_with_image")]
    GenerateWithImage { image_path: String, prompt: String, max_tokens: i32 },
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "shutdown")]
    Shutdown,
}

#[derive(Serialize)]
struct Response {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl Response {
    fn ok(result: String) -> Self {
        Self {
            status: "ok".into(),
            result: Some(result),
            error: None,
        }
    }
    fn error(msg: String) -> Self {
        Self {
            status: "error".into(),
            result: None,
            error: Some(msg),
        }
    }
    fn pong() -> Self {
        Self {
            status: "pong".into(),
            result: None,
            error: None,
        }
    }
}

// ── Engine ──────────────────────────────────────────────────────────────────

struct LlmSidecar {
    backend: LlamaBackend,
    model: LlamaModel,
    mtmd: Option<MtmdContext>,
    config: SidecarConfig,
    n_ctx: u32,
}

struct SidecarConfig {
    n_ctx: u32,
    n_threads: Option<i32>,
    seed: u32,
}

impl LlmSidecar {
    fn load(model_path: &str, mmproj_path: Option<&str>) -> Result<Self, String> {
        let backend = LlamaBackend::init()
            .map_err(|e| format!("Failed to init llama backend: {e}"))?;

        let model_params = pin!(LlamaModelParams::default());
        let model = LlamaModel::load_from_file(&backend, model_path, &model_params)
            .map_err(|e| format!("Failed to load model: {e}"))?;

        eprintln!("[sidecar] Model loaded: {}", model_path);

        let config = SidecarConfig {
            n_ctx: 4096,
            n_threads: None,
            seed: 1234,
        };

        // Load mmproj if provided
        let mtmd = match mmproj_path {
            Some(path) if !path.is_empty() && PathBuf::from(path).exists() => {
                eprintln!("[sidecar] Loading mmproj: {}", path);
                let mtmd_params = MtmdContextParams {
                    use_gpu: false, // Must be false — crash with use_gpu:true on non-CUDA systems
                    print_timings: false,
                    n_threads: config.n_threads.unwrap_or(4),
                    media_marker: CString::new("<__media__>").unwrap(),
                };
                match MtmdContext::init_from_file(path, &model, &mtmd_params) {
                    Ok(ctx) => {
                        let has_vision = ctx.support_vision();
                        eprintln!("[sidecar] mmproj loaded (vision={})", has_vision);
                        Some(ctx)
                    }
                    Err(e) => {
                        eprintln!("[sidecar] Warning: mmproj failed ({e}), text-only mode");
                        None
                    }
                }
            }
            _ => {
                eprintln!("[sidecar] No mmproj, text-only mode");
                None
            }
        };

        let n_ctx = config.n_ctx;
        Ok(Self { backend, model, mtmd, config, n_ctx })
    }

    fn is_multimodal(&self) -> bool {
        self.mtmd.as_ref().is_some_and(|ctx| ctx.support_vision())
    }

    fn generate(&self, prompt: &str, max_tokens: i32) -> Result<String, String> {
        let tokens = self.model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| format!("Tokenize failed: {e}"))?;

        let n_prompt = tokens.len() as i32;
        let available = self.n_ctx as i32 - n_prompt;
        if available <= 0 {
            return Err(format!("Prompt ({} tokens) exceeds context ({})", n_prompt, self.n_ctx));
        }
        let effective_max = max_tokens.min(available);

        let dynamic_batch = u32::try_from(tokens.len())
            .unwrap_or(self.n_ctx)
            .max(1)
            .min(self.n_ctx);

        let mut ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(self.n_ctx).unwrap()))
            .with_n_batch(dynamic_batch)
            .with_n_ubatch(dynamic_batch);

        if let Some(t) = self.config.n_threads {
            ctx_params = ctx_params.with_n_threads(t).with_n_threads_batch(t);
        }

        let mut ctx = self.model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| format!("Context failed: {e}"))?;

        if u32::try_from(n_prompt).unwrap_or(u32::MAX) > ctx.n_batch() {
            return Err(format!("Prompt ({n_prompt} tokens) exceeds batch ({})", ctx.n_batch()));
        }

        let mut batch = LlamaBatch::new(tokens.len().max(1), 1);
        let last = (tokens.len() - 1) as i32;
        for (i, tok) in (0_i32..).zip(tokens.into_iter()) {
            batch.add(tok, i, &[0], i == last)
                .map_err(|e| format!("Batch add failed: {e}"))?;
        }
        ctx.decode(&mut batch).map_err(|e| format!("Decode failed: {e}"))?;

        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::dist(self.config.seed),
            LlamaSampler::greedy(),
        ]);
        let mut decoder = UTF_8.new_decoder();
        let mut output = String::new();
        let mut n_cur = batch.n_tokens();
        let n_len = n_prompt + effective_max;

        while n_cur <= n_len {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);
            if self.model.is_eog_token(token) { break; }
            let piece = self.model.token_to_piece(token, &mut decoder, true, None)
                .map_err(|e| format!("Token decode failed: {e}"))?;
            output.push_str(&piece);
            batch.clear();
            batch.add(token, n_cur, &[0], true).map_err(|e| format!("Batch add failed: {e}"))?;
            ctx.decode(&mut batch).map_err(|e| format!("Decode failed: {e}"))?;
            n_cur += 1;
        }

        Ok(output.trim().to_string())
    }

    fn generate_with_image(&self, image_path: &str, prompt: &str, max_tokens: i32) -> Result<String, String> {
        let mtmd = self.mtmd.as_ref()
            .ok_or_else(|| "Multimodal not available".to_string())?;

        let bitmap = MtmdBitmap::from_file(mtmd, image_path)
            .map_err(|e| format!("Image load failed ({image_path}): {e:?}"))?;

        eprintln!("[sidecar] Image loaded: {}x{}", bitmap.nx(), bitmap.ny());

        let media_marker = llama_cpp_2::mtmd::mtmd_default_marker();
        let full_prompt = format!("{media_marker}\n{prompt}");

        let input_text = MtmdInputText {
            text: full_prompt,
            add_special: true,
            parse_special: true,
        };

        let chunks = mtmd.tokenize(input_text, &[&bitmap])
            .map_err(|e| format!("Tokenize failed: {e:?}"))?;

        let n_prompt = chunks.total_tokens() as i32;
        let available = self.n_ctx as i32 - n_prompt;
        if available <= 0 {
            return Err(format!("Multimodal prompt ({} tokens) exceeds context ({})", n_prompt, self.n_ctx));
        }
        let effective_max = max_tokens.min(available);

        let dynamic_batch = u32::try_from(n_prompt)
            .unwrap_or(self.n_ctx)
            .max(512)
            .min(self.n_ctx);

        let mut ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(self.n_ctx).unwrap()))
            .with_n_batch(dynamic_batch)
            .with_n_ubatch(dynamic_batch);

        if let Some(t) = self.config.n_threads {
            ctx_params = ctx_params.with_n_threads(t).with_n_threads_batch(t);
        }

        let mut ctx = self.model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| format!("Context failed: {e}"))?;

        chunks.eval_chunks(mtmd, &mut ctx, 0, 0, dynamic_batch as i32, false)
            .map_err(|e| format!("eval_chunks failed: {e:?}"))?;

        let n_len = n_prompt + effective_max;
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::dist(self.config.seed),
            LlamaSampler::greedy(),
        ]);
        let mut decoder = UTF_8.new_decoder();
        let mut output = String::new();
        let mut n_cur = n_prompt;
        let mut gen_batch = LlamaBatch::new(1, 1);
        let mut first = true;

        while n_cur <= n_len {
            let token = if first {
                first = false;
                sampler.sample(&ctx, n_prompt as i32 - 1)
            } else {
                sampler.sample(&ctx, 0)
            };
            sampler.accept(token);
            if self.model.is_eog_token(token) { break; }
            let piece = self.model.token_to_piece(token, &mut decoder, true, None)
                .map_err(|e| format!("Token decode failed: {e}"))?;
            output.push_str(&piece);
            gen_batch.clear();
            gen_batch.add(token, n_cur, &[0], true).map_err(|e| format!("Batch add failed: {e}"))?;
            ctx.decode(&mut gen_batch).map_err(|e| format!("Decode failed: {e}"))?;
            n_cur += 1;
        }

        Ok(output.trim().to_string())
    }
}

// ── IPC helpers ─────────────────────────────────────────────────────────────

fn send_response(response: &Response) {
    let json = serde_json::to_string(response).unwrap_or_else(|e| {
        format!(r#"{{"status":"error","error":"JSON serialize failed: {e}"}}"#)
    });
    println!("{JSON_BEGIN}{json}{JSON_END}");
    let _ = io::stdout().flush();
}

fn read_command(stdin: &mut io::BufReader<io::Stdin>) -> Option<Command> {
    let mut line = String::new();
    loop {
        line.clear();
        if stdin.read_line(&mut line).ok()? == 0 {
            return None; // EOF — parent closed
        }
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        // Strip input marker if present
        let json_str = if let Some(rest) = trimmed.strip_prefix(INPUT_MARKER) {
            rest.trim()
        } else {
            trimmed
        };

        match serde_json::from_str(json_str) {
            Ok(cmd) => return Some(cmd),
            Err(e) => {
                eprintln!("[sidecar] Failed to parse command: {e}");
                eprintln!("[sidecar] Input was: {json_str}");
                // Don't die — send error and continue
                send_response(&Response::error(format!("Parse error: {e}")));
                continue;
            }
        }
    }
}

// ── Main ────────────────────────────────────────────────────────────────────

fn main() {
    // Suppress Windows error dialogs and CRT debug assertions.
    // SetErrorMode handles SEH/Windows errors; _CrtSetReportMode suppresses
    // the "Debug Assertion Failed" dialogs from the debug CRT (which only
    // appear in debug builds and can block the process indefinitely).
    #[cfg(target_os = "windows")]
    unsafe {
        const SEM_FAILCRITICALERRORS: u32 = 0x0001;
        const SEM_NOGPFAULTERRORBOX: u32 = 0x0002;
        const SEM_NOOPENFILEERRORBOX: u32 = 0x8000;
        extern "system" { fn SetErrorMode(uMode: u32) -> u32; }
        SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX);

        // Suppress CRT debug assertions in debug builds.
        // _CrtSetReportMode(_CRT_ASSERT, 0) = disable all assertion output.
        // Only linked in debug builds — in release, this is a no-op.
        #[cfg(debug_assertions)]
        {
            extern "C" {
                fn _CrtSetReportMode(reportType: i32, reportMode: i32) -> i32;
            }
            const _CRT_ASSERT: i32 = 2;
            const _CRTDBG_MODE_FILE: i32 = 4;
            const _CRTDBG_FILE_STDERR: i32 = 2;
            // Route assertions to stderr instead of dialog box
            _CrtSetReportMode(_CRT_ASSERT, _CRTDBG_MODE_FILE);
            _CrtSetReportMode(_CRT_ASSERT, _CRTDBG_FILE_STDERR);
        }
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: llm-sidecar <model_path> [mmproj_path]");
        eprintln!("Communicates via JSON on stdin/stdout with sentinel markers.");
        std::process::exit(1);
    }

    let model_path = &args[1];
    let mmproj_path = args.get(2).map(|s| s.as_str()).filter(|s| !s.is_empty());

    eprintln!("[sidecar] Starting LLM Vision Sidecar");
    eprintln!("[sidecar] Model: {}", model_path);
    if let Some(mmp) = mmproj_path {
        eprintln!("[sidecar] Mmproj: {}", mmp);
    }

    let engine = match LlmSidecar::load(model_path, mmproj_path) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("[sidecar] FATAL: {e}");
            send_response(&Response::error(format!("Engine load failed: {e}")));
            std::process::exit(2);
        }
    };

    eprintln!("[sidecar] Ready (multimodal={})", engine.is_multimodal());
    // Signal readiness to parent
    send_response(&Response::pong());

    let mut stdin = io::BufReader::new(io::stdin());
    while let Some(cmd) = read_command(&mut stdin) {
        match cmd {
            Command::Generate { prompt, max_tokens } => {
                match engine.generate(&prompt, max_tokens) {
                    Ok(result) => send_response(&Response::ok(result)),
                    Err(e) => send_response(&Response::error(e)),
                }
            }
            Command::GenerateWithImage { image_path, prompt, max_tokens } => {
                if engine.is_multimodal() {
                    match engine.generate_with_image(&image_path, &prompt, max_tokens) {
                        Ok(result) => send_response(&Response::ok(result)),
                        Err(e) => send_response(&Response::error(e)),
                    }
                } else {
                    // Fallback to text-only
                    match engine.generate(&prompt, max_tokens) {
                        Ok(result) => send_response(&Response::ok(result)),
                        Err(e) => send_response(&Response::error(e)),
                    }
                }
            }
            Command::Ping => {
                send_response(&Response::pong());
            }
            Command::Shutdown => {
                eprintln!("[sidecar] Shutdown requested");
                send_response(&Response::ok("shutting down".into()));
                break;
            }
        }
    }

    eprintln!("[sidecar] Exiting");
}