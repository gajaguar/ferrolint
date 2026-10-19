//! Test plugin for the comment and plugin option queries: it reports each
//! comment that holds a marker named in the plugin's own `markers` option,
//! a comma-separated list. Without the option it reports nothing.
//!
//! | Code   | Reports                                  |
//! | :----- | :--------------------------------------- |
//! | MRK001 | a comment carrying one of the markers    |

use ferrolint_plugin_sdk::prelude::*;

#[derive(Default)]
struct Markers;

impl Checker for Markers {
    fn info() -> Info {
        Info::new("markers", env!("CARGO_PKG_VERSION")).rule(
            Rule::new(
                "MRK001",
                "marker-comment",
                "A comment carries a marker the plugin's options name.",
            )
            .severity(Severity::Warning),
        )
    }

    fn start_file(&mut self, cx: &mut Context<'_>) {
        let markers: Vec<&str> = cx
            .plugin_option("markers")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|marker| !marker.is_empty())
            .collect();
        let found: Vec<_> = cx
            .comments()
            .iter()
            .filter_map(|comment| {
                let marker = markers
                    .iter()
                    .find(|marker| comment.text.contains(**marker))?;
                Some(Diagnostic::new(
                    "MRK001",
                    format!("{marker}: {}", comment.text),
                    comment.range,
                ))
            })
            .collect();
        for diagnostic in found {
            cx.report(diagnostic);
        }
    }
}

export_checker!(Markers);
