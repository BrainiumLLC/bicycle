use std::{
    collections::VecDeque,
    fmt, fs, io,
    path::{Path, PathBuf},
};

static TEMPLATE_EXT: &'static str = "hbs";

/// Instruction for performing a filesystem action or template processing.
#[derive(Debug)]
pub enum Action {
    /// Specifies to create a new directory at `dest`.
    CreateDirectory { dest: PathBuf },
    /// Specifies to copy the file at `src` to `dest`.
    CopyFile { src: PathBuf, dest: PathBuf },
    /// Specifies to render the template at `src` to `dest`.
    WriteTemplate { src: PathBuf, dest: PathBuf },
}

impl Action {
    /// Create a new [`Action::CreateDirectory`].
    /// Uses `transform_path` to transform `dest`.
    pub fn new_create_directory<E>(
        dest: &Path,
        transform_path: impl Fn(&Path) -> Result<PathBuf, E>,
    ) -> Result<Self, E> {
        Ok(Action::CreateDirectory {
            dest: transform_path(dest)?,
        })
    }

    /// Create a new [`Action::CopyFile`].
    /// Uses `transform_path` to transform `dest`.
    pub fn new_copy_file<E>(
        src: &Path,
        dest: &Path,
        transform_path: impl Fn(&Path) -> Result<PathBuf, E>,
    ) -> Result<Self, E> {
        let dest = append_path(dest, src, false);
        Ok(Action::CopyFile {
            src: src.to_owned(),
            dest: transform_path(&dest)?,
        })
    }

    /// Create a new [`Action::WriteTemplate`].
    /// Uses `transform_path` to transform `dest`.
    pub fn new_write_template<E>(
        src: &Path,
        dest: &Path,
        transform_path: impl Fn(&Path) -> Result<PathBuf, E>,
    ) -> Result<Self, E> {
        let dest = append_path(dest, src, true);
        Ok(Action::WriteTemplate {
            src: src.to_owned(),
            dest: transform_path(&dest)?,
        })
    }

    /// Gets the destination of any [`Action`] variant.
    pub fn dest(&self) -> &Path {
        match self {
            Action::CreateDirectory { dest }
            | Action::CopyFile { dest, .. }
            | Action::WriteTemplate { dest, .. } => &dest,
        }
    }
}

fn append_path(base: &Path, other: &Path, strip_extension: bool) -> PathBuf {
    let tail = if strip_extension {
        other.file_stem().unwrap()
    } else {
        other.file_name().unwrap()
    };
    base.join(tail)
}

fn file_action<E>(
    src: &Path,
    dest: &Path,
    transform_path: impl Fn(&Path) -> Result<PathBuf, E>,
) -> Result<Action, E> {
    let is_template = src
        .extension()
        .map(|ext| ext == TEMPLATE_EXT)
        .unwrap_or(false);
    if is_template {
        Action::new_write_template(src, dest, transform_path)
    } else {
        Action::new_copy_file(src, dest, transform_path)
    }
}

/// An error encountered when traversing a file tree.
#[derive(Debug)]
pub enum TraversalError<E: fmt::Debug = crate::RenderingError> {
    /// Failed to get directory listing.
    ReadDirectoryError(io::Error),
    /// Failed to inspect entry from directory listing.
    ReadEntryError(io::Error),
    /// Failed to transform path.
    TransformPathError(E),
}

impl<E: fmt::Debug> From<E> for TraversalError<E> {
    fn from(err: E) -> Self {
        TraversalError::TransformPathError(err)
    }
}

fn traverse_dir<E: fmt::Debug>(
    src: &Path,
    dest: &Path,
    transform_path: &impl Fn(&Path) -> Result<PathBuf, E>,
    actions: &mut VecDeque<Action>,
) -> Result<(), TraversalError<E>> {
    if src.is_file() {
        actions.push_back(file_action(src, dest, transform_path)?);
    } else {
        actions.push_front(Action::new_create_directory(dest, transform_path)?);
        for entry in fs::read_dir(src).map_err(TraversalError::ReadDirectoryError)? {
            let path = entry.map_err(TraversalError::ReadEntryError)?.path();
            if path.is_dir() {
                traverse_dir(
                    &path,
                    &append_path(dest, &path, false),
                    transform_path,
                    actions,
                )?;
            } else {
                actions.push_back(file_action(&path, dest, transform_path)?);
            }
        }
    }
    Ok(())
}

/// Traverse file tree at `src` to generate an [`Action`] list.
/// The [`Action`] list specifies how to generate the `src` file tree at `dest`,
/// and can be executed by [`Bicycle::process_actions`](crate::Bicycle::process_actions).
///
/// File tree contents are interpreted as follows:
/// - Each directory in the file tree generates an [`Action::CreateDirectory`].
///   Directories are traversed recursively.
/// - Each file that doesn't end in the extension `.hbs` generates an [`Action::CopyFile`].
/// - Each file that ends in the extension `.hbs` generates an [`Action::WriteTemplate`].
///
/// `transform_path` is used to post-process destination path strings.
/// [`Bicycle::transform_path`](crate::Bicycle::transform_path) is one possible implementation.
pub fn traverse<E: fmt::Debug>(
    src: impl AsRef<Path>,
    dest: impl AsRef<Path>,
    transform_path: impl Fn(&Path) -> Result<PathBuf, E>,
) -> Result<VecDeque<Action>, TraversalError<E>> {
    let src = src.as_ref();
    let dest = dest.as_ref();
    let mut actions = VecDeque::new();
    traverse_dir(src, dest, &transform_path, &mut actions).map(|_| actions)
}
