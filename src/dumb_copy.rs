use crate::*;
use std::fmt::{self, Display};

#[derive(Debug)]
pub enum DumbCopyError {
    TraversalFailed {
        src: PathBuf,
        cause: TraversalError<std::convert::Infallible>,
    },
    ProcessingFailed(ProcessingError),
}

impl Display for DumbCopyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TraversalFailed { src, cause } => {
                write!(f, "Failed to traverse files at {:?}: {}", src, cause)
            }
            Self::ProcessingFailed(err) => write!(f, "{}", err),
        }
    }
}

/// Perform a recursive copy without actually doing any templating.
/// This seems silly to have in this crate, but it's easy to do using
/// our primitives (and tedious to do without them), so here it is.
pub fn dumb_copy(src: impl AsRef<Path>, dest: impl AsRef<Path>) -> Result<(), DumbCopyError> {
    let src = src.as_ref();
    let actions = traverse(src, dest, no_transform, None).map_err(|cause| {
        DumbCopyError::TraversalFailed {
            src: src.to_owned(),
            cause,
        }
    })?;
    let bike = Bicycle::default();
    bike.process_actions(actions.iter(), |_| ())
        .map_err(DumbCopyError::ProcessingFailed)
}
