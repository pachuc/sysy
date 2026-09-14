use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize, Serializer, de::DeserializeOwned};
use serde_json::{Map, Value};
use tempfile::NamedTempFile;
use thiserror::Error;

use crate::{Container, Design, Edge, Layout, Node, Note, Problem};

pub const VERSION: u64 = 1;
const KEYS: [&str; 7] = [
    "version",
    "design",
    "containers",
    "nodes",
    "edges",
    "notes",
    "layout",
];

#[derive(Debug, Error)]
pub enum Error {
    #[error("design file I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("{kind} '{id}' does not exist")]
    NotFound { kind: &'static str, id: String },
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid design:\n{}", problem_messages(.0))]
    Validation(Vec<Problem>),
}

fn problem_messages(problems: &[Problem]) -> String {
    problems
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Default, Deserialize, Serialize)]
struct Metadata {
    title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

// Struct field order defines the canonical JSON key order.
#[derive(Serialize)]
struct Document<'a> {
    version: u64,
    design: Metadata,
    containers: Vec<&'a Container>,
    nodes: Vec<&'a Node>,
    edges: Vec<&'a Edge>,
    notes: Vec<&'a Note>,
    layout: &'a Layout,
}

/// Read a design, collecting file-format and model problems together.
///
/// # Errors
/// Returns an I/O error if reading fails, a JSON error for invalid JSON syntax,
/// or all discovered validation problems for a JSON document with invalid data.
pub fn load(path: impl AsRef<Path>) -> Result<Design, Error> {
    let value: Value = serde_json::from_reader(File::open(path)?)?;
    let Some(object) = value.as_object() else {
        return Err(Error::Validation(vec![Problem::InvalidField {
            field: "document".into(),
            message: "expected an object".into(),
        }]));
    };
    let mut problems = Vec::new();
    match object.get("version") {
        Some(version) if version.as_u64() == Some(VERSION) => {}
        Some(version) => problems.push(Problem::UnknownVersion {
            found: version.clone(),
            supported: VERSION,
        }),
        None => problems.push(Problem::InvalidField {
            field: "version".into(),
            message: "missing field".into(),
        }),
    }
    for key in object.keys() {
        if !KEYS.contains(&key.as_str()) {
            problems.push(Problem::UnknownTopLevelKey { key: key.clone() });
        }
    }
    let metadata: Metadata = read_field(object, "design", &mut problems).unwrap_or_default();
    let design = Design {
        title: metadata.title,
        description: metadata.description,
        containers: read_elements(object, "containers", &mut problems),
        nodes: read_elements(object, "nodes", &mut problems),
        edges: read_elements(object, "edges", &mut problems),
        notes: read_elements(object, "notes", &mut problems),
        layout: read_layout(object, &mut problems),
    };
    problems.extend(design.validate());
    if problems.is_empty() {
        Ok(design)
    } else {
        Err(Error::Validation(problems))
    }
}

fn read_field<T: DeserializeOwned>(
    object: &Map<String, Value>,
    field: &str,
    problems: &mut Vec<Problem>,
) -> Option<T> {
    let Some(value) = object.get(field) else {
        problems.push(Problem::InvalidField {
            field: field.into(),
            message: "missing field".into(),
        });
        return None;
    };
    decode(value, field, problems)
}

fn decode<T: DeserializeOwned>(
    value: &Value,
    field: &str,
    problems: &mut Vec<Problem>,
) -> Option<T> {
    match T::deserialize(value) {
        Ok(value) => Some(value),
        Err(error) => {
            problems.push(Problem::InvalidField {
                field: field.into(),
                message: error.to_string(),
            });
            None
        }
    }
}

fn read_elements<T: DeserializeOwned>(
    object: &Map<String, Value>,
    field: &str,
    problems: &mut Vec<Problem>,
) -> Vec<T> {
    // Decode each element independently so a malformed record cannot hide
    // problems in the remaining records or sections.
    let Some(Value::Array(values)) = object.get(field) else {
        problems.push(Problem::InvalidField {
            field: field.into(),
            message: "expected an array".into(),
        });
        return Vec::new();
    };
    values
        .iter()
        .enumerate()
        .filter_map(|(index, value)| decode(value, &format!("{field}[{index}]"), problems))
        .collect()
}

fn read_layout(object: &Map<String, Value>, problems: &mut Vec<Problem>) -> Layout {
    // A design can be authored before any positions have been saved.
    let Some(value) = object.get("layout") else {
        return Layout::new();
    };
    let Value::Object(entries) = value else {
        problems.push(Problem::InvalidField {
            field: "layout".into(),
            message: "expected an object".into(),
        });
        return Layout::new();
    };
    entries
        .iter()
        .filter_map(|(id, value)| {
            decode(value, &format!("layout.{id}"), problems).map(|entry| (id.clone(), entry))
        })
        .collect()
}

/// Validate and atomically replace a file with canonical JSON.
///
/// Arrays are sorted by id without changing the caller's design. Optional
/// fields are omitted when absent; empty tags and false bidirectionality are
/// omitted too. Layout uses `x`, `y`, and an optional `size` object containing
/// `width` and `height`.
///
/// # Errors
/// Returns all validation problems before writing, or an error if serialization,
/// writing, syncing, or renaming fails. A failure before the rename leaves an
/// existing target untouched.
pub fn save(path: impl AsRef<Path>, design: &Design) -> Result<(), Error> {
    let problems = design.validate();
    if !problems.is_empty() {
        return Err(Error::Validation(problems));
    }
    write_design(path.as_ref(), design, false)
}

/// Create a design file without replacing an existing path.
///
/// # Errors
/// Returns validation problems or a serialization or I/O error, including when
/// the destination already exists. Existing files are left untouched.
pub fn create(path: impl AsRef<Path>, design: &Design) -> Result<(), Error> {
    let problems = design.validate();
    if !problems.is_empty() {
        return Err(Error::Validation(problems));
    }
    write_design(path.as_ref(), design, true)
}

fn write_design(path: &Path, design: &Design, create_new: bool) -> Result<(), Error> {
    let mut bytes = serde_json::to_vec_pretty(design)?;
    bytes.push(b'\n');
    atomic_write(path, |file| file.write_all(&bytes), create_new)?;
    Ok(())
}

// CLI output and saved files share one representation, including sorted arrays.
impl Serialize for Design {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Document {
            version: VERSION,
            design: Metadata {
                title: self.title.clone(),
                description: self.description.clone(),
            },
            containers: ordered(&self.containers, |item| &item.id),
            nodes: ordered(&self.nodes, |item| &item.id),
            edges: ordered(&self.edges, |item| &item.id),
            notes: ordered(&self.notes, |item| &item.id),
            layout: &self.layout,
        }
        .serialize(serializer)
    }
}

fn ordered<T>(items: &[T], id: impl Fn(&T) -> &str) -> Vec<&T> {
    let mut items: Vec<_> = items.iter().collect();
    items.sort_by(|left, right| id(left).cmp(id(right)));
    items
}

fn atomic_write(
    path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
    create_new: bool,
) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = NamedTempFile::new_in(parent)?;
    write(temporary.as_file_mut())?;
    temporary.as_file_mut().flush()?;
    temporary.as_file().sync_all()?;
    if create_new {
        temporary
            .persist_noclobber(path)
            .map_err(|error| error.error)?;
    } else {
        temporary.persist(path).map_err(|error| error.error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_after_partial_write_preserves_original_and_removes_temporary() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("design.json");
        let original = include_bytes!("../tests/fixtures/checkout.json");
        std::fs::write(&target, original).unwrap();

        let result = atomic_write(
            &target,
            |file| {
                file.write_all(b"{\"version\":")?;
                Err(io::Error::other("simulated interruption before rename"))
            },
            false,
        );

        assert!(result.is_err());
        assert_eq!(std::fs::read(&target).unwrap(), original);
        assert!(load(&target).is_ok());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
