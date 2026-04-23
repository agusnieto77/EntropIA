use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::pin::pin;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
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
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            n_ctx: 4096,
            n_threads: None,
            seed: 1234,
        }
    }
}

/// Wraps llama.cpp via llama-cpp-2 crate. Loads a GGUF model once and runs
/// inference on demand. NOT Send — must live inside a single worker thread.
pub struct LlmEngine {
    backend: LlamaBackend,
    model: LlamaModel,
    config: LlmConfig,
}

impl LlmEngine {
    /// Load a GGUF model from disk. Returns an error if the file is missing or
    /// the model cannot be loaded.
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

        Ok(Self { backend, model, config })
    }

    /// Returns the model file path.
    pub fn model_path(&self) -> &Path {
        &self.config.model_path
    }

    /// Run text generation with the given prompt. Returns the generated text
    /// (excluding the prompt). `max_tokens` limits the output length.
    pub fn generate(&self, prompt: &str, max_tokens: i32) -> Result<String, String> {
        let mut ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(self.config.n_ctx).unwrap()));

        if let Some(threads) = self.config.n_threads {
            ctx_params = ctx_params.with_n_threads(threads);
            ctx_params = ctx_params.with_n_threads_batch(threads);
        }

        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| format!("Failed to create context: {e}"))?;

        let tokens = self
            .model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| format!("Failed to tokenize prompt: {e}"))?;

        let n_prompt = tokens.len() as i32;
        let n_len = n_prompt + max_tokens;

        if n_len > self.config.n_ctx as i32 {
            return Err(format!(
                "Prompt ({n_prompt} tokens) + max_tokens ({max_tokens}) exceeds context window ({})",
                self.config.n_ctx
            ));
        }

        // Feed prompt tokens
        let mut batch = LlamaBatch::new(512, 1);
        let last_index = (tokens.len() - 1) as i32;
        for (i, token) in (0_i32..).zip(tokens.into_iter()) {
            batch
                .add(token, i, &[0], i == last_index)
                .map_err(|e| format!("Failed to add token to batch: {e}"))?;
        }

        ctx.decode(&mut batch)
            .map_err(|e| format!("Failed to decode prompt: {e}"))?;

        // Generation loop
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
}
