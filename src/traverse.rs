use std::{
    collections::VecDeque,
    error::Error as StdError,
    fmt::{Debug, Display},
    fs, io,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Clone, Copy, Debug)]
pub enum Tag {
    /// Specifies to create a new directory at `dst`.
    CreateDirectory,
    /// Specifies to copy the file at `src` to `dst`.
    CopyFile,
    /// Specifies to render the template at `src` to `dst`.
    WriteTemplate,
}

impl Tag {
    pub fn create_directory(&self) -> bool {
        matches!(self, Self::CreateDirectory)
    }

    pub fn copy_file(&self) -> bool {
        matches!(self, Self::CopyFile)
    }

    pub fn write_template(&self) -> bool {
        matches!(self, Self::WriteTemplate)
    }

    fn strip_extension(&self) -> bool {
        self.write_template()
    }
}

/// Instruction for performing a filesystem action or template processing.
#[derive(Debug)]
pub struct Action {
    src: PathBuf,
    dst: PathBuf,
    tag: Tag,
}

impl Action {
    pub fn new<E: Debug + Display + StdError>(
        src: impl Into<PathBuf>,
        dst: impl AsRef<Path>,
        transform_dst: impl Fn(&Path) -> Result<PathBuf, E>,
        tag: Tag,
    ) -> Result<Self, E> {
        let src = src.into();
        let dst = append_path(dst, &src, tag.strip_extension());
        let transformed_dst = transform_dst(&dst)?;
        log::info!("transformed {:?} into {:?}", dst, transformed_dst);
        Ok(Self { src, dst, tag })
    }

    pub fn detect<E: Debug + Display + StdError>(
        src: impl Into<PathBuf>,
        dst: impl AsRef<Path>,
        transform_dst: impl Fn(&Path) -> Result<PathBuf, E>,
        template_ext: Option<&str>,
    ) -> Result<Self, E> {
        let src = src.into();
        let tag = if src.is_dir() {
            Tag::CreateDirectory
        } else {
            template_ext
                .and_then(|template_ext| src.extension().filter(|ext| *ext == template_ext))
                .map(|_| Tag::WriteTemplate)
                .unwrap_or_else(|| Tag::CopyFile)
        };
        log::info!("detected tag {:?} for path {:?}", tag, src);
        Self::new(src, dst, transform_dst, tag)
    }

    pub fn push_onto(self, vec: &mut VecDeque<Self>) {
        if self.tag.create_directory() {
            log::info!("pushed onto front of action list: {:#?}", self);
            vec.push_front(self)
        } else {
            log::info!("pushed onto back of action list: {:#?}", self);
            vec.push_back(self)
        }
    }

    pub fn src(&self) -> &Path {
        &self.src
    }

    pub fn dst(&self) -> &Path {
        &self.dst
    }

    pub fn tag(&self) -> Tag {
        self.tag
    }
}

fn append_path(base: impl AsRef<Path>, other: &Path, strip_extension: bool) -> PathBuf {
    let tail = if strip_extension {
        other.file_stem().unwrap()
    } else {
        other.file_name().unwrap()
    };
    let base = base.as_ref();
    let appended = base.join(tail);
    log::debug!(
        "appended tail {:?} to base {:?} (strip extension set to {:?})",
        tail,
        base,
        strip_extension
    );
    appended
}

/// An error encountered when traversing a file tree.
#[derive(Debug, Error)]
pub enum TraversalError<E: Debug + Display + StdError + 'static = crate::RenderingError> {
    /// Failed to get directory listing.
    #[error("Failed to read directory at {path:?}: {cause}")]
    DirectoryReadFailed {
        path: PathBuf,
        #[source]
        cause: io::Error,
    },
    /// Failed to inspect entry from directory listing.
    #[error("Failed to read directory entry in {dir:?}: {cause}")]
    EntryReadFailed {
        dir: PathBuf,
        #[source]
        cause: io::Error,
    },
    /// Failed to transform path.
    #[error("Failed to transform path at {path:?}: {cause}")]
    PathTransformFailed {
        path: PathBuf,
        #[source]
        cause: E,
    },
}

fn traverse_dir<E: Debug + Display + StdError>(
    src: &Path,
    dst: &Path,
    transform_dst: &impl Fn(&Path) -> Result<PathBuf, E>,
    template_ext: Option<&str>,
    actions: &mut VecDeque<Action>,
) -> Result<(), TraversalError<E>> {
    Action::detect(src, dst, transform_dst, template_ext)
        .map_err(|cause| TraversalError::PathTransformFailed {
            path: dst.to_owned(),
            cause,
        })?
        .push_onto(actions);
    if src.is_dir() {
        log::info!("descending into dir {:?}", src);
        for entry in fs::read_dir(src).map_err(|cause| TraversalError::DirectoryReadFailed {
            path: src.to_owned(),
            cause,
        })? {
            let new_src = entry
                .map_err(|cause| TraversalError::EntryReadFailed {
                    dir: src.to_owned(),
                    cause,
                })?
                .path();
            if new_src.is_dir() {
                let new_dst = append_path(dst, &new_src, false);
                traverse_dir(&new_src, &new_dst, transform_dst, template_ext, actions)?;
            } else {
                Action::detect(&new_src, dst, transform_dst, template_ext)
                    .map_err(|cause| TraversalError::PathTransformFailed {
                        path: dst.to_owned(),
                        cause,
                    })?
                    .push_onto(actions);
            }
        }
    }
    Ok(())
}

/// Traverse file tree at `src` to generate an [`Action`] list.
/// The [`Action`] list specifies how to generate the `src` file tree at `dst`,
/// and can be executed by [`Bicycle::process_actions`](crate::Bicycle::process_actions).
///
/// File tree contents are interpreted as follows:
/// - Each directory in the file tree generates an [`Action::CreateDirectory`].
///   Directories are traversed recursively.
/// - Each file that doesn't end in `template_ext` generates an [`Action::CopyFile`].
/// - Each file that ends in `template_ext` generates an [`Action::WriteTemplate`].
///
/// `transform_dst` is used to post-process destination path strings.
/// [`Bicycle::transform_dst`](crate::Bicycle::transform_dst) is one possible implementation.
pub fn traverse<E: Debug + Display + StdError>(
    src: impl AsRef<Path>,
    dst: impl AsRef<Path>,
    transform_dst: impl Fn(&Path) -> Result<PathBuf, E>,
    template_ext: Option<&str>,
) -> Result<VecDeque<Action>, TraversalError<E>> {
    let src = src.as_ref();
    let dst = dst.as_ref();
    let mut actions = VecDeque::new();
    traverse_dir(src, dst, &transform_dst, template_ext, &mut actions).map(|_| actions)
}

/// Pass this to `traverse` if you don't want any path transformation at all.
pub fn no_transform(path: &Path) -> Result<PathBuf, std::convert::Infallible> {
    Ok(path.to_owned())
}

/// `Some("hbs")`. Pass this to `traverse` to get the same template
/// identification behavior as `Bicycle::process`.
pub static DEFAULT_TEMPLATE_EXT: Option<&'static str> = Some("hbs");
