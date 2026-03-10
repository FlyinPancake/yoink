use leptos::prelude::*;
use yoink_shared::Quality;

use super::select::{Select, SelectGroup, SelectOption};

// ── Helpers ────────────────────────────────────────────────

/// Human-friendly label for a `Quality` value.
pub fn quality_label(quality: Quality) -> &'static str {
    match quality {
        Quality::HiRes => "Hi-Res Lossless",
        Quality::Lossless => "Lossless",
        Quality::High => "High",
        Quality::Low => "Low",
    }
}

/// All quality variants in display order (highest to lowest).
const QUALITY_OPTIONS: [Quality; 4] = [
    Quality::HiRes,
    Quality::Lossless,
    Quality::High,
    Quality::Low,
];

// ── Component ──────────────────────────────────────────────

/// A shadcn/ui-inspired select dropdown for quality overrides.
///
/// Thin wrapper around [`Select`] that builds the option groups
/// for `Option<Quality>` (with a "use default" entry).
#[component]
pub fn QualitySelect(
    /// Current quality override — `None` means "use default".
    selected: Option<Quality>,
    /// The quality that applies when `selected` is `None`.
    default_quality: Quality,
    /// Label prefix for the default option, e.g. `"Use default"` or `"Album default"`.
    #[prop(default = "Use default")]
    default_label_prefix: &'static str,
    /// Called when the user picks a different option.
    on_change: Callback<Option<Quality>>,
) -> impl IntoView {
    // Display text for the trigger
    let display_text = match selected {
        Some(q) => quality_label(q).to_string(),
        None => format!(
            "{} ({})",
            default_label_prefix,
            quality_label(default_quality)
        ),
    };

    // Build option groups: a "default" group, then the quality variants.
    let default_group = SelectGroup {
        options: vec![SelectOption {
            value: None,
            label: format!(
                "{} ({})",
                default_label_prefix,
                quality_label(default_quality)
            ),
        }],
        separator_after: true,
    };

    let quality_group = SelectGroup {
        options: QUALITY_OPTIONS
            .iter()
            .map(|&q| SelectOption {
                value: Some(q),
                label: quality_label(q).to_string(),
            })
            .collect(),
        separator_after: false,
    };

    view! {
        <Select
            selected=selected
            display_text=display_text
            groups=vec![default_group, quality_group]
            on_change=on_change
        />
    }
}
