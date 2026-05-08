//! Static registry of all managed Python dependencies.
//!
//! Each entry describes how to detect and install one dependency. The registry
//! is a `&'static [DependencySpec]` so it never allocates and is safe to access
//! from multiple threads without synchronisation.

use super::DependencyId;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Specification for a single managed dependency.
pub struct DependencySpec {
    /// Canonical identifier — used as the map key everywhere.
    pub id: DependencyId,
    /// Human-readable name shown in the UI.
    pub display_name: &'static str,
    /// pip/uv install specifier, e.g. `"fastembed>=0.4.0"`.
    /// `None` means the dependency has a custom install path (e.g. spaCy model).
    pub pip_spec: Option<&'static str>,
    /// One-liner Python code that prints `"ok"` when the dependency is available.
    pub probe_code: &'static str,
    /// Whether the main AI pipeline cannot function without this dependency.
    pub critical: bool,
    /// Managed-environment prerequisites that must exist before this dep can install.
    pub managed_prerequisites: &'static [DependencyId],
    /// Relative install order — lower numbers are installed first.
    pub install_order: u8,
}

const NO_PREREQUISITES: &[DependencyId] = &[];
const SPACY_MODEL_PREREQUISITES: &[DependencyId] = &[DependencyId::Spacy];

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

static ALL_DEPS: &[DependencySpec] = &[
    DependencySpec {
        id: DependencyId::Python,
        display_name: "Python",
        pip_spec: None,
        probe_code: "import sys; print('ok')",
        critical: true,
        managed_prerequisites: NO_PREREQUISITES,
        install_order: 0,
    },
    DependencySpec {
        id: DependencyId::Fastembed,
        display_name: "fastembed",
        pip_spec: Some("fastembed>=0.4.0"),
        probe_code: "import fastembed; print('ok')",
        critical: true,
        managed_prerequisites: NO_PREREQUISITES,
        install_order: 1,
    },
    DependencySpec {
        id: DependencyId::PaddleOcr,
        display_name: "PaddleOCR",
        pip_spec: Some("paddleocr[doc-parser]>=2.9.0"),
        probe_code: "import paddleocr; print('ok')",
        critical: true,
        managed_prerequisites: NO_PREREQUISITES,
        install_order: 2,
    },
    DependencySpec {
        id: DependencyId::FasterWhisper,
        display_name: "faster-whisper",
        pip_spec: Some("faster-whisper>=1.0.0"),
        probe_code: "import faster_whisper; print('ok')",
        critical: false,
        managed_prerequisites: NO_PREREQUISITES,
        install_order: 3,
    },
    DependencySpec {
        id: DependencyId::Spacy,
        display_name: "spaCy",
        pip_spec: Some("spacy>=3.7.0,<4.0.0"),
        probe_code: "import spacy; print('ok')",
        critical: false,
        managed_prerequisites: NO_PREREQUISITES,
        install_order: 4,
    },
    DependencySpec {
        id: DependencyId::SpacyModelEs,
        display_name: "spaCy model (es_core_news_sm)",
        pip_spec: None, // installed from a pinned wheel URL via the managed uv venv flow
        probe_code: "import spacy; spacy.load('es_core_news_sm'); print('ok')",
        critical: false,
        managed_prerequisites: SPACY_MODEL_PREREQUISITES,
        install_order: 5,
    },
];

/// Return the full static registry of all managed dependencies.
pub fn all_deps() -> &'static [DependencySpec] {
    ALL_DEPS
}

/// Look up a single dependency by id.
pub fn find_dep(id: &DependencyId) -> Option<&'static DependencySpec> {
    ALL_DEPS.iter().find(|spec| &spec.id == id)
}

/// Return all dependencies in deterministic install order.
pub fn all_deps_in_install_order() -> Vec<&'static DependencySpec> {
    let mut deps = ALL_DEPS.iter().collect::<Vec<_>>();
    deps.sort_by_key(|spec| spec.install_order);
    deps
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_length() {
        assert_eq!(
            all_deps().len(),
            6,
            "Registry should have exactly 6 entries"
        );
    }

    #[test]
    fn test_registry_order() {
        let deps = all_deps();
        for window in deps.windows(2) {
            assert!(
                window[0].install_order < window[1].install_order,
                "Deps should be ordered by install_order: {} ({}) >= {} ({})",
                window[0].display_name,
                window[0].install_order,
                window[1].display_name,
                window[1].install_order,
            );
        }
    }

    #[test]
    fn test_find_dep() {
        let python = find_dep(&DependencyId::Python);
        assert!(python.is_some(), "Python dep must be in registry");
        assert_eq!(python.unwrap().install_order, 0);

        let spacy_model = find_dep(&DependencyId::SpacyModelEs);
        assert!(spacy_model.is_some());
        assert!(
            spacy_model.unwrap().pip_spec.is_none(),
            "SpacyModelEs has no pip_spec"
        );

        assert!(
            find_dep(&DependencyId::Fastembed).is_some(),
            "Fastembed must be in registry"
        );
    }

    #[test]
    fn test_spacy_model_declares_spacy_prerequisite() {
        let spacy_model = find_dep(&DependencyId::SpacyModelEs).expect("spacy model present");
        assert_eq!(spacy_model.managed_prerequisites, SPACY_MODEL_PREREQUISITES);
    }

    #[test]
    fn test_all_deps_in_install_order_is_sorted() {
        let deps = all_deps_in_install_order();
        for window in deps.windows(2) {
            assert!(
                window[0].install_order <= window[1].install_order,
                "install order helper must stay sorted"
            );
        }
    }
}
