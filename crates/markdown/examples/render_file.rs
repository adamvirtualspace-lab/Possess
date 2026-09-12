//! Render a Markdown file the way the preview does, for comparing against the
//! JavaScript renderer this replaces.
//!
//! Usage: render_file <file.md> [note-path] [note1,note2,...]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let source = std::fs::read_to_string(&args[1]).expect("read markdown");
    let note_path = args.get(2).map(String::as_str).filter(|p| !p.is_empty());
    let notes: Vec<String> = args
        .get(3)
        .map(|list| list.split(',').map(str::to_string).collect())
        .unwrap_or_default();

    let context = possess_markdown::RenderContext { note_path, notes: &notes };
    print!("{}", possess_markdown::render(&source, &context));
}
