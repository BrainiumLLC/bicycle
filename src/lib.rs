//! `bicycle` is [`handlebars`] with wheels. 🚴🏽‍♀️

mod json_map;
mod traverse;

pub use self::{json_map::*, traverse::*};
pub use handlebars::{self, HelperDef};
use handlebars::Handlebars;
use std::{
    fmt, fs,
    io::{self, Read, Write},
    iter,
    path::{Path, PathBuf},
};

pub type CustomEscapeFn = &'static (dyn Fn(&str) -> String + 'static + Send + Sync);

/// Specifies how to escape template variables prior to rendering.
pub enum EscapeFn {
    /// The default setting. Doesn't change the variables at all.
    None,
    /// Escape anything that looks like HTML. This is recommended when rendering HTML templates with user-provided data.
    Html,
    /// Escape using a custom function.
    Custom(CustomEscapeFn),
}

impl fmt::Debug for EscapeFn {
    fn fmt(&self, fmtr: &mut fmt::Formatter) -> fmt::Result {
        fmtr.pad(match self {
            EscapeFn::None => "None",
            EscapeFn::Html => "Html",
            EscapeFn::Custom(_) => "Custom(..)",
        })
    }
}

impl Default for EscapeFn {
    fn default() -> Self {
        EscapeFn::None
    }
}

impl From<CustomEscapeFn> for EscapeFn {
    fn from(custom: CustomEscapeFn) -> Self {
        EscapeFn::Custom(custom)
    }
}

/// An error encountered when rendering a template.
#[derive(Debug)]
pub enum RenderingError {
    RenderingError(handlebars::TemplateRenderError),
}

impl From<handlebars::TemplateRenderError> for RenderingError {
    fn from(err: handlebars::TemplateRenderError) -> Self {
        RenderingError::RenderingError(err)
    }
}

/// An error encountered when processing an [`Action`].
#[derive(Debug)]
pub enum ProcessingError {
    /// Failed to traverse files.
    TraversalError(TraversalError<RenderingError>),
    /// Failed to create directory.
    CreateDirectoryError(io::Error),
    /// Failed to copy file.
    CopyFileError(io::Error),
    /// Failed to open or read input file.
    ReadTemplateError(io::Error),
    /// Failed to render template.
    RenderTemplateError(RenderingError),
    /// Failed to create or write output file.
    WriteTemplateError(io::Error),
}

impl From<TraversalError<RenderingError>> for ProcessingError {
    fn from(err: TraversalError<RenderingError>) -> Self {
        ProcessingError::TraversalError(err)
    }
}

impl From<RenderingError> for ProcessingError {
    fn from(err: RenderingError) -> Self {
        ProcessingError::RenderTemplateError(err)
    }
}

#[derive(Debug)]
pub struct Bicycle {
    handlebars: Handlebars,
    base_data: JsonMap,
}

impl Bicycle {
    /// Creates a new [`Bicycle`] instance, using the provided arguments to
    /// configure the underlying [`handlebars::Handlebars`] instance.
    ///
    /// For info on `helpers`, consult the [`handlebars` docs](../handlebars/index.html#custom-helper).
    ///
    /// `base_data` is data that will be available for all invocations of all methods on this instance.
    ///
    /// # Examples
    /// ```
    /// use bicycle::{
    ///     handlebars::{handlebars_helper, HelperDef},
    ///     Bicycle, EscapeFn, JsonMap,
    /// };
    /// use std::collections::HashMap;
    ///
    /// // An escape function that just replaces spaces with an angry emoji...
    /// fn spaces_make_me_very_mad(raw: &str) -> String {
    ///     raw.replace(' ', "😡")
    /// }
    ///
    /// // A helper to reverse strings.
    /// handlebars_helper!(reverse: |s: str|
    ///     // This doesn't correctly account for graphemes, so
    ///     // use a less naïve implementation for real apps.
    ///     s.chars().rev().collect::<String>()
    /// );
    ///
    /// // You could just as well use a [`Vec`] of tuples, or in this case,
    /// // [`std::iter::once`].
    /// let mut helpers = HashMap::<_, Box<dyn HelperDef>>::new();
    /// helpers.insert("reverse", Box::new(reverse));
    ///
    /// let bike = Bicycle::new(
    ///     EscapeFn::Custom(&spaces_make_me_very_mad),
    ///     helpers,
    ///     JsonMap::default(),
    /// );
    /// ```
    pub fn new<'helper_name>(
        escape_fn: EscapeFn,
        helpers: impl iter::IntoIterator<Item = (&'helper_name str, Box<dyn HelperDef + 'static>)>,
        base_data: JsonMap,
    ) -> Self {
        let mut handlebars = Handlebars::new();
        handlebars.set_strict_mode(true);
        match escape_fn {
            EscapeFn::Custom(escape_fn) => handlebars.register_escape_fn(escape_fn),
            _ => handlebars.register_escape_fn(match escape_fn {
                EscapeFn::None => handlebars::no_escape,
                EscapeFn::Html => handlebars::html_escape,
                _ => unsafe { std::hint::unreachable_unchecked() },
            }),
        }
        for (name, helper) in helpers {
            handlebars.register_helper(name, helper);
        }
        Self {
            handlebars,
            base_data,
        }
    }

    /// Renders a template.
    ///
    /// Use `insert_data` to define any variables needed for the template.
    ///
    /// # Examples
    /// ```
    /// use bicycle::Bicycle;
    ///
    /// let bike = Bicycle::default();
    /// let rendered = bike.render("Hello {{name}}!", |map| {
    ///     map.insert("name", "Shinji");
    /// }).unwrap();
    /// assert_eq!(rendered, "Hello Shinji!");
    /// ```
    pub fn render(
        &self,
        template: &str,
        insert_data: impl FnOnce(&mut JsonMap),
    ) -> Result<String, RenderingError> {
        let mut data = self.base_data.clone();
        insert_data(&mut data);
        self.handlebars
            .render_template(template, &data.0)
            .map_err(Into::into)
    }

    /// Executes an [`Action`].
    ///
    /// - [`Action::CreateDirectory`] is executed with the same semantics as `mkdir -p`:
    ///   any missing parent directories are also created, and creation succeeds even if
    ///   the directory already exists. Failure results in a [`ProcessingError::CreateDirectoryError`].
    /// - [`Action::CopyFile`] is executed with the same semantics as `cp`:
    ///   if the destination file already exists, it will be overwritted with a copy of
    ///   the source file. Failure results in a [`ProcessingError::CopyFileError`].
    /// - [`Action::WriteTemplate`] is executed by reading the source file,
    ///   rendering the contents as a template (using `insert_data` to pass
    ///   any required values to the underlying [`Bicycle::render`] call),
    ///   and then finally writing the result to the destination file. The destination
    ///   file will be overwritten if it already exists. Failure for each step results
    ///   in [`ProcessingError::ReadTemplateError`], [`ProcessingError::RenderTemplateError`],
    ///   and [`ProcessingError::WriteTemplateError`], respectively.
    pub fn process_action(
        &self,
        action: &Action,
        insert_data: impl Fn(&mut JsonMap),
    ) -> Result<(), ProcessingError> {
        log::info!("{:#?}", action);
        match action {
            Action::CreateDirectory { dest } => {
                fs::create_dir_all(&dest).map_err(ProcessingError::CreateDirectoryError)?;
            }
            Action::CopyFile { src, dest } => {
                fs::copy(src, dest).map_err(ProcessingError::CopyFileError)?;
            }
            Action::WriteTemplate { src, dest } => {
                let mut template = String::new();
                fs::File::open(src)
                    .and_then(|mut file| file.read_to_string(&mut template))
                    .map_err(ProcessingError::ReadTemplateError)?;
                let rendered = self.render(&template, insert_data)?;
                fs::File::create(dest)
                    .and_then(|mut file| file.write_all(rendered.as_bytes()))
                    .map_err(ProcessingError::WriteTemplateError)?;
            }
        }
        Ok(())
    }

    /// Iterates over `actions`, passing each item to [`Bicycle::process_action`].
    pub fn process_actions<'iter_item>(
        &self,
        actions: impl iter::Iterator<Item = &'iter_item Action>,
        insert_data: impl Fn(&mut JsonMap),
    ) -> Result<(), ProcessingError> {
        for action in actions {
            self.process_action(action, &insert_data)?;
        }
        Ok(())
    }

    /// A convenience method that calls [`traverse`](traverse()) and passes the output to [`Bicycle::process_actions`].
    /// Uses [`Bicycle::transform_path`] as the `transform_path` argument to [`traverse`](traverse()).
    pub fn process(
        &self,
        src: impl AsRef<Path>,
        dest: impl AsRef<Path>,
        insert_data: impl Fn(&mut JsonMap),
    ) -> Result<(), ProcessingError> {
        traverse(src, dest, |path| self.transform_path(path, &insert_data))
            .map_err(ProcessingError::TraversalError)
            .and_then(|actions| self.process_actions(actions.iter(), insert_data))
    }

    /// Renders a path string itself as a template.
    /// Intended to be used as the `transform_path` argument to [`traverse`](traverse()).
    pub fn transform_path(
        &self,
        path: &Path,
        insert_data: impl FnOnce(&mut JsonMap),
    ) -> Result<PathBuf, RenderingError> {
        let path_str = path.to_str().unwrap();
        // This is naïve, but optimistically isn't a problem in practice.
        if path_str.contains("{{") {
            self.render(path_str, insert_data).map(Into::into)
        } else {
            Ok(path.to_owned())
        }
    }
}

impl Default for Bicycle {
    fn default() -> Self {
        Self::new(Default::default(), iter::empty(), Default::default())
    }
}
