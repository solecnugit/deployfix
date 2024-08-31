# DeployFix

## Build Image From Scratch

1. Make sure you have Docker installed on your machine.
2. Build the image using the following command from the root of the project directory (where the Dockerfile is located):
```bash
$    docker build -t deployfix .
```
> It may take a while to build the image, depending on your internet connection CPU and processors.
3. Run the image using the following command:
```bash
$    docker run --rm -it deployfix
```
4. Running tests inside the container:
```bash
$    ./run_cargo_tests.sh   # Run the unit tests
$    ./run_k8s_tests.sh     # Run the manually constructed tests for k8s
$    ./run_kwok_sim_test.sh # Run the simulation tests
```

## CLI Usage

You can run the CLI using the following command:

The following command will run the main process of DeployFix mentioned in the paper, which generates the feasible fix solution to repair the unsatisfiable deployment configuration files when conflict rules is identified.
The cycle check is to detect the circular dependencies in the directed graphs involves all the affinity constraints inside the deployment configuration files.
```bash
$    ./target/release/deployfix-cli k8s go <SOURCE_DIR> <INJECTION_DIR> <OUTPUT> --recommend --cycle-check --env-file=<ENV_FILE_PATH>
```
where, `<SOURCE_DIR>` is the path to the directory contains the deployment configuration files, `<INJECTION_DIR>` is the path to the directory contains the intermediate representation files, specify the directory to be empty if no injection is needed, and `<OUTPUT>` is the path to the directory to store the output files.
The `--recommend` flag is to recommend and generate repaired deployment configurations when unsatisfiable, the `--cycle-check` flag is to enable circular dependency check, and the `--env-file` flag is to specify the dynamic environment file, the format is `node_name key=value;key=value;...`.
In addition, to reject entities with no corresponding definitions, you can add `--reject-unknown` to the command.


```bash
$    ./target/release/deployfix-cli --help

Usage: deployfix-cli [OPTIONS] [COMMAND]

Commands:
  check
  k8s
  yarn
  help   Print this message or the help of the given subcommand(s)

Options:
  -l, --log-dir <LOG_DIR>
  -h, --help               Print help
  -V, --version            Print version
```

### Check Command

```bash
$    ./target/release/deployfix-cli check --help

Usage: deployfix-cli check [OPTIONS] <PATH>

Arguments:
  <PATH>  # Path to the directory contains the intemediate representation files

Options:
  -f, --format <FORMAT>  # Format of the intermediate representation files
  -d, --domain <DOMAIN>  # Scheduling domain to check, leave it empty to check all domains
      --default-domain-key <DEFAULT_DOMAIN_KEY>  # Default domain key
  -c, --cycle-check      # Check circular dependencies in the affinity graph
  -h, --help             # Print help
```

### K8s Command

```bash
$    ./target/release/deployfix-cli k8s --help

Usage: deployfix-cli k8s [COMMAND]

Commands:
  import  # Translate deployment configuration to intermediate representation
  inject  # Inject the intermediate representation into the deployment configuration
  go      # The main process of DeployFix
  help    # Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help

$    ./target/release/deployfix-cli k8s go --help
Usage: deployfix-cli k8s go [OPTIONS] <SOURCE_DIR> <INJECTION_DIR> <OUTPUT>

Arguments:
  <SOURCE_DIR>     Path to K8s files # Path to the directory contains the deployment configuration files
  <INJECTION_DIR>  Path to deployfix files # Path to the directory contains the intermediate representation files, specify the directory to be empty or the same as <SOURCE_DIR> if no injection is needed
  <OUTPUT>         Path to output # Path to the directory to store the output files

Options:
  -r, --recommend    Recommend and generate repaired deployment configurations when unsatisfiable
  --cycle-check      Enable circular dependency check
  --reject-unknown   Enable rejecting unknown entities
  --env-file         Specfic the dynamic environment file, format: `node_name key=value;key=value;...`
  -h, --help         Print help
```

### Yarn Command

```bash
$    ./target/release/deployfix-cli yarn --help
Usage: deployfix-cli yarn [COMMAND]

Commands:
  import
  inject
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

## About Simulation

Due to the rate limit of Kwok, it may take a lot time to wait for the creating and deploying progress.
(For the default dataset we provided, it may take about 1h to finish the simulation.)

1. If it takes too long to finish, please decrease the number of applications or nodes, inside the `sim-scripts/generate_dataset.py`
2. If no pods are scheduled, please check kwok's status and the logs of the kube-scheduler. Try to restart the kwok cluster and run `python sim-scripts/run_fake_deployment --skip-setup` to skip the setup process.