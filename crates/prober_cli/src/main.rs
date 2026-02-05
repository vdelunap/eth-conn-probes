use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "eth-prober", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Run {
        #[arg(long)]
        config: String,

        #[arg(long, default_value_t = false)]
        no_send: bool,

        #[arg(long, default_value_t = false)]
        pretty: bool,
    },
    PrintDefaultConfig,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Command::Run {
            config,
            no_send,
            pretty,
        } => {
            let cfg = prober_core::config::Config::from_toml_file(std::path::Path::new(&config))?;
            let report = prober_core::run_plan(cfg.clone()).await?;

            if pretty {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}", serde_json::to_string(&report)?);
            }

            if cfg.reporting.enabled && !no_send {
                prober_core::reporting::send_report(&report, &cfg.reporting.report_url, cfg.reporting.timeout_ms).await?;
            }
        }
        Command::PrintDefaultConfig => {
            let s = include_str!("../../../config/default.toml");
            print!("{s}");
        }
    }

    Ok(())
}
