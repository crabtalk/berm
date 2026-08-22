//! berm.app — a workbench for harnesses.
//!
//! The app *is* bermd. It owns the [`Service`], binds the endpoint, and runs
//! both on a tokio runtime beside gpui's, so launching this window starts the
//! daemon: a `berm` CLI in a terminal and an agent's MCP client reach the same
//! harnesses the window paints.

use anyhow::{Context as _, Result};
use bermd::{Deployed, Service};
use bezel::{
    gpui::{AnyElement, App, ClipboardItem, Context, Entity, Window, div, prelude::*, px},
    motion,
    theme::Theme,
    ui::{
        focus,
        input::{self, Shape, TextField},
        widgets::{self, Status},
    },
};
use deploy::Sheet;
use run::Invocation;
use std::{collections::HashSet, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{net::TcpListener, runtime::Runtime, sync::broadcast};

mod deploy;
mod detail;
mod run;
mod sidebar;
mod utils;

/// Loopback and bermd's own port, so this app and the daemon are
/// interchangeable and a `berm` CLI finds either without being told.
const ADDR: &str = "127.0.0.1:7777";

/// The strip the traffic lights ride on. Both panes start below it: the window
/// has no titlebar of its own, and the whole strip is the system's drag region.
pub const TITLEBAR: f32 = 44.0;

pub fn init(cx: &mut App) {
    focus::init(cx);
    input::init(cx);
}

/// What the app is, once it is anything.
enum Engine {
    Starting,
    Serving {
        service: Arc<Service>,
        addr: SocketAddr,
    },
    /// A taken port means a second berm already holds this root — not something
    /// to half-work through.
    Failed(String),
}

/// Which half of a harness the pane is showing: what it claims to be, or what
/// it does.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Tools,
    Run,
}

pub struct Workbench {
    /// Beside gpui's own executor, because `Service` is tokio: the
    /// `spawn_blocking` a call enters the guest on panics outside a runtime.
    rt: Runtime,
    engine: Engine,
    /// What the service was holding as of the last refresh.
    harnesses: Vec<Arc<Deployed>>,
    /// By name, not index: a refresh renumbers the list, and a harness removed
    /// under the pane should empty it rather than hand the selection to a
    /// neighbour.
    selection: Option<String>,
    /// Which schemas are open, keyed the way MCP names a tool.
    expanded: HashSet<String>,
    tab: Tab,
    /// The tool the Run pane is pointed at.
    tool: Option<String>,
    arguments: Entity<TextField>,
    /// Every invocation this session, oldest first.
    transcript: Vec<Invocation>,
    /// The deploy sheet, while it is open.
    sheet: Option<Sheet>,
    /// Whether the header is asking about removing the selected harness.
    confirming: bool,
    /// Why the last change to the deployed set was refused. One field for both
    /// deploy and removal: what a person needs is the reason, and which of the
    /// two it was is already on screen.
    refused: Option<String>,
}

impl Workbench {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let rt = Runtime::new().expect("failed to start a tokio runtime");
        let starting = rt.spawn(start());
        cx.spawn(async move |this, cx| {
            let engine = match starting.await {
                Ok(Ok((service, addr))) => Engine::Serving { service, addr },
                Ok(Err(error)) => Engine::Failed(format!("{error:#}")),
                Err(error) => Engine::Failed(format!("startup panicked: {error}")),
            };
            let _ = this.update(cx, |this, cx| {
                this.engine = engine;
                this.refresh(cx);
                this.watch(cx);
                cx.notify();
            });
        })
        .detach();

        Self {
            rt,
            engine: Engine::Starting,
            harnesses: Vec::new(),
            selection: None,
            expanded: HashSet::new(),
            tab: Tab::Tools,
            tool: None,
            arguments: cx.new(|cx| {
                TextField::new(cx)
                    .with_shape(Shape::Rows(8))
                    .with_placeholder("{}")
            }),
            transcript: Vec::new(),
            sheet: None,
            confirming: false,
            refused: None,
        }
    }

    fn selected(&self) -> Option<&Arc<Deployed>> {
        let name = self.selection.as_ref()?;
        self.harnesses
            .iter()
            .find(|deployed| deployed.name == *name)
    }

    /// Show a harness, with the Run pane pointed at its first tool.
    fn select(&mut self, name: &str, cx: &mut Context<Self>) {
        self.selection = Some(name.to_owned());
        self.tool = None;
        self.confirming = false;
        let first = self
            .harnesses
            .iter()
            .find(|deployed| deployed.name == name)
            .and_then(|deployed| deployed.manifest().tools.first().cloned());
        if let Some(tool) = first {
            self.choose(&tool, cx);
        }
    }

    /// Re-read the deployed set.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let Engine::Serving { service, .. } = &self.engine else {
            return;
        };
        let service = service.clone();
        let listing = self.rt.spawn(async move { service.list().await });
        cx.spawn(async move |this, cx| {
            let Ok(harnesses) = listing.await else { return };
            let _ = this.update(cx, |this, cx| {
                this.harnesses = harnesses;
                cx.notify();
            });
        })
        .detach();
    }

    /// Follow the deployed set, which a `berm deploy` from a terminal moves
    /// under this window. The same broadcast MCP sessions turn into
    /// `tools/list_changed`; lagging past its backlog costs a notification, not
    /// the watch.
    fn watch(&self, cx: &mut Context<Self>) {
        let Engine::Serving { service, .. } = &self.engine else {
            return;
        };
        let mut changed = service.subscribe();
        cx.spawn(async move |this, cx| {
            while !matches!(
                changed.recv().await,
                Err(broadcast::error::RecvError::Closed)
            ) {
                if this.update(cx, |this, cx| this.refresh(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// The line that is the app's reason to stay running: what to point an
    /// agent at.
    fn status(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let bar = div()
            .flex_none()
            .h(px(30.0))
            .px(px(14.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .bg(theme.surface)
            .border_t_1()
            .border_color(theme.border)
            .text_size(px(11.5))
            .text_color(theme.text_muted);

        match &self.engine {
            Engine::Starting => bar
                .child(widgets::status_dot(theme.warning))
                .child("starting…")
                .into_any_element(),
            Engine::Failed(error) => bar
                .child(widgets::status_dot(theme.danger))
                .child(div().text_color(theme.danger_muted).child(error.clone()))
                .into_any_element(),
            Engine::Serving { addr, .. } => {
                let mcp = format!("http://{addr}/mcp");
                bar.id("status")
                    .cursor_pointer()
                    .child(widgets::status_dot(theme.success))
                    .child(format!("serving {addr} · mcp at {mcp}"))
                    .child(
                        div()
                            .text_color(theme.text_faint)
                            .child("— click to copy the endpoint"),
                    )
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(mcp.clone()));
                    }))
                    .into_any_element()
            }
        }
    }
}

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        // Hover fades paint once and stick unless a frame is asked for.
        if motion::hover_fades_active() {
            window.request_animation_frame();
        }
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg)
            .text_color(theme.text)
            .font_family(theme.font_sans.clone())
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(self.sidebar(&theme, cx))
                    .child(self.detail(&theme, cx)),
            )
            // A refusal the sheet is not already showing — a removal's, or a
            // deploy's after the sheet closed.
            .when(self.sheet.is_none(), |root| {
                root.when_some(self.refused.clone(), |root, error| {
                    root.child(
                        div()
                            .px(px(28.0))
                            .pb(px(12.0))
                            .child(theme.error_strip(error)),
                    )
                })
            })
            .child(self.status(&theme, cx))
            .child(self.sheet(&theme, cx))
    }
}

/// Open the engine and put it on the wire.
///
/// The listener is bound here rather than inside [`Service::serve`] so a taken
/// port is known before anything claims to be serving.
async fn start() -> Result<(Arc<Service>, SocketAddr)> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    let service = Service::new(PathBuf::from(home).join(".berm")).await?;
    let listener = TcpListener::bind(ADDR)
        .await
        .with_context(|| format!("failed to bind {ADDR}"))?;
    let addr = listener
        .local_addr()
        .context("the listener has no address")?;

    tokio::spawn(service.clone().serve(listener));
    Ok((service, addr))
}
