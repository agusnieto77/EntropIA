//! Layout region types for document structure detection.
//!
//! Defines the core data types used by the DocLayout-YOLO layout analysis
//! engine: categories, bounding boxes, regions, and results.

use serde::{Deserialize, Serialize};

/// Semantic category detected by DocLayout-YOLO.
///
/// Each variant corresponds to a document element type from the
/// DocStructBench dataset used to train the model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LayoutCategory {
    /// Headings, multi-level titles.
    Title,
    /// Main body text paragraphs.
    PlainText,
    /// Headers, footers, page numbers, marginal notes — typically
    /// deprioritized in reading order.
    Abandoned,
    /// Isolated images/figures (no text to extract).
    Figure,
    /// Captions describing figures.
    FigureCaption,
    /// Tabular data regions.
    Table,
    /// Captions describing tables.
    TableCaption,
    /// Footnotes attached to tables.
    TableFootnote,
    /// Standalone mathematical expressions.
    IsolateFormula,
    /// Labels/numbering for formulas (e.g. "Eq. 1").
    FormulaCaption,
}

/// Bounding box in pixel coordinates within the source image.
///
/// Origin (0,0) is the top-left corner of the image.
/// Note: this uses `i32` for width/height (matching YOLO output)
/// rather than `u32` as in the OCR module's `BoundingBox`, because
/// YOLO detection coordinates can produce zero or small dimensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// A single detected layout region with category, position, and confidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutRegion {
    pub category: LayoutCategory,
    pub bbox: BoundingBox,
    pub confidence: f32,
    /// Index indicating reading order position (0 = first to read).
    /// Assigned by `compute_reading_order`.
    pub reading_order: usize,
}

/// Complete layout detection result for an image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutResult {
    pub regions: Vec<LayoutRegion>,
    /// Original image width in pixels.
    pub image_width: u32,
    /// Original image height in pixels.
    pub image_height: u32,
    /// Model identifier (e.g. "doclayout_yolo").
    pub model: String,
}