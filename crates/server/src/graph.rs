//! The node graph: notes as nodes, tasks as rows, `needs::`/`sync::` as wires.
//!
//! The server owns this so that a status change, and every task it unblocks
//! in other files, is read, decided and written in one place under one lock.
//! The vault is re-read on each request: it's small, and it means an edit
//! made in Obsidian or the editor is always seen.

use std::collections::{BTreeMap, HashMap};

use axum::extract::{Query, State};
use axum::Json;
use possess_common::{
    FlowPositionRequest, FlowPositionResponse, GraphEdge, GraphNode, GraphResponse, GraphStatus,
    GraphTask,
};
use possess_markdown::tasks::{
    note_key, set_flow_position as rewrite_position, NeedsState, ParsedFile, Role, Vault, Warning,
};
use serde::Deserialize;

use crate::activity::edit_error;
use crate::error::{AppError, AppResult};
use crate::notes::mtime_secs;
use crate::vault::AppState;

/// Distance between automatic layout columns, in canvas pixels.
const COLUMN: i32 = 340;

/// Read every note in the vault. (The server keeps one parsed copy in its
/// engine; this is for tests.)
#[cfg(test)]
pub fn load_vault(base: &std::path::Path) -> std::io::Result<Vault> {
    let mut vault = Vault::new();
    for entry in crate::notes::walk_notes(base) {
        let Ok(rel) = entry.path().strip_prefix(base) else {
            continue;
        };
        // A file that isn't UTF-8 isn't a note we can parse; skip it.
        if let Ok(src) = std::fs::read_to_string(entry.path()) {
            vault.insert(&rel.to_string_lossy(), &src);
        }
    }
    Ok(vault)
}

/// The note's first `# ` heading, else its file name.
fn title(src: &str, key: &str) -> String {
    src.lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| key.rsplit('/').next().unwrap_or(key).to_string())
}

fn role_name(role: Option<Role>) -> Option<String> {
    role.map(|r| r.as_str().to_string())
}

/// A note's tasks as graph rows.
fn task_rows(vault: &Vault, key: &str, file: &ParsedFile, warnings: &[Warning]) -> Vec<GraphTask> {
    file.tasks
        .iter()
        .map(|task| {
            let status = file.format.get(task.ch);
            let role = file.format.role_of(task.ch);
            let needs = vault.needs_state(key, task);
            let locked = matches!(needs, NeedsState::Unmet | NeedsState::Broken);
            // A blocked task waiting on its wires can't be clicked; one
            // blocked by hand (no needs) can.
            let wire_locked = locked && role == Some(Role::Blocked);
            GraphTask {
                line: task.line,
                ch: task.ch,
                symbol: status.map_or("⚠".into(), |s| s.symbol.clone()),
                label: status.map_or("not in this file's format".into(), |s| s.label.clone()),
                known: task.known,
                text: task.text.clone(),
                id: task.id.clone(),
                people: task.people.clone(),
                done: role == Some(Role::Done),
                has_needs: !task.needs.is_empty(),
                locked,
                next: if wire_locked || !task.known {
                    None
                } else {
                    file.format.next_on_click(task.ch)
                },
                warning: warnings
                    .iter()
                    .find(|w| w.line == task.line)
                    .map(|w| w.message.clone()),
            }
        })
        .collect()
}

/// Height of a node in the Tasks view. Matches the UI's fixed geometry
/// (crates/ui/src/graph.rs): header, one row per task, footer.
fn node_height(node: &GraphNode) -> i32 {
    54 + node.tasks.len().max(1) as i32 * 30 + 28
}

/// Lay out the nodes that have no saved position.
///
/// Notes joined by wires get columns by wire depth (rig → anim →
/// lighting → …), each column stacked by real node height. Notes with no
/// wires at all go in a grid underneath, so a vault full of plain to-do
/// lists doesn't stretch the pipeline into one tall column.
fn layout(nodes: &mut [GraphNode], edges: &[GraphEdge]) {
    const GAP: i32 = 40;
    let index: HashMap<String, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.path.clone(), i))
        .collect();
    // Only wires between two notes count: one inside a note connects it to
    // nothing else.
    let mut wired = vec![false; nodes.len()];
    for edge in edges.iter().filter(|e| e.from_node != e.to_node) {
        for end in [&edge.from_node, &edge.to_node] {
            if let Some(&i) = index.get(end) {
                wired[i] = true;
            }
        }
    }

    let mut depth = vec![0usize; nodes.len()];
    // Longest path from a source, relaxed at most once per node, so a cycle
    // can't loop forever.
    for _ in 0..nodes.len() {
        let mut changed = false;
        // Sync wires count too: a director's note that mirrors an anim task
        // reads best to the right of it.
        for edge in edges.iter().filter(|e| e.from_node != e.to_node) {
            let (Some(&from), Some(&to)) = (index.get(&edge.from_node), index.get(&edge.to_node))
            else {
                continue;
            };
            if from != to && depth[to] < depth[from] + 1 {
                depth[to] = depth[from] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // The wired pipeline, in columns.
    let mut column_y: BTreeMap<usize, i32> = BTreeMap::new();
    for (i, node) in nodes.iter_mut().enumerate() {
        if node.placed || !wired[i] {
            continue;
        }
        let y = column_y.entry(depth[i]).or_insert(0);
        node.x = depth[i] as i32 * COLUMN;
        node.y = *y;
        *y += node_height(node) + GAP;
    }

    // Everything else in a grid below whatever is already on the canvas.
    let bottom = nodes
        .iter()
        .enumerate()
        .filter(|(i, n)| n.placed || wired[*i])
        .map(|(_, n)| n.y + node_height(n))
        .max()
        .map_or(0, |b| b + GAP * 3);
    let per_row = column_y.len().max(4);
    let mut row_top = bottom;
    let mut row_height = 0;
    let mut col = 0;
    for (i, node) in nodes.iter_mut().enumerate() {
        if node.placed || wired[i] {
            continue;
        }
        if col == per_row {
            col = 0;
            row_top += row_height + GAP;
            row_height = 0;
        }
        node.x = col as i32 * COLUMN;
        node.y = row_top;
        row_height = row_height.max(node_height(node));
        col += 1;
    }
}

/// A note that is in the graph, before it becomes a node.
struct Member {
    key: String,
    /// Saved in the flow note.
    pos: Option<(i32, i32)>,
    /// Its line in the flow note.
    flow_line: Option<usize>,
}

/// Build the graph for a flow note, or for every note with tasks.
pub fn build(vault: &Vault, flow: Option<&str>) -> Result<GraphResponse, AppError> {
    let members: Vec<Member> = match flow {
        Some(flow_path) => {
            let file = vault
                .file(flow_path)
                .ok_or_else(|| AppError::not_found(format!("No such note: {flow_path}")))?;
            let flow = file.flow.as_ref().ok_or_else(|| {
                AppError::bad_request(format!(
                    "{flow_path} isn't a flow: it has no ::flow:: block"
                ))
            })?;
            flow.nodes
                .iter()
                .map(|n| Member {
                    key: note_key(&n.path),
                    pos: n.pos,
                    flow_line: Some(n.line),
                })
                .collect()
        }
        None => vault
            .files()
            .filter(|(_, file)| !file.tasks.is_empty())
            .map(|(key, _)| Member {
                key: key.to_string(),
                pos: None,
                flow_line: None,
            })
            .collect(),
    };

    let all_warnings = vault.warnings();
    let mut nodes: Vec<GraphNode> = members
        .iter()
        .map(
            |Member {
                 key,
                 pos,
                 flow_line,
             }| {
                let file = vault.file(key);
                let warnings = all_warnings
                    .get(key.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                let tasks = file
                    .map(|f| task_rows(vault, key, f, warnings))
                    .unwrap_or_default();
                GraphNode {
                    path: key.clone(),
                    file: format!("{key}.md"),
                    title: vault
                        .source(key)
                        .map_or_else(|| key.clone(), |src| title(src, key)),
                    missing: file.is_none(),
                    x: pos.map_or(0, |p| p.0),
                    y: pos.map_or(0, |p| p.1),
                    placed: pos.is_some(),
                    flow_line: *flow_line,
                    statuses: file
                        .map(|f| {
                            f.format
                                .statuses
                                .iter()
                                .map(|s| GraphStatus {
                                    ch: s.ch,
                                    symbol: s.symbol.clone(),
                                    label: s.label.clone(),
                                    role: role_name(s.role),
                                    right_click_only: f.format.is_right_click_only(s.ch),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    open: tasks.iter().filter(|t| !t.done).count(),
                    tasks,
                    wires_in: 0,
                    wires_out: 0,
                    warnings: warnings.len(),
                }
            },
        )
        .collect();

    // Wires between nodes of this graph only, drawn from the upstream task
    // to the task the wire is written on.
    let in_graph: std::collections::HashSet<&str> =
        members.iter().map(|m| m.key.as_str()).collect();
    let mut edges = Vec::new();
    for Member { key, .. } in &members {
        let Some(file) = vault.file(key) else {
            continue;
        };
        for task in &file.tasks {
            let links = task
                .needs
                .iter()
                .map(|l| ("needs", l))
                .chain(task.sync.iter().map(|l| ("sync", l)));
            for (kind, link) in links {
                let Some((from_key, from_task)) = vault.resolve(key, link) else {
                    continue;
                };
                if !in_graph.contains(from_key) {
                    continue;
                }
                edges.push(GraphEdge {
                    kind: kind.to_string(),
                    from_node: from_key.to_string(),
                    from_line: from_task.line,
                    to_node: key.clone(),
                    to_line: task.line,
                    live: vault.is_done(from_key, from_task),
                });
            }
        }
    }
    for node in &mut nodes {
        // Wires between notes only: one inside a note is shown by its lock.
        let between = |e: &&GraphEdge| e.from_node != e.to_node;
        node.wires_in = edges
            .iter()
            .filter(between)
            .filter(|e| e.to_node == node.path)
            .count();
        node.wires_out = edges
            .iter()
            .filter(between)
            .filter(|e| e.from_node == node.path)
            .count();
    }

    layout(&mut nodes, &edges);
    Ok(GraphResponse {
        flow: flow.map(note_key),
        nodes,
        edges,
    })
}

#[derive(Deserialize)]
pub struct GraphQuery {
    flow: Option<String>,
}

pub async fn get_graph(
    State(state): State<AppState>,
    Query(params): Query<GraphQuery>,
) -> AppResult<Json<GraphResponse>> {
    let flow = params.flow.as_deref().filter(|f| !f.is_empty());
    // From the engine's parsed copy: no re-reading the vault per request.
    let engine = state.engine().await;
    Ok(Json(build(engine.notes(), flow)?))
}

pub async fn set_flow_position(
    State(state): State<AppState>,
    Json(req): Json<FlowPositionRequest>,
) -> AppResult<Json<FlowPositionResponse>> {
    // Under the engine's lock, like every other write it needs to know about.
    let _engine = state.engine().await;
    let full = state.resolve(&format!("{}.md", note_key(&req.flow)))?;
    if !full.is_file() {
        return Err(AppError::not_found(format!("No such note: {}", req.flow)));
    }
    let src = std::fs::read_to_string(&full)?;
    let moved = rewrite_position(&src, req.line, req.x, req.y).map_err(edit_error)?;
    std::fs::write(&full, moved)?;
    Ok(Json(FlowPositionResponse {
        saved: true,
        mtime: mtime_secs(&full),
    }))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    /// A throwaway copy of the sample vault.
    fn sample_copy() -> std::path::PathBuf {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample-vault");
        let dst = std::env::temp_dir().join(format!("possess-graph-{}", uuid::Uuid::new_v4()));
        for entry in walkdir::WalkDir::new(&src)
            .into_iter()
            .filter_map(Result::ok)
        {
            let rel = entry.path().strip_prefix(&src).unwrap();
            let target = dst.join(rel);
            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&target).unwrap();
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
        crate::vault::canonical(&dst)
    }

    #[test]
    fn whole_vault_graph_has_every_note_with_tasks() {
        let base = sample_copy();
        let graph = build(&load_vault(&base).unwrap(), None).unwrap();
        assert_eq!(graph.flow, None);
        assert!(graph.nodes.iter().all(|n| !n.tasks.is_empty()));
        assert!(graph.nodes.iter().any(|n| n.path == "lighting/sh040"));
        assert!(!graph.nodes.iter().any(|n| n.path == "review/sh040-reply"));
        // Automatic layout puts each node right of everything it needs.
        let x = |p: &str| graph.nodes.iter().find(|n| n.path == p).unwrap().x;
        assert!(x("rig/hero") < x("anim/sh040"));
        assert!(x("anim/sh040") < x("lighting/sh040"));
        assert!(x("lighting/sh040") < x("comp/sh040"));
        assert!(x("comp/sh040") < x("delivery/ep04"));
        // Notes without wires sit in a grid below the pipeline, not in it.
        let node = |p: &str| graph.nodes.iter().find(|n| n.path == p).unwrap();
        let lowest_wired = ["rig/hero", "anim/sh040", "fx/sh040-water", "comp/sh040"]
            .iter()
            .map(|p| node(p).y + node_height(node(p)))
            .max()
            .unwrap();
        assert!(node("_parser-tests/roles").y > lowest_wired);
        // No two nodes overlap.
        for a in &graph.nodes {
            for b in &graph.nodes {
                if a.path < b.path {
                    let apart = a.x + 264 <= b.x
                        || b.x + 264 <= a.x
                        || a.y + node_height(a) <= b.y
                        || b.y + node_height(b) <= a.y;
                    assert!(apart, "{} overlaps {}", a.path, b.path);
                }
            }
        }
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn flow_graph_uses_the_flow_note() {
        let base = sample_copy();
        let graph = build(&load_vault(&base).unwrap(), Some("flows/ep04-sh040.md")).unwrap();
        assert_eq!(graph.flow.as_deref(), Some("flows/ep04-sh040"));
        assert_eq!(graph.nodes.len(), 9);
        let lighting = graph
            .nodes
            .iter()
            .find(|n| n.path == "lighting/sh040")
            .unwrap();
        assert_eq!((lighting.x, lighting.y, lighting.placed), (640, 120, true));
        assert_eq!(lighting.flow_line, Some(9));
        assert_eq!((lighting.wires_in, lighting.wires_out), (2, 1));
        let l2 = lighting
            .tasks
            .iter()
            .find(|t| t.id.as_deref() == Some("l2"))
            .unwrap();
        assert!(l2.locked);
        assert_eq!(
            l2.next, None,
            "a task waiting on its wires can't be clicked"
        );
        // The task-less reply note is a node because the flow lists it.
        assert!(graph
            .nodes
            .iter()
            .any(|n| n.path == "review/sh040-reply" && n.tasks.is_empty()));
        // The sync wire from anim a3 to director notes d1.
        assert!(graph.edges.iter().any(|e| e.kind == "sync"
            && e.from_node == "anim/sh040"
            && e.to_node == "review/director-notes"));

        assert!(build(&load_vault(&base).unwrap(), Some("anim/sh040")).is_err());
        std::fs::remove_dir_all(base).ok();
    }
}
