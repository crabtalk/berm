//! What has been published, and putting one of it on this machine.

use crate::{Found, Showing, TITLEBAR, Tab, Workbench, utils};
use berm_index::Entry;
use bezel::{
    gpui::{AnyElement, Context, SharedString, div, prelude::*, px},
    theme::Theme,
    ui::widgets::{self, ButtonStyle, Buttons, Layout, Scaffolding},
};
use std::time::Duration;

/// How long a keystroke waits for the next one before the index is asked.
const DEBOUNCE: Duration = Duration::from_millis(200);

impl Workbench {
    /// Follow the field. `observe` fires for a cursor move as well as an edit,
    /// so the term already asked about is what says whether to ask again.
    pub(crate) fn search(&mut self, cx: &mut Context<Self>) {
        let term = self.query.read(cx).content().to_string();
        if term == self.term {
            return;
        }
        self.term = term;
        self.asked += 1;
        match self.term.is_empty() {
            true => self.found = Found::Idle,
            false => self.look(cx),
        }
        cx.notify();
    }

    /// Ask the index what the term matches, once the typing stops.
    ///
    /// The previous answer stays on screen until this one lands, so a rail that
    /// has something in it never blinks empty between keystrokes.
    pub(crate) fn look(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.index.clone() else {
            return;
        };
        let (term, asked) = (self.term.clone(), self.asked);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            // Another keystroke landed while this one waited out the pause.
            let Ok(Some(searching)) = this.update(cx, |this, _| {
                (this.asked == asked).then(|| this.rt.spawn_blocking(move || source.search(&term)))
            }) else {
                return;
            };

            let found = match searching.await {
                Ok(Ok(entries)) => Found::Listed(entries),
                Ok(Err(error)) => Found::Unreachable(format!("{error:#}")),
                Err(error) => Found::Unreachable(format!("the search panicked: {error}")),
            };
            let _ = this.update(cx, |this, cx| {
                if this.asked != asked {
                    return;
                }
                this.found = found;
                cx.notify();
            });
        })
        .detach();
    }

    /// Deploy what an entry points at, under the name its author gave the
    /// image — the last segment of the repository, as `browse` uses a file's
    /// stem.
    fn install(&mut self, entry: &Entry, cx: &mut Context<Self>) {
        let name = utils::split(entry).0.to_owned();
        self.deploy(name, entry.reference.clone(), cx);
    }

    /// Whether these exact bytes are already here. By digest rather than by
    /// name, because a name is this machine's and the bytes are the harness.
    fn installed(&self, entry: &Entry) -> bool {
        let digest = entry
            .digest
            .strip_prefix("sha256:")
            .unwrap_or(&entry.digest);
        self.harnesses
            .iter()
            .any(|deployed| deployed.digest == digest)
    }

    /// One published harness in the rail.
    pub(crate) fn result(
        &self,
        entry: &Entry,
        first: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (name, version) = utils::split(entry);
        let selected = matches!(
            &self.selection,
            Some(Showing::Published(showing)) if showing.reference == entry.reference
        );
        let tools = entry.tools.len();
        let showing = entry.clone();

        theme
            .card_row(first)
            .id(SharedString::from(entry.reference.clone()))
            .py(px(10.0))
            .cursor_pointer()
            .when(selected, |row| row.bg(theme.element_active))
            .hover(widgets::card_row_hover)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    // The name and its version, not the whole reference: a rail
                    // this wide truncates a registry path mid-word.
                    .child(theme.row_title(name.to_owned()))
                    .child(theme.meta_line(vec![
                        div()
                            .child(format!("{tools} tool{}", if tools == 1 { "" } else { "s" }))
                            .into_any_element(),
                        div().child(version.to_owned()).into_any_element(),
                    ])),
            )
            .child(self.get(entry, theme, cx))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selection = Some(Showing::Published(showing.clone()));
                this.confirming = false;
                cx.notify();
            }))
            .into_any_element()
    }

    /// The one button that makes a published harness real, and what it says
    /// while it is not a button.
    fn get(&self, entry: &Entry, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let note = |copy: &'static str| {
            div()
                .flex_none()
                .text_size(px(11.5))
                .text_color(theme.text_muted)
                .child(copy)
                .into_any_element()
        };
        if self.deploying.as_ref() == Some(&entry.reference) {
            return note("installing…");
        }
        if self.installed(entry) {
            return note("installed");
        }

        let entry = entry.clone();
        theme
            .button("Install", ButtonStyle::Prominent, None)
            .id(SharedString::from(format!("install-{}", entry.reference)))
            .flex_none()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.install(&entry, cx);
                cx.notify();
            }))
            .into_any_element()
    }

    /// A published harness read from the index: the same tools the pane shows
    /// for a deployed one, from a listing rather than from an image.
    pub(crate) fn published(
        &self,
        entry: &Entry,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
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
                            .gap(px(12.0))
                            .child(
                                theme.page_header(entry.reference.clone(), Some(entry.tools.len())),
                            )
                            .child(div().flex_1())
                            .child(self.get(entry, theme, cx)),
                    )
                    .child(
                        theme
                            .page_subtitle(utils::short(&entry.digest).to_owned())
                            .font_family(theme.font_mono.clone()),
                    )
                    .when_some(entry.publisher.clone(), |header, publisher| {
                        header.child(theme.page_subtitle(format!("published by {publisher}")))
                    })
                    .child(theme.tab_bar().mt(px(16.0)).child(self.tab(
                        Tab::Tools,
                        "Tools",
                        theme,
                        cx,
                    ))),
            )
            // Named for what it would be called here, not for where it is
            // published: the badge is the name MCP would answer to.
            .child(self.tools(utils::split(entry).0, &entry.usage, &entry.tools, theme, cx))
            .into_any_element()
    }
}
