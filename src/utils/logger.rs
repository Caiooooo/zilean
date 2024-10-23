use time::macros::format_description;
use tracing_subscriber::{util::SubscriberInitExt, EnvFilter};

// const GIT_VERSION: &str = git_version::git_version!(args = ["--always", "--exclude=*", "--dirty"]);
pub fn init_logger() {
    use tracing_subscriber::filter::LevelFilter;

    let timer = tracing_subscriber::fmt::time::OffsetTime::new(
        time::UtcOffset::from_hms(8, 0, 0).unwrap(),
        format_description!("[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]"),
    );

    tracing_subscriber::fmt::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_env_var("RUST_LOG")
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()
                .unwrap_or_else(|e| {
                    panic!("failed to parse RUST_LOG: {}", e);
                }),
        )
        .with_line_number(true)
        .with_timer(timer)
        .compact()
        .finish()
        .try_init()
        .expect("failed to init logger");
}
