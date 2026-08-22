//! Running a tool, and what came back.

use crate::{Engine, Workbench, utils};
use berm_api::ToolSpec;
use bermd::Deployed;
use bezel::{
    gpui::{AnyElement, Context, SharedString, div, prelude::*, px},
    theme::Theme,
    ui::widgets::{ButtonStyle, Buttons, Content, Controls, Scaffolding, Status},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// One invocation, and how it went.
pub struct Invocation {
    /// Which harness ran it — the transcript is one list, and a pane shows its
    /// own.
    harness: String,
    tool: String,
    arguments: String,
    elapsed: Duration,
    outcome: Outcome,
}

/// The three ways an invocation ends, which is the distinction berm is built
/// around: only the last one is berm's fault.
enum Outcome {
    Running,
    /// The harness returned.
    Done(String),
    /// The harness ran and reported failure — a tool result a model should see
    /// and react to, not a protocol error.
    Failed(String),
    /// A trap, or no such tool. The host's.
    Refused(String),
}

impl Workbench {
    /// Point the Run pane at a tool and seed its arguments from the schema.
    pub(crate) fn choose(&mut self, tool: &ToolSpec, cx: &mut Context<Self>) {
        self.tool = Some(tool.name.clone());
        let arguments = utils::skeleton(&tool.parameters);
        self.arguments
            .update(cx, |field, cx| field.set_content(arguments, cx));
    }

    fn call(&mut self, cx: &mut Context<Self>) {
        let Engine::Serving { service, .. } = &self.engine else {
            return;
        };
        let (Some(harness), Some(tool)) = (
            self.selected().map(|deployed| deployed.name.clone()),
            self.tool.clone(),
        ) else {
            return;
        };
        let service = service.clone();
        let arguments = self.arguments.read(cx).content().to_string();

        let at = self.transcript.len();
        self.transcript.push(Invocation {
            harness: harness.clone(),
            tool: tool.clone(),
            arguments: arguments.clone(),
            elapsed: Duration::ZERO,
            outcome: Outcome::Running,
        });

        let started = Instant::now();
        let call = self
            .rt
            .spawn(async move { service.call(&harness, &tool, arguments.into_bytes()).await });
        cx.spawn(async move |this, cx| {
            let outcome = match call.await {
                Ok(Ok(Ok(result))) => Outcome::Done(result),
                Ok(Ok(Err(failure))) => Outcome::Failed(failure),
                Ok(Err(error)) => Outcome::Refused(format!("{error:#}")),
                Err(error) => Outcome::Refused(format!("invocation panicked: {error}")),
            };
            let elapsed = started.elapsed();
            let _ = this.update(cx, |this, cx| {
                if let Some(entry) = this.transcript.get_mut(at) {
                    entry.outcome = outcome;
                    entry.elapsed = elapsed;
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn run_pane(
        &self,
        deployed: &Arc<Deployed>,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tools = &deployed.manifest().tools;
        div()
            .id("run")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                div()
                    .p(px(28.0))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(theme.field_label("Tool"))
                    .child(
                        theme.toggle_group().children(
                            tools
                                .iter()
                                .map(|tool| {
                                    let name = tool.name.clone();
                                    let selected = self.tool.as_ref() == Some(&name);
                                    let chosen = tool.clone();
                                    theme
                                        .toggle_group_item(name.clone(), selected)
                                        .id(SharedString::from(format!("tool-{name}")))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.choose(&chosen, cx);
                                            cx.notify();
                                        }))
                                        .into_any_element()
                                })
                                .collect::<Vec<_>>(),
                        ),
                    )
                    .child(theme.field_label("Arguments").mt(px(6.0)))
                    .child(self.arguments.clone())
                    .child(
                        div().mt(px(4.0)).child(
                            theme
                                .button("Run", ButtonStyle::Prominent, None)
                                .id("run-tool")
                                .on_click(cx.listener(|this, _, _, cx| this.call(cx))),
                        ),
                    )
                    .children(
                        self.transcript
                            .iter()
                            .enumerate()
                            .filter(|(_, entry)| entry.harness == deployed.name)
                            // Newest first: the result you just asked for is
                            // under the button you pressed, not below the scroll.
                            .rev()
                            .map(|(at, entry)| entry.render(at, theme))
                            .collect::<Vec<_>>(),
                    ),
            )
            .into_any_element()
    }
}

impl Invocation {
    /// The rule down the left is the outcome: which of the three this was reads
    /// before any of the text does.
    fn render(&self, at: usize, theme: &Theme) -> AnyElement {
        let (tone, body, badge) = match &self.outcome {
            Outcome::Running => (theme.text_faint, None, None),
            Outcome::Done(result) => (theme.success, Some(result.clone()), None),
            Outcome::Failed(failure) => (
                theme.warning,
                Some(failure.clone()),
                Some("harness failure"),
            ),
            Outcome::Refused(error) => (theme.danger, Some(error.clone()), Some("refused")),
        };

        div()
            .mt(px(16.0))
            .border_l_2()
            .border_color(tone)
            .pl(px(12.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(theme.row_title(self.tool.clone()))
                    .when_some(badge, |row, label| row.child(theme.badge(label)))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(theme.text_muted)
                            .child(match self.outcome {
                                Outcome::Running => "running…".to_owned(),
                                _ => format!("{:.1} ms", self.elapsed.as_secs_f64() * 1000.0),
                            }),
                    ),
            )
            .child(
                div()
                    .font_family(theme.font_mono.clone())
                    .text_size(px(11.5))
                    .text_color(theme.text_faint)
                    .child(self.arguments.replace('\n', " ")),
            )
            .when_some(body, |entry, text| {
                entry.child(
                    theme
                        // Its place in the transcript, which is append-only and
                        // so never renumbers what is already on screen.
                        .step_output(SharedString::from(format!("result-{at}")), text)
                        .w_full()
                        .border_t_0()
                        .mt(px(4.0))
                        .rounded(px(Theme::CONTROL_RADIUS))
                        .bg(theme.surface_raised),
                )
            })
            .into_any_element()
    }
}
