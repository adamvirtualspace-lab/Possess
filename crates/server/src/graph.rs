//! The node graph: notes as nodes, tasks as rows, `needs::`/`sync::` as wires.
//!
//! The server owns this so that a status change, and every task it unblocks
//! in other files, is read, decided and written in one place under one lock.
//! The vault is re-read on each request: it's small, and it means an edit
//! made in Obsidian or the editor is always seen.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use axum::extract::{Query, State};
use axum::Json;
use possess_common::{
    FlowPositionRequest, FlowPositionResponse, GraphEdge, GraphNode, GraphResponse, GraphStatus,
    GraphTask, StatusChange, TaskStatusRequest, TaskStatusResponse,
};
use possess_markdown::tasks::{
    note_key, set_flow_position as rewrite_position, EditError, NeedsState, ParsedFile, Role,
    Vault, Warning,
};
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::notes::{mtime_secs, walk_notes};
use crate::vault::AppState;

/// Gap between automatic layout columns and rows, in canvas pixels.
const COLUMN: i32 = 340;
const ROW: i32 = 260;

/// Read every note in the vault.
pub fn load_vault(base: &Path) -> std::io::Result<Vault> {
    let mut vault = Vault::new();
    for entry in walk_notes(base) {
        let Ok(rel) = entry.path().strip_prefix(base) else { continue };
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

/// Columns by dependency depth (rig → anim → lighting → …), rows in order
/// within a column. Only nodes without a saved position are moved.
fn layout(nodes: &mut [GraphNode], edges: &[GraphEdge]) {
    let index: HashMap<String, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.path.clone(), i))
        .collect();
    let mut depth = vec![0usize; nodes.len()];
    // Longest path from a source, relaxed at most once per node, so a cycle
    // can't loop forever.
    for _ in 0..nodes.len() {
        let mut changed = false;
        for edge in edges.iter().filter(|e| e.kind == "needs") {
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
    let mut rows: BTreeMap<usize, i32> = BTreeMap::new();
    for (i, node) in nodes.iter_mut().enumerate() {
        if node.placed {
            continue;
        }
        let row = rows.entry(depth[i]).or_insert(0);
        node.x = depth[i] as i32 * COLUMN;
        node.y = *row * ROW;
        *row += 1;
    }
}

/// Build the graph for a flow note, or for every note with tasks.
pub fn build(vault: &Vault, flow: Option<&str>) -> Result<GraphResponse, AppError> {
    // (path, saved position, line in the flow)
    let members: Vec<(String, Option<(i32, i32)>, Option<usize>)> = match flow {
        Some(flow_path) => {
            let file = vault
                .file(flow_path)
                .ok_or_else(|| AppError::not_found(format!("No such note: {flow_path}")))?;
            let flow = file.flow.as_ref().ok_or_else(|| {
                AppError::bad_request(format!("{flow_path} isn't a flow: it has no ::flow:: block"))
            })?;
            flow.nodes
                .iter()
                .map(|n| (note_key(&n.path), n.pos, Some(n.line)))
                .collect()
        }
        None => vault
            .files()
            .filter(|(_, file)| !file.tasks.is_empty())
            .map(|(key, _)| (key.to_string(), None, None))
            .collect(),
    };

    let all_warnings = vault.warnings();
    let mut nodes: Vec<GraphNode> = members
        .iter()
        .map(|(key, pos, flow_line)| {
            let file = vault.file(key);
            let warnings = all_warnings.get(key.as_str()).map(Vec::as_slice).unwrap_or(&[]);
            let tasks = file.map(|f| task_rows(vault, key, f, warnings)).unwrap_or_default();
            GraphNode {
                path: key.clone(),
                file: format!("{key}.md"),
                title: vault.source(key).map_or_else(|| key.clone(), |src| title(src, key)),
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
        })
        .collect();

    // Wires between nodes of this graph only, drawn from the upstream task
    // to the task the wire is written on.
    let in_graph: HashMap<&str, ()> = members.iter().map(|(k, _, _)| (k.as_str(), ())).collect();
    let mut edges = Vec::new();
    for (key, _, _) in &members {
        let Some(file) = vault.file(key) else { continue };
        for task in &file.tasks {
            let links = task
                .needs
                .iter()
                .map(|l| ("needs", l))
                .chain(task.sync.iter().map(|l| ("sync", l)));
            for (kind, link) in links {
                let Some((from_key, from_task)) = vault.resolve(key, link) else { continue };
                if !in_graph.contains_key(from_key) {
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
        node.wires_in = edges.iter().filter(|e| e.to_node == node.path).count();
        node.wires_out = edges.iter().filter(|e| e.from_node == node.path).count();
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
    let vault = load_vault(&state.vault_resolved())?;
    let flow = params.flow.as_deref().filter(|f| !f.is_empty());
    Ok(Json(build(&vault, flow)?))
}

fn edit_error(err: EditError) -> AppError {
    match err {
        EditError::Stale { .. } => AppError::conflict(err.to_string()),
        EditError::NoFile { .. } | EditError::NoTask { .. } | EditError::NoNode { .. } => {
            AppError::not_found(err.to_string())
        }
        EditError::UnknownStatus { .. } => AppError::bad_request(err.to_string()),
    }
}

/// Change one task's status, then write it and every task it unblocked or
/// blocked again.
pub fn apply_status(base: &Path, req: &TaskStatusRequest) -> AppResult<Vec<StatusChange>> {
    let mut vault = load_vault(base)?;
    let key = note_key(&req.path);

    // A blocked task waiting on its wires is the app's to change, not a click's.
    if let Some(file) = vault.file(&key) {
        if let Some(task) = file.task_at_line(req.line) {
            let blocked = file.format.role_of(task.ch) == Some(Role::Blocked);
            let waiting = matches!(
                vault.needs_state(&key, task),
                NeedsState::Unmet | NeedsState::Broken
            );
            if blocked && waiting && task.ch == req.from {
                return Err(AppError::conflict(
                    "This task is waiting on its needs:: wires, so it can't be changed yet.",
                ));
            }
        }
    }

    let changes = vault
        .set_status(&key, req.line, req.from, req.to)
        .map_err(edit_error)?;

    let mut written: Vec<&str> = Vec::new();
    for change in &changes {
        if written.contains(&change.path.as_str()) {
            continue;
        }
        written.push(&change.path);
        let full = crate::vault::resolve_in_vault(base, &format!("{}.md", change.path))?;
        let src = vault
            .source(&change.path)
            .ok_or_else(|| AppError::internal("changed file vanished from the vault"))?;
        std::fs::write(full, src)?;
    }

    Ok(changes
        .into_iter()
        .map(|c| StatusChange {
            path: c.path,
            line: c.line,
            from: c.from,
            to: c.to,
        })
        .collect())
}

pub async fn set_task_status(
    State(state): State<AppState>,
    Json(req): Json<TaskStatusRequest>,
) -> AppResult<Json<TaskStatusResponse>> {
    let _guard = state.graph_lock.lock().await;
    let changes = apply_status(&state.vault_resolved(), &req)?;
    Ok(Json(TaskStatusResponse { changes }))
}

pub async fn set_flow_position(
    State(state): State<AppState>,
    Json(req): Json<FlowPositionRequest>,
) -> AppResult<Json<FlowPositionResponse>> {
    let _guard = state.graph_lock.lock().await;
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
    use super::*;

    /// A throwaway copy of the sample vault.
    fn sample_copy() -> std::path::PathBuf {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample-vault");
        let dst = std::env::temp_dir().join(format!("possess-graph-{}", uuid::Uuid::new_v4()));
        for entry in walkdir::WalkDir::new(&src).into_iter().filter_map(Result::ok) {
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
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn flow_graph_uses_the_flow_note() {
        let base = sample_copy();
        let graph = build(&load_vault(&base).unwrap(), Some("flows/ep04-sh040.md")).unwrap();
        assert_eq!(graph.flow.as_deref(), Some("flows/ep04-sh040"));
        assert_eq!(graph.nodes.len(), 9);
        let lighting = graph.nodes.iter().find(|n| n.path == "lighting/sh040").unwrap();
        assert_eq!((lighting.x, lighting.y, lighting.placed), (640, 120, true));
        assert_eq!(lighting.flow_line, Some(9));
        assert_eq!((lighting.wires_in, lighting.wires_out), (2, 1));
        let l2 = lighting.tasks.iter().find(|t| t.id.as_deref() == Some("l2")).unwrap();
        assert!(l2.locked);
        assert_eq!(l2.next, None, "a task waiting on its wires can't be clicked");
        // The task-less reply note is a node because the flow lists it.
        assert!(graph.nodes.iter().any(|n| n.path == "review/sh040-reply" && n.tasks.is_empty()));
        // The sync wire from anim a3 to director notes d1.
        assert!(graph.edges.iter().any(|e| e.kind == "sync"
            && e.from_node == "anim/sh040"
            && e.to_node == "review/director-notes"));

        assert!(build(&load_vault(&base).unwrap(), Some("anim/sh040")).is_err());
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn status_changes_are_written_with_their_flips() {
        let base = sample_copy();
        let req = |path: &str, line, from, to| TaskStatusRequest {
            path: path.into(),
            line,
            from,
            to,
        };
        apply_status(&base, &req("anim/sh040.md", 10, ' ', 'x')).unwrap();
        let changes = apply_status(&base, &req("fx/sh040-water", 10, ' ', 'x')).unwrap();
        assert_eq!(changes.len(), 2);
        let lighting = std::fs::read_to_string(base.join("lighting/sh040.md")).unwrap();
        assert!(lighting.contains("- [ ] Render lighting pass @maya @unit7 ^l2"));

        // Stale: the client thinks it's still open.
        let err = apply_status(&base, &req("anim/sh040", 10, ' ', '?')).unwrap_err();
        assert_eq!(err.status.as_u16(), 409);
        // Waiting on wires: comp still needs l2 and f3.
        let err = apply_status(&base, &req("comp/sh040", 16, '-', ' ')).unwrap_err();
        assert_eq!(err.status.as_u16(), 409);
        // Outside the vault.
        assert!(apply_status(&base, &req("../escape", 1, ' ', 'x')).is_err());
        std::fs::remove_dir_all(base).ok();
    }
}
