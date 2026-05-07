use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::{EnvFilter, fmt};

pub fn init_tracing(verbose: bool, to_stderr: bool) -> anyhow::Result<()> {
    // silenciar arboard + subir nivel a ERROR en TUI mode
    let filter = if verbose {
        EnvFilter::try_new("debug").unwrap_or_default()
    } else if to_stderr {
        // TUI mode: solo errores críticos, silenciar arboard y libs ruidosas
        EnvFilter::try_new("error,arboard=off,lite_agent=info").unwrap_or_default()
    } else {
        EnvFilter::try_new("info,rustls=warn,hyper=warn").unwrap_or_default()
    };

    let writer: BoxMakeWriter = if to_stderr {
        BoxMakeWriter::new(std::io::stderr)
    } else {
        BoxMakeWriter::new(std::io::stdout)
    };

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(writer)
        .init();
    Ok(())
}
