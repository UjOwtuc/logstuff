//! General types applicable to any Application
use crate::config::Config;

/// Indicates whether the run loop should halt
pub enum Stopping {
    /// The run loop should halt
    Yes,

    /// The run loop should continue
    No,
}

/// The application; domain-specific program logic
pub trait Application: Sized {
    type Err: ::std::error::Error + 'static;

    /// Create a new instance given the options and config
    fn new(_: crate::Args, _: Config) -> Result<Self, Self::Err>;

    /// Called repeatedly in the main loop of the application.
    fn run_once(&mut self) -> Result<Stopping, Self::Err>;

    /// Called when the application is shutting down
    fn shutdown(self) -> Result<(), Self::Err> {
        Ok(())
    }
}

/// Run an Application of type T
///
/// `run` creates an application from `opts` and `config`. A run loop is entered
/// where `run_once` is repeatedly called on the `T`. Between calls, any
/// arriving signals are checked for and passed to the application via
/// `received_signal`.
pub fn run<T>(opts: crate::Args, config: Config) -> Result<(), <T as Application>::Err>
where
    T: Application,
{
    let mut app = T::new(opts, config)?;

    log::debug!("app initialized, starting main loop");
    loop {
        if let Stopping::Yes = app.run_once()? {
            break;
        }
    }

    log::debug!("main loop terminated, shutting down");
    app.shutdown()?;
    Ok(())
}
