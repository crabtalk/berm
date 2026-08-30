//! berm.app — a workbench for programs.
//!
//! The app *is* bermd. It owns the [`Service`], binds the endpoint, and runs
//! both on a tokio runtime beside gpui's, so launching this window starts the
//! daemon: a `berm` CLI in a terminal and an agent's MCP client reach the same
//! programs the window paints.

use anyhow::{Context as _, Result};
use berm::Program;
use berm::syscall::call;
use berm_index::{Entry, Source};
use bermd::{Policy, Service};
use bezel::{
    gpui::{
        AnyElement, App, ClipboardItem, Context, Entity, FocusHandle, ScrollHandle, Window, div,
        prelude::*, px,
    },
    motion,
    theme::Theme,
    ui::{
        focus,
        input::{self, Shape, TextField},
        scroll::TransientState,
        widgets::{self, Status},
    },
};
use deploy::Sheet;
use run::Invocation;
use std::{collections::HashSet, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{net::TcpListener, runtime::Runtime, sync::broadcast};

mod deploy;
mod detail;
mod index;
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

/// Which half of a program the pane is showing: what it claims to be, or what
/// it does.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Tools,
    Run,
}

/// What the pane is on: a program this machine holds, or one the index lists.
///
/// The entry is held whole rather than looked up, so editing the search does
/// not empty the pane under the person reading it.
enum Showing {
    Deployed(String),
    Published(Entry),
}

/// What the index last said about the term in the field.
enum Found {
    /// Nothing typed, or nothing asked yet.
    Idle,
    Listed(Vec<Entry>),
    /// The list could not be read at all — a clone that failed, a service that
    /// is not there.
    Unreachable(String),
}

pub struct Workbench {
    /// Beside gpui's own executor, because `Service` is tokio: the
    /// `spawn_blocking` a call enters the guest on panics outside a runtime.
    rt: Runtime,
    engine: Engine,
    /// What the service was holding as of the last refresh.
    programs: Vec<Arc<Program>>,
    /// By name, not index: a refresh renumbers the list, and a program removed
    /// under the pane should empty it rather than hand the selection to a
    /// neighbour.
    selection: Option<Showing>,
    /// The list of published programs, opened once — a `.git` default clones
    /// the first time, and doing that under a keystroke would race a second
    /// clone into the same directory.
    index: Option<Arc<Source>>,
    query: Entity<TextField>,
    /// The term [`Found`] is an answer to, which is also what says whether the
    /// rail is showing what is deployed or what is published.
    term: String,
    found: Found,
    /// Bumped per edit, so an answer that comes back under an old number is
    /// dropped rather than painted over a newer one.
    asked: usize,
    rail: ScrollHandle,
    rail_bar: TransientState,
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
    /// The image being deployed, while it is being deployed — a path from the
    /// sheet or a reference from the index, whichever asked.
    deploying: Option<String>,
    /// Whether the header is asking about removing the selected program.
    confirming: bool,
    /// Why the last change to the deployed set was refused. One field for both
    /// deploy and removal: what a person needs is the reason, and which of the
    /// two it was is already on screen.
    refused: Option<String>,
    /// The window's resting focus: a press moves focus to the innermost handle
    /// it lands in, so without one here a click off a field never takes it.
    focus_handle: FocusHandle,
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

        // Beside the engine rather than after it: reading the list needs
        // nothing from the service, and the clone it may do is slow.
        let opening = rt.spawn_blocking(|| Source::new(None));
        cx.spawn(async move |this, cx| {
            let opened = match opening.await {
                Ok(Ok(source)) => Ok(Arc::new(source)),
                Ok(Err(error)) => Err(format!("{error:#}")),
                Err(error) => Err(format!("opening the index panicked: {error}")),
            };
            let _ = this.update(cx, |this, cx| {
                match opened {
                    Ok(source) => {
                        this.index = Some(source);
                        // A term typed while this was opening was never asked.
                        if !this.term.is_empty() {
                            this.look(cx);
                        }
                    }
                    Err(error) => this.found = Found::Unreachable(error),
                }
                cx.notify();
            });
        })
        .detach();

        let query = cx.new(|cx| TextField::new(cx).with_placeholder("Search the index…"));
        cx.observe(&query, |this, _, cx| this.search(cx)).detach();

        Self {
            rt,
            engine: Engine::Starting,
            programs: Vec::new(),
            selection: None,
            index: None,
            query,
            term: String::new(),
            found: Found::Idle,
            asked: 0,
            rail: ScrollHandle::new(),
            rail_bar: TransientState::new(),
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
            deploying: None,
            confirming: false,
            refused: None,
            focus_handle: cx.focus_handle(),
        }
    }

    /// The deployed program the pane is on, if it is on one.
    fn selected(&self) -> Option<&Arc<Program>> {
        let Some(Showing::Deployed(name)) = &self.selection else {
            return None;
        };
        self.programs
            .iter()
            .find(|deployed| *deployed.name == **name)
    }

    /// Show a program, with the Run pane pointed at its first tool.
    fn select(&mut self, name: &str, cx: &mut Context<Self>) {
        self.selection = Some(Showing::Deployed(name.to_owned()));
        self.tool = None;
        self.confirming = false;
        let first = self
            .programs
            .iter()
            .find(|deployed| &*deployed.name == name)
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
        let listing = self.rt.spawn(async move { service.list() });
        cx.spawn(async move |this, cx| {
            let Ok(programs) = listing.await else { return };
            let _ = this.update(cx, |this, cx| {
                this.programs = programs;
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
        // Traversal on the root, so `tab` works wherever focus happens to be.
        focus::traversal(div())
            .track_focus(&self.focus_handle)
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
    // The workbench exposes no network policy, so it grants none.
    let service = Service::new(
        PathBuf::from(home).join(".berm"),
        call::DEFAULT_CALL_DEPTH,
        Policy::default(),
    )
    .await?;
    let listener = TcpListener::bind(ADDR)
        .await
        .with_context(|| format!("failed to bind {ADDR}"))?;
    let addr = listener
        .local_addr()
        .context("the listener has no address")?;

    tokio::spawn(service.clone().serve(listener));
    Ok((service, addr))
}
