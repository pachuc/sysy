mod summary;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::{Value, json};
use sysy_core::{
    Container, ContainerUpdate, Design, DesignUpdate, Edge, EdgeKind, EdgeUpdate, Node, NodeKind,
    NodeUpdate, Note, NoteUpdate, OptionalUpdate,
};

#[derive(Parser)]
#[command(name = "sysy", about = "System designs as data", version)]
struct Cli {
    /// Print machine-readable JSON instead of text
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the version of this CLI
    Version,
    /// Create an empty design without overwriting an existing file
    New {
        path: PathBuf,
        #[arg(long)]
        title: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// Change the design title or description, preserving other fields
    Set {
        path: PathBuf,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, conflicts_with = "clear_description")]
        description: Option<String>,
        /// Clear the optional description
        #[arg(long)]
        clear_description: bool,
    },
    /// Add, edit, remove, or list nodes
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },
    /// Add, edit, remove, or list containers
    Container {
        #[command(subcommand)]
        command: ContainerCommand,
    },
    /// Add, edit, remove, or list edges
    Edge {
        #[command(subcommand)]
        command: EdgeCommand,
    },
    /// Add, edit, remove, or list notes
    Note {
        #[command(subcommand)]
        command: NoteCommand,
    },
    /// Open the design viewer
    Ui { path: PathBuf },
    /// Compute and save positions, preserving pins unless reset
    Layout {
        path: PathBuf,
        #[arg(long)]
        reset: bool,
    },
    /// Print the whole design
    Show { path: PathBuf },
    /// Check the design and report all problems
    Validate { path: PathBuf },
}

#[derive(Args)]
struct Target {
    path: PathBuf,
    id: String,
}

#[derive(Subcommand)]
enum NodeCommand {
    /// Add a node
    Add(NodeAdd),
    /// Change supplied fields, preserving all others
    Set(NodeSet),
    /// Remove the node, its edges, attached notes, and their layout entries
    Remove(Target),
    /// List records sorted by id
    List { path: PathBuf },
}

#[derive(Args)]
struct NodeAdd {
    #[command(flatten)]
    target: Target,
    /// Component kind
    #[arg(long, value_enum)]
    kind: CliNodeKind,
    /// Display label
    #[arg(long)]
    label: String,
    /// Description
    #[arg(long)]
    description: Option<String>,
    /// Tags (repeat to supply several; set replaces all tags)
    #[arg(long)]
    tag: Vec<String>,
    /// Containing container
    #[arg(long = "in")]
    container: Option<String>,
}

#[derive(Args)]
struct NodeSet {
    #[command(flatten)]
    target: Target,
    /// Component kind
    #[arg(long, value_enum)]
    kind: Option<CliNodeKind>,
    /// Display label
    #[arg(long)]
    label: Option<String>,
    /// Description
    #[arg(long, conflicts_with = "clear_description")]
    description: Option<String>,
    /// Tags (repeat to supply several; set replaces all tags)
    #[arg(long, conflicts_with = "clear_tag")]
    tag: Vec<String>,
    /// Containing container
    #[arg(long = "in", conflicts_with = "clear_container")]
    container: Option<String>,
    /// Clear description
    #[arg(long = "clear-description")]
    clear_description: bool,
    /// Move to the top level
    #[arg(long = "clear-in")]
    clear_container: bool,
    /// Clear tags
    #[arg(long = "clear-tags")]
    clear_tag: bool,
}

#[derive(Subcommand)]
enum ContainerCommand {
    /// Add a container
    Add(ContainerAdd),
    /// Change supplied fields, preserving all others
    Set(ContainerSet),
    /// Reparent children and remove the container, its edges, and attached notes
    Remove(Target),
    /// List records sorted by id
    List { path: PathBuf },
}

#[derive(Args)]
struct ContainerAdd {
    #[command(flatten)]
    target: Target,
    /// Display label
    #[arg(long)]
    label: String,
    /// Description
    #[arg(long)]
    description: Option<String>,
    /// Parent container
    #[arg(long = "in")]
    parent: Option<String>,
}

#[derive(Args)]
struct ContainerSet {
    #[command(flatten)]
    target: Target,
    /// Display label
    #[arg(long)]
    label: Option<String>,
    /// Description
    #[arg(long, conflicts_with = "clear_description")]
    description: Option<String>,
    /// Parent container
    #[arg(long = "in", conflicts_with = "clear_parent")]
    parent: Option<String>,
    /// Clear description
    #[arg(long = "clear-description")]
    clear_description: bool,
    /// Move to the top level
    #[arg(long = "clear-in")]
    clear_parent: bool,
}

#[derive(Subcommand)]
enum EdgeCommand {
    /// Add an edge
    Add(EdgeAdd),
    /// Change supplied fields, preserving all others
    Set(EdgeSet),
    /// Remove the edge, attached notes, and their layout entries
    Remove(Target),
    /// List records sorted by id
    List { path: PathBuf },
}

#[derive(Args)]
struct EdgeAdd {
    #[command(flatten)]
    target: Target,
    /// Source node or container
    #[arg(long)]
    from: String,
    /// Destination node or container
    #[arg(long)]
    to: String,
    /// Line kind (defaults to sync on add)
    #[arg(long, value_enum, default_value = "sync")]
    kind: CliEdgeKind,
    /// Display label
    #[arg(long)]
    label: Option<String>,
    /// Draw arrows in both directions
    #[arg(long)]
    bidirectional: bool,
}

#[derive(Args)]
struct EdgeSet {
    #[command(flatten)]
    target: Target,
    /// Source node or container
    #[arg(long)]
    from: Option<String>,
    /// Destination node or container
    #[arg(long)]
    to: Option<String>,
    /// Line kind (defaults to sync on add)
    #[arg(long, value_enum)]
    kind: Option<CliEdgeKind>,
    /// Display label
    #[arg(long, conflicts_with = "clear_label")]
    label: Option<String>,
    /// Draw arrows in both directions; use --bidirectional=false to disable
    #[arg(long, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
    bidirectional: Option<bool>,
    /// Clear label
    #[arg(long = "clear-label")]
    clear_label: bool,
}

#[derive(Subcommand)]
enum NoteCommand {
    /// Add a note
    Add(NoteAdd),
    /// Change supplied fields, preserving all others
    Set(NoteSet),
    /// Remove the note and its layout entry
    Remove(Target),
    /// List records sorted by id
    List { path: PathBuf },
}

#[derive(Args)]
struct NoteAdd {
    #[command(flatten)]
    target: Target,
    /// Note text
    #[arg(long)]
    text: String,
    /// Node, container, or edge to attach to
    #[arg(long)]
    on: Option<String>,
}

#[derive(Args)]
struct NoteSet {
    #[command(flatten)]
    target: Target,
    /// Note text
    #[arg(long)]
    text: Option<String>,
    /// Node, container, or edge to attach to
    #[arg(long, conflicts_with = "clear_on")]
    on: Option<String>,
    /// Detach the note
    #[arg(long = "clear-on")]
    clear_on: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum CliNodeKind {
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

impl From<CliNodeKind> for NodeKind {
    fn from(value: CliNodeKind) -> Self {
        match value {
            CliNodeKind::Service => Self::Service,
            CliNodeKind::Database => Self::Database,
            CliNodeKind::Queue => Self::Queue,
            CliNodeKind::Cache => Self::Cache,
            CliNodeKind::Storage => Self::Storage,
            CliNodeKind::Client => Self::Client,
            CliNodeKind::External => Self::External,
            CliNodeKind::Function => Self::Function,
            CliNodeKind::Generic => Self::Generic,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum CliEdgeKind {
    Sync,
    Async,
    Data,
    Dependency,
}

impl From<CliEdgeKind> for EdgeKind {
    fn from(value: CliEdgeKind) -> Self {
        match value {
            CliEdgeKind::Sync => Self::Sync,
            CliEdgeKind::Async => Self::Async,
            CliEdgeKind::Data => Self::Data,
            CliEdgeKind::Dependency => Self::Dependency,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command) {
        Ok((value, message)) => {
            if cli.json {
                println!("{value}");
            } else {
                println!("{message}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", json!({"error": {"message": error.to_string()}}));
            ExitCode::FAILURE
        }
    }
}

type Output = (Value, String);

fn output(record: impl Serialize, message: impl Into<String>) -> Result<Output> {
    Ok((serde_json::to_value(record)?, message.into()))
}

fn mutate<T: Serialize>(
    path: &Path,
    change: impl FnOnce(&mut Design) -> Result<T, sysy_core::Error>,
    message: &str,
) -> Result<Output> {
    let mut design = sysy_core::load(path)?;
    let record = change(&mut design)?;
    sysy_core::save(path, &design)?;
    output(record, message)
}

fn run(command: Command) -> Result<Output> {
    match command {
        Command::Version => output(
            json!({"version": env!("CARGO_PKG_VERSION")}),
            format!("sysy {}", env!("CARGO_PKG_VERSION")),
        ),
        Command::New {
            path,
            title,
            description,
        } => {
            let design = Design {
                title,
                description,
                ..Design::default()
            };
            sysy_core::create(&path, &design)?;
            output(&design, format!("Created {}", path.display()))
        }
        Command::Set {
            path,
            title,
            description,
            clear_description,
        } => mutate(
            &path,
            |design| {
                design.set(DesignUpdate {
                    title,
                    description: optional(description, clear_description),
                })
            },
            "Updated design",
        ),
        Command::Ui { path } => {
            sysy_ui::run(&path)?;
            output(json!({"path": path}), "Viewer closed")
        }
        Command::Layout { path, reset } => {
            let mut design = sysy_core::load(&path)?;
            if reset {
                design.layout.clear();
            }
            design.layout = sysy_layout::layout(&design);
            sysy_core::save(&path, &design)?;
            output(
                &design.layout,
                format!("Saved layout to {}", path.display()),
            )
        }
        Command::Show { path } => {
            let design = sysy_core::load(path)?;
            let message = summary::design_summary(&design);
            output(design, message)
        }
        Command::Validate { path } => {
            sysy_core::load(path)?;
            output(Vec::<String>::new(), "Design is valid")
        }
        Command::Node { command } => run_node(command),
        Command::Container { command } => run_container(command),
        Command::Edge { command } => run_edge(command),
        Command::Note { command } => run_note(command),
    }
}

fn run_node(command: NodeCommand) -> Result<Output> {
    match command {
        NodeCommand::Add(args) => {
            let record = Node {
                id: args.target.id,
                kind: args.kind.into(),
                label: args.label,
                description: args.description,
                tags: args.tag,
                container: args.container,
            };
            mutate(
                &args.target.path,
                |design| design.add_node(record),
                "Added node",
            )
        }
        NodeCommand::Set(args) => {
            let update = NodeUpdate {
                kind: args.kind.map(Into::into),
                label: args.label,
                description: optional(args.description, args.clear_description),
                tags: if args.clear_tag || !args.tag.is_empty() {
                    Some(args.tag)
                } else {
                    None
                },
                container: optional(args.container, args.clear_container),
            };
            mutate(
                &args.target.path,
                |design| design.set_node(&args.target.id, update),
                "Updated node",
            )
        }
        NodeCommand::Remove(target) => mutate(
            &target.path,
            |design| design.remove_node(&target.id),
            "Removed node",
        ),
        NodeCommand::List { path } => {
            let design = sysy_core::load(path)?;
            let records = design.list_nodes();
            let message = format!("{} nodes", records.len());
            output(records, message)
        }
    }
}

fn run_container(command: ContainerCommand) -> Result<Output> {
    match command {
        ContainerCommand::Add(args) => {
            let record = Container {
                id: args.target.id,
                label: args.label,
                description: args.description,
                parent: args.parent,
            };
            mutate(
                &args.target.path,
                |design| design.add_container(record),
                "Added container",
            )
        }
        ContainerCommand::Set(args) => {
            let update = ContainerUpdate {
                label: args.label,
                description: optional(args.description, args.clear_description),
                parent: optional(args.parent, args.clear_parent),
            };
            mutate(
                &args.target.path,
                |design| design.set_container(&args.target.id, update),
                "Updated container",
            )
        }
        ContainerCommand::Remove(target) => mutate(
            &target.path,
            |design| design.remove_container(&target.id),
            "Removed container",
        ),
        ContainerCommand::List { path } => {
            let design = sysy_core::load(path)?;
            let records = design.list_containers();
            let message = format!("{} containers", records.len());
            output(records, message)
        }
    }
}

fn run_edge(command: EdgeCommand) -> Result<Output> {
    match command {
        EdgeCommand::Add(args) => {
            let record = Edge {
                id: args.target.id,
                from: args.from,
                to: args.to,
                kind: args.kind.into(),
                label: args.label,
                bidirectional: args.bidirectional,
            };
            mutate(
                &args.target.path,
                |design| design.add_edge(record),
                "Added edge",
            )
        }
        EdgeCommand::Set(args) => {
            let update = EdgeUpdate {
                from: args.from,
                to: args.to,
                kind: args.kind.map(Into::into),
                label: optional(args.label, args.clear_label),
                bidirectional: args.bidirectional,
            };
            mutate(
                &args.target.path,
                |design| design.set_edge(&args.target.id, update),
                "Updated edge",
            )
        }
        EdgeCommand::Remove(target) => mutate(
            &target.path,
            |design| design.remove_edge(&target.id),
            "Removed edge",
        ),
        EdgeCommand::List { path } => {
            let design = sysy_core::load(path)?;
            let records = design.list_edges();
            let message = format!("{} edges", records.len());
            output(records, message)
        }
    }
}

fn run_note(command: NoteCommand) -> Result<Output> {
    match command {
        NoteCommand::Add(args) => {
            let record = Note {
                id: args.target.id,
                text: args.text,
                on: args.on,
            };
            mutate(
                &args.target.path,
                |design| design.add_note(record),
                "Added note",
            )
        }
        NoteCommand::Set(args) => {
            let update = NoteUpdate {
                text: args.text,
                on: optional(args.on, args.clear_on),
            };
            mutate(
                &args.target.path,
                |design| design.set_note(&args.target.id, update),
                "Updated note",
            )
        }
        NoteCommand::Remove(target) => mutate(
            &target.path,
            |design| design.remove_note(&target.id),
            "Removed note",
        ),
        NoteCommand::List { path } => {
            let design = sysy_core::load(path)?;
            let records = design.list_notes();
            let message = format!("{} notes", records.len());
            output(records, message)
        }
    }
}

fn optional<T>(value: Option<T>, clear: bool) -> OptionalUpdate<T> {
    if clear {
        OptionalUpdate::Clear
    } else {
        value.map_or(OptionalUpdate::Keep, OptionalUpdate::Set)
    }
}
