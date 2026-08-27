//! Changing what is deployed.

use crate::{Engine, Workbench, utils};
use bezel::{
    gpui::{AnyElement, Context, Entity, PathPromptOptions, div, prelude::*, px},
    theme::{self, Theme},
    ui::{
        input::TextField,
        widgets::{ButtonStyle, Buttons, Scaffolding, Status},
    },
};

/// The deploy sheet, while it is open.
pub struct Sheet {
    name: Entity<TextField>,
    image: Entity<TextField>,
}

impl Sheet {
    pub fn new(cx: &mut Context<Workbench>) -> Self {
        Self {
            name: cx.new(|cx| TextField::new(cx).with_placeholder("echo")),
            image: cx.new(|cx| {
                TextField::new(cx).with_placeholder("./harness.elf or ghcr.io/org/echo:v1")
            }),
        }
    }
}

impl Workbench {
    /// Compile an image and make its tools reachable.
    ///
    /// Deploying is where a broken harness is refused, so the compiler's own
    /// words go on screen rather than "deploy failed" — for an author, that
    /// message is the whole point of the screen.
    pub(crate) fn deploy(&mut self, name: String, spec: String, cx: &mut Context<Self>) {
        let Engine::Serving { service, .. } = &self.engine else {
            return;
        };
        let service = service.clone();
        let image = spec.clone();

        let deploying = self.rt.spawn(async move {
            // Reading an image reaches a registry or the filesystem, neither of
            // which a runtime worker can afford to block on.
            let elf = tokio::task::spawn_blocking(move || utils::image(&image)).await??;
            service.deploy(&name, elf).await
        });
        cx.spawn(async move |this, cx| {
            let refused = match deploying.await {
                Ok(Ok(_)) => None,
                Ok(Err(error)) => Some(format!("{error:#}")),
                Err(error) => Some(format!("deploy panicked: {error}")),
            };
            let _ = this.update(cx, |this, cx| {
                this.refused = refused;
                this.deploying = None;
                // The `changed` broadcast brings the new harness in.
                if this.refused.is_none() {
                    this.sheet = None;
                }
                cx.notify();
            });
        })
        .detach();

        self.refused = None;
        self.deploying = Some(spec);
        cx.notify();
    }

    /// What the sheet's two fields are for.
    fn submit(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = &self.sheet else { return };
        let name = sheet.name.read(cx).content().to_string();
        let spec = sheet.image.read(cx).content().to_string();
        self.deploy(name, spec, cx);
    }

    fn undeploy(&mut self, cx: &mut Context<Self>) {
        let Engine::Serving { service, .. } = &self.engine else {
            return;
        };
        let Some(name) = self.selected().map(|deployed| deployed.name.clone()) else {
            return;
        };
        let service = service.clone();
        let removing = self.rt.spawn(async move { service.undeploy(&name).await });
        cx.spawn(async move |this, cx| {
            let refused = match removing.await {
                Ok(Ok(_)) => None,
                Ok(Err(error)) => Some(format!("{error:#}")),
                Err(error) => Some(format!("removal panicked: {error}")),
            };
            let _ = this.update(cx, |this, cx| {
                this.refused = refused;
                cx.notify();
            });
        })
        .detach();

        self.refused = None;
        self.selection = None;
        self.confirming = false;
        cx.notify();
    }

    /// The trash affordance in the harness header, which asks once.
    pub(crate) fn remove(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        if !self.confirming {
            return theme
                .button("Remove", ButtonStyle::Ghost, None)
                .id("remove")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.confirming = true;
                    cx.notify();
                }))
                .into_any_element();
        }

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .child(
                theme
                    .button("Cancel", ButtonStyle::Ghost, None)
                    .id("remove-cancel")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.confirming = false;
                        cx.notify();
                    })),
            )
            .child(
                theme
                    .button("Remove for good", ButtonStyle::Destructive, None)
                    .id("remove-confirm")
                    .on_click(cx.listener(|this, _, _, cx| this.undeploy(cx))),
            )
            .into_any_element()
    }

    pub(crate) fn sheet(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(sheet) = &self.sheet else {
            return div().into_any_element();
        };

        div()
            .absolute()
            .inset_0()
            .bg(theme::scrim(theme::SCRIM_ALPHA_DARK))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(460.0))
                    .p(px(24.0))
                    .rounded(px(Theme::surface_radius()))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface_dialog)
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(theme.page_header("Deploy a harness", None))
                    .child(theme.page_subtitle(
                        "Compiled on the way in, so a broken image is refused here.",
                    ))
                    .child(theme.field_label("Name").mt(px(12.0)))
                    .child(sheet.name.clone())
                    .child(theme.field_label("Image").mt(px(8.0)))
                    .child(sheet.image.clone())
                    .child(
                        div().child(
                            theme
                                .button("Browse…", ButtonStyle::Ghost, None)
                                .id("browse")
                                .on_click(cx.listener(Self::browse)),
                        ),
                    )
                    .when_some(self.refused.clone(), |card, error| {
                        card.child(theme.error_strip(error))
                    })
                    .child(
                        div()
                            .mt(px(16.0))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_end()
                            .gap(px(8.0))
                            .when(self.deploying.is_some(), |row| {
                                row.child(
                                    div()
                                        .mr_auto()
                                        .text_size(px(12.0))
                                        .text_color(theme.text_muted)
                                        .child("compiling…"),
                                )
                            })
                            .child(
                                theme
                                    .button("Cancel", ButtonStyle::Ghost, None)
                                    .id("deploy-cancel")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.sheet = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                theme
                                    .button("Deploy", ButtonStyle::Prominent, None)
                                    .id("deploy-confirm")
                                    .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// The native picker, which also names the harness after the file — the
    /// name an author would have typed anyway.
    fn browse(
        &mut self,
        _: &bezel::gpui::ClickEvent,
        _: &mut bezel::gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_owned();
            let _ = this.update(cx, |this, cx| {
                let Some(sheet) = &this.sheet else { return };
                let (name, image) = (sheet.name.clone(), sheet.image.clone());
                image.update(cx, |field, cx| {
                    field.set_content(path.to_string_lossy().into_owned(), cx)
                });
                if name.read(cx).content().is_empty() {
                    name.update(cx, |field, cx| field.set_content(stem, cx));
                }
                cx.notify();
            });
        })
        .detach();
    }
}
