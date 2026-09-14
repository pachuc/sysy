use sysy_core::Design;

pub(super) fn design_summary(design: &Design) -> String {
    let mut lines = vec![design.title.clone()];
    if let Some(description) = &design.description {
        lines.push(description.clone());
    }
    contents(design, None, 0, &mut lines);
    lines.push("Edges:".into());
    for edge in design.list_edges() {
        let kind = serde_json::to_value(edge.kind).expect("edge kinds serialize as strings");
        let arrow = if edge.bidirectional { "<->" } else { "->" };
        lines.push(format!(
            "  {} {arrow} {} ({}){}",
            edge.from,
            edge.to,
            kind.as_str().expect("edge kind is a string"),
            edge.label
                .as_ref()
                .map_or_else(String::new, |label| format!(": {label}")),
        ));
    }
    lines.push("Notes:".into());
    for note in design.list_notes() {
        lines.push(format!(
            "  {}{}: {}",
            note.id,
            note.on
                .as_ref()
                .map_or_else(String::new, |id| format!(" (on {id})")),
            note.text,
        ));
    }
    lines.join("\n")
}

fn contents(design: &Design, parent: Option<&str>, depth: usize, lines: &mut Vec<String>) {
    let indent = "  ".repeat(depth);
    for container in design.list_containers() {
        if container.parent.as_deref() == parent {
            lines.push(format!("{indent}{} [{}]", container.label, container.id));
            contents(design, Some(&container.id), depth + 1, lines);
        }
    }
    for node in design.list_nodes() {
        if node.container.as_deref() == parent {
            let kind = serde_json::to_value(node.kind).expect("node kinds serialize as strings");
            lines.push(format!(
                "{indent}{} [{}] ({})",
                node.label,
                node.id,
                kind.as_str().expect("node kind is a string"),
            ));
        }
    }
}
