mod annotate;

pub use annotate::ConflictAnnotater;
use flexi_logger::FileSpec;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use log::{debug, error, info, warn};

use crate::{
    model::{get_parser, Entity},
    plugin::{k8s::K8SCommands, yarn::YarnCommands},
    solver::{self, get_solver, SolverOutput},
    util,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[clap(short, long)]
    log_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    Check {
        #[clap(value_name = "PATH")]
        path: PathBuf,
        #[clap(short, long, value_name = "FORMAT")]
        format: Option<String>,
        #[clap(short, long)]
        domain: Option<String>,
        #[clap(long)]
        default_domain_key: Option<String>,
        #[clap(short, long, default_value = "true")]
        cycle_check: bool,
    },
    K8S {
        #[command(subcommand)]
        command: Option<K8SCommands>,
    },
    Yarn {
        #[command(subcommand)]
        command: Option<YarnCommands>,
    },
}

fn init_logger(path: Option<PathBuf>) {
    if let Some(path) = path {
        // Set env logger format
        flexi_logger::Logger::try_with_env_or_str("info")
            .expect("Failed to initialize logger")
            .log_to_file(FileSpec::default().directory(path))
            .write_mode(flexi_logger::WriteMode::BufferAndFlush)
            .duplicate_to_stderr(flexi_logger::Duplicate::Warn)
            .format(|write, now, record| {
                let now = now.format("%Y-%m-%d %H:%M");

                write!(write, "{} [{}] {}", now, record.level(), record.args())
            })
            .start()
            .expect("Failed to initialize logger");
    } else {
        flexi_logger::Logger::try_with_env_or_str("info")
            .expect("Failed to initialize logger")
            .format(|write, now, record| {
                let now = now.format("%Y-%m-%d %H:%M");

                write!(write, "{} [{}] {}", now, record.level(), record.args())
            })
            .start()
            .expect("Failed to initialize logger");
    }
}

pub fn run() {
    let cli = Cli::parse();
    init_logger(cli.log_dir);

    match cli.command {
        Some(Commands::Check {
            path,
            format,
            domain,
            default_domain_key,
            cycle_check,
        }) => {
            let format = match format {
                Some(f) => f,
                None => path.extension().unwrap().to_str().unwrap().to_string(),
            };

            let format = match format.as_str() {
                "ir" => "deployfix",
                x => x,
            };

            debug!("Importing from {} with format {:?}", path.display(), format);

            let parser = get_parser(&format).unwrap();
            let data = std::fs::read_to_string(&path).unwrap();
            let entities = parser.parse(&data, path.into()).unwrap();
            debug!("Imported entities: {:?}", entities);

            let mut no_conflict = true;

            if let Some(domain) = domain {
                assert!(default_domain_key.is_some());

                let default_domain_key = default_domain_key.unwrap();
                let entities = util::split_by_metadata(&entities, &domain, &default_domain_key);

                for (domain, entities) in entities {
                    info!("Checking domain {}...", domain);

                    no_conflict &= solve(entities, cycle_check);
                }
            } else {
                no_conflict = solve(entities, cycle_check);
            }

            if no_conflict {
                info!("No conflict found");
            }
        }
        Some(Commands::K8S { command }) => {
            if let Some(command) = command {
                crate::plugin::k8s::execute(command)
            } else {
                warn!("No command specified")
            }
        }
        Some(Commands::Yarn { command }) => {
            if let Some(command) = command {
                crate::plugin::yarn::execute(command)
            } else {
                warn!("No command specified")
            }
        }
        None => {
            warn!("No command specified")
        }
    }
}

fn solve(entities: Vec<Entity>, cycle_check: bool) -> bool {
    let entity_map = entities.try_into().unwrap();

    let result = if cycle_check {
        let ring_solver = get_solver("ring").unwrap();
        let ring_result = ring_solver.solve(&entity_map);
        debug!("Ring Solver Result: {:?}", ring_result);

        let solver = get_solver("z3").unwrap();
        let result = solver.solve(&entity_map);

        debug!("Z3 Solver Result: {:?}", result);

        ring_result.merge(result)
    } else {
        let solver = get_solver("z3").unwrap();
        let result = solver.solve(&entity_map);

        debug!("Z3 Solver Result: {:?}", result);

        result
    };

    if let SolverOutput::Conflict(conflicts) = result {
        let conflicts_annotations = conflicts
            .into_iter()
            .flat_map(|(k, v)| v.into_iter().map(move |v| (k.clone(), v)))
            .map(|(name, rule)| ConflictAnnotater::new(name.as_str(), &rule).annotate())
            .collect::<Vec<_>>();

        let conflicts = conflicts_annotations.join("\n\n");

        error!("{}", conflicts);

        false
    } else {
        true
    }
}
