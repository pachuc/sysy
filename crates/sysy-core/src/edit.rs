use std::collections::BTreeSet;

use crate::{Container, Design, Edge, EdgeKind, Error, Node, NodeKind, Note};

/// A change to an optional field, distinguishing preservation from removal.
#[derive(Clone, Debug, Default)]
pub enum OptionalUpdate<T> {
    #[default]
    Keep,
    Set(T),
    Clear,
}

impl<T> OptionalUpdate<T> {
    fn apply(self, field: &mut Option<T>) {
        match self {
            Self::Keep => {}
            Self::Set(value) => *field = Some(value),
            Self::Clear => *field = None,
        }
    }
}

/// Changes to design metadata; omitted fields preserve their current values.
#[derive(Clone, Debug, Default)]
pub struct DesignUpdate {
    pub title: Option<String>,
    pub description: OptionalUpdate<String>,
}

/// Fields to change; `None` preserves the current value.
/// Optional fields use `OptionalUpdate` to distinguish setting and clearing.
#[derive(Clone, Debug, Default)]
pub struct NodeUpdate {
    pub kind: Option<NodeKind>,
    pub label: Option<String>,
    pub description: OptionalUpdate<String>,
    pub tags: Option<Vec<String>>,
    pub container: OptionalUpdate<String>,
}

/// Fields to change; `None` preserves the current value.
/// Optional fields use `OptionalUpdate` to distinguish setting and clearing.
#[derive(Clone, Debug, Default)]
pub struct ContainerUpdate {
    pub label: Option<String>,
    pub description: OptionalUpdate<String>,
    pub parent: OptionalUpdate<String>,
}

/// Fields to change; `None` preserves the current value.
/// Optional fields use `OptionalUpdate` to distinguish setting and clearing.
#[derive(Clone, Debug, Default)]
pub struct EdgeUpdate {
    pub from: Option<String>,
    pub to: Option<String>,
    pub kind: Option<EdgeKind>,
    pub label: OptionalUpdate<String>,
    pub bidirectional: Option<bool>,
}

/// Fields to change; `None` preserves the current value.
/// Optional fields use `OptionalUpdate` to distinguish setting and clearing.
#[derive(Clone, Debug, Default)]
pub struct NoteUpdate {
    pub text: Option<String>,
    pub on: OptionalUpdate<String>,
}

impl Design {
    // Validate a candidate before replacing the caller's state so the viewer can
    // recover from a rejected edit without reloading the file.
    fn edit<T>(&mut self, change: impl FnOnce(&mut Self) -> Result<T, Error>) -> Result<T, Error> {
        let mut candidate = self.clone();
        let record = change(&mut candidate)?;
        let problems = candidate.validate();
        if !problems.is_empty() {
            return Err(Error::Validation(problems));
        }
        *self = candidate;
        Ok(record)
    }

    /// Update metadata without changing architecture or saved positions.
    ///
    /// # Errors
    /// Returns all validation problems, leaving the design unchanged.
    pub fn set(&mut self, update: DesignUpdate) -> Result<Self, Error> {
        self.edit(|design| {
            if let Some(title) = update.title {
                design.title = title;
            }
            update.description.apply(&mut design.description);
            Ok(design.clone())
        })
    }

    /// Add a node and validate the whole design.
    ///
    /// # Errors
    /// Returns all validation problems, leaving the design unchanged.
    pub fn add_node(&mut self, record: Node) -> Result<Node, Error> {
        self.edit(|design| {
            design.nodes.push(record.clone());
            Ok(record)
        })
    }

    /// Update an existing node, preserving fields that are not supplied.
    ///
    /// # Errors
    /// Returns an error for a missing id or all validation problems, leaving
    /// the design unchanged.
    pub fn set_node(&mut self, id: &str, update: NodeUpdate) -> Result<Node, Error> {
        self.edit(|design| {
            let record = design
                .nodes
                .iter_mut()
                .find(|record| record.id == id)
                .ok_or_else(|| missing("node", id))?;
            if let Some(value) = update.kind {
                record.kind = value;
            }
            if let Some(value) = update.label {
                record.label = value;
            }
            update.description.apply(&mut record.description);
            if let Some(value) = update.tags {
                record.tags = value;
            }
            update.container.apply(&mut record.container);
            Ok(record.clone())
        })
    }

    /// Return nodes sorted by id.
    #[must_use]
    pub fn list_nodes(&self) -> Vec<&Node> {
        let mut records: Vec<_> = self.nodes.iter().collect();
        records.sort_by(|left, right| left.id.cmp(&right.id));
        records
    }

    /// Remove a node and clean up dependent records and layout entries.
    ///
    /// # Errors
    /// Returns an error for a missing id or all validation problems, leaving
    /// the design unchanged.
    pub fn remove_node(&mut self, id: &str) -> Result<Node, Error> {
        self.edit(|design| {
            let index = design
                .nodes
                .iter()
                .position(|record| record.id == id)
                .ok_or_else(|| missing("node", id))?;
            let record = design.nodes.remove(index);
            design.remove_dependents(id);
            Ok(record)
        })
    }

    /// Add a container and validate the whole design.
    ///
    /// # Errors
    /// Returns all validation problems, leaving the design unchanged.
    pub fn add_container(&mut self, record: Container) -> Result<Container, Error> {
        self.edit(|design| {
            design.containers.push(record.clone());
            Ok(record)
        })
    }

    /// Update an existing container, preserving fields that are not supplied.
    ///
    /// # Errors
    /// Returns an error for a missing id or all validation problems, leaving
    /// the design unchanged.
    pub fn set_container(&mut self, id: &str, update: ContainerUpdate) -> Result<Container, Error> {
        self.edit(|design| {
            let record = design
                .containers
                .iter_mut()
                .find(|record| record.id == id)
                .ok_or_else(|| missing("container", id))?;
            if let Some(value) = update.label {
                record.label = value;
            }
            update.description.apply(&mut record.description);
            update.parent.apply(&mut record.parent);
            Ok(record.clone())
        })
    }

    /// Return containers sorted by id.
    #[must_use]
    pub fn list_containers(&self) -> Vec<&Container> {
        let mut records: Vec<_> = self.containers.iter().collect();
        records.sort_by(|left, right| left.id.cmp(&right.id));
        records
    }

    /// Remove a container and clean up dependent records and layout entries.
    /// Direct children move to the removed container's parent.
    ///
    /// # Errors
    /// Returns an error for a missing id or all validation problems, leaving
    /// the design unchanged.
    pub fn remove_container(&mut self, id: &str) -> Result<Container, Error> {
        self.edit(|design| {
            let index = design
                .containers
                .iter()
                .position(|record| record.id == id)
                .ok_or_else(|| missing("container", id))?;
            let record = design.containers.remove(index);
            for node in &mut design.nodes {
                if node.container.as_deref() == Some(id) {
                    node.container.clone_from(&record.parent);
                }
            }
            for child in &mut design.containers {
                if child.parent.as_deref() == Some(id) {
                    child.parent.clone_from(&record.parent);
                }
            }
            design.remove_dependents(id);
            Ok(record)
        })
    }

    /// Add a edge and validate the whole design.
    ///
    /// # Errors
    /// Returns all validation problems, leaving the design unchanged.
    pub fn add_edge(&mut self, record: Edge) -> Result<Edge, Error> {
        self.edit(|design| {
            design.edges.push(record.clone());
            Ok(record)
        })
    }

    /// Update an existing edge, preserving fields that are not supplied.
    ///
    /// # Errors
    /// Returns an error for a missing id or all validation problems, leaving
    /// the design unchanged.
    pub fn set_edge(&mut self, id: &str, update: EdgeUpdate) -> Result<Edge, Error> {
        self.edit(|design| {
            let record = design
                .edges
                .iter_mut()
                .find(|record| record.id == id)
                .ok_or_else(|| missing("edge", id))?;
            if let Some(value) = update.from {
                record.from = value;
            }
            if let Some(value) = update.to {
                record.to = value;
            }
            if let Some(value) = update.kind {
                record.kind = value;
            }
            update.label.apply(&mut record.label);
            if let Some(value) = update.bidirectional {
                record.bidirectional = value;
            }
            Ok(record.clone())
        })
    }

    /// Return edges sorted by id.
    #[must_use]
    pub fn list_edges(&self) -> Vec<&Edge> {
        let mut records: Vec<_> = self.edges.iter().collect();
        records.sort_by(|left, right| left.id.cmp(&right.id));
        records
    }

    /// Remove a edge and clean up dependent records and layout entries.
    ///
    /// # Errors
    /// Returns an error for a missing id or all validation problems, leaving
    /// the design unchanged.
    pub fn remove_edge(&mut self, id: &str) -> Result<Edge, Error> {
        self.edit(|design| {
            let index = design
                .edges
                .iter()
                .position(|record| record.id == id)
                .ok_or_else(|| missing("edge", id))?;
            let record = design.edges.remove(index);
            design.remove_dependents(id);
            Ok(record)
        })
    }

    /// Add a note and validate the whole design.
    ///
    /// # Errors
    /// Returns all validation problems, leaving the design unchanged.
    pub fn add_note(&mut self, record: Note) -> Result<Note, Error> {
        self.edit(|design| {
            design.notes.push(record.clone());
            Ok(record)
        })
    }

    /// Update an existing note, preserving fields that are not supplied.
    ///
    /// # Errors
    /// Returns an error for a missing id or all validation problems, leaving
    /// the design unchanged.
    pub fn set_note(&mut self, id: &str, update: NoteUpdate) -> Result<Note, Error> {
        self.edit(|design| {
            let record = design
                .notes
                .iter_mut()
                .find(|record| record.id == id)
                .ok_or_else(|| missing("note", id))?;
            if let Some(value) = update.text {
                record.text = value;
            }
            update.on.apply(&mut record.on);
            Ok(record.clone())
        })
    }

    /// Return notes sorted by id.
    #[must_use]
    pub fn list_notes(&self) -> Vec<&Note> {
        let mut records: Vec<_> = self.notes.iter().collect();
        records.sort_by(|left, right| left.id.cmp(&right.id));
        records
    }

    /// Remove a note and clean up dependent records and layout entries.
    ///
    /// # Errors
    /// Returns an error for a missing id or all validation problems, leaving
    /// the design unchanged.
    pub fn remove_note(&mut self, id: &str) -> Result<Note, Error> {
        self.edit(|design| {
            let index = design
                .notes
                .iter()
                .position(|record| record.id == id)
                .ok_or_else(|| missing("note", id))?;
            let record = design.notes.remove(index);
            design.remove_dependents(id);
            Ok(record)
        })
    }

    fn remove_dependents(&mut self, id: &str) {
        let mut removed = BTreeSet::from([id.to_owned()]);
        self.edges.retain(|edge| {
            if edge.from == id || edge.to == id {
                removed.insert(edge.id.clone());
                false
            } else {
                true
            }
        });
        self.notes.retain(|note| {
            if note
                .on
                .as_ref()
                .is_some_and(|target| removed.contains(target))
            {
                removed.insert(note.id.clone());
                false
            } else {
                true
            }
        });
        self.layout.retain(|id, _| !removed.contains(id));
    }
}

fn missing(kind: &'static str, id: &str) -> Error {
    Error::NotFound {
        kind,
        id: id.into(),
    }
}
