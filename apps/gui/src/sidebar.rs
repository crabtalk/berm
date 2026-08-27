//! The rail of harnesses — what is deployed, or what the index lists.

use crate::{Found, Sheet, Showing, TITLEBAR, Workbench, utils};
use berm::Harness;
use bezel::{
    gpui::{AnyElement, Context, SharedString, div, prelude::*, px},
    theme::Theme,
    ui::{
        scroll,
        widgets::{self, ButtonStyle, Buttons, Scaffolding},
    },
};
use std::sync::Arc;

const WIDTH: f32 = 260.0;

impl Workbench {
    pub(crate) fn sidebar(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        // One rail, two lists: an empty field is what this machine holds, and a
        // term is what has been published.
        let (heading, count) = match self.term.is_empty() {
            true => ("Harnesses", self.harnesses.len()),
            false => match &self.found {
                Found::Listed(entries) => ("Index", entries.len()),
                _ => ("Index", 0),
            },
        };

        div()
            .w(px(WIDTH))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface)
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .px(px(20.0))
                    .pt(px(TITLEBAR))
                    .pb(px(10.0))
                    .child(theme.page_header(heading, Some(count))),
            )
            .child(div().px(px(16.0)).pb(px(10.0)).child(self.query.clone()))
            .child(self.list(theme, cx))
            .child(
                div()
                    .flex_none()
                    .p(px(16.0))
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        theme
                            .button("Deploy…", ButtonStyle::Prominent, None)
                            .id("deploy")
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refused = None;
                                this.sheet = Some(Sheet::new(cx));
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    fn list(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let rail = div()
            .id("harnesses")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.rail);

        let note = |copy: String| {
            div()
                .px(px(20.0))
                .py(px(10.0))
                .text_size(px(12.5))
                .text_color(theme.text_muted)
                .child(copy)
        };
        let rail = if self.term.is_empty() {
            rail.children(
                self.harnesses
                    .iter()
                    .enumerate()
                    .map(|(index, deployed)| self.row(deployed, index == 0, theme, cx))
                    .collect::<Vec<_>>(),
            )
        } else {
            match &self.found {
                Found::Idle => rail.child(note("searching…".to_owned())),
                Found::Unreachable(error) => {
                    rail.child(note(error.clone()).text_color(theme.danger_muted))
                }
                Found::Listed(entries) if entries.is_empty() => {
                    rail.child(note(format!("nothing published matches {:?}", self.term)))
                }
                Found::Listed(entries) => rail.children(
                    entries
                        .iter()
                        .enumerate()
                        .map(|(index, entry)| self.result(entry, index == 0, theme, cx))
                        .collect::<Vec<_>>(),
                ),
            }
        };

        // Relative, because the bar is absolute in here and overlays the list
        // rather than taking a gutter out of it.
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(rail)
            .child(scroll::transient(
                "harnesses-bar",
                &self.rail,
                &self.rail_bar,
                cx.reduce_motion(),
            ))
            .into_any_element()
    }

    fn row(
        &self,
        deployed: &Arc<Harness>,
        first: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = deployed.name.to_string();
        let selected =
            matches!(&self.selection, Some(Showing::Deployed(at)) if at.as_str() == &*name);
        let tools = deployed.manifest().tools.len();

        theme
            .card_row(first)
            .id(SharedString::from(name.clone()))
            .py(px(10.0))
            .cursor_pointer()
            .when(selected, |row| row.bg(theme.element_active))
            .hover(widgets::card_row_hover)
            .child(widgets::status_dot(theme.success))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title(name.clone()))
                    .child(theme.meta_line(vec![
                        div()
                            .child(format!("{tools} tool{}", if tools == 1 { "" } else { "s" }))
                            .into_any_element(),
                        div()
                            .font_family(theme.font_mono.clone())
                            .child(utils::short(&deployed.digest).to_owned())
                            .into_any_element(),
                    ])),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select(&name, cx);
                cx.notify();
            }))
            .into_any_element()
    }
}
