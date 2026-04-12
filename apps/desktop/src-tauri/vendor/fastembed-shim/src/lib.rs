#[derive(Clone, Copy, Debug)]
pub enum EmbeddingModel {
    AllMiniLML6V2,
}

#[derive(Clone, Debug)]
pub struct InitOptions {
    _model: EmbeddingModel,
    _show_download_progress: bool,
}

impl InitOptions {
    pub fn new(model: EmbeddingModel) -> Self {
        Self {
            _model: model,
            _show_download_progress: true,
        }
    }

    pub fn with_show_download_progress(mut self, show: bool) -> Self {
        self._show_download_progress = show;
        self
    }
}

#[derive(Default)]
pub struct TextEmbedding;

impl TextEmbedding {
    pub fn try_new(_options: InitOptions) -> Result<Self, String> {
        Err("fastembed is disabled on Windows default contract build".to_string())
    }

    pub fn embed(
        &mut self,
        _texts: Vec<String>,
        _batch_size: Option<usize>,
    ) -> Result<Vec<Vec<f32>>, String> {
        Err("fastembed is disabled on Windows default contract build".to_string())
    }
}
