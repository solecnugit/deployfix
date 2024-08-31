use std::path::PathBuf;

use clap::Subcommand;
use log::{debug, info, warn};

use crate::{
    model::{get_parser, merge_entities, DeployIRFormatter, EntitySource},
    plugin::yarn::{formatter::YarnFormatter, parser::parser::YarnSpecParser},
};

#[derive(Subcommand)]
pub enum YarnCommands {
    Import {
        #[clap(value_name = "PATH", help = "Paths to Yarn Placement Spec files")]
        paths: Vec<PathBuf>,
    },
    Inject {
        #[clap(value_name = "OUTPUT", help = "Output Yarn Placement Spec File")]
        output_file: PathBuf,
        #[clap(value_name = "PATH", help = "Paths to deployfix files")]
        paths: Vec<PathBuf>,
    },
}

fn inject(entities: Vec<crate::model::Entity>, output_file_path: PathBuf) {
    let formatter = YarnFormatter::new();
    let output = formatter.format(&entities);

    let parent_dir = output_file_path.parent().unwrap();
    if !parent_dir.exists() {
        std::fs::create_dir_all(parent_dir).unwrap();
    }

    if output_file_path.exists() {
        std::fs::remove_file(&output_file_path).unwrap();

        warn!("Removed existing file {}", output_file_path.display());
    }

    std::fs::write(output_file_path, output).unwrap();
}

pub fn execute(commands: YarnCommands) {
    match commands {
        YarnCommands::Import { paths } => {
            let entities = paths
                .into_iter()
                .flat_map(|path| {
                    let parser = YarnSpecParser::new();
                    let data = std::fs::read_to_string(&path).unwrap();

                    parser.parse(&data, path).unwrap()
                })
                .collect::<Vec<_>>();

            let entities = merge_entities(
                entities,
                Some(|a, b| match (a, b) {
                    (EntitySource::File(a), EntitySource::File(b)) => {
                        if !a.ends_with(".spec") {
                            warn!("Replacing {} with {}", a, b);
                            *a = b;
                        }
                    }
                    _ => {}
                }),
            );
            debug!("Imported entities: {:?}", entities);

            let output = DeployIRFormatter::format(&entities);

            info!("{}", output);

            std::fs::write("output.deployfix", output).unwrap();
        }
        YarnCommands::Inject {
            output_file: output_dir,
            paths,
        } => {
            let entities = paths
                .into_iter()
                .flat_map(|path| {
                    debug!("Importing from {}", path.display());

                    get_parser("deployfix")
                        .unwrap()
                        .parse(
                            &std::fs::read_to_string(&path).unwrap(),
                            crate::model::EntitySource::File(path.to_str().unwrap().to_string()),
                        )
                        .unwrap()
                })
                .collect::<Vec<_>>();

            let entities = merge_entities(
                entities,
                Some(|a, b| match (a, b) {
                    (EntitySource::File(a), EntitySource::File(b)) => {
                        if !a.ends_with(".spec") {
                            warn!("Replacing {} with {}", a, b);
                            *a = b;
                        }
                    }
                    _ => {}
                }),
            );

            debug!("Imported entities: {:?}", entities);

            inject(entities, output_dir)
        }
    }
}
