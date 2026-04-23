//! Reading order algorithm for layout regions.
//!
//! Groups regions into columns by horizontal overlap, sorts columns
//! left-to-right, and within each column sorts regions top-to-bottom.
//! The result is newspaper-style reading order: start at the top-left,
//! go down the column, then move to the next column on the right.
//!
//! Abandoned regions (headers, footers, page numbers) are handled specially:
//! if at the top of the page → placed first; if at the bottom → placed last.

use super::region::{LayoutCategory, LayoutRegion};

/// Sort layout regions into reading order.
///
/// Reading order: top-left → down the column → next column to the right.
///
/// Algorithm:
/// 1. Separate abandoned regions (headers/footers) from main content
/// 2. Group main content regions into columns by horizontal overlap (≥50%)
/// 3. Sort columns left-to-right by their minimum X
/// 4. Within each column, sort regions top-to-bottom by Y
/// 5. Assign `reading_order` indices sequentially
/// 6. Abandoned at page top → first; abandoned at page bottom → last
pub fn compute_reading_order(regions: &mut [LayoutRegion], _image_width: u32) {
    if regions.is_empty() {
        return;
    }

    // ── Step 1: Separate abandoned from main content ──────────────────────
    let image_height_threshold = regions
        .iter()
        .map(|r| (r.bbox.y + r.bbox.height) as u32)
        .max()
        .unwrap_or(1) as f32
        * 0.15;

    let mut top_abandoned: Vec<usize> = Vec::new();
    let mut main_indices: Vec<usize> = Vec::new();
    let mut bottom_abandoned: Vec<usize> = Vec::new();

    for (i, region) in regions.iter().enumerate() {
        match region.category {
            LayoutCategory::Abandoned => {
                if (region.bbox.y as f32) < image_height_threshold {
                    top_abandoned.push(i);
                } else {
                    bottom_abandoned.push(i);
                }
            }
            _ => {
                main_indices.push(i);
            }
        }
    }

    // ── Step 2: Group main regions into columns by horizontal overlap ──
    let n = main_indices.len();
    if n == 0 {
        // All regions are abandoned — just assign in visual order
        let mut all_abandoned: Vec<usize> = top_abandoned
            .into_iter()
            .chain(bottom_abandoned.into_iter())
            .collect();
        all_abandoned.sort_by_key(|&i| regions[i].bbox.y);
        for (order, &idx) in all_abandoned.iter().enumerate() {
            regions[idx].reading_order = order;
        }
        return;
    }

    // Union-Find parent array
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut Vec<usize>, i: usize) -> usize {
        if parent[i] != i {
            parent[i] = find(parent, parent[i]);
        }
        parent[i]
    }

    fn union(parent: &mut Vec<usize>, a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[ra] = rb;
        }
    }

    // Merge regions with ≥50% horizontal overlap into the same column
    for i in 0..n {
        for j in (i + 1)..n {
            let ri = &regions[main_indices[i]];
            let rj = &regions[main_indices[j]];

            if x_overlap_ratio(ri, rj) >= 0.5 {
                union(&mut parent, i, j);
            }
        }
    }

    // Collect columns: map from root → list of region indices
    let mut column_map: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        column_map.entry(root).or_default().push(main_indices[i]);
    }

    // ── Step 3: Sort columns left-to-right by minimum X ────────────
    let mut columns: Vec<Vec<usize>> = column_map.into_values().collect();
    columns.sort_by_key(|col| {
        col.iter()
            .map(|&idx| regions[idx].bbox.x)
            .min()
            .unwrap_or(0)
    });

    // ── Step 4: Within each column, sort top-to-bottom by Y ─────────────
    for col in &mut columns {
        col.sort_by_key(|&idx| regions[idx].bbox.y);
    }

    // ── Step 5: Assign reading_order indices ────────────────────────────
    let mut order: usize = 0;

    // Top abandoned regions first (headers at the very top of the page)
    top_abandoned.sort_by_key(|&i| regions[i].bbox.y);
    for &idx in &top_abandoned {
        regions[idx].reading_order = order;
        order += 1;
    }

    // Main content: column by column, top-to-bottom within each
    for col in &columns {
        for &idx in col {
            regions[idx].reading_order = order;
            order += 1;
        }
    }

    // Bottom abandoned regions last (footers, page numbers)
    bottom_abandoned.sort_by_key(|&i| regions[i].bbox.y);
    for &idx in &bottom_abandoned {
        regions[idx].reading_order = order;
        order += 1;
    }
}

/// Calculate the horizontal overlap ratio between two regions.
///
/// Returns the ratio of the overlapping horizontal span to the smaller
/// width of the two regions. A value ≥0.5 means they belong to the same column.
fn x_overlap_ratio(a: &LayoutRegion, b: &LayoutRegion) -> f32 {
    let a_left = a.bbox.x;
    let a_right = a.bbox.x + a.bbox.width;
    let b_left = b.bbox.x;
    let b_right = b.bbox.x + b.bbox.width;

    let overlap_start = a_left.max(b_left);
    let overlap_end = a_right.min(b_right);
    let overlap = (overlap_end - overlap_start).max(0);

    let min_width = a.bbox.width.min(b.bbox.width);

    if min_width == 0 {
        return 0.0;
    }

    overlap as f32 / min_width as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::region::BoundingBox;

    fn make_region(category: LayoutCategory, x: i32, y: i32, w: i32, h: i32) -> LayoutRegion {
        LayoutRegion {
            category,
            bbox: BoundingBox {
                x,
                y,
                width: w,
                height: h,
            },
            confidence: 0.9,
            reading_order: 0,
        }
    }

    #[test]
    fn test_empty_regions() {
        let mut regions: Vec<LayoutRegion> = vec![];
        compute_reading_order(&mut regions, 800);
        assert!(regions.is_empty());
    }

    #[test]
    fn test_single_region() {
        let mut regions = vec![make_region(LayoutCategory::PlainText, 0, 0, 400, 100)];
        compute_reading_order(&mut regions, 800);
        assert_eq!(regions[0].reading_order, 0);
    }

    #[test]
    fn test_single_column_top_to_bottom() {
        let mut regions = vec![
            make_region(LayoutCategory::Title, 50, 10, 700, 60),      // Top title
            make_region(LayoutCategory::PlainText, 50, 100, 700, 200), // Middle text
            make_region(LayoutCategory::PlainText, 50, 350, 700, 200), // Bottom text
        ];
        compute_reading_order(&mut regions, 800);

        // All regions overlap ≥50% horizontally → same column
        // Sorted by Y: title (10) → text (100) → text (350)
        assert_eq!(regions[0].reading_order, 0); // Title at y=10
        assert_eq!(regions[1].reading_order, 1); // Text at y=100
        assert_eq!(regions[2].reading_order, 2); // Text at y=350
    }

    #[test]
    fn test_two_columns_left_to_right() {
        // Two distinct columns with no horizontal overlap.
        // Left column: x=0-350, right column: x=450-800.
        // Reading order: left column top-to-bottom, then right column top-to-bottom.
        let mut regions = vec![
            make_region(LayoutCategory::PlainText, 0, 50, 350, 100),   // Left col, top
            make_region(LayoutCategory::PlainText, 0, 200, 350, 100),  // Left col, bottom
            make_region(LayoutCategory::PlainText, 450, 50, 350, 100),  // Right col, top
            make_region(LayoutCategory::PlainText, 450, 200, 350, 100), // Right col, bottom
        ];
        compute_reading_order(&mut regions, 800);

        // Left column (x=0-350) first, then right column (x=450-800)
        // Within each column, top-to-bottom
        assert_eq!(regions[0].reading_order, 0); // Left top (x=0, y=50)
        assert_eq!(regions[1].reading_order, 1); // Left bottom (x=0, y=200)
        assert_eq!(regions[2].reading_order, 2); // Right top (x=450, y=50)
        assert_eq!(regions[3].reading_order, 3); // Right bottom (x=450, y=200)
    }

    #[test]
    fn test_columns_do_not_interleave() {
        // Key test: two columns where right column has a region at a lower Y
        // than a left column region. The result must NOT interleave.
        //
        // Left col:  A(x=0, y=50),   B(x=0, y=200)
        // Right col: C(x=400, y=100), D(x=400, y=300)
        //
        // Reading order must be: A → B → C → D (column-by-column)
        // NOT: A → C → B → D (interleaved by Y)
        let mut regions = vec![
            make_region(LayoutCategory::PlainText, 0, 50, 300, 100),    // A: left top
            make_region(LayoutCategory::PlainText, 0, 200, 300, 100),  // B: left bottom
            make_region(LayoutCategory::PlainText, 400, 100, 300, 100), // C: right upper
            make_region(LayoutCategory::PlainText, 400, 300, 300, 100), // D: right lower
        ];
        compute_reading_order(&mut regions, 800);

        let a = regions.iter().find(|r| r.bbox.x == 0 && r.bbox.y == 50).unwrap();
        let b = regions.iter().find(|r| r.bbox.x == 0 && r.bbox.y == 200).unwrap();
        let c = regions.iter().find(|r| r.bbox.x == 400 && r.bbox.y == 100).unwrap();
        let d = regions.iter().find(|r| r.bbox.x == 400 && r.bbox.y == 300).unwrap();

        // Column order: A first, then B, then C, then D
        assert!(a.reading_order < b.reading_order, "A before B (same column, top to bottom)");
        assert!(b.reading_order < c.reading_order, "B before C (left column before right column)");
        assert!(c.reading_order < d.reading_order, "C before D (same column, top to bottom)");
    }

    #[test]
    fn test_abandoned_at_bottom() {
        let mut regions = vec![
            make_region(LayoutCategory::PlainText, 50, 100, 700, 200),  // Main text
            make_region(LayoutCategory::Abandoned, 50, 800, 700, 30),   // Footer at bottom
        ];
        compute_reading_order(&mut regions, 800);

        let text_idx = regions.iter().position(|r| r.category == LayoutCategory::PlainText).unwrap();
        let footer_idx = regions.iter().position(|r| r.category == LayoutCategory::Abandoned).unwrap();
        assert!(regions[text_idx].reading_order < regions[footer_idx].reading_order);
    }

    #[test]
    fn test_abandoned_at_top() {
        let mut regions = vec![
            make_region(LayoutCategory::Abandoned, 50, 5, 700, 30),     // Header at top (y < 15% of ~350)
            make_region(LayoutCategory::Title, 50, 80, 700, 60),       // Title
            make_region(LayoutCategory::PlainText, 50, 200, 700, 200), // Text
        ];
        compute_reading_order(&mut regions, 800);

        let header_idx = regions.iter().position(|r| r.category == LayoutCategory::Abandoned).unwrap();
        assert_eq!(regions[header_idx].reading_order, 0);
    }

    #[test]
    fn test_x_overlap_ratio_no_overlap() {
        let a = make_region(LayoutCategory::PlainText, 0, 0, 100, 50);
        let b = make_region(LayoutCategory::PlainText, 200, 0, 100, 50);
        assert_eq!(x_overlap_ratio(&a, &b), 0.0);
    }

    #[test]
    fn test_x_overlap_ratio_full_overlap() {
        let a = make_region(LayoutCategory::PlainText, 0, 0, 100, 50);
        let b = make_region(LayoutCategory::PlainText, 0, 0, 100, 50);
        assert_eq!(x_overlap_ratio(&a, &b), 1.0);
    }

    #[test]
    fn test_x_overlap_ratio_partial() {
        let a = make_region(LayoutCategory::PlainText, 0, 0, 100, 50);
        let b = make_region(LayoutCategory::PlainText, 50, 0, 100, 50);
        // Overlap: 50 pixels on the smaller width (100)
        assert!((x_overlap_ratio(&a, &b) - 0.5).abs() < 0.01);
    }
}