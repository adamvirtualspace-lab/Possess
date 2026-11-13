//! The Graph view: notes as nodes, tasks as rows, `needs::`/`sync::` as wires.
//!
//! Everything it shows comes from `GET /api/graph`, and every change goes
//! back through the server, which writes the files and the tasks a change
//! unblocks. This view only draws, and remembers where you've panned.
//!
//! Geometry is fixed (node width, header and row heights), so a wire's ends
//! are computed rather than measured from the DOM. Nodes are keyed by path
//! plus a reload counter: they keep their DOM while you drag them and are
//! rebuilt when the graph reloads. The pointer is captured on the canvas,
//! which never re-renders, so a drag survives any redraw under it.

use std::collections::HashMap;

use leptos::html;
use leptos::prelude::*;
use possess_common::{GraphEdge, GraphNode, GraphResponse, GraphStatus, GraphTask};

use crate::api;
use crate::state::{AppState, ViewMode};

const NODE_W: f64 = 264.0;
const HEAD_H: f64 = 54.0;
const ROW_H: f64 = 30.0;
const FOOT_H: f64 = 28.0;
/// A pointer has to move this far before a press on a node becomes a drag,
/// so a click on its title still opens the note.
const DRAG_SLOP: f64 = 4.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    Tasks,
    Compact,
}

/// What the pointer is doing on the canvas.
#[derive(Clone)]
enum Drag {
    Pan {
        start: (f64, f64),
        pan: (f64, f64),
    },
    Node {
        path: String,
        start: (f64, f64),
        origin: (f64, f64),
        moved: bool,
    },
}

/// The status menu, opened by right-clicking a task's status.
#[derive(Clone)]
struct Menu {
    node: String,
    line: usize,
    /// Where to draw it, relative to the graph panel.
    at: (f64, f64),
}

fn node_height(node: &GraphNode, detail: Detail) -> f64 {
    let rows = match detail {
        Detail::Compact => 1,
        Detail::Tasks => node.tasks.len().max(1),
    };
    HEAD_H + rows as f64 * ROW_H + FOOT_H
}

/// Where a wire meets a task row, in world coordinates. Compact nodes have
/// one strip, so every wire meets its middle.
fn pin_y(node: &GraphNode, line: usize, y: f64, detail: Detail) -> f64 {
    let row = match detail {
        Detail::Compact => 0,
        Detail::Tasks => node.tasks.iter().position(|t| t.line == line).unwrap_or(0),
    };
    y + HEAD_H + row as f64 * ROW_H + ROW_H / 2.0
}

/// A colour per top-level folder, so departments read at a glance.
fn department(path: &str) -> &'static str {
    const COLOURS: [&str; 6] = [
        "var(--dept-1)",
        "var(--dept-2)",
        "var(--dept-3)",
        "var(--dept-4)",
        "var(--dept-5)",
        "var(--dept-6)",
    ];
    let folder = path.split('/').next().unwrap_or(path);
    let hash = folder
        .bytes()
        .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    COLOURS[hash as usize % COLOURS.len()]
}

fn wire_path(from: (f64, f64), to: (f64, f64)) -> String {
    let bend = ((to.0 - from.0).abs() / 2.0).max(60.0);
    format!(
        "M {} {} C {} {}, {} {}, {} {}",
        from.0,
        from.1,
        from.0 + bend,
        from.1,
        to.0 - bend,
        to.1,
        to.0,
        to.1
    )
}

/// The flow to show: the open note, if it has a `::flow::` block.
fn current_flow(state: &AppState) -> Option<String> {
    let path = state.current_file.get_untracked()?;
    let content = state.editor().content();
    possess_markdown::tasks::parse(&content).flow.map(|_| path)
}

#[derive(Clone, Copy)]
struct Graph {
    data: RwSignal<Option<GraphResponse>>,
    /// Bumped on every reload, so nodes are rebuilt only then.
    revision: RwSignal<u32>,
    error: RwSignal<Option<String>>,
    pan: RwSignal<(f64, f64)>,
    zoom: RwSignal<f64>,
    /// Positions moved since the last reload, by node path.
    moved: RwSignal<HashMap<String, (f64, f64)>>,
    detail: RwSignal<Detail>,
    drag: RwSignal<Option<Drag>>,
    menu: RwSignal<Option<Menu>>,
    /// Fit to the screen after the next load.
    fit_pending: RwSignal<bool>,
}

impl Graph {
    fn position(&self, node: &GraphNode) -> (f64, f64) {
        self.moved
            .with(|m| m.get(&node.path).copied())
            .unwrap_or((node.x as f64, node.y as f64))
    }
}

async fn reload(graph: Graph, flow: Option<String>) {
    match api::get_graph(flow.as_deref()).await {
        Ok(data) => {
            graph.moved.set(HashMap::new());
            graph.data.set(Some(data));
            graph.revision.update(|r| *r += 1);
            graph.error.set(None);
        }
        Err(err) => graph.error.set(Some(err)),
    }
}

#[component]
pub fn GraphView() -> impl IntoView {
    let state = expect_context::<AppState>();
    let graph = Graph {
        data: RwSignal::new(None),
        revision: RwSignal::new(0),
        error: RwSignal::new(None),
        pan: RwSignal::new((40.0, 40.0)),
        zoom: RwSignal::new(1.0),
        moved: RwSignal::new(HashMap::new()),
        detail: RwSignal::new(Detail::Tasks),
        drag: RwSignal::new(None),
        menu: RwSignal::new(None),
        fit_pending: RwSignal::new(true),
    };
    let canvas = NodeRef::<html::Div>::new();
    let flow = RwSignal::new(None::<String>);
    let activity_open = RwSignal::new(true);

    // Redraw when the server reports a change (another app ticked a task,
    // or an undo), unless a node is being dragged: it would jump back.
    {
        let state = state.clone();
        Effect::new(move |previous: Option<u64>| {
            let revision = state.revision.get();
            let visible = state.view_mode.get_untracked() == ViewMode::Graph;
            if previous.is_some() && visible && graph.drag.get_untracked().is_none() {
                leptos::task::spawn_local(reload(graph, flow.get_untracked()));
            }
            revision
        });
    }

    // Load whenever the Graph view is shown, for whatever note is open then.
    {
        let state = state.clone();
        Effect::new(move |_| {
            if state.view_mode.get() != ViewMode::Graph {
                return;
            }
            let wanted = current_flow(&state);
            if wanted != flow.get_untracked() {
                graph.fit_pending.set(true);
            }
            flow.set(wanted.clone());
            // The graph reads notes from disk, and the open one may hold
            // edits the autosave hasn't written yet.
            let state = state.clone();
            leptos::task::spawn_local(async move {
                if let Err(err) = state.save_now().await {
                    graph.error.set(Some(err));
                }
                reload(graph, wanted).await;
            });
        });
    }

    // Fit once the canvas has a size and the data is in.
    Effect::new(move |_| {
        graph.revision.track();
        if graph.fit_pending.get_untracked() && graph.data.with(Option::is_some) {
            if let Some(el) = canvas.get() {
                if el.client_width() > 0 {
                    fit(graph, &el);
                    graph.fit_pending.set(false);
                }
            }
        }
    });

    let capture = move |ev: &web_sys::PointerEvent| {
        if let Some(el) = canvas.get_untracked() {
            let _ = el.set_pointer_capture(ev.pointer_id());
        }
    };

    let on_canvas_down = move |ev: web_sys::PointerEvent| {
        if ev.button() != 0 {
            return;
        }
        graph.menu.set(None);
        capture(&ev);
        graph.drag.set(Some(Drag::Pan {
            start: (ev.client_x() as f64, ev.client_y() as f64),
            pan: graph.pan.get_untracked(),
        }));
    };

    let on_move = move |ev: web_sys::PointerEvent| {
        let Some(drag) = graph.drag.get_untracked() else {
            return;
        };
        let (x, y) = (ev.client_x() as f64, ev.client_y() as f64);
        match drag {
            Drag::Pan { start, pan } => {
                graph.pan.set((pan.0 + x - start.0, pan.1 + y - start.1));
            }
            Drag::Node {
                path,
                start,
                origin,
                moved,
            } => {
                let z = graph.zoom.get_untracked();
                let (dx, dy) = ((x - start.0) / z, (y - start.1) / z);
                if !moved && dx.hypot(dy) * z < DRAG_SLOP {
                    return;
                }
                graph.moved.update(|m| {
                    m.insert(
                        path.clone(),
                        ((origin.0 + dx).round(), (origin.1 + dy).round()),
                    );
                });
                graph.drag.set(Some(Drag::Node {
                    path,
                    start,
                    origin,
                    moved: true,
                }));
            }
        }
    };

    let on_up = {
        let state = state.clone();
        move |_ev: web_sys::PointerEvent| {
            let Some(drag) = graph.drag.get_untracked() else {
                return;
            };
            graph.drag.set(None);
            let Drag::Node { path, moved, .. } = drag else {
                return;
            };
            if !moved {
                // A press that didn't move is a click on the title: open it.
                let file = graph.data.with_untracked(|d| {
                    d.as_ref()
                        .and_then(|d| d.nodes.iter().find(|n| n.path == path))
                        .filter(|n| !n.missing)
                        .map(|n| n.file.clone())
                });
                if let Some(file) = file {
                    open_note(state.clone(), file);
                }
                return;
            }
            save_position(state.clone(), graph, flow.get_untracked(), path);
        }
    };

    let on_wheel = move |ev: web_sys::WheelEvent| {
        ev.prevent_default();
        let Some(el) = canvas.get_untracked() else {
            return;
        };
        let rect = el.get_bounding_client_rect();
        let cursor = (
            ev.client_x() as f64 - rect.left(),
            ev.client_y() as f64 - rect.top(),
        );
        let z = graph.zoom.get_untracked();
        let next = (z * (1.0 - ev.delta_y() * 0.0015)).clamp(0.2, 2.5);
        let (px, py) = graph.pan.get_untracked();
        let ratio = next / z;
        graph.zoom.set(next);
        graph.pan.set((
            cursor.0 - (cursor.0 - px) * ratio,
            cursor.1 - (cursor.1 - py) * ratio,
        ));
    };

    let zoom_by = move |factor: f64| {
        let Some(el) = canvas.get_untracked() else {
            return;
        };
        let centre = (
            el.client_width() as f64 / 2.0,
            el.client_height() as f64 / 2.0,
        );
        let z = graph.zoom.get_untracked();
        let next = (z * factor).clamp(0.2, 2.5);
        let (px, py) = graph.pan.get_untracked();
        let ratio = next / z;
        graph.zoom.set(next);
        graph.pan.set((
            centre.0 - (centre.0 - px) * ratio,
            centre.1 - (centre.1 - py) * ratio,
        ));
    };

    let world_style = move || {
        let (px, py) = graph.pan.get();
        format!(
            "transform: translate({px}px, {py}px) scale({});",
            graph.zoom.get()
        )
    };
    let grid_style = move || {
        let (px, py) = graph.pan.get();
        let size = 24.0 * graph.zoom.get();
        format!("background-size: {size}px {size}px; background-position: {px}px {py}px;")
    };

    let title = move || match flow.get() {
        Some(path) => path.trim_end_matches(".md").to_string(),
        None => "Whole vault".to_string(),
    };

    let show = {
        let state = state.clone();
        move || state.view_mode.get() == ViewMode::Graph
    };

    view! {
        <div
            id="graph-panel"
            class="graph-panel"
            style:display=move || if show() { "flex" } else { "none" }
        >
            <div class="graph-toolbar">
                <span class="graph-title">
                    <span class="graph-kind">{move || if flow.get().is_some() { "flow" } else { "graph" }}</span>
                    {title}
                </span>
                <span class="graph-error">{move || graph.error.get()}</span>
                <div class="btn-group">
                    <span class="graph-label">"Nodes show"</span>
                    <button
                        class="btn"
                        class:active=move || graph.detail.get() == Detail::Tasks
                        on:click=move |_| graph.detail.set(Detail::Tasks)
                    >"Tasks"</button>
                    <button
                        class="btn"
                        class:active=move || graph.detail.get() == Detail::Compact
                        on:click=move |_| graph.detail.set(Detail::Compact)
                    >"Compact"</button>
                </div>
                <div class="btn-group">
                    <button
                        class="btn"
                        id="btn-activity"
                        class:active=move || activity_open.get()
                        on:click=move |_| activity_open.update(|open| *open = !*open)
                    >"Activity"</button>
                </div>
                <div class="btn-group">
                    <button class="btn" title="Zoom out" on:click=move |_| zoom_by(1.0 / 1.2)>"−"</button>
                    <button
                        class="btn"
                        title="Fit everything on screen"
                        on:click=move |_| {
                            if let Some(el) = canvas.get_untracked() {
                                fit(graph, &el);
                            }
                        }
                    >"Fit"</button>
                    <button class="btn" title="Zoom in" on:click=move |_| zoom_by(1.2)>"+"</button>
                </div>
            </div>

            <div class="graph-body">
            <div
                class="graph-canvas"
                class:is-panning=move || matches!(graph.drag.get(), Some(Drag::Pan { .. }))
                node_ref=canvas
                style=grid_style
                on:pointerdown=on_canvas_down
                on:pointermove=on_move
                on:pointerup=on_up
                on:wheel=on_wheel
                on:contextmenu=|ev: web_sys::MouseEvent| ev.prevent_default()
            >
                <div class="graph-world" style=world_style>
                    <Wires graph=graph />
                    {move || {
                        graph.revision.track();
                        let nodes = graph
                            .data
                            .with_untracked(|d| d.as_ref().map(|d| d.nodes.clone()).unwrap_or_default());
                        nodes
                            .into_iter()
                            .map(|node| {
                                view! {
                                    <Node
                                        graph=graph
                                        node=node
                                        flow=flow
                                        capture=capture
                                    />
                                }
                            })
                            .collect_view()
                    }}
                </div>
                {move || {
                    graph.data.with(|d| {
                        d.as_ref().filter(|d| d.nodes.is_empty()).map(|_| view! {
                            <div class="graph-empty">
                                "No notes with tasks yet. Add a task like "
                                <code>"- [ ] Something to do"</code>
                                " to a note, or open a flow note."
                            </div>
                        })
                    })
                }}
            </div>
            <crate::activity::ActivityPanel
                open=activity_open
                on_change=move || leptos::task::spawn_local(reload(graph, flow.get_untracked()))
            />
            </div>

            <StatusMenu graph=graph flow=flow />
        </div>
    }
}

/// Every wire, behind the nodes.
#[component]
fn Wires(graph: Graph) -> impl IntoView {
    let paths = move || {
        let detail = graph.detail.get();
        graph.moved.track();
        graph.data.with(|data| {
            let Some(data) = data else { return Vec::new() };
            let by_path: HashMap<&str, &GraphNode> =
                data.nodes.iter().map(|n| (n.path.as_str(), n)).collect();
            data.edges
                .iter()
                // A wire inside one note would loop back on itself; its lock
                // already says what it means.
                .filter(|edge| edge.from_node != edge.to_node)
                .filter_map(|edge: &GraphEdge| {
                    let from = by_path.get(edge.from_node.as_str())?;
                    let to = by_path.get(edge.to_node.as_str())?;
                    let (fx, fy) = graph.position(from);
                    let (tx, ty) = graph.position(to);
                    let start = (fx + NODE_W, pin_y(from, edge.from_line, fy, detail));
                    let end = (tx, pin_y(to, edge.to_line, ty, detail));
                    let class = match (edge.kind.as_str(), edge.live) {
                        ("sync", _) => "wire wire-sync",
                        (_, true) => "wire wire-live",
                        _ => "wire wire-waiting",
                    };
                    Some((wire_path(start, end), class, start, end))
                })
                .collect::<Vec<_>>()
        })
    };

    view! {
        <svg class="graph-wires" width="1" height="1">
            {move || {
                paths()
                    .into_iter()
                    .map(|(d, class, start, end)| {
                        view! {
                            <path class=class d=d></path>
                            <circle class="pin" cx=start.0 cy=start.1 r="4"></circle>
                            <circle class="pin" cx=end.0 cy=end.1 r="4"></circle>
                        }
                    })
                    .collect_view()
            }}
        </svg>
    }
}

#[component]
fn Node(
    graph: Graph,
    node: GraphNode,
    flow: RwSignal<Option<String>>,
    capture: impl Fn(&web_sys::PointerEvent) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let path = node.path.clone();
    let style = {
        let node = node.clone();
        move || {
            let (x, y) = graph.position(&node);
            format!("left: {x}px; top: {y}px; width: {NODE_W}px;")
        }
    };
    let dragging = {
        let path = path.clone();
        move || matches!(graph.drag.get(), Some(Drag::Node { path: ref p, moved: true, .. }) if *p == path)
    };

    let on_head_down = {
        let node = node.clone();
        move |ev: web_sys::PointerEvent| {
            if ev.button() != 0 {
                return;
            }
            ev.stop_propagation();
            graph.menu.set(None);
            capture(&ev);
            graph.drag.set(Some(Drag::Node {
                path: node.path.clone(),
                start: (ev.client_x() as f64, ev.client_y() as f64),
                origin: graph.position(&node),
                moved: false,
            }));
        }
    };

    let footer = format!("in {} · out {}", node.wires_in, node.wires_out);
    let counts = format!("{} open of {}", node.open, node.tasks.len());
    let strip = department(&node.path);
    let compact_strip: String = node
        .tasks
        .iter()
        .map(|t| t.symbol.clone())
        .collect::<Vec<_>>()
        .join(" ");
    let tasks = node.tasks.clone();
    let node_path = node.path.clone();
    let has_tasks = !tasks.is_empty();
    let warnings = node.warnings;

    view! {
        <div
            class="graph-node"
            class:is-missing=node.missing
            class:is-dragging=dragging
            data-path=path.clone()
            style=style
            on:pointerdown=|ev: web_sys::PointerEvent| ev.stop_propagation()
        >
            <div class="graph-node-head" on:pointerdown=on_head_down title="Drag to move, click to open">
                <span class="graph-node-strip" style:background=strip></span>
                <div class="graph-node-names">
                    <span class="graph-node-title">{node.title.clone()}</span>
                    <span class="graph-node-path">
                        {if node.missing { format!("{} · missing note", node.file) } else { node.file.clone() }}
                    </span>
                </div>
                {(warnings > 0).then(|| view! {
                    <span class="graph-node-warn" title=format!("{warnings} warning(s) in this note")>"⚠"</span>
                })}
            </div>
            <div class="graph-node-body">
                {move || match graph.detail.get() {
                    Detail::Compact => view! {
                        <div class="graph-compact">{compact_strip.clone()}</div>
                    }.into_any(),
                    Detail::Tasks if !has_tasks => view! {
                        <div class="graph-row graph-row-empty">"No tasks"</div>
                    }.into_any(),
                    Detail::Tasks => tasks
                        .iter()
                        .map(|task| view! {
                            <TaskRow
                                graph=graph
                                node=node_path.clone()
                                task=task.clone()
                                flow=flow
                            />
                        })
                        .collect_view()
                        .into_any(),
                }}
            </div>
            <div class="graph-node-foot">
                <span>{footer}</span>
                <span>{counts}</span>
            </div>
        </div>
    }
}

#[component]
fn TaskRow(
    graph: Graph,
    node: String,
    task: GraphTask,
    flow: RwSignal<Option<String>>,
) -> impl IntoView {
    let state = expect_context::<AppState>();
    let click = {
        let node = node.clone();
        let task = task.clone();
        let state = state.clone();
        move |ev: web_sys::MouseEvent| {
            ev.stop_propagation();
            let Some(next) = task.next else { return };
            change_status(
                state.clone(),
                graph,
                flow.get_untracked(),
                node.clone(),
                task.line,
                task.ch,
                next,
            );
        }
    };
    let right_click = {
        let node = node.clone();
        let line = task.line;
        move |ev: web_sys::MouseEvent| {
            ev.prevent_default();
            ev.stop_propagation();
            let panel = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("graph-panel"));
            let (left, top) = panel
                .map(|p| p.get_bounding_client_rect())
                .map_or((0.0, 0.0), |r| (r.left(), r.top()));
            graph.menu.set(Some(Menu {
                node: node.clone(),
                line,
                at: (ev.client_x() as f64 - left, ev.client_y() as f64 - top),
            }));
        }
    };

    let tooltip = match (&task.warning, task.locked) {
        (Some(w), _) => format!("{} · ⚠ {w}", task.label),
        (None, true) => format!("{} · waiting on its needs", task.label),
        _ => task.label.clone(),
    };
    let people = task
        .people
        .iter()
        .map(|p| format!("@{p}"))
        .collect::<Vec<_>>()
        .join(" ");

    view! {
        <div
            class="graph-row"
            class:is-done=task.done
            class:is-locked=task.locked
            class:has-warning=task.warning.is_some()
            data-line=task.line
        >
            <button
                class="graph-status"
                class:is-clickable=task.next.is_some()
                title=tooltip
                on:pointerdown=|ev: web_sys::PointerEvent| ev.stop_propagation()
                on:click=click
                on:contextmenu=right_click
            >
                {task.symbol.clone()}
            </button>
            <span class="graph-task-text" title=task.text.clone()>{task.text.clone()}</span>
            {(!people.is_empty()).then(|| view! { <span class="graph-people">{people}</span> })}
            {task.locked.then(|| view! { <span class="graph-lock" title="Waiting on its needs">"🔒"</span> })}
            {task.warning.is_some().then(|| view! { <span class="graph-row-warn">"⚠"</span> })}
        </div>
    }
}

/// Right-click on a status: every status in the file's format.
#[component]
fn StatusMenu(graph: Graph, flow: RwSignal<Option<String>>) -> impl IntoView {
    let state = expect_context::<AppState>();
    view! {
        {move || {
            let menu = graph.menu.get()?;
            let (node, task) = graph.data.with(|d| {
                let node = d.as_ref()?.nodes.iter().find(|n| n.path == menu.node)?.clone();
                let task = node.tasks.iter().find(|t| t.line == menu.line)?.clone();
                Some((node, task))
            })?;
            // A task waiting on its wires is the app's to change.
            let wire_locked = task.locked && task.next.is_none() && task.known;
            let has_needs = task.has_needs;
            let main: Vec<GraphStatus> = node.statuses.iter().filter(|s| !s.right_click_only).cloned().collect();
            let extra: Vec<GraphStatus> = node.statuses.iter().filter(|s| s.right_click_only).cloned().collect();
            let item = {
                let state = state.clone();
                let node = node.clone();
                let task = task.clone();
                move |status: GraphStatus| {
                    let current = status.ch == task.ch;
                    let blocked_role = status.role.as_deref() == Some("blocked");
                    let reason = if wire_locked {
                        Some("waiting on its needs:: wires")
                    } else if blocked_role && has_needs {
                        Some("set by its needs:: wires")
                    } else {
                        None
                    };
                    let disabled = reason.is_some() || current;
                    let state = state.clone();
                    let path = node.path.clone();
                    let (line, from, to) = (task.line, task.ch, status.ch);
                    view! {
                        <button
                            class="graph-menu-item"
                            class:is-current=current
                            disabled=disabled
                            title=reason.unwrap_or("")
                            on:click=move |_| {
                                graph.menu.set(None);
                                change_status(state.clone(), graph, flow.get_untracked(), path.clone(), line, from, to);
                            }
                        >
                            <span class="graph-menu-symbol">{status.symbol.clone()}</span>
                            <span class="graph-menu-label">{status.label.clone()}</span>
                            <code class="graph-menu-char">{format!("[{}]", status.ch)}</code>
                            <span class="graph-menu-role">{status.role.clone().unwrap_or_default()}</span>
                        </button>
                    }
                }
            };
            let main_items = main.into_iter().map(item.clone()).collect_view();
            let extra_items = (!extra.is_empty()).then(|| view! {
                <div class="graph-menu-heading">"Right-click only"</div>
                {extra.into_iter().map(item.clone()).collect_view()}
            });
            Some(view! {
                <div
                    class="graph-menu"
                    style=format!("left: {}px; top: {}px;", menu.at.0, menu.at.1)
                    on:pointerdown=|ev: web_sys::PointerEvent| ev.stop_propagation()
                >
                    <div class="graph-menu-heading">"Set status"</div>
                    <div class="graph-menu-task">{task.text.clone()}</div>
                    {main_items}
                    {extra_items}
                </div>
            })
        }}
    }
}

/// Ask the server to change a status, then redraw. The open note is saved
/// first, so the server never works from a stale copy of it, and reloaded
/// afterwards if the change touched it.
fn change_status(
    state: AppState,
    graph: Graph,
    flow: Option<String>,
    node: String,
    line: usize,
    from: char,
    to: char,
) {
    leptos::task::spawn_local(async move {
        if let Err(err) = state.save_now().await {
            graph.error.set(Some(err));
            return;
        }
        match api::set_task_status(&node, line, from, to).await {
            Ok(result) => state.reload_if_touched(&result.changes).await,
            Err(err) => graph.error.set(Some(err)),
        }
        reload(graph, flow).await;
    });
}

/// Write a dragged node's position into the flow note. Outside a flow there
/// is nowhere to save it, so it stays where it was dropped until a reload.
fn save_position(state: AppState, graph: Graph, flow: Option<String>, path: String) {
    let Some(flow) = flow else { return };
    let target = graph.data.with_untracked(|d| {
        d.as_ref()?
            .nodes
            .iter()
            .find(|n| n.path == path)
            .and_then(|n| n.flow_line)
    });
    let Some(line) = target else { return };
    let Some((x, y)) = graph.moved.with_untracked(|m| m.get(&path).copied()) else {
        return;
    };
    leptos::task::spawn_local(async move {
        match api::set_flow_position(&flow, line, x as i32, y as i32).await {
            Ok(_) => {
                // The flow note may be open in the editor: show the new line.
                if state.current_file.get_untracked().as_deref() == Some(flow.as_str()) {
                    let _ = state.open(&flow).await;
                }
                graph.error.set(None);
            }
            Err(err) => graph.error.set(Some(err)),
        }
    });
}

fn open_note(state: AppState, file: String) {
    leptos::task::spawn_local(async move {
        if let Err(err) = state.open_and_reveal(&file).await {
            leptos::logging::error!("Could not open note: {err}");
            return;
        }
        state.view_mode.set(ViewMode::Edit);
        state.editor().show();
    });
}

/// Zoom and pan so every node is on screen.
fn fit(graph: Graph, el: &web_sys::HtmlDivElement) {
    let detail = graph.detail.get_untracked();
    let Some((min_x, min_y, max_x, max_y)) = graph.data.with_untracked(|d| {
        let d = d.as_ref()?;
        d.nodes
            .iter()
            .fold(None, |acc: Option<(f64, f64, f64, f64)>, n| {
                let (x, y) = graph.position(n);
                let (r, b) = (x + NODE_W, y + node_height(n, detail));
                Some(match acc {
                    None => (x, y, r, b),
                    Some((a, c, e, f)) => (a.min(x), c.min(y), e.max(r), f.max(b)),
                })
            })
    }) else {
        return;
    };
    let pad = 48.0;
    let (w, h) = (el.client_width() as f64, el.client_height() as f64);
    let zoom = ((w - pad * 2.0) / (max_x - min_x))
        .min((h - pad * 2.0) / (max_y - min_y))
        .clamp(0.2, 1.0);
    graph.zoom.set(zoom);
    graph.pan.set((
        (w - (max_x - min_x) * zoom) / 2.0 - min_x * zoom,
        (h - (max_y - min_y) * zoom) / 2.0 - min_y * zoom,
    ));
}
