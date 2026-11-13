//! Each `sample-vault/_parser-tests/` file says what a parser must find in an
//! "Expected" note. These tests check exactly that, so Possess and Obsessed
//! are held to the same cases.

use super::*;

macro_rules! parser_test {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../sample-vault/_parser-tests/",
            $name
        ))
    };
}

/// (symbol, label) for each task, in order: what the app would draw.
fn drawn(file: &ParsedFile) -> Vec<(String, String)> {
    file.tasks
        .iter()
        .map(|t| match file.format.get(t.ch) {
            Some(s) => (s.symbol.clone(), s.label.clone()),
            None => ("⚠".into(), "unknown".into()),
        })
        .collect()
}

fn codes(file: &ParsedFile) -> Vec<(usize, &'static str)> {
    file.warnings
        .iter()
        .map(|w| (w.line, w.code.as_str()))
        .collect()
}

fn texts(file: &ParsedFile) -> Vec<&str> {
    file.tasks.iter().map(|t| t.text.as_str()).collect()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

#[test]
fn default_format() {
    let file = parse(parser_test!("default-format.md"));
    assert_eq!(file.format, StatusFormat::default());
    assert_eq!(file.format.line, None);
    assert_eq!(
        drawn(&file),
        pairs(&[
            ("🔳", "ready to start"),
            ("✅", "Finished"),
            ("✅", "Finished"),
            ("⚪", "cannot start"),
            ("❓", "ready to check"),
        ])
    );
    // The built-in default marks [-] as blocked; a capital X is done.
    assert_eq!(file.format.role_of('-'), Some(Role::Blocked));
    assert_eq!(file.format.role_of('X'), Some(Role::Done));
    assert!(file.warnings.is_empty(), "{:?}", file.warnings);
}

#[test]
fn format_problems() {
    let file = parse(parser_test!("format-problems.md"));
    // 3 entries survive, and [x] is added as done.
    let chars: Vec<char> = file.format.statuses.iter().map(|s| s.ch).collect();
    assert_eq!(chars, [' ', '~', '!', 'x']);
    assert_eq!(file.format.role_of('x'), Some(Role::Done));
    assert_eq!(file.format.role_of(' '), Some(Role::Open));
    assert_eq!(
        drawn(&file),
        pairs(&[
            ("🌱", "idea"),
            ("🌿", "growing"),
            ("🔥", "urgent"),
            ("✅", "Finished"),
            ("⚠", "unknown"),
        ])
    );
    assert_eq!(
        codes(&file),
        [
            (10, "done-added"),
            (13, "entry-no-char"),
            (21, "status-unknown")
        ]
    );
    assert!(!file.tasks[4].known);
}

#[test]
fn format_spacing() {
    let file = parse(parser_test!("format-spacing.md"));
    assert_eq!(
        drawn(&file),
        pairs(&[
            ("⚪", "cannot start"),
            ("🔳", "ready to start"),
            ("❓", "ready to check"),
            ("✅", "Finished"),
        ])
    );
    // No roles are written: the markdown defaults apply, and [-] is just an
    // in-between status here.
    assert_eq!(file.format.role_of(' '), Some(Role::Open));
    assert_eq!(file.format.role_of('x'), Some(Role::Done));
    assert_eq!(file.format.role_of('-'), None);
    assert!(file.warnings.is_empty(), "{:?}", file.warnings);
}

#[test]
fn ids_people_wires() {
    let file = parse(parser_test!("ids-people-wires.md"));
    assert_eq!(file.tasks.len(), 4);
    let [boat, tex, send, ping] = &file.tasks[..] else {
        unreachable!()
    };

    assert_eq!(boat.text, "Model the boat @lia @teo");
    assert_eq!(boat.id.as_deref(), Some("boat"));
    assert_eq!(boat.people, ["lia", "teo"]);
    assert!(boat.needs.is_empty() && boat.sync.is_empty());

    assert_eq!(tex.id.as_deref(), Some("tex"));
    assert_eq!(tex.people, ["lia"]);
    // The two needs lines add up.
    let needs: Vec<(Option<&str>, &str)> = tex
        .needs
        .iter()
        .map(|r| (r.path.as_deref(), r.id.as_str()))
        .collect();
    assert_eq!(needs, [(None, "boat"), (Some("rig/hero"), "r1")]);

    assert_eq!(send.id, None);
    assert!(send.people.is_empty(), "an email address isn't a mention");

    assert_eq!(ping.id.as_deref(), Some("ping"));
    assert_eq!(ping.people, ["bram"]);
    assert_eq!(ping.sync.len(), 1);
    assert_eq!(
        (ping.sync[0].path.as_deref(), ping.sync[0].id.as_str()),
        (None, "tex")
    );

    assert!(file.warnings.is_empty(), "{:?}", file.warnings);
}

#[test]
fn ids_people_wires_blocked_by_another_file() {
    let mut vault = Vault::new();
    vault.insert(
        "_parser-tests/ids-people-wires.md",
        parser_test!("ids-people-wires.md"),
    );
    vault.insert(
        "rig/hero.md",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../sample-vault/rig/hero.md"
        )),
    );
    let file = vault.file("_parser-tests/ids-people-wires").unwrap();
    let tex = file.task_by_id("tex").unwrap();
    assert_eq!(
        vault.needs_state("_parser-tests/ids-people-wires", tex),
        NeedsState::Unmet
    );
}

#[test]
fn last_format_wins() {
    let file = parse(parser_test!("last-format-wins.md"));
    assert_eq!(file.format.line, Some(11));
    assert_eq!(drawn(&file), pairs(&[("🟦", "To do"), ("🏁", "Done")]));
    assert_eq!(codes(&file), [(7, "format-ignored")]);
}

#[test]
fn not_tasks() {
    let file = parse(parser_test!("not-tasks.md"));
    assert_eq!(
        texts(&file),
        ["Dash task", "Subtask", "Star task", "Plus task"]
    );
    // The format inside the code fence is ignored.
    assert_eq!(file.format, StatusFormat::default());
    assert!(file.warnings.is_empty(), "{:?}", file.warnings);
}

#[test]
fn roles() {
    let file = parse(parser_test!("roles.md"));
    let f = &file.format;
    assert_eq!(f.role_of('a'), Some(Role::Done));
    assert_eq!(f.role_of('o'), Some(Role::Open));
    assert_eq!(f.role_of('w'), Some(Role::Blocked));
    // [a] already claims done, so [x] is just "rejected".
    assert_eq!(f.role_of('x'), None);
    assert_eq!(f.role_of('?'), None);
    assert_eq!(
        drawn(&file),
        pairs(&[
            ("⏳", "waiting"),
            ("🟡", "to do"),
            ("❓", "in review"),
            ("👍", "Approved"),
            ("❌", "rejected"),
        ])
    );
    assert_eq!(codes(&file), [(10, "role-unknown")]);

    // "Check the pier" needs a done task but is saved as [w]: flagged, and
    // nothing is changed.
    let mut vault = Vault::new();
    vault.insert("roles.md", parser_test!("roles.md"));
    let check = file.task_by_id("check").unwrap();
    assert_eq!(vault.needs_state("roles", check), NeedsState::Met);
    let warnings = vault.warnings();
    let found: Vec<_> = warnings["roles"]
        .iter()
        .map(|w| (w.line, w.code.as_str()))
        .collect();
    assert_eq!(found, [(10, "role-unknown"), (12, "needs-mismatch")]);
}

#[test]
fn round_trip() {
    let src = parser_test!("round-trip.md");
    let file = parse(src);
    assert_eq!(file.tasks.len(), 4);
    assert!(
        !src.ends_with('\n'),
        "the fixture must keep its missing final newline"
    );

    let tab = file
        .tasks
        .iter()
        .find(|t| t.text == "Indented with a tab")
        .unwrap();
    let ticked = set_status(src, tab.line, 'x').unwrap();
    let expected = src.replacen(
        "\t- [ ] Indented with a tab",
        "\t- [x] Indented with a tab",
        1,
    );
    assert_eq!(ticked, expected, "only the one character may change");
    assert_eq!(ticked.len(), src.len());

    // And back again gives the original bytes.
    assert_eq!(set_status(&ticked, tab.line, ' ').unwrap(), src);
}

#[test]
fn set_status_keeps_crlf_and_multibyte_statuses() {
    let src = "::statusformat:: [ ] = a = open, [✓] = b = done = done ::statusformat::\r\n\
               - [ ] One ^a\r\n- [✓] Two\r\n";
    let once = set_status(src, 2, '✓').unwrap();
    assert_eq!(once, src.replacen("- [ ] One", "- [✓] One", 1));
    let back = set_status(&once, 3, ' ').unwrap();
    assert_eq!(back, once.replacen("- [✓] Two", "- [ ] Two", 1));
}

#[test]
fn set_status_refuses_what_isnt_there() {
    let src = "# Title\n- [ ] Task\n";
    assert_eq!(set_status(src, 1, 'x'), Err(EditError::NoTask { line: 1 }));
    assert_eq!(
        set_status(src, 2, 'k'),
        Err(EditError::UnknownStatus { ch: 'k' })
    );
}

#[test]
fn clicking_and_the_right_click_menu() {
    let comp = parse(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../sample-vault/comp/sh040.md"
    )));
    let f = &comp.format;
    // Clicking walks the order and skips blocked; done stops.
    assert_eq!(f.next_on_click('-'), Some(' '));
    assert_eq!(f.next_on_click(' '), Some('/'));
    assert_eq!(f.next_on_click('?'), Some('x'));
    assert_eq!(f.next_on_click('x'), None);
    // Retake comes after done: right-click only.
    assert!(f.is_right_click_only('r'));
    assert!(!f.is_right_click_only('/'));
}

#[test]
fn ids_and_people_edge_cases() {
    let file = parse(
        "- [ ] A ^one\n- [ ] B ^one\n- [ ] Cost is 2^10\n- [ ] Trailing id ^t-2   \n\
         - [ ] @lead first, then @lead again and @second_one\n",
    );
    assert_eq!(file.tasks[0].id.as_deref(), Some("one"));
    assert_eq!(file.tasks[1].id, None);
    assert_eq!(codes(&file), [(2, "id-duplicate")]);
    // ^ in the middle of a word isn't an ID.
    assert_eq!(file.tasks[2].id, None);
    assert_eq!(file.tasks[2].text, "Cost is 2^10");
    assert_eq!(file.tasks[3].id.as_deref(), Some("t-2"));
    assert_eq!(file.tasks[3].text, "Trailing id");
    assert_eq!(file.tasks[4].people, ["lead", "second_one"]);
}

#[test]
fn format_edge_cases() {
    let file = parse(
        "::statusformat:: [ab] = 1 = two chars, [y] = only symbol, [o] = o = one = open, \
         [p] = p = two = open, [o] = 2 = again ::statusformat::\n- [o] Task\n",
    );
    assert_eq!(
        codes(&file),
        [
            (1, "entry-bad-char"),
            (1, "entry-incomplete"),
            (1, "role-duplicate"),
            (1, "entry-duplicate"),
            (1, "done-added"),
        ]
    );
    assert_eq!(file.format.role_of('p'), None);

    let unclosed = parse("::statusformat:: [ ] = a = b\n- [ ] Task\n");
    assert_eq!(codes(&unclosed), [(1, "format-unclosed")]);
    assert_eq!(unclosed.format, StatusFormat::default());
}

#[test]
fn wires_only_attach_to_the_task_above() {
    let file = parse(
        "- needs:: [[#^nobody]]\n\
         - [ ] Parent ^p\n  - [ ] Child ^c\n  - needs:: [[#^c]]\n\
         \n\
         Paragraph.\n  - needs:: [[#^p]]\n\
         - [ ] Other\n  - needs:: [[plain link]]\n",
    );
    // A wire line with no task above it is just a list item.
    assert!(file.tasks[0].needs.iter().map(|r| r.id.as_str()).eq(["c"]));
    assert!(file.tasks[1].needs.is_empty());
    assert_eq!(codes(&file), [(9, "link-not-task")]);
}

/// The whole sample vault, as the mockup's Warnings panel shows it: six
/// warnings, all planted in `_parser-tests/`, plus the four from the flow
/// test added with SYNTAX.md v0.3. The pipeline files are clean,
/// and every wire in them resolves.
#[test]
fn sample_vault_warnings() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample-vault");
    let mut vault = Vault::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                let rel = path
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                vault.insert(&rel, &std::fs::read_to_string(&path).unwrap());
            }
        }
    }
    // README, 9 parser tests, 9 pipeline notes, 1 flow.
    assert_eq!(vault.files().count(), 20);

    let found: Vec<(String, usize, &str)> = vault
        .warnings()
        .into_iter()
        .flat_map(|(path, list)| {
            list.into_iter()
                .map(move |w| (path.to_string(), w.line, w.code.as_str()))
        })
        .collect();
    let expected = [
        ("_parser-tests/flow-problems", 12, "flow-ignored"),
        ("_parser-tests/flow-problems", 18, "position-bad"),
        ("_parser-tests/flow-problems", 19, "node-duplicate"),
        ("_parser-tests/flow-problems", 21, "node-missing"),
        ("_parser-tests/format-problems", 10, "done-added"),
        ("_parser-tests/format-problems", 13, "entry-no-char"),
        ("_parser-tests/format-problems", 21, "status-unknown"),
        ("_parser-tests/last-format-wins", 7, "format-ignored"),
        ("_parser-tests/roles", 10, "role-unknown"),
        ("_parser-tests/roles", 12, "needs-mismatch"),
    ];
    let expected: Vec<(String, usize, &str)> = expected
        .iter()
        .map(|(p, l, c)| (p.to_string(), *l, *c))
        .collect();
    assert_eq!(found, expected);

    // The README's pipeline table, checked against the files.
    let comp = vault.file("comp/sh040").unwrap();
    let c1 = comp.task_by_id("c1").unwrap();
    assert_eq!(c1.needs.len(), 3);
    assert_eq!(vault.needs_state("comp/sh040", c1), NeedsState::Unmet);
    let anim = vault.file("anim/sh040").unwrap();
    assert_eq!(
        vault.needs_state("anim/sh040", anim.task_by_id("a2").unwrap()),
        NeedsState::Met,
        "rig/hero ^r2 is done, so Polish pass is unblocked"
    );
}

/// Reading and writing back unchanged must give the same bytes for every
/// file, which `set_status` guarantees by construction: check it on all of
/// them by setting each known task to the status it already has.
#[test]
fn sample_vault_round_trips() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample-vault");
    for dir in std::fs::read_dir(&root).unwrap() {
        let dir = dir.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let src = std::fs::read_to_string(&path).unwrap();
            for task in parse(&src).tasks.iter().filter(|t| t.known) {
                let same = set_status(&src, task.line, task.ch).unwrap();
                assert_eq!(same, src, "{} line {}", path.display(), task.line);
            }
        }
    }
}

// ── Flows (SYNTAX.md section 7) ──

fn sample_vault() -> Vault {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample-vault");
    let mut vault = Vault::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                let rel = path
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                vault.insert(&rel, &std::fs::read_to_string(&path).unwrap());
            }
        }
    }
    vault
}

#[test]
fn flow_problems() {
    let file = parse(parser_test!("flow-problems.md"));
    let flow = file.flow.as_ref().expect("the second block is the flow");
    assert_eq!(flow.line, 16);
    let nodes: Vec<(&str, Option<(i32, i32)>)> = flow
        .nodes
        .iter()
        .map(|n| (n.path.as_str(), n.pos))
        .collect();
    assert_eq!(
        nodes,
        [
            ("anim/sh040", Some((320, 40))),
            ("fx/sh040-water", None),
            ("ghost/missing", None)
        ]
    );
    assert_eq!(
        codes(&file),
        [
            (12, "flow-ignored"),
            (18, "position-bad"),
            (19, "node-duplicate")
        ]
    );
    // Nothing in a flow block is a task.
    assert!(file.tasks.is_empty());
}

#[test]
fn sample_flow_lists_the_pipeline() {
    let vault = sample_vault();
    let flow = vault
        .file("flows/ep04-sh040")
        .unwrap()
        .flow
        .as_ref()
        .unwrap();
    assert_eq!(flow.nodes.len(), 9);
    assert!(flow.nodes.iter().all(|n| n.pos.is_some()));
    assert!(flow.nodes.iter().all(|n| vault.file(&n.path).is_some()));
    assert_eq!(flow.nodes[3].path, "lighting/sh040");
    assert_eq!(flow.nodes[3].pos, Some((640, 120)));
}

#[test]
fn moving_a_node_rewrites_only_its_numbers() {
    let src = parser_test!("flow-problems.md");
    // A valid position, a broken one, and none at all.
    let moved = set_flow_position(src, 17, -5, 1200).unwrap();
    assert_eq!(
        moved,
        src.replacen("[[anim/sh040]] @ 320, 40", "[[anim/sh040]] @ -5, 1200", 1)
    );
    let fixed = set_flow_position(src, 18, 1, 2).unwrap();
    assert_eq!(
        fixed,
        src.replacen(
            "[[fx/sh040-water]] @ left, 3",
            "[[fx/sh040-water]] @ 1, 2",
            1
        )
    );
    let placed = set_flow_position(src, 21, 7, 8).unwrap();
    assert_eq!(
        placed,
        src.replacen("- [[ghost/missing]]\n", "- [[ghost/missing]] @ 7, 8\n", 1)
    );
    // The ignored duplicate and the earlier block aren't nodes.
    assert_eq!(
        set_flow_position(src, 19, 0, 0),
        Err(EditError::NoNode { line: 19 })
    );
    assert_eq!(
        set_flow_position(src, 13, 0, 0),
        Err(EditError::NoNode { line: 13 })
    );

    let crlf = "::flow::\r\n- [[a]] @ 1, 2\r\n::flow::\r\n";
    assert_eq!(
        set_flow_position(crlf, 2, 3, 4).unwrap(),
        "::flow::\r\n- [[a]] @ 3, 4\r\n::flow::\r\n"
    );
}

#[test]
fn flow_edge_cases() {
    let unclosed = parse("::flow::\n- [[a]]\n");
    assert_eq!(unclosed.flow, None);
    assert_eq!(codes(&unclosed), [(1, "flow-unclosed")]);

    let file = parse(
        "::flow::\n- [[a.md|Alias]] @ 1,2\n  * [[b#Heading]]\n- [ ] not a task here\n::flow::\n",
    );
    let flow = file.flow.unwrap();
    let paths: Vec<&str> = flow.nodes.iter().map(|n| n.path.as_str()).collect();
    assert_eq!(paths, ["a", "b"]);
    assert_eq!(flow.nodes[0].pos, Some((1, 2)));
    assert!(file.tasks.is_empty());
}

#[test]
fn sample_vault_flow_warnings() {
    let vault = sample_vault();
    let warnings = vault.warnings();
    let found: Vec<_> = warnings["_parser-tests/flow-problems"]
        .iter()
        .map(|w| (w.line, w.code.as_str()))
        .collect();
    assert_eq!(
        found,
        [
            (12, "flow-ignored"),
            (18, "position-bad"),
            (19, "node-duplicate"),
            (21, "node-missing")
        ]
    );
    assert!(!warnings.contains_key("flows/ep04-sh040"));
}

// ── Status changes and what they set off (SYNTAX.md section 6) ──

/// Milestone 4's "done when": ticking Polish pass (anim) and Cache water to
/// disk (fx) flips Render lighting pass from ⚪ to 🔳, in the file.
#[test]
fn ticking_both_needs_unblocks_lighting() {
    let mut vault = sample_vault();
    let lighting = |v: &Vault| {
        v.file("lighting/sh040")
            .unwrap()
            .task_by_id("l2")
            .unwrap()
            .ch
    };
    assert_eq!(lighting(&vault), '-');

    // Polish pass: anim line 10. Lighting still waits for the water cache.
    let changes = vault.set_status("anim/sh040", 10, ' ', 'x').unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(lighting(&vault), '-');

    // Cache water to disk: fx line 10. Now both needs are done.
    let changes = vault.set_status("fx/sh040-water", 10, ' ', 'x').unwrap();
    assert_eq!(
        changes,
        [
            Change {
                path: "fx/sh040-water".into(),
                line: 10,
                from: ' ',
                to: 'x'
            },
            Change {
                path: "lighting/sh040".into(),
                line: 8,
                from: '-',
                to: ' '
            },
        ]
    );
    assert_eq!(lighting(&vault), ' ');
    assert!(vault
        .source("lighting/sh040")
        .unwrap()
        .contains("- [ ] Render lighting pass @maya @unit7 ^l2"));
    // Comp also needs f2, but still waits for l2 and f3: no flip.
    assert_eq!(
        vault
            .file("comp/sh040")
            .unwrap()
            .task_by_id("c1")
            .unwrap()
            .ch,
        '-'
    );
}

#[test]
fn a_retake_blocks_waiting_tasks_again() {
    let mut vault = sample_vault();
    vault.set_status("anim/sh040", 10, ' ', 'x').unwrap();
    vault.set_status("fx/sh040-water", 10, ' ', 'x').unwrap();

    // Polish pass goes back to ready to check: lighting is open, so it's
    // blocked again.
    let changes = vault.set_status("anim/sh040", 10, 'x', '?').unwrap();
    assert_eq!(
        changes[1],
        Change {
            path: "lighting/sh040".into(),
            line: 8,
            from: ' ',
            to: '-'
        }
    );

    // Once lighting has moved on, a retake leaves it alone.
    vault.set_status("anim/sh040", 10, '?', 'x').unwrap();
    vault.set_status("lighting/sh040", 8, ' ', '?').unwrap();
    let changes = vault.set_status("anim/sh040", 10, 'x', '?').unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        vault
            .file("lighting/sh040")
            .unwrap()
            .task_by_id("l2")
            .unwrap()
            .ch,
        '?'
    );
}

#[test]
fn a_stale_status_changes_nothing() {
    let mut vault = sample_vault();
    let before = vault.source("anim/sh040").unwrap().to_string();
    assert_eq!(
        vault.set_status("anim/sh040", 10, 'x', '?'),
        Err(EditError::Stale {
            line: 10,
            found: ' '
        })
    );
    assert_eq!(vault.source("anim/sh040").unwrap(), before);
    assert!(matches!(
        vault.set_status("nope", 1, ' ', 'x'),
        Err(EditError::NoFile { .. })
    ));
}

#[test]
fn files_without_a_blocked_status_are_never_flipped() {
    let mut vault = Vault::new();
    vault.insert("a.md", "- [ ] Upstream ^up\n");
    vault.insert(
        "b.md",
        "::statusformat:: [ ] = o = open, [x] = d = done ::statusformat::\n- [ ] Waits\n  - needs:: [[a#^up]]\n",
    );
    let changes = vault.set_status("a", 1, ' ', 'x').unwrap();
    assert_eq!(changes.len(), 1);
}
