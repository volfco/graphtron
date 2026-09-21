use dioxus::prelude::*;
use graphtron::LegendFormat;
use std::collections::HashSet;

/// DOM legend: one clickable chip per series. Clicking toggles the series in
/// the shared `hidden` set, which `GraphtronChart` uses to filter what it draws.
///
/// Rendered automatically by `GraphtronChart` when `legend: true`, or usable
/// standalone with your own entries/hidden wiring.
#[component]
pub fn Legend(
    entries: Vec<(String, u32)>,
    hidden: Signal<HashSet<usize>>,
    /// `GraphtronChart` preformats values according to this setting. Kept as a
    /// prop so standalone callers can use the same semantic configuration.
    #[props(default)]
    format: LegendFormat,
) -> Element {
    let format_hint = match format {
        LegendFormat::NameOnly => "series names",
        LegendFormat::NameAndValue => "series names and values",
        LegendFormat::ValueOnly => "series values",
    };
    rsx! {
        div {
            class: "graphtron-legend",
            role: "group",
            aria_label: "Chart legend showing {format_hint}; activate a series to toggle its visibility",
            style: "display: flex; flex-wrap: wrap; gap: 4px 12px; padding: 4px 2px; \
                    font-size: 11px; line-height: 1.6; user-select: none;",
            for (i, (name, color)) in entries.iter().enumerate() {
                {
                    let is_hidden = hidden().contains(&i);
                    let css = format!("#{color:06x}");
                    let opacity = if is_hidden { "0.35" } else { "1.0" };
                    let deco = if is_hidden { "line-through" } else { "none" };
                    let action = if is_hidden { "Show" } else { "Hide" };
                    rsx! {
                        button {
                            key: "{i}",
                            r#type: "button",
                            aria_pressed: (!is_hidden).to_string(),
                            aria_label: "{action} {name}",
                            style: "display: inline-flex; align-items: center; gap: 5px; border: 0; \
                                    background: transparent; color: inherit; font: inherit; padding: 0; \
                                    cursor: pointer; opacity: {opacity}; text-decoration: {deco};",
                            onclick: move |_| {
                                let mut h = hidden();
                                if !h.remove(&i) { h.insert(i); }
                                hidden.set(h);
                            },
                            span {
                                style: "width: 10px; height: 3px; border-radius: 1px; \
                                        background: {css}; display: inline-block;",
                            }
                            "{name}"
                        }
                    }
                }
            }
        }
    }
}
