//! What a harness says it is: its usage, its tools, their arguments.

use crate::{TITLEBAR, Tab, Workbench, utils};
use berm_api::ToolSpec;
use bermd::Deployed;
use bezel::{
    gpui::{AnyElement, Context, SharedString, div, prelude::*, px},
    theme::Theme,
    ui::{
        icons,
        widgets::{self, Content, Layout, Scaffolding},
    },
};
use std::sync::Arc;

impl Workbench {
    pub(crate) fn detail(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(deployed) = self.selected() else {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(theme.empty_state(
                    icons::WIDGET,
                    "No harness selected",
                    "Pick one on the left to read its tools.",
                ))
                .into_any_element();
        };

        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(self.header(deployed, theme, cx))
            .child(match self.tab {
                Tab::Tools => self.tools(deployed, theme, cx),
                Tab::Run => self.run_pane(deployed, theme, cx),
            })
            .into_any_element()
    }

    fn header(
        &self,
        deployed: &Arc<Deployed>,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_none()
            .px(px(28.0))
            .pt(px(TITLEBAR))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(
                        theme.page_header(
                            deployed.name.clone(),
                            Some(deployed.manifest().tools.len()),
                        ),
                    )
                    .child(div().flex_1())
                    .child(self.remove(theme, cx)),
            )
            .child(
                theme
                    .page_subtitle(utils::short(&deployed.digest).to_owned())
                    .font_family(theme.font_mono.clone()),
            )
            .child(
                theme
                    .tab_bar()
                    .mt(px(16.0))
                    .child(self.tab(Tab::Tools, "Tools", theme, cx))
                    .child(self.tab(Tab::Run, "Run", theme, cx)),
            )
            .into_any_element()
    }

    fn tab(
        &self,
        tab: Tab,
        label: &'static str,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        theme
            .tab(label, self.tab == tab)
            .id(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.tab = tab;
                cx.notify();
            }))
            .into_any_element()
    }

    fn tools(&self, deployed: &Arc<Deployed>, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let manifest = deployed.manifest();
        div()
            .id("tools")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                div()
                    .px(px(28.0))
                    .pb(px(28.0))
                    .flex()
                    .flex_col()
                    // Before the tools, because choosing among them is what it
                    // answers and no single tool's description does.
                    .when(!manifest.usage.is_empty(), |pane| {
                        pane.child(
                            div()
                                .mt(px(20.0))
                                .text_size(px(13.0))
                                .text_color(theme.text_muted)
                                .child(manifest.usage.clone()),
                        )
                    })
                    .child(
                        theme.group_box().children(
                            manifest
                                .tools
                                .iter()
                                .enumerate()
                                .map(|(index, tool)| {
                                    self.tool(&deployed.name, tool, index == 0, theme, cx)
                                })
                                .collect::<Vec<_>>(),
                        ),
                    ),
            )
            .into_any_element()
    }

    fn tool(
        &self,
        harness: &str,
        tool: &ToolSpec,
        first: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Named the way MCP names it, which is also the key the open set uses.
        let key = format!("{harness}.{}", tool.name);
        let open = self.expanded.contains(&key);
        let schema = serde_json::to_string_pretty(&tool.parameters).unwrap_or_default();

        theme
            .card_row(first)
            .flex_col()
            .items_start()
            .gap(px(4.0))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(theme.row_title(tool.name.clone()))
                    .child(theme.badge(SharedString::from(key.clone()))),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(theme.text_muted)
                    .child(tool.description.clone()),
            )
            .child(
                theme
                    .collapsible_header("arguments", open)
                    .id(SharedString::from(key.clone()))
                    .hover(widgets::collapsible_header_hover)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.expanded.remove(&key) {
                            this.expanded.insert(key.clone());
                        }
                        cx.notify();
                    })),
            )
            .when(open, |row| {
                row.child(
                    div()
                        .w_full()
                        .px(px(10.0))
                        .py(px(8.0))
                        .rounded(px(Theme::CONTROL_RADIUS))
                        .bg(theme.surface_raised)
                        .font_family(theme.font_mono.clone())
                        .text_size(px(11.5))
                        .text_color(theme.text_muted)
                        .child(schema),
                )
            })
            .into_any_element()
    }
}
