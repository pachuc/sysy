use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::Design;

/// A single file-format or model problem. Validation collects these together.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum Problem {
    #[error("unsupported version {found}; supported version is {supported}")]
    UnknownVersion {
        found: serde_json::Value,
        supported: u64,
    },
    #[error("unknown top-level key '{key}'")]
    UnknownTopLevelKey { key: String },
    #[error("invalid field '{field}': {message}")]
    InvalidField { field: String, message: String },
    #[error("invalid id '{id}': expected lowercase letters, digits, and inner hyphens")]
    InvalidId { id: String },
    #[error("duplicate id '{id}'")]
    DuplicateId { id: String },
    #[error("element '{id}' has {field} reference to missing element '{target}'")]
    MissingReference {
        id: String,
        field: &'static str,
        target: String,
    },
    #[error("element '{id}' has {field} reference to '{target}'; expected {expected}")]
    InvalidReference {
        id: String,
        field: &'static str,
        target: String,
        expected: &'static str,
    },
    #[error("edge '{id}' connects two containers; at least one endpoint must be a node")]
    InvalidEdge { id: String },
    #[error("container parent cycle: {ids:?}")]
    ContainerCycle { ids: Vec<String> },
    #[error("invalid layout for '{id}': {message}")]
    InvalidLayout { id: String, message: &'static str },
}

#[derive(Clone, Copy, PartialEq)]
enum ElementKind {
    Container,
    Node,
    Edge,
    Note,
}

/// Check ids, references, container ancestry, and layout without changing a design.
#[must_use]
pub fn validate(design: &Design) -> Vec<Problem> {
    let mut problems = Vec::new();
    let elements = index_elements(design, &mut problems);
    for node in &design.nodes {
        if let Some(container) = &node.container {
            check_reference(
                &elements,
                &node.id,
                "container",
                container,
                &[ElementKind::Container],
                &mut problems,
            );
        }
    }
    for container in &design.containers {
        if let Some(parent) = &container.parent {
            check_reference(
                &elements,
                &container.id,
                "parent",
                parent,
                &[ElementKind::Container],
                &mut problems,
            );
        }
    }
    for edge in &design.edges {
        for (field, target) in [("from", &edge.from), ("to", &edge.to)] {
            check_reference(
                &elements,
                &edge.id,
                field,
                target,
                &[ElementKind::Node, ElementKind::Container],
                &mut problems,
            );
        }
        if elements.get(edge.from.as_str()) == Some(&ElementKind::Container)
            && elements.get(edge.to.as_str()) == Some(&ElementKind::Container)
        {
            problems.push(Problem::InvalidEdge {
                id: edge.id.clone(),
            });
        }
    }
    for note in &design.notes {
        if let Some(target) = &note.on {
            check_reference(
                &elements,
                &note.id,
                "on",
                target,
                &[ElementKind::Node, ElementKind::Container, ElementKind::Edge],
                &mut problems,
            );
        }
    }
    check_cycles(design, &mut problems);
    check_layout(design, &elements, &mut problems);
    problems
}

fn index_elements<'a>(
    design: &'a Design,
    problems: &mut Vec<Problem>,
) -> BTreeMap<&'a str, ElementKind> {
    let mut elements = BTreeMap::new();
    let ids = design
        .containers
        .iter()
        .map(|item| (&item.id, ElementKind::Container))
        .chain(
            design
                .nodes
                .iter()
                .map(|item| (&item.id, ElementKind::Node)),
        )
        .chain(
            design
                .edges
                .iter()
                .map(|item| (&item.id, ElementKind::Edge)),
        )
        .chain(
            design
                .notes
                .iter()
                .map(|item| (&item.id, ElementKind::Note)),
        );
    for (id, kind) in ids {
        if !valid_id(id) {
            problems.push(Problem::InvalidId { id: id.clone() });
        }
        if elements.insert(id.as_str(), kind).is_some() {
            problems.push(Problem::DuplicateId { id: id.clone() });
        }
    }
    elements
}

fn valid_id(id: &str) -> bool {
    let alphanumeric = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    !id.is_empty()
        && id.bytes().all(|byte| alphanumeric(byte) || byte == b'-')
        && id.bytes().next().is_some_and(alphanumeric)
        && id.bytes().next_back().is_some_and(alphanumeric)
}

fn check_reference(
    elements: &BTreeMap<&str, ElementKind>,
    id: &str,
    field: &'static str,
    target: &str,
    allowed: &[ElementKind],
    problems: &mut Vec<Problem>,
) {
    match elements.get(target) {
        None => problems.push(Problem::MissingReference {
            id: id.into(),
            field,
            target: target.into(),
        }),
        Some(kind) if !allowed.contains(kind) => problems.push(Problem::InvalidReference {
            id: id.into(),
            field,
            target: target.into(),
            expected: match allowed.len() {
                1 => "a container",
                2 => "a node or container",
                _ => "a node, container, or edge",
            },
        }),
        Some(_) => {}
    }
}

fn check_cycles(design: &Design, problems: &mut Vec<Problem>) {
    let parents: BTreeMap<_, _> = design
        .containers
        .iter()
        .map(|container| (container.id.as_str(), container.parent.as_deref()))
        .collect();
    let mut visited = BTreeSet::new();
    // Walk iteratively so valid, deeply nested designs do not exhaust the stack.
    for &start in parents.keys() {
        let mut path = Vec::new();
        let mut indices = BTreeMap::new();
        let mut current = Some(start);
        while let Some(id) = current {
            if let Some(&index) = indices.get(id) {
                let mut ids: Vec<String> =
                    path[index..].iter().map(|id: &&str| (*id).into()).collect();
                // Start at the smallest id so a cycle has one stable representation.
                if let Some((index, _)) = ids.iter().enumerate().min_by_key(|(_, id)| *id) {
                    ids.rotate_left(index);
                }
                problems.push(Problem::ContainerCycle { ids });
                break;
            }
            if !visited.insert(id) {
                break;
            }
            indices.insert(id, path.len());
            path.push(id);
            current = parents.get(id).copied().flatten();
        }
    }
}

fn check_layout(
    design: &Design,
    elements: &BTreeMap<&str, ElementKind>,
    problems: &mut Vec<Problem>,
) {
    for (id, entry) in &design.layout {
        if !valid_id(id) {
            problems.push(Problem::InvalidId { id: id.clone() });
        }
        if !elements.contains_key(id.as_str()) {
            problems.push(Problem::MissingReference {
                id: id.clone(),
                field: "layout",
                target: id.clone(),
            });
        }
        if !entry.x.is_finite() || !entry.y.is_finite() {
            problems.push(Problem::InvalidLayout {
                id: id.clone(),
                message: "coordinates must be finite",
            });
        }
        if let Some(size) = entry.size {
            if elements.get(id.as_str()) != Some(&ElementKind::Container) {
                problems.push(Problem::InvalidLayout {
                    id: id.clone(),
                    message: "only containers may have a size",
                });
            }
            if !size.width.is_finite() || !size.height.is_finite() {
                problems.push(Problem::InvalidLayout {
                    id: id.clone(),
                    message: "size must be finite",
                });
            }
        }
    }
}
