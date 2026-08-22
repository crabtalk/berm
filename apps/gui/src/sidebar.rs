//! The rail of deployed harnesses.

use crate::{Sheet, Workbench, utils};
use bermd::Deployed;
use bezel::{
    gpui::{AnyElement, Context, SharedString, div, prelude::*, px},
    theme::Theme,
    ui::widgets::{self, ButtonStyle, Buttons, Scaffolding},
};
use std::sync::Arc;

const WIDTH: f32 = 260.0;

impl Workbench {
    pub(crate) fn sidebar(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
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
                    .pt(px(20.0))
                    .pb(px(12.0))
                    .child(theme.page_header("Harnesses", Some(self.harnesses.len()))),
            )
            .child(
                div()
                    .id("harnesses")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(
                        self.harnesses
                            .iter()
                            .enumerate()
                            .map(|(index, deployed)| self.row(deployed, index == 0, theme, cx))
                            .collect::<Vec<_>>(),
                    ),
            )
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

    fn row(
        &self,
        deployed: &Arc<Deployed>,
        first: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = deployed.name.clone();
        let selected = self.selection.as_ref() == Some(&name);
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
