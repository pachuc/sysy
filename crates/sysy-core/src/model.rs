use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A design and its independently editable layout state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Design {
    pub title: String,
    pub description: Option<String>,
    pub containers: Vec<Container>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub notes: Vec<Note>,
    pub layout: Layout,
}

impl Design {
    /// Return all problems that would prevent this design from being saved.
    #[must_use]
    pub fn validate(&self) -> Vec<crate::Problem> {
        crate::validate(self)
    }
}

/// A component's visual kind; it does not add behavior.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Service,
    Database,
    Queue,
    Cache,
    Storage,
    Client,
    External,
    Function,
    Generic,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Container {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeKind {
    Sync,
    Async,
    Data,
    Dependency,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Edge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bidirectional: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Note {
    pub id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
}

/// Entries are pinned positions, ordered by element id in the file.
pub type Layout = BTreeMap<String, LayoutEntry>;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct LayoutEntry {
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<Size>,
}

/// Only containers may have a saved size.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}
