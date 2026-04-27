use std::num::NonZeroU32;
use std::path::PathBuf;
use std::pin::pin;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

/// Configuration for the LLM engine.
pub struct LlmConfig {
    pub model_path: PathBuf,
    pub n_ctx: u32,
    pub n_threads: Option<i32>,
    pub seed: u32,
    /// Optional path to the multimodal projection file (mmproj).
    /// Currently DISABLED — mmproj crashes with STATUS_STACK_BUFFER_OVERRUN
    /// when loaded inside the Tauri process (conflict with pdfium/ort/tesseract).
    /// Kept for sidecar/subprocess implementation.
    pub mmproj_path: Option<PathBuf>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            n_ctx: 4096,
            n_threads: None,
            seed: 1234,
            mmproj_path: None,
        }
    }
}

/// Wraps llama.cpp via llama-cpp-2 crate. Loads a GGUF model once and runs
/// inference on demand. Multimodal (vision) is DISABLED because mmproj causes
/// STATUS_STACK_BUFFER_OVERRUN when loaded inside the Tauri process.
///
/// Diagnostic testing proved mmproj loads fine in isolation (tools/llm-diag/),
/// so the crash is caused by a conflict with another native library in the
/// Tauri process (pdfium, onnxruntime, or tesseract).
///
/// TODO: Implement sidecar/subprocess approach for vision (Option C).
pub struct LlmEngine {
    backend: LlamaBackend,
    model: LlamaModel,
    config: LlmConfig,
    /// Multimodal context — disabled until sidecar approach is implemented.
    _mtmd: Option<()>,
}

impl LlmEngine {
    /// Load a GGUF model from disk.
    ///
    /// Multimodal (mmproj) loading is INTENTIONALLY DISABLED because it causes
    /// STATUS_STACK_BUFFER_OVERRUN inside the Tauri process. A standalone test
    /// (tools/llm-diag/) proved mmproj works fine in isolation — the crash is
    /// caused by a conflict with another native library (pdfium/ort/tesseract).
    pub fn init(config: LlmConfig) -> Result<Self, String> {
        if !config.model_path.exists() {
            return Err(format!(
                "Model file not found: {}",
                config.model_path.display()
            ));
        }

        let backend = LlamaBackend::init()
            .map_err(|e| format!("Failed to init llama backend: {e}"))?;

        let model_params = pin!(LlamaModelParams::default());

        let model = LlamaModel::load_from_file(&backend, &config.model_path, &model_params)
            .map_err(|e| format!("Failed to load model {}: {e}", config.model_path.display()))?;

        eprintln!(
            "[llm] Model loaded: {} (n_ctx={})",
            config.model_path.display(),
            config.n_ctx
        );

        // Multimodal DISABLED in-process — mmproj crashes with STATUS_STACK_BUFFER_OVERRUN
        // in the Tauri process (conflict with pdfium/ort/tesseract).
        // Vision inference is handled by the sidecar subprocess (llm-sidecar).
        // The engine still receives mmproj_path for detection/logging only.
        if let Some(ref path) = config.mmproj_path {
            if path.exists() {
                eprintln!(
                    "[llm] mmproj found at {} — vision handled by sidecar (in-process disabled)",
                    path.display()
                );
            } else {
                eprintln!("[llm] No mmproj found, running text-only");
            }
        } else {
            eprintln!("[llm] No mmproj configured, running text-only");
        }

        Ok(Self {
            backend,
            model,
            config,
            _mtmd: None,
        })
    }

    /// Returns the configured context window size.
    pub fn n_ctx(&self) -> u32 {
        self.config.n_ctx
    }

    /// Returns `true` if multimodal (vision) capabilities are available.
    /// Currently always returns false — vision disabled pending sidecar.
    pub fn is_multimodal(&self) -> bool {
        false
    }

    /// Run text generation with the given prompt. Returns the generated text
    /// (excluding the prompt). `max_tokens` limits the output length.
    pub fn generate(&self, prompt: &str, max_tokens: i32) -> Result<String, String> {
        let tokens = self
            .model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| format!("Failed to tokenize prompt: {e}"))?;

        let n_prompt = tokens.len() as i32;
        let prompt_chars = prompt.chars().count();

        let available = self.config.n_ctx as i32 - n_prompt;
        if available <= 0 {
            return Err(format!(
                "Prompt ({} tokens) exceeds context window ({}). \
                 Truncate input text before generating.",
                n_prompt, self.config.n_ctx
            ));
        }
        let effective_max_tokens = max_tokens.min(available);
        if effective_max_tokens < max_tokens {
            eprintln!(
                "[llm] Reducing max_tokens from {} to {} \
                 (prompt={}/n_ctx={})",
                max_tokens, effective_max_tokens, n_prompt, self.config.n_ctx
            );
        }

        let dynamic_batch = u32::try_from(tokens.len())
            .unwrap_or(self.config.n_ctx)
            .max(1)
            .min(self.config.n_ctx);

        let mut ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(self.config.n_ctx).unwrap()))
            .with_n_batch(dynamic_batch)
            .with_n_ubatch(dynamic_batch);

        if let Some(threads) = self.config.n_threads {
            ctx_params = ctx_params.with_n_threads(threads);
            ctx_params = ctx_params.with_n_threads_batch(threads);
        }

        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| format!("Failed to create context: {e}"))?;

        let ctx_n_batch = ctx.n_batch();
        let ctx_n_ubatch = ctx.n_ubatch();
        let ctx_n_ctx = ctx.n_ctx();

        eprintln!(
            "[llm] generate request: prompt_chars={}, prompt_tokens={}, requested_max_tokens={}, effective_max_tokens={}, n_ctx={}, n_batch={}, n_ubatch={}",
            prompt_chars, n_prompt, max_tokens, effective_max_tokens, ctx_n_ctx, ctx_n_batch, ctx_n_ubatch
        );

        if u32::try_from(n_prompt).unwrap_or(u32::MAX) > ctx_n_batch {
            return Err(format!(
                "Prompt token count ({}) exceeds llama batch size ({}). Request blocked before decode to avoid runtime abort.",
                n_prompt, ctx_n_batch
            ));
        }

        let n_len = n_prompt + effective_max_tokens;

        let mut batch = LlamaBatch::new(tokens.len().max(1), 1);
        let last_index = (tokens.len() - 1) as i32;
        for (i, token) in (0_i32..).zip(tokens.into_iter()) {
            batch
                .add(token, i, &[0], i == last_index)
                .map_err(|e| format!("Failed to add token to batch: {e}"))?;
        }

        ctx.decode(&mut batch)
            .map_err(|e| format!("Failed to decode prompt: {e}"))?;

        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::dist(self.config.seed),
            LlamaSampler::greedy(),
        ]);

        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();
        let mut n_cur = batch.n_tokens();

        while n_cur <= n_len {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                break;
            }

            let piece = self
                .model
                .token_to_piece(token, &mut decoder, true, None)
                .map_err(|e| format!("Failed to decode token: {e}"))?;
            output.push_str(&piece);

            batch.clear();
            batch
                .add(token, n_cur, &[0], true)
                .map_err(|e| format!("Failed to add token to batch: {e}"))?;

            ctx.decode(&mut batch)
                .map_err(|e| format!("Failed to decode: {e}"))?;

            n_cur += 1;
        }

        Ok(output.trim().to_string())
    }

    /// Run generation with an image + text prompt.
    /// Currently DISABLED — falls back to text-only.
    /// TODO: Re-enable via sidecar subprocess (Option C).
    #[allow(dead_code)]
    pub fn generate_with_image(
        &self,
        _image_path: &str,
        text_prompt: &str,
        max_tokens: i32,
    ) -> Result<String, String> {
        eprintln!("[llm] Vision disabled — falling back to text-only generation");
        self.generate(text_prompt, max_tokens)
    }
}